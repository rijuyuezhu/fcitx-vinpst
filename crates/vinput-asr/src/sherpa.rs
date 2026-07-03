//! Local `sherpa-onnx` ASR backend seam.
//!
//! This module owns typed config parsing, model layout validation, and the
//! optional official `sherpa-onnx` runtime adapter. The runtime remains behind a
//! Cargo feature so default CI and command-demo installs do not download or link
//! native ASR libraries.

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use vinput_config::{AsrProviderConfig, AsrProviderKind};

use crate::AsrError;
#[cfg(feature = "sherpa-onnx-backend")]
use crate::{
    AsrBackend, BackendCapabilities, BackendDescriptor, RecognitionContext, RecognitionEvent,
    RecognitionSession,
};

/// Legacy local provider id used by bundled config and diagnostics.
pub const SHERPA_ONNX_PROVIDER_ID: &str = "sherpa-onnx";

/// Parsed local `sherpa-onnx` provider settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SherpaOnnxSpec {
    /// Provider id from config.
    pub provider_id: String,
    /// Optional model id/path from config.
    pub model: Option<String>,
    /// Optional hotwords file path from config.
    pub hotwords_file: Option<String>,
    /// Optional backend timeout from config.
    pub timeout_ms: Option<u64>,
}

/// Resolved local filesystem inputs for the future sherpa runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SherpaOnnxModelPaths {
    /// Resolved model directory.
    pub model_dir: PathBuf,
    /// Resolved hotwords file, when configured.
    pub hotwords_file: Option<PathBuf>,
}

/// Supported local `sherpa-onnx` offline model layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SherpaOnnxOfflineModelLayout {
    /// `SenseVoice` model directory with a model file and tokens file.
    SenseVoice {
        /// ONNX model file.
        model: PathBuf,
        /// Tokens file.
        tokens: PathBuf,
        /// Language tag passed to sherpa-onnx.
        language: String,
        /// Whether sherpa-onnx inverse text normalization is enabled.
        use_itn: bool,
    },
}

/// Resolved local inputs and inferred runtime layout for offline recognition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SherpaOnnxOfflineRuntimePlan {
    /// Validated model and hotwords paths.
    pub paths: SherpaOnnxModelPaths,
    /// Inferred offline model layout.
    pub layout: SherpaOnnxOfflineModelLayout,
}

/// Local sherpa model path validation errors.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SherpaOnnxModelPathError {
    /// Provider does not configure a model.
    #[error("sherpa-onnx provider `{provider_id}` does not configure a model")]
    MissingModel {
        /// Provider id.
        provider_id: String,
    },
    /// Configured model value is empty after trimming.
    #[error("sherpa-onnx provider `{provider_id}` has an empty model path")]
    EmptyModel {
        /// Provider id.
        provider_id: String,
    },
    /// Configured path looks like a URL and is not a local filesystem path.
    #[error("sherpa-onnx provider `{provider_id}` path `{path}` must be local")]
    UrlLikePath {
        /// Provider id.
        provider_id: String,
        /// Rejected path.
        path: String,
    },
    /// Resolved model path does not exist.
    #[error("sherpa-onnx model directory `{path}` does not exist")]
    MissingModelDir {
        /// Resolved model path.
        path: String,
    },
    /// Resolved model path exists but is not a directory.
    #[error("sherpa-onnx model path `{path}` is not a directory")]
    ModelPathNotDirectory {
        /// Resolved model path.
        path: String,
    },
    /// Configured hotwords value is empty after trimming.
    #[error("sherpa-onnx provider `{provider_id}` has an empty hotwords path")]
    EmptyHotwords {
        /// Provider id.
        provider_id: String,
    },
    /// Resolved hotwords path does not exist.
    #[error("sherpa-onnx hotwords file `{path}` does not exist")]
    MissingHotwordsFile {
        /// Resolved hotwords path.
        path: String,
    },
    /// Resolved hotwords path exists but is not a regular file.
    #[error("sherpa-onnx hotwords path `{path}` is not a regular file")]
    HotwordsPathNotFile {
        /// Resolved hotwords path.
        path: String,
    },
    /// Model directory exists but does not match a supported offline layout.
    #[error(
        "sherpa-onnx model directory `{path}` does not contain a supported offline model layout"
    )]
    UnsupportedOfflineLayout {
        /// Resolved model path.
        path: String,
    },
    /// Model directory contains a model but is missing the required tokens file.
    #[error("sherpa-onnx model directory `{path}` is missing tokens.txt")]
    MissingTokensFile {
        /// Resolved model path.
        path: String,
    },
}

impl SherpaOnnxSpec {
    /// Parses a config provider into the future local `sherpa-onnx` spec.
    pub fn from_provider(provider: &AsrProviderConfig) -> Result<Self, AsrError> {
        if provider.id != SHERPA_ONNX_PROVIDER_ID || provider.kind != AsrProviderKind::Local {
            return Err(AsrError::UnsupportedProviderKind {
                provider_id: provider.id.clone(),
                kind: crate::factory::provider_kind_label(&provider.kind).to_owned(),
            });
        }

        Ok(Self {
            provider_id: provider.id.clone(),
            model: provider.model.clone(),
            hotwords_file: provider.hotwords_file.clone(),
            timeout_ms: provider.timeout_ms,
        })
    }

    /// Resolves configured model and hotwords paths against a local model root.
    ///
    /// Relative model values are resolved under `model_root`; absolute paths are
    /// preserved. Relative hotwords paths are resolved under the resolved model
    /// directory. This validates only filesystem shape required before a future
    /// runtime is constructed; it does not load sherpa-onnx or mutate files.
    pub fn resolve_model_paths(
        &self,
        model_root: impl AsRef<Path>,
    ) -> Result<SherpaOnnxModelPaths, SherpaOnnxModelPathError> {
        let model = self
            .model
            .as_deref()
            .ok_or_else(|| SherpaOnnxModelPathError::MissingModel {
                provider_id: self.provider_id.clone(),
            })?
            .trim();
        if model.is_empty() {
            return Err(SherpaOnnxModelPathError::EmptyModel {
                provider_id: self.provider_id.clone(),
            });
        }
        reject_url_like(&self.provider_id, model)?;

        let model_dir = resolve_against(model_root.as_ref(), model);
        if !model_dir.exists() {
            return Err(SherpaOnnxModelPathError::MissingModelDir {
                path: display_path(&model_dir),
            });
        }
        if !model_dir.is_dir() {
            return Err(SherpaOnnxModelPathError::ModelPathNotDirectory {
                path: display_path(&model_dir),
            });
        }

        let hotwords_file = self
            .hotwords_file
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(|path| {
                reject_url_like(&self.provider_id, path)?;
                let resolved = resolve_against(&model_dir, path);
                if !resolved.exists() {
                    return Err(SherpaOnnxModelPathError::MissingHotwordsFile {
                        path: display_path(&resolved),
                    });
                }
                if !resolved.is_file() {
                    return Err(SherpaOnnxModelPathError::HotwordsPathNotFile {
                        path: display_path(&resolved),
                    });
                }
                Ok(resolved)
            })
            .transpose()?;
        if self
            .hotwords_file
            .as_deref()
            .is_some_and(|path| path.trim().is_empty())
        {
            return Err(SherpaOnnxModelPathError::EmptyHotwords {
                provider_id: self.provider_id.clone(),
            });
        }

        Ok(SherpaOnnxModelPaths {
            model_dir,
            hotwords_file,
        })
    }

    /// Resolves local inputs and infers the first supported offline runtime layout.
    pub fn resolve_offline_runtime_plan(
        &self,
        model_root: impl AsRef<Path>,
    ) -> Result<SherpaOnnxOfflineRuntimePlan, SherpaOnnxModelPathError> {
        let paths = self.resolve_model_paths(model_root)?;
        let layout = infer_offline_layout(&paths.model_dir)?;
        Ok(SherpaOnnxOfflineRuntimePlan { paths, layout })
    }

    /// Returns the explicit runtime-unavailable error for builds without sherpa runtime support.
    #[must_use]
    pub fn runtime_unavailable_error(&self) -> AsrError {
        AsrError::Backend(format!(
            "sherpa-onnx runtime for provider `{}` is not enabled; build with feature `sherpa-onnx-backend`",
            self.provider_id
        ))
    }
}

fn infer_offline_layout(
    model_dir: &Path,
) -> Result<SherpaOnnxOfflineModelLayout, SherpaOnnxModelPathError> {
    let model = ["model.int8.onnx", "model.onnx"]
        .into_iter()
        .map(|file_name| model_dir.join(file_name))
        .find(|path| path.is_file())
        .ok_or_else(|| SherpaOnnxModelPathError::UnsupportedOfflineLayout {
            path: display_path(model_dir),
        })?;
    let tokens = model_dir.join("tokens.txt");
    if !tokens.is_file() {
        return Err(SherpaOnnxModelPathError::MissingTokensFile {
            path: display_path(model_dir),
        });
    }
    Ok(SherpaOnnxOfflineModelLayout::SenseVoice {
        model,
        tokens,
        language: "auto".to_owned(),
        use_itn: true,
    })
}

fn resolve_against(root: &Path, value: &str) -> PathBuf {
    let path = Path::new(value);
    if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    }
}

fn reject_url_like(provider_id: &str, value: &str) -> Result<(), SherpaOnnxModelPathError> {
    if value.contains("://") {
        Err(SherpaOnnxModelPathError::UrlLikePath {
            provider_id: provider_id.to_owned(),
            path: value.to_owned(),
        })
    } else {
        Ok(())
    }
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

/// Feature-gated offline `sherpa-onnx` backend using the official Rust API.
#[cfg(feature = "sherpa-onnx-backend")]
pub struct SherpaOnnxBackend {
    spec: SherpaOnnxSpec,
    descriptor: BackendDescriptor,
    recognizer: std::sync::Arc<sherpa_onnx::OfflineRecognizer>,
}

#[cfg(feature = "sherpa-onnx-backend")]
impl std::fmt::Debug for SherpaOnnxBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SherpaOnnxBackend")
            .field("provider_id", &self.spec.provider_id)
            .field("model", &self.spec.model)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "sherpa-onnx-backend")]
impl SherpaOnnxBackend {
    /// Builds an offline `sherpa-onnx` backend from config.
    pub fn with_config(provider: &AsrProviderConfig) -> Result<Self, AsrError> {
        let spec = SherpaOnnxSpec::from_provider(provider)?;
        let model_root = std::env::var_os("VINPUT_SHERPA_MODEL_ROOT")
            .map_or_else(|| PathBuf::from("."), PathBuf::from);
        let plan = spec
            .resolve_offline_runtime_plan(model_root)
            .map_err(|error| AsrError::Backend(error.to_string()))?;
        let config = offline_recognizer_config(&plan);
        let recognizer = sherpa_onnx::OfflineRecognizer::create(&config).ok_or_else(|| {
            AsrError::Backend(format!(
                "failed to create sherpa-onnx offline recognizer for provider `{}`",
                spec.provider_id
            ))
        })?;
        let model_id = spec.model.clone().unwrap_or_default();
        Ok(Self {
            descriptor: BackendDescriptor::new(
                spec.provider_id.clone(),
                model_id,
                "sherpa-onnx offline ASR",
                BackendCapabilities::buffered(),
            ),
            spec,
            recognizer: std::sync::Arc::new(recognizer),
        })
    }
}

#[cfg(feature = "sherpa-onnx-backend")]
fn offline_recognizer_config(
    plan: &SherpaOnnxOfflineRuntimePlan,
) -> sherpa_onnx::OfflineRecognizerConfig {
    let mut config = sherpa_onnx::OfflineRecognizerConfig::default();
    match &plan.layout {
        SherpaOnnxOfflineModelLayout::SenseVoice {
            model,
            tokens,
            language,
            use_itn,
        } => {
            config.model_config.sense_voice = sherpa_onnx::OfflineSenseVoiceModelConfig {
                model: Some(display_path(model)),
                language: Some(language.clone()),
                use_itn: *use_itn,
            };
            config.model_config.tokens = Some(display_path(tokens));
        }
    }
    if let Some(hotwords_file) = &plan.paths.hotwords_file {
        config.hotwords_file = Some(display_path(hotwords_file));
    }
    config
}

#[cfg(feature = "sherpa-onnx-backend")]
impl AsrBackend for SherpaOnnxBackend {
    fn describe(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn create_session(
        &self,
        _context: RecognitionContext,
    ) -> Result<Box<dyn RecognitionSession>, AsrError> {
        Ok(Box::new(SherpaOnnxRecognitionSession {
            recognizer: std::sync::Arc::clone(&self.recognizer),
            pcm: vinput_audio::PcmSpec::default(),
            samples: Vec::new(),
            events: Vec::new(),
            finished: false,
            cancelled: false,
        }))
    }
}

#[cfg(feature = "sherpa-onnx-backend")]
struct SherpaOnnxRecognitionSession {
    recognizer: std::sync::Arc<sherpa_onnx::OfflineRecognizer>,
    pcm: vinput_audio::PcmSpec,
    samples: Vec<i16>,
    events: Vec<RecognitionEvent>,
    finished: bool,
    cancelled: bool,
}

#[cfg(feature = "sherpa-onnx-backend")]
impl std::fmt::Debug for SherpaOnnxRecognitionSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SherpaOnnxRecognitionSession")
            .field("pcm", &self.pcm)
            .field("sample_count", &self.samples.len())
            .field("finished", &self.finished)
            .field("cancelled", &self.cancelled)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "sherpa-onnx-backend")]
impl RecognitionSession for SherpaOnnxRecognitionSession {
    fn push_pcm(&mut self, pcm: &vinput_audio::PcmBuffer) -> Result<(), AsrError> {
        if self.cancelled {
            return Err(AsrError::Cancelled);
        }
        if self.finished {
            return Err(AsrError::AlreadyFinished);
        }
        let next_pcm = pcm.spec();
        if !self.samples.is_empty() && self.pcm != next_pcm {
            return Err(AsrError::Backend(format!(
                "sherpa-onnx PCM spec changed from {} Hz/{} channel(s) to {} Hz/{} channel(s)",
                self.pcm.sample_rate_hz,
                self.pcm.channels,
                next_pcm.sample_rate_hz,
                next_pcm.channels
            )));
        }
        self.pcm = next_pcm;
        self.samples.extend_from_slice(pcm.samples());
        Ok(())
    }

    fn push_audio(&mut self, samples: &[i16]) -> Result<(), AsrError> {
        if self.cancelled {
            return Err(AsrError::Cancelled);
        }
        if self.finished {
            return Err(AsrError::AlreadyFinished);
        }
        self.samples.extend_from_slice(samples);
        Ok(())
    }

    fn finish(&mut self) -> Result<(), AsrError> {
        if self.cancelled {
            return Err(AsrError::Cancelled);
        }
        if self.finished {
            return Err(AsrError::AlreadyFinished);
        }
        self.finished = true;
        let stream = self.recognizer.create_stream();
        let samples = self
            .samples
            .iter()
            .map(|sample| f32::from(*sample) / f32::from(i16::MAX))
            .collect::<Vec<_>>();
        let sample_rate = i32::try_from(self.pcm.sample_rate_hz).map_err(|_| {
            AsrError::Backend(format!(
                "sherpa-onnx sample rate {} does not fit i32",
                self.pcm.sample_rate_hz
            ))
        })?;
        stream.accept_waveform(sample_rate, &samples);
        self.recognizer.decode(&stream);
        let result = stream.get_result().ok_or_else(|| {
            AsrError::Backend("sherpa-onnx recognizer returned no result".to_owned())
        })?;
        let text = result.text.trim().to_owned();
        if text.is_empty() {
            return Err(AsrError::Backend(
                "sherpa-onnx recognizer returned empty text".to_owned(),
            ));
        }
        self.events = vec![
            RecognitionEvent::FinalText { text },
            RecognitionEvent::Completed,
        ];
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), AsrError> {
        self.cancelled = true;
        self.events.clear();
        Ok(())
    }

    fn poll_events(&mut self) -> Result<Vec<RecognitionEvent>, AsrError> {
        Ok(std::mem::take(&mut self.events))
    }
}

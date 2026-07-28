//! Local `sherpa-onnx` ASR backend seam.
//!
//! This module owns typed config parsing, model layout validation, and the
//! optional official `sherpa-onnx` runtime adapter. The runtime remains behind a
//! Cargo feature so default CI and command-demo installs do not download or link
//! native ASR libraries.

use std::{
    fs,
    path::{Path, PathBuf},
};

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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
    /// Qwen3 ASR model directory with encoder, decoder, frontend, and tokenizer assets.
    Qwen3Asr {
        /// Convolution frontend ONNX model.
        conv_frontend: PathBuf,
        /// Encoder ONNX model.
        encoder: PathBuf,
        /// Decoder ONNX model.
        decoder: PathBuf,
        /// Tokenizer file or directory.
        tokenizer: PathBuf,
        /// Maximum total decoder sequence length.
        max_total_len: i32,
        /// Maximum number of generated tokens.
        max_new_tokens: i32,
        /// Sampling temperature.
        temperature: f32,
        /// Nucleus sampling probability.
        top_p: f32,
        /// Sampling seed.
        seed: i32,
        /// Optional Qwen3-specific hotwords file.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hotwords: Option<PathBuf>,
    },
}

/// Resolved local inputs and inferred runtime layout for offline recognition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SherpaOnnxOfflineRuntimePlan {
    /// Validated model and hotwords paths.
    pub paths: SherpaOnnxModelPaths,
    /// Inferred offline model layout.
    pub layout: SherpaOnnxOfflineModelLayout,
    /// Source used to infer the layout, such as `metadata` or `files`.
    pub layout_source: String,
    /// vinput-model.json path when metadata drove layout selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_path: Option<PathBuf>,
}

struct InferredOfflineLayout {
    layout: SherpaOnnxOfflineModelLayout,
    source: String,
    metadata_path: Option<PathBuf>,
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
    /// vinput-model.json exists but cannot be read or parsed.
    #[error("sherpa-onnx model metadata `{path}` is invalid: {message}")]
    InvalidModelMetadata {
        /// Metadata JSON path.
        path: String,
        /// Sanitized read or parse message.
        message: String,
    },
    /// Model directory exists but does not match a supported offline layout.
    #[error(
        "sherpa-onnx model directory `{path}` does not contain a supported offline model layout"
    )]
    UnsupportedOfflineLayout {
        /// Resolved model path.
        path: String,
    },
    /// Metadata declares a valid sherpa family that this runtime does not support yet.
    #[error("sherpa-onnx offline model family `{family}` is not supported for `{path}`")]
    UnsupportedOfflineFamily {
        /// Resolved model path.
        path: String,
        /// Declared model family.
        family: String,
    },
    /// Model directory exists but does not contain typed online metadata.
    #[error(
        "sherpa-onnx model directory `{path}` does not contain a supported online model layout"
    )]
    UnsupportedOnlineLayout {
        /// Resolved model path.
        path: String,
    },
    /// Metadata declares an online sherpa family that this runtime does not support yet.
    #[error("sherpa-onnx online model family `{family}` is not supported for `{path}`")]
    UnsupportedOnlineFamily {
        /// Resolved model path.
        path: String,
        /// Declared model family.
        family: String,
    },
    /// A family-specific model asset is absent from the extracted model directory.
    #[error("sherpa-onnx {family} model asset `{asset}` is missing at `{path}`")]
    MissingModelAsset {
        /// Declared model family.
        family: String,
        /// Family-specific asset field.
        asset: String,
        /// Resolved asset path.
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
        let inferred = infer_offline_layout(&paths.model_dir)?;
        Ok(SherpaOnnxOfflineRuntimePlan {
            paths,
            layout: inferred.layout,
            layout_source: inferred.source,
            metadata_path: inferred.metadata_path,
        })
    }

    /// Returns whether typed model metadata selects native online recognition.
    pub fn uses_online_runtime(
        &self,
        model_root: impl AsRef<Path>,
    ) -> Result<bool, SherpaOnnxModelPathError> {
        let paths = self.resolve_model_paths(model_root)?;
        crate::sherpa_online::metadata_requests_online(&paths.model_dir)
    }

    /// Resolves typed online metadata into a native recognizer plan.
    pub fn resolve_online_runtime_plan(
        &self,
        model_root: impl AsRef<Path>,
    ) -> Result<crate::SherpaOnnxOnlineRuntimePlan, SherpaOnnxModelPathError> {
        let paths = self.resolve_model_paths(model_root)?;
        crate::sherpa_online::resolve_online_runtime_plan(&paths)
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
) -> Result<InferredOfflineLayout, SherpaOnnxModelPathError> {
    if let Some(inferred) = infer_offline_layout_from_metadata(model_dir)? {
        return Ok(inferred);
    }
    Ok(InferredOfflineLayout {
        layout: infer_sense_voice_layout_from_files(model_dir, "auto", true)?,
        source: "files".to_owned(),
        metadata_path: None,
    })
}

fn infer_offline_layout_from_metadata(
    model_dir: &Path,
) -> Result<Option<InferredOfflineLayout>, SherpaOnnxModelPathError> {
    let metadata_path = model_dir.join("vinput-model.json");
    if !metadata_path.is_file() {
        return Ok(None);
    }
    let metadata_text = fs::read_to_string(&metadata_path).map_err(|error| {
        SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(&metadata_path),
            message: error.kind().to_string(),
        }
    })?;
    let metadata = serde_json::from_str::<serde_json::Value>(&metadata_text).map_err(|error| {
        SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(&metadata_path),
            message: error.to_string(),
        }
    })?;
    let family = metadata
        .pointer("/family")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            metadata
                .pointer("/model_type")
                .and_then(serde_json::Value::as_str)
        })
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if family == Some("qwen3_asr") {
        return infer_qwen3_asr_layout_from_metadata(model_dir, &metadata, metadata_path).map(Some);
    }
    if family != Some("sense_voice") {
        return Err(match family {
            Some(family) => SherpaOnnxModelPathError::UnsupportedOfflineFamily {
                path: display_path(model_dir),
                family: family.to_owned(),
            },
            None => SherpaOnnxModelPathError::UnsupportedOfflineLayout {
                path: display_path(model_dir),
            },
        });
    }
    let sense_voice = metadata.pointer("/model/sense_voice");
    let model = sense_voice
        .and_then(|value| value.get("model"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| resolve_against(model_dir, value))
        .or_else(|| find_sense_voice_model_file(model_dir))
        .ok_or_else(|| SherpaOnnxModelPathError::UnsupportedOfflineLayout {
            path: display_path(model_dir),
        })?;
    if !model.is_file() {
        return Err(SherpaOnnxModelPathError::UnsupportedOfflineLayout {
            path: display_path(model_dir),
        });
    }
    let tokens = metadata
        .pointer("/model/tokens")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map_or_else(
            || model_dir.join("tokens.txt"),
            |value| resolve_against(model_dir, value),
        );
    if !tokens.is_file() {
        return Err(SherpaOnnxModelPathError::MissingTokensFile {
            path: display_path(model_dir),
        });
    }
    let language = sense_voice
        .and_then(|value| value.get("language"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| metadata.get("language").and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("auto")
        .to_owned();
    let use_itn = sense_voice
        .and_then(|value| value.get("use_itn"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    Ok(Some(InferredOfflineLayout {
        layout: SherpaOnnxOfflineModelLayout::SenseVoice {
            model,
            tokens,
            language,
            use_itn,
        },
        source: "metadata".to_owned(),
        metadata_path: Some(metadata_path),
    }))
}

fn infer_qwen3_asr_layout_from_metadata(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: PathBuf,
) -> Result<InferredOfflineLayout, SherpaOnnxModelPathError> {
    let qwen3 = metadata
        .pointer("/model/qwen3_asr")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(&metadata_path),
            message: "missing object `/model/qwen3_asr`".to_owned(),
        })?;
    let conv_frontend = required_qwen3_asset(model_dir, qwen3, "conv_frontend", false)?;
    let encoder = required_qwen3_asset(model_dir, qwen3, "encoder", false)?;
    let decoder = required_qwen3_asset(model_dir, qwen3, "decoder", false)?;
    let tokenizer = required_qwen3_asset(model_dir, qwen3, "tokenizer", true)?;
    let hotwords = optional_qwen3_asset(model_dir, qwen3, "hotwords")?;
    Ok(InferredOfflineLayout {
        layout: SherpaOnnxOfflineModelLayout::Qwen3Asr {
            conv_frontend,
            encoder,
            decoder,
            tokenizer,
            max_total_len: qwen3_i32(qwen3, "max_total_len", 512, &metadata_path)?,
            max_new_tokens: qwen3_i32(qwen3, "max_new_tokens", 128, &metadata_path)?,
            temperature: qwen3_f32(qwen3, "temperature", 1e-6, &metadata_path)?,
            top_p: qwen3_f32(qwen3, "top_p", 0.8, &metadata_path)?,
            seed: qwen3_i32(qwen3, "seed", 42, &metadata_path)?,
            hotwords,
        },
        source: "metadata".to_owned(),
        metadata_path: Some(metadata_path),
    })
}

fn required_qwen3_asset(
    model_dir: &Path,
    config: &serde_json::Map<String, serde_json::Value>,
    field: &str,
    allow_directory: bool,
) -> Result<PathBuf, SherpaOnnxModelPathError> {
    let value = config
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(&model_dir.join("vinput-model.json")),
            message: format!("missing string `/model/qwen3_asr/{field}`"),
        })?;
    let path = resolve_against(model_dir, value);
    let valid = if allow_directory {
        path.exists()
    } else {
        path.is_file()
    };
    if !valid {
        return Err(SherpaOnnxModelPathError::MissingModelAsset {
            family: "qwen3_asr".to_owned(),
            asset: field.to_owned(),
            path: display_path(&path),
        });
    }
    Ok(path)
}

fn optional_qwen3_asset(
    model_dir: &Path,
    config: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<Option<PathBuf>, SherpaOnnxModelPathError> {
    let Some(value) = config
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let path = resolve_against(model_dir, value);
    if !path.is_file() {
        return Err(SherpaOnnxModelPathError::MissingModelAsset {
            family: "qwen3_asr".to_owned(),
            asset: field.to_owned(),
            path: display_path(&path),
        });
    }
    Ok(Some(path))
}

fn qwen3_i32(
    config: &serde_json::Map<String, serde_json::Value>,
    field: &str,
    default: i32,
    metadata_path: &Path,
) -> Result<i32, SherpaOnnxModelPathError> {
    let Some(value) = config.get(field) else {
        return Ok(default);
    };
    let value = value
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(metadata_path),
            message: format!("`/model/qwen3_asr/{field}` must be a 32-bit integer"),
        })?;
    Ok(value)
}

fn qwen3_f32(
    config: &serde_json::Map<String, serde_json::Value>,
    field: &str,
    default: f32,
    metadata_path: &Path,
) -> Result<f32, SherpaOnnxModelPathError> {
    let Some(value) = config.get(field) else {
        return Ok(default);
    };
    let value = value
        .as_f64()
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(metadata_path),
            message: format!("`/model/qwen3_asr/{field}` must be numeric"),
        })?;
    if !value.is_finite() || value < f64::from(f32::MIN) || value > f64::from(f32::MAX) {
        return Err(SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(metadata_path),
            message: format!("`/model/qwen3_asr/{field}` is outside the f32 range"),
        });
    }
    #[allow(clippy::cast_possible_truncation)]
    let value = value as f32;
    Ok(value)
}

fn infer_sense_voice_layout_from_files(
    model_dir: &Path,
    language: &str,
    use_itn: bool,
) -> Result<SherpaOnnxOfflineModelLayout, SherpaOnnxModelPathError> {
    let model = find_sense_voice_model_file(model_dir).ok_or_else(|| {
        SherpaOnnxModelPathError::UnsupportedOfflineLayout {
            path: display_path(model_dir),
        }
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
        language: language.to_owned(),
        use_itn,
    })
}

fn find_sense_voice_model_file(model_dir: &Path) -> Option<PathBuf> {
    ["model.int8.onnx", "model.onnx"]
        .into_iter()
        .map(|file_name| model_dir.join(file_name))
        .find(|path| path.is_file())
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
    runtime: SherpaOnnxRuntime,
}

#[cfg(feature = "sherpa-onnx-backend")]
enum SherpaOnnxRuntime {
    Offline(std::sync::Arc<sherpa_onnx::OfflineRecognizer>),
    Online(crate::sherpa_online::SherpaOnnxOnlineRuntime),
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
    /// Builds an offline or online `sherpa-onnx` backend from typed model metadata.
    pub fn with_config(provider: &AsrProviderConfig) -> Result<Self, AsrError> {
        let spec = SherpaOnnxSpec::from_provider(provider)?;
        let model_root = std::env::var_os("VINPUT_SHERPA_MODEL_ROOT")
            .map_or_else(|| PathBuf::from("."), PathBuf::from);
        let online = spec
            .uses_online_runtime(&model_root)
            .map_err(|error| AsrError::Backend(error.to_string()))?;
        let (runtime, description, capabilities) = if online {
            let plan = spec
                .resolve_online_runtime_plan(&model_root)
                .map_err(|error| AsrError::Backend(error.to_string()))?;
            let runtime =
                crate::sherpa_online::SherpaOnnxOnlineRuntime::create(&plan, &spec.provider_id)?;
            (
                SherpaOnnxRuntime::Online(runtime),
                "sherpa-onnx online ASR",
                BackendCapabilities::streaming(),
            )
        } else {
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
            (
                SherpaOnnxRuntime::Offline(std::sync::Arc::new(recognizer)),
                "sherpa-onnx offline ASR",
                BackendCapabilities::buffered(),
            )
        };
        let model_id = spec.model.clone().unwrap_or_default();
        Ok(Self {
            descriptor: BackendDescriptor::new(
                spec.provider_id.clone(),
                model_id,
                description,
                capabilities,
            ),
            spec,
            runtime,
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
        SherpaOnnxOfflineModelLayout::Qwen3Asr {
            conv_frontend,
            encoder,
            decoder,
            tokenizer,
            max_total_len,
            max_new_tokens,
            temperature,
            top_p,
            seed,
            hotwords,
        } => {
            config.model_config.qwen3_asr = sherpa_onnx::OfflineQwen3ASRModelConfig {
                conv_frontend: Some(display_path(conv_frontend)),
                encoder: Some(display_path(encoder)),
                decoder: Some(display_path(decoder)),
                tokenizer: Some(display_path(tokenizer)),
                max_total_len: *max_total_len,
                max_new_tokens: *max_new_tokens,
                temperature: *temperature,
                top_p: *top_p,
                seed: *seed,
                hotwords: hotwords.as_deref().map(display_path),
            };
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
        match &self.runtime {
            SherpaOnnxRuntime::Offline(recognizer) => Ok(Box::new(SherpaOnnxRecognitionSession {
                recognizer: std::sync::Arc::clone(recognizer),
                pcm: vinput_audio::PcmSpec::default(),
                samples: Vec::new(),
                events: Vec::new(),
                finished: false,
                cancelled: false,
            })),
            SherpaOnnxRuntime::Online(runtime) => Ok(runtime.create_session()),
        }
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

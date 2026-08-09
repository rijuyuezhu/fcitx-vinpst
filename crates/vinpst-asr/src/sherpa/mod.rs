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
use vinpst_config::{AsrProviderConfig, AsrProviderKind};

use crate::AsrError;

#[cfg(any(test, feature = "sherpa-onnx-backend"))]
pub(crate) fn sherpa_result_text(text: &str, tokens: &[String]) -> String {
    let text = text.trim();
    if !text.is_empty() {
        return text.to_owned();
    }
    tokens.concat().trim().to_owned()
}

use offline_layout::{display_path, infer_offline_layout, reject_url_like, resolve_against};

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
    /// Offline transducer model with encoder, decoder, joiner, and tokens assets.
    Transducer {
        /// Encoder model.
        encoder: PathBuf,
        /// Decoder model.
        decoder: PathBuf,
        /// Joiner model.
        joiner: PathBuf,
        /// Tokens file.
        tokens: PathBuf,
    },
    /// Dolphin model directory with a single ONNX model and tokens file.
    Dolphin {
        /// ONNX model file.
        model: PathBuf,
        /// Tokens file.
        tokens: PathBuf,
    },
    /// Paraformer model directory with a single ONNX model and tokens file.
    Paraformer {
        /// ONNX model file.
        model: PathBuf,
        /// Tokens file.
        tokens: PathBuf,
    },
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
    /// Moonshine v1 model directory with preprocessor, encoder, and decoder assets.
    MoonshineV1 {
        /// Audio preprocessor ONNX model.
        preprocessor: PathBuf,
        /// Encoder ONNX model.
        encoder: PathBuf,
        /// Decoder used for the first token.
        uncached_decoder: PathBuf,
        /// Decoder used after the first token.
        cached_decoder: PathBuf,
        /// Tokens file.
        tokens: PathBuf,
    },
}

/// Shared native configuration resolved from offline registry metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SherpaOnnxOfflineSettings {
    /// Number of native inference threads.
    pub num_threads: i32,
    /// Runtime provider such as `cpu`.
    pub provider: String,
    /// Native runtime debug logging flag.
    pub debug: bool,
    /// Optional native model type override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_type: Option<String>,
    /// Optional token modeling unit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modeling_unit: Option<String>,
    /// Optional BPE vocabulary file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bpe_vocab: Option<PathBuf>,
    /// Optional `TeleSpeech` CTC resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telespeech_ctc: Option<PathBuf>,
    /// Feature sample rate.
    pub sample_rate: i32,
    /// Feature dimension.
    pub feature_dim: i32,
    /// Optional language-model file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lm_model: Option<PathBuf>,
    /// Language-model scale.
    pub lm_scale: f32,
    /// Whether model metadata declares hotword support.
    pub supports_hotwords: bool,
    /// Decoding method.
    pub decoding_method: String,
    /// Maximum active decoding paths.
    pub max_active_paths: i32,
    /// Optional metadata-provided hotwords file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotwords_file: Option<PathBuf>,
    /// Hotwords score.
    pub hotwords_score: f32,
    /// Optional rule FSTs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_fsts: Option<PathBuf>,
    /// Optional rule FARs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_fars: Option<PathBuf>,
    /// CTC blank penalty.
    pub blank_penalty: f32,
    /// Optional homophone lexicon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homophone_lexicon: Option<PathBuf>,
    /// Optional homophone rule FSTs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homophone_rule_fsts: Option<PathBuf>,
}

/// Resolved local inputs and inferred runtime layout for offline recognition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SherpaOnnxOfflineRuntimePlan {
    /// Validated model and hotwords paths.
    pub paths: SherpaOnnxModelPaths,
    /// Inferred offline model layout.
    pub layout: SherpaOnnxOfflineModelLayout,
    /// Shared native model and recognizer settings.
    pub settings: SherpaOnnxOfflineSettings,
    /// Source used to infer the layout, such as `metadata` or `files`.
    pub layout_source: String,
    /// vinpst-model.json path when metadata drove layout selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_path: Option<PathBuf>,
}

struct InferredOfflineLayout {
    layout: SherpaOnnxOfflineModelLayout,
    settings: SherpaOnnxOfflineSettings,
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
    /// vinpst-model.json exists but cannot be read or parsed.
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
    /// Parses a local config provider into the `sherpa-onnx` runtime spec.
    ///
    /// `SHERPA_ONNX_PROVIDER_ID` is the bundled/default provider id, not a
    /// runtime type discriminator. Custom local provider ids use the same
    /// model-metadata-driven runtime selection as upstream.
    pub fn from_provider(provider: &AsrProviderConfig) -> Result<Self, AsrError> {
        if provider.kind != AsrProviderKind::Local {
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
            settings: inferred.settings,
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

#[cfg(feature = "sherpa-onnx-backend")]
mod backend;
mod offline_layout;

#[cfg(feature = "sherpa-onnx-backend")]
pub use backend::SherpaOnnxBackend;

#[cfg(test)]
mod result_text_tests {
    use super::sherpa_result_text;

    #[test]
    fn result_text_prefers_text_and_falls_back_to_tokens() {
        assert_eq!(
            sherpa_result_text("  direct  ", &["ignored".to_owned()]),
            "direct"
        );
        assert_eq!(
            sherpa_result_text("   ", &[" token".to_owned(), " text ".to_owned()]),
            "token text"
        );
        assert_eq!(sherpa_result_text("", &[]), "");
    }
}

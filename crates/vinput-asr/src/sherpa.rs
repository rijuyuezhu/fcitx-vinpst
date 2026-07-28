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
#[cfg(feature = "sherpa-onnx-backend")]
use vinput_config::VadConfig;
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
    /// vinput-model.json path when metadata drove layout selection.
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

fn infer_offline_layout(
    model_dir: &Path,
) -> Result<InferredOfflineLayout, SherpaOnnxModelPathError> {
    if let Some(inferred) = infer_offline_layout_from_metadata(model_dir)? {
        return Ok(inferred);
    }
    Ok(InferredOfflineLayout {
        layout: infer_sense_voice_layout_from_files(model_dir, "auto", true)?,
        settings: default_offline_settings("sense_voice"),
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
        .filter(|value| !value.is_empty())
        .ok_or_else(|| SherpaOnnxModelPathError::UnsupportedOfflineLayout {
            path: display_path(model_dir),
        })?;
    match family {
        "dolphin" => infer_single_model_layout_from_metadata(
            model_dir,
            &metadata,
            metadata_path.clone(),
            parse_offline_settings(model_dir, &metadata, &metadata_path, family)?,
            "dolphin",
            |model, tokens| SherpaOnnxOfflineModelLayout::Dolphin { model, tokens },
        )
        .map(Some),
        "paraformer" => infer_paraformer_layout_from_metadata(
            model_dir,
            &metadata,
            metadata_path.clone(),
            parse_offline_settings(model_dir, &metadata, &metadata_path, family)?,
        )
        .map(Some),
        "qwen3_asr" => infer_qwen3_asr_layout_from_metadata(
            model_dir,
            &metadata,
            metadata_path.clone(),
            parse_offline_settings(model_dir, &metadata, &metadata_path, family)?,
        )
        .map(Some),
        "moonshine" | "moonshine_v1" => infer_moonshine_layout_from_metadata(
            model_dir,
            &metadata,
            metadata_path.clone(),
            parse_offline_settings(model_dir, &metadata, &metadata_path, family)?,
        )
        .map(Some),
        "sense_voice" => infer_sense_voice_layout_from_metadata(
            model_dir,
            &metadata,
            metadata_path.clone(),
            parse_offline_settings(model_dir, &metadata, &metadata_path, family)?,
        )
        .map(Some),
        _ => Err(SherpaOnnxModelPathError::UnsupportedOfflineFamily {
            path: display_path(model_dir),
            family: family.to_owned(),
        }),
    }
}

fn infer_sense_voice_layout_from_metadata(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: PathBuf,
    settings: SherpaOnnxOfflineSettings,
) -> Result<InferredOfflineLayout, SherpaOnnxModelPathError> {
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
    Ok(InferredOfflineLayout {
        layout: SherpaOnnxOfflineModelLayout::SenseVoice {
            model,
            tokens,
            language,
            use_itn,
        },
        settings,
        source: "metadata".to_owned(),
        metadata_path: Some(metadata_path),
    })
}

fn infer_single_model_layout_from_metadata(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: PathBuf,
    settings: SherpaOnnxOfflineSettings,
    family: &str,
    layout: impl FnOnce(PathBuf, PathBuf) -> SherpaOnnxOfflineModelLayout,
) -> Result<InferredOfflineLayout, SherpaOnnxModelPathError> {
    let family_config = metadata
        .pointer(&format!("/model/{family}"))
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(&metadata_path),
            message: format!("missing object `/model/{family}`"),
        })?;
    Ok(InferredOfflineLayout {
        layout: layout(
            required_model_asset(model_dir, family_config, family, "model")?,
            metadata_asset_path(model_dir, metadata, "/model/tokens", family, "tokens")?,
        ),
        settings,
        source: "metadata".to_owned(),
        metadata_path: Some(metadata_path),
    })
}

fn infer_paraformer_layout_from_metadata(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: PathBuf,
    settings: SherpaOnnxOfflineSettings,
) -> Result<InferredOfflineLayout, SherpaOnnxModelPathError> {
    infer_single_model_layout_from_metadata(
        model_dir,
        metadata,
        metadata_path,
        settings,
        "paraformer",
        |model, tokens| SherpaOnnxOfflineModelLayout::Paraformer { model, tokens },
    )
}

fn infer_qwen3_asr_layout_from_metadata(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: PathBuf,
    settings: SherpaOnnxOfflineSettings,
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
        settings,
        source: "metadata".to_owned(),
        metadata_path: Some(metadata_path),
    })
}

fn infer_moonshine_layout_from_metadata(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: PathBuf,
    settings: SherpaOnnxOfflineSettings,
) -> Result<InferredOfflineLayout, SherpaOnnxModelPathError> {
    let model_type = metadata
        .pointer("/model_type")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("moonshine_v1");
    if model_type != "moonshine_v1" && model_type != "moonshine" {
        return Err(SherpaOnnxModelPathError::UnsupportedOfflineFamily {
            path: display_path(model_dir),
            family: model_type.to_owned(),
        });
    }
    let moonshine = metadata
        .pointer("/model/moonshine")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(&metadata_path),
            message: "missing object `/model/moonshine`".to_owned(),
        })?;
    let tokens = metadata_asset_path(model_dir, metadata, "/model/tokens", "moonshine", "tokens")?;
    Ok(InferredOfflineLayout {
        layout: SherpaOnnxOfflineModelLayout::MoonshineV1 {
            preprocessor: required_model_asset(model_dir, moonshine, "moonshine", "preprocessor")?,
            encoder: required_model_asset(model_dir, moonshine, "moonshine", "encoder")?,
            uncached_decoder: required_model_asset(
                model_dir,
                moonshine,
                "moonshine",
                "uncached_decoder",
            )?,
            cached_decoder: required_model_asset(
                model_dir,
                moonshine,
                "moonshine",
                "cached_decoder",
            )?,
            tokens,
        },
        settings,
        source: "metadata".to_owned(),
        metadata_path: Some(metadata_path),
    })
}

fn required_model_asset(
    model_dir: &Path,
    config: &serde_json::Map<String, serde_json::Value>,
    family: &str,
    field: &str,
) -> Result<PathBuf, SherpaOnnxModelPathError> {
    let value = config
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(&model_dir.join("vinput-model.json")),
            message: format!("missing string `/model/{family}/{field}`"),
        })?;
    let path = resolve_against(model_dir, value);
    if !path.is_file() {
        return Err(SherpaOnnxModelPathError::MissingModelAsset {
            family: family.to_owned(),
            asset: field.to_owned(),
            path: display_path(&path),
        });
    }
    Ok(path)
}

fn metadata_asset_path(
    model_dir: &Path,
    metadata: &serde_json::Value,
    pointer: &str,
    family: &str,
    field: &str,
) -> Result<PathBuf, SherpaOnnxModelPathError> {
    let value = metadata
        .pointer(pointer)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(&model_dir.join("vinput-model.json")),
            message: format!("missing string `{pointer}`"),
        })?;
    let path = resolve_against(model_dir, value);
    if !path.is_file() {
        return Err(SherpaOnnxModelPathError::MissingModelAsset {
            family: family.to_owned(),
            asset: field.to_owned(),
            path: display_path(&path),
        });
    }
    Ok(path)
}

fn default_offline_settings(family: &str) -> SherpaOnnxOfflineSettings {
    SherpaOnnxOfflineSettings {
        num_threads: 1,
        provider: "cpu".to_owned(),
        debug: false,
        model_type: Some(family.to_owned()),
        modeling_unit: Some("cjkchar".to_owned()),
        bpe_vocab: None,
        telespeech_ctc: None,
        sample_rate: 16_000,
        feature_dim: 80,
        lm_model: None,
        lm_scale: 0.5,
        decoding_method: "greedy_search".to_owned(),
        max_active_paths: 4,
        hotwords_file: None,
        hotwords_score: 1.5,
        rule_fsts: None,
        rule_fars: None,
        blank_penalty: 0.0,
        homophone_lexicon: None,
        homophone_rule_fsts: None,
    }
}

fn parse_offline_settings(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: &Path,
    family: &str,
) -> Result<SherpaOnnxOfflineSettings, SherpaOnnxModelPathError> {
    let mut settings = default_offline_settings(family);
    parse_offline_model_settings(&mut settings, model_dir, metadata, metadata_path, family)?;
    parse_offline_recognizer_settings(&mut settings, model_dir, metadata, metadata_path, family)?;
    Ok(settings)
}

fn parse_offline_model_settings(
    settings: &mut SherpaOnnxOfflineSettings,
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: &Path,
    family: &str,
) -> Result<(), SherpaOnnxModelPathError> {
    settings.num_threads = metadata_positive_i32(
        metadata,
        "/model/num_threads",
        settings.num_threads,
        metadata_path,
    )?;
    if let Some(provider) = metadata_optional_string(metadata, "/model/provider") {
        settings.provider = provider;
    }
    settings.debug = metadata_boolish(metadata, "/model/debug", settings.debug, metadata_path)?;
    if let Some(model_type) = metadata_optional_string(metadata, "/model/model_type") {
        settings.model_type = Some(model_type);
    }
    if let Some(modeling_unit) = metadata_optional_string(metadata, "/model/modeling_unit") {
        settings.modeling_unit = Some(modeling_unit);
    }
    settings.bpe_vocab =
        metadata_optional_file(model_dir, metadata, "/model/bpe_vocab", family, "bpe_vocab")?;
    settings.telespeech_ctc = metadata_optional_file(
        model_dir,
        metadata,
        "/model/telespeech_ctc",
        family,
        "telespeech_ctc",
    )?;
    Ok(())
}

fn parse_offline_recognizer_settings(
    settings: &mut SherpaOnnxOfflineSettings,
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: &Path,
    family: &str,
) -> Result<(), SherpaOnnxModelPathError> {
    settings.sample_rate = metadata_positive_i32(
        metadata,
        "/recognizer/feat_config/sample_rate",
        settings.sample_rate,
        metadata_path,
    )?;
    settings.feature_dim = metadata_positive_i32(
        metadata,
        "/recognizer/feat_config/feature_dim",
        settings.feature_dim,
        metadata_path,
    )?;
    settings.lm_model = metadata_optional_file(
        model_dir,
        metadata,
        "/recognizer/lm_config/model",
        family,
        "lm_model",
    )?;
    settings.lm_scale = metadata_finite_f32(
        metadata,
        "/recognizer/lm_config/scale",
        settings.lm_scale,
        metadata_path,
    )?;
    if let Some(decoding_method) = metadata_optional_string(metadata, "/recognizer/decoding_method")
    {
        settings.decoding_method = decoding_method;
    }
    settings.max_active_paths = metadata_positive_i32(
        metadata,
        "/recognizer/max_active_paths",
        settings.max_active_paths,
        metadata_path,
    )?;
    settings.hotwords_file = metadata_optional_file(
        model_dir,
        metadata,
        "/recognizer/hotwords_file",
        family,
        "hotwords_file",
    )?;
    settings.hotwords_score = metadata_finite_f32(
        metadata,
        "/recognizer/hotwords_score",
        settings.hotwords_score,
        metadata_path,
    )?;
    settings.rule_fsts = metadata_optional_file(
        model_dir,
        metadata,
        "/recognizer/rule_fsts",
        family,
        "rule_fsts",
    )?;
    settings.rule_fars = metadata_optional_file(
        model_dir,
        metadata,
        "/recognizer/rule_fars",
        family,
        "rule_fars",
    )?;
    settings.blank_penalty = metadata_finite_f32(
        metadata,
        "/recognizer/blank_penalty",
        settings.blank_penalty,
        metadata_path,
    )?;
    settings.homophone_lexicon = metadata_optional_file(
        model_dir,
        metadata,
        "/recognizer/hr/lexicon",
        family,
        "homophone_lexicon",
    )?;
    settings.homophone_rule_fsts = metadata_optional_file(
        model_dir,
        metadata,
        "/recognizer/hr/rule_fsts",
        family,
        "homophone_rule_fsts",
    )?;
    Ok(())
}

fn metadata_optional_string(metadata: &serde_json::Value, pointer: &str) -> Option<String> {
    metadata
        .pointer(pointer)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn metadata_optional_file(
    model_dir: &Path,
    metadata: &serde_json::Value,
    pointer: &str,
    family: &str,
    asset: &str,
) -> Result<Option<PathBuf>, SherpaOnnxModelPathError> {
    let Some(value) = metadata_optional_string(metadata, pointer) else {
        return Ok(None);
    };
    let path = resolve_against(model_dir, &value);
    if !path.is_file() {
        return Err(SherpaOnnxModelPathError::MissingModelAsset {
            family: family.to_owned(),
            asset: asset.to_owned(),
            path: display_path(&path),
        });
    }
    Ok(Some(path))
}

fn metadata_positive_i32(
    metadata: &serde_json::Value,
    pointer: &str,
    default: i32,
    metadata_path: &Path,
) -> Result<i32, SherpaOnnxModelPathError> {
    let Some(value) = metadata.pointer(pointer) else {
        return Ok(default);
    };
    let value = value
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(metadata_path),
            message: format!("`{pointer}` must be a positive 32-bit integer"),
        })?;
    Ok(value)
}

fn metadata_boolish(
    metadata: &serde_json::Value,
    pointer: &str,
    default: bool,
    metadata_path: &Path,
) -> Result<bool, SherpaOnnxModelPathError> {
    let Some(value) = metadata.pointer(pointer) else {
        return Ok(default);
    };
    match (value.as_bool(), value.as_i64()) {
        (Some(value), _) => Ok(value),
        (None, Some(0)) => Ok(false),
        (None, Some(1)) => Ok(true),
        _ => Err(SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(metadata_path),
            message: format!("`{pointer}` must be a boolean or 0/1"),
        }),
    }
}

fn metadata_finite_f32(
    metadata: &serde_json::Value,
    pointer: &str,
    default: f32,
    metadata_path: &Path,
) -> Result<f32, SherpaOnnxModelPathError> {
    let Some(value) = metadata.pointer(pointer) else {
        return Ok(default);
    };
    let value = value
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(metadata_path),
            message: format!("`{pointer}` must be finite numeric value"),
        })?;
    if value < f64::from(f32::MIN) || value > f64::from(f32::MAX) {
        return Err(SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(metadata_path),
            message: format!("`{pointer}` is outside the f32 range"),
        });
    }
    #[allow(clippy::cast_possible_truncation)]
    Ok(value as f32)
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
    Offline(SherpaOnnxOfflineRuntime),
    Online(crate::sherpa_online::SherpaOnnxOnlineRuntime),
}

#[cfg(feature = "sherpa-onnx-backend")]
struct SherpaOnnxOfflineRuntime {
    recognizer: std::sync::Arc<sherpa_onnx::OfflineRecognizer>,
    vad: Option<std::sync::Arc<std::sync::Mutex<crate::sherpa_vad::SherpaOnnxVadTrimmer>>>,
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
        Self::with_config_and_vad(provider, None)
    }

    /// Builds a backend and enables legacy offline VAD when a model can be resolved.
    pub fn with_config_and_vad(
        provider: &AsrProviderConfig,
        vad_config: Option<&VadConfig>,
    ) -> Result<Self, AsrError> {
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
            let vad = vad_config.and_then(|config| {
                if !config.enabled {
                    return None;
                }
                let Some(plan) = crate::sherpa_vad::SherpaOnnxVadPlan::resolve(config) else {
                    eprintln!(
                        "vinput: VAD enabled but silero_vad.onnx was not found; using untrimmed audio"
                    );
                    return None;
                };
                let Some(trimmer) = crate::sherpa_vad::SherpaOnnxVadTrimmer::create(&plan) else {
                    eprintln!(
                        "vinput: failed to load Silero VAD model {}; using untrimmed audio",
                        plan.model.display()
                    );
                    return None;
                };
                Some(std::sync::Arc::new(std::sync::Mutex::new(trimmer)))
            });
            (
                SherpaOnnxRuntime::Offline(SherpaOnnxOfflineRuntime {
                    recognizer: std::sync::Arc::new(recognizer),
                    vad,
                }),
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
fn apply_offline_settings(
    config: &mut sherpa_onnx::OfflineRecognizerConfig,
    plan: &SherpaOnnxOfflineRuntimePlan,
) {
    let settings = &plan.settings;
    config.model_config.num_threads = settings.num_threads;
    config.model_config.provider = Some(settings.provider.clone());
    config.model_config.debug = settings.debug;
    config
        .model_config
        .model_type
        .clone_from(&settings.model_type);
    config
        .model_config
        .modeling_unit
        .clone_from(&settings.modeling_unit);
    config.model_config.bpe_vocab = settings.bpe_vocab.as_deref().map(display_path);
    config.model_config.telespeech_ctc = settings.telespeech_ctc.as_deref().map(display_path);
    config.feat_config.sample_rate = settings.sample_rate;
    config.feat_config.feature_dim = settings.feature_dim;
    config.lm_config.model = settings.lm_model.as_deref().map(display_path);
    config.lm_config.scale = settings.lm_scale;
    config.decoding_method = Some(settings.decoding_method.clone());
    config.max_active_paths = settings.max_active_paths;
    config.hotwords_file = plan
        .paths
        .hotwords_file
        .as_deref()
        .or(settings.hotwords_file.as_deref())
        .map(display_path);
    config.hotwords_score = settings.hotwords_score;
    config.rule_fsts = settings.rule_fsts.as_deref().map(display_path);
    config.rule_fars = settings.rule_fars.as_deref().map(display_path);
    config.blank_penalty = settings.blank_penalty;
    config.hr.lexicon = settings.homophone_lexicon.as_deref().map(display_path);
    config.hr.rule_fsts = settings.homophone_rule_fsts.as_deref().map(display_path);
}

#[cfg(feature = "sherpa-onnx-backend")]
fn offline_recognizer_config(
    plan: &SherpaOnnxOfflineRuntimePlan,
) -> sherpa_onnx::OfflineRecognizerConfig {
    let mut config = sherpa_onnx::OfflineRecognizerConfig::default();
    apply_offline_settings(&mut config, plan);
    match &plan.layout {
        SherpaOnnxOfflineModelLayout::Dolphin { model, tokens } => {
            config.model_config.dolphin = sherpa_onnx::OfflineDolphinModelConfig {
                model: Some(display_path(model)),
            };
            config.model_config.tokens = Some(display_path(tokens));
        }
        SherpaOnnxOfflineModelLayout::Paraformer { model, tokens } => {
            config.model_config.paraformer = sherpa_onnx::OfflineParaformerModelConfig {
                model: Some(display_path(model)),
            };
            config.model_config.tokens = Some(display_path(tokens));
        }
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
        SherpaOnnxOfflineModelLayout::MoonshineV1 {
            preprocessor,
            encoder,
            uncached_decoder,
            cached_decoder,
            tokens,
        } => {
            config.model_config.moonshine = sherpa_onnx::OfflineMoonshineModelConfig {
                preprocessor: Some(display_path(preprocessor)),
                encoder: Some(display_path(encoder)),
                uncached_decoder: Some(display_path(uncached_decoder)),
                cached_decoder: Some(display_path(cached_decoder)),
                merged_decoder: None,
            };
            config.model_config.tokens = Some(display_path(tokens));
        }
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
            SherpaOnnxRuntime::Offline(runtime) => Ok(Box::new(SherpaOnnxRecognitionSession {
                recognizer: std::sync::Arc::clone(&runtime.recognizer),
                vad: runtime.vad.as_ref().map(std::sync::Arc::clone),
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
    vad: Option<std::sync::Arc<std::sync::Mutex<crate::sherpa_vad::SherpaOnnxVadTrimmer>>>,
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
        let mut samples = self
            .samples
            .iter()
            .map(|sample| f32::from(*sample) / f32::from(i16::MAX))
            .collect::<Vec<_>>();
        if self.pcm.channels == 1
            && let Some(vad) = &self.vad
        {
            samples = vad
                .lock()
                .map_err(|_| AsrError::Backend("sherpa-onnx VAD lock poisoned".to_owned()))?
                .trim(&samples, self.pcm.sample_rate_hz);
        }
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

//! Native online `sherpa-onnx` model planning and recognition sessions.

use std::{
    fs,
    path::{Path, PathBuf},
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{SherpaOnnxModelPathError, SherpaOnnxModelPaths};

/// Supported native online model layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SherpaOnnxOnlineModelLayout {
    /// Streaming transducer model with encoder, decoder, and joiner assets.
    Transducer {
        /// Encoder model.
        encoder: PathBuf,
        /// Decoder model.
        decoder: PathBuf,
        /// Joiner model.
        joiner: PathBuf,
    },
    /// Streaming Zipformer2 CTC model.
    Zipformer2Ctc {
        /// CTC model.
        model: PathBuf,
    },
}

/// Validated native online recognizer configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SherpaOnnxOnlineRuntimePlan {
    /// Validated model and optional provider hotwords paths.
    pub paths: SherpaOnnxModelPaths,
    /// Family-specific online model layout.
    pub layout: SherpaOnnxOnlineModelLayout,
    /// Tokens file.
    pub tokens: PathBuf,
    /// Number of inference threads.
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
    /// Feature sample rate.
    pub sample_rate: i32,
    /// Feature dimension.
    pub feature_dim: i32,
    /// Decoding method.
    pub decoding_method: String,
    /// Maximum active modified-beam-search paths.
    pub max_active_paths: i32,
    /// Whether native endpointing is enabled.
    pub enable_endpoint: bool,
    /// Endpoint rule 1 trailing silence threshold.
    pub rule1_min_trailing_silence: f32,
    /// Endpoint rule 2 trailing silence threshold.
    pub rule2_min_trailing_silence: f32,
    /// Endpoint rule 3 minimum utterance length.
    pub rule3_min_utterance_length: f32,
    /// Optional recognizer hotwords file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotwords_file: Option<PathBuf>,
    /// Hotwords score.
    pub hotwords_score: f32,
    /// Optional CTC FST graph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ctc_graph: Option<PathBuf>,
    /// Maximum active CTC FST states.
    pub ctc_max_active: i32,
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
    /// Metadata file that selected the runtime.
    pub metadata_path: PathBuf,
}

struct OnlineModelMetadata {
    layout: SherpaOnnxOnlineModelLayout,
    tokens: PathBuf,
    num_threads: i32,
    provider: String,
    debug: bool,
    model_type: Option<String>,
    modeling_unit: Option<String>,
    bpe_vocab: Option<PathBuf>,
}

struct OnlineRecognizerMetadata {
    sample_rate: i32,
    feature_dim: i32,
    decoding_method: String,
    max_active_paths: i32,
    enable_endpoint: bool,
    rule1_min_trailing_silence: f32,
    rule2_min_trailing_silence: f32,
    rule3_min_utterance_length: f32,
    hotwords_file: Option<PathBuf>,
    hotwords_score: f32,
    ctc_graph: Option<PathBuf>,
    ctc_max_active: i32,
    rule_fsts: Option<PathBuf>,
    rule_fars: Option<PathBuf>,
    blank_penalty: f32,
    homophone_lexicon: Option<PathBuf>,
    homophone_rule_fsts: Option<PathBuf>,
}

/// Returns whether typed metadata selects the native online runtime.
pub(crate) fn metadata_requests_online(model_dir: &Path) -> Result<bool, SherpaOnnxModelPathError> {
    let metadata_path = model_dir.join("vinput-model.json");
    if !metadata_path.is_file() {
        return Ok(false);
    }
    let metadata = read_metadata(&metadata_path)?;
    let runtime = optional_string(&metadata, "/runtime");
    let backend = optional_string(&metadata, "/backend");
    Ok(runtime.as_deref() == Some("online") || backend.as_deref() == Some("sherpa-streaming"))
}

/// Resolves typed online metadata into a native recognizer plan.
pub(crate) fn resolve_online_runtime_plan(
    paths: &SherpaOnnxModelPaths,
) -> Result<SherpaOnnxOnlineRuntimePlan, SherpaOnnxModelPathError> {
    let metadata_path = paths.model_dir.join("vinput-model.json");
    if !metadata_path.is_file() {
        return Err(SherpaOnnxModelPathError::UnsupportedOnlineLayout {
            path: display_path(&paths.model_dir),
        });
    }
    let metadata = read_metadata(&metadata_path)?;
    let runtime = optional_string(&metadata, "/runtime");
    let backend = optional_string(&metadata, "/backend");
    if runtime.as_deref() != Some("online") && backend.as_deref() != Some("sherpa-streaming") {
        return Err(SherpaOnnxModelPathError::UnsupportedOnlineLayout {
            path: display_path(&paths.model_dir),
        });
    }

    let model = parse_model_metadata(&paths.model_dir, &metadata, &metadata_path)?;
    let recognizer = parse_recognizer_metadata(&paths.model_dir, &metadata, &metadata_path)?;

    Ok(SherpaOnnxOnlineRuntimePlan {
        paths: paths.clone(),
        layout: model.layout,
        tokens: model.tokens,
        num_threads: model.num_threads,
        provider: model.provider,
        debug: model.debug,
        model_type: model.model_type,
        modeling_unit: model.modeling_unit,
        bpe_vocab: model.bpe_vocab,
        sample_rate: recognizer.sample_rate,
        feature_dim: recognizer.feature_dim,
        decoding_method: recognizer.decoding_method,
        max_active_paths: recognizer.max_active_paths,
        enable_endpoint: recognizer.enable_endpoint,
        rule1_min_trailing_silence: recognizer.rule1_min_trailing_silence,
        rule2_min_trailing_silence: recognizer.rule2_min_trailing_silence,
        rule3_min_utterance_length: recognizer.rule3_min_utterance_length,
        hotwords_file: paths.hotwords_file.clone().or(recognizer.hotwords_file),
        hotwords_score: recognizer.hotwords_score,
        ctc_graph: recognizer.ctc_graph,
        ctc_max_active: recognizer.ctc_max_active,
        rule_fsts: recognizer.rule_fsts,
        rule_fars: recognizer.rule_fars,
        blank_penalty: recognizer.blank_penalty,
        homophone_lexicon: recognizer.homophone_lexicon,
        homophone_rule_fsts: recognizer.homophone_rule_fsts,
        metadata_path,
    })
}

fn parse_model_metadata(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: &Path,
) -> Result<OnlineModelMetadata, SherpaOnnxModelPathError> {
    let family = optional_string(metadata, "/family")
        .or_else(|| optional_string(metadata, "/model_type"))
        .ok_or_else(|| SherpaOnnxModelPathError::UnsupportedOnlineLayout {
            path: display_path(model_dir),
        })?;
    let layout = match family.as_str() {
        "transducer" => SherpaOnnxOnlineModelLayout::Transducer {
            encoder: required_file(model_dir, metadata, "/model/transducer/encoder", &family)?,
            decoder: required_file(model_dir, metadata, "/model/transducer/decoder", &family)?,
            joiner: required_file(model_dir, metadata, "/model/transducer/joiner", &family)?,
        },
        "zipformer2_ctc" => SherpaOnnxOnlineModelLayout::Zipformer2Ctc {
            model: required_file(model_dir, metadata, "/model/zipformer2_ctc/model", &family)?,
        },
        _ => {
            return Err(SherpaOnnxModelPathError::UnsupportedOnlineFamily {
                path: display_path(model_dir),
                family,
            });
        }
    };
    Ok(OnlineModelMetadata {
        layout,
        tokens: required_file(model_dir, metadata, "/model/tokens", "online")?,
        num_threads: positive_i32(metadata, "/model/num_threads", 1, metadata_path)?,
        provider: optional_string(metadata, "/model/provider").unwrap_or_else(|| "cpu".to_owned()),
        debug: boolish(metadata, "/model/debug", false, metadata_path)?,
        model_type: optional_string(metadata, "/model/model_type"),
        modeling_unit: optional_string(metadata, "/model/modeling_unit"),
        bpe_vocab: optional_file(model_dir, metadata, "/model/bpe_vocab", "online")?,
    })
}

fn parse_recognizer_metadata(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: &Path,
) -> Result<OnlineRecognizerMetadata, SherpaOnnxModelPathError> {
    Ok(OnlineRecognizerMetadata {
        sample_rate: positive_i32(
            metadata,
            "/recognizer/feat_config/sample_rate",
            16_000,
            metadata_path,
        )?,
        feature_dim: positive_i32(
            metadata,
            "/recognizer/feat_config/feature_dim",
            80,
            metadata_path,
        )?,
        decoding_method: optional_string(metadata, "/recognizer/decoding_method")
            .unwrap_or_else(|| "greedy_search".to_owned()),
        max_active_paths: positive_i32(metadata, "/recognizer/max_active_paths", 4, metadata_path)?,
        enable_endpoint: boolish(
            metadata,
            "/recognizer/enable_endpoint",
            false,
            metadata_path,
        )?,
        rule1_min_trailing_silence: finite_f32(
            metadata,
            "/recognizer/rule1_min_trailing_silence",
            2.4,
            metadata_path,
        )?,
        rule2_min_trailing_silence: finite_f32(
            metadata,
            "/recognizer/rule2_min_trailing_silence",
            1.2,
            metadata_path,
        )?,
        rule3_min_utterance_length: finite_f32(
            metadata,
            "/recognizer/rule3_min_utterance_length",
            20.0,
            metadata_path,
        )?,
        hotwords_file: optional_file(model_dir, metadata, "/recognizer/hotwords_file", "online")?,
        hotwords_score: finite_f32(metadata, "/recognizer/hotwords_score", 1.5, metadata_path)?,
        ctc_graph: optional_file(
            model_dir,
            metadata,
            "/recognizer/ctc_fst_decoder_config/graph",
            "online",
        )?,
        ctc_max_active: positive_i32(
            metadata,
            "/recognizer/ctc_fst_decoder_config/max_active",
            3_000,
            metadata_path,
        )?,
        rule_fsts: optional_file(model_dir, metadata, "/recognizer/rule_fsts", "online")?,
        rule_fars: optional_file(model_dir, metadata, "/recognizer/rule_fars", "online")?,
        blank_penalty: finite_f32(metadata, "/recognizer/blank_penalty", 0.0, metadata_path)?,
        homophone_lexicon: optional_file(model_dir, metadata, "/recognizer/hr/lexicon", "online")?,
        homophone_rule_fsts: optional_file(
            model_dir,
            metadata,
            "/recognizer/hr/rule_fsts",
            "online",
        )?,
    })
}

fn read_metadata(path: &Path) -> Result<serde_json::Value, SherpaOnnxModelPathError> {
    let text = fs::read_to_string(path).map_err(|error| {
        SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(path),
            message: error.kind().to_string(),
        }
    })?;
    serde_json::from_str(&text).map_err(|error| SherpaOnnxModelPathError::InvalidModelMetadata {
        path: display_path(path),
        message: error.to_string(),
    })
}

fn optional_string(value: &serde_json::Value, pointer: &str) -> Option<String> {
    value
        .pointer(pointer)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn required_file(
    model_dir: &Path,
    metadata: &serde_json::Value,
    pointer: &str,
    family: &str,
) -> Result<PathBuf, SherpaOnnxModelPathError> {
    let value = optional_string(metadata, pointer).ok_or_else(|| {
        SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(&model_dir.join("vinput-model.json")),
            message: format!("missing string `{pointer}`"),
        }
    })?;
    let path = resolve_against(model_dir, &value);
    if !path.is_file() {
        return Err(SherpaOnnxModelPathError::MissingModelAsset {
            family: family.to_owned(),
            asset: pointer.rsplit('/').next().unwrap_or(pointer).to_owned(),
            path: display_path(&path),
        });
    }
    Ok(path)
}

fn optional_file(
    model_dir: &Path,
    metadata: &serde_json::Value,
    pointer: &str,
    family: &str,
) -> Result<Option<PathBuf>, SherpaOnnxModelPathError> {
    let Some(value) = optional_string(metadata, pointer) else {
        return Ok(None);
    };
    let path = resolve_against(model_dir, &value);
    if !path.is_file() {
        return Err(SherpaOnnxModelPathError::MissingModelAsset {
            family: family.to_owned(),
            asset: pointer.rsplit('/').next().unwrap_or(pointer).to_owned(),
            path: display_path(&path),
        });
    }
    Ok(Some(path))
}

fn positive_i32(
    value: &serde_json::Value,
    pointer: &str,
    default: i32,
    metadata_path: &Path,
) -> Result<i32, SherpaOnnxModelPathError> {
    let Some(raw) = value.pointer(pointer) else {
        return Ok(default);
    };
    let parsed = raw
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            invalid_metadata(
                metadata_path,
                format!("`{pointer}` must be a positive 32-bit integer"),
            )
        })?;
    Ok(parsed)
}

fn boolish(
    value: &serde_json::Value,
    pointer: &str,
    default: bool,
    metadata_path: &Path,
) -> Result<bool, SherpaOnnxModelPathError> {
    let Some(raw) = value.pointer(pointer) else {
        return Ok(default);
    };
    raw.as_bool()
        .or_else(|| {
            raw.as_i64().and_then(|value| match value {
                0 => Some(false),
                1 => Some(true),
                _ => None,
            })
        })
        .ok_or_else(|| {
            invalid_metadata(metadata_path, format!("`{pointer}` must be boolean or 0/1"))
        })
}

fn finite_f32(
    value: &serde_json::Value,
    pointer: &str,
    default: f32,
    metadata_path: &Path,
) -> Result<f32, SherpaOnnxModelPathError> {
    let Some(raw) = value.pointer(pointer) else {
        return Ok(default);
    };
    let parsed = raw
        .as_f64()
        .filter(|value| {
            value.is_finite() && *value >= f64::from(f32::MIN) && *value <= f64::from(f32::MAX)
        })
        .ok_or_else(|| {
            invalid_metadata(
                metadata_path,
                format!("`{pointer}` must be a finite f32 value"),
            )
        })?;
    #[allow(clippy::cast_possible_truncation)]
    let parsed = parsed as f32;
    Ok(parsed)
}

fn invalid_metadata(path: &Path, message: String) -> SherpaOnnxModelPathError {
    SherpaOnnxModelPathError::InvalidModelMetadata {
        path: display_path(path),
        message,
    }
}

fn resolve_against(root: &Path, value: &str) -> PathBuf {
    let path = Path::new(value);
    if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    }
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

#[cfg(feature = "sherpa-onnx-backend")]
pub(crate) struct SherpaOnnxOnlineRuntime {
    recognizer: std::sync::Arc<sherpa_onnx::OnlineRecognizer>,
}

#[cfg(feature = "sherpa-onnx-backend")]
impl SherpaOnnxOnlineRuntime {
    pub(crate) fn create(
        plan: &SherpaOnnxOnlineRuntimePlan,
        provider_id: &str,
    ) -> Result<Self, crate::AsrError> {
        let config = online_recognizer_config(plan);
        let recognizer = sherpa_onnx::OnlineRecognizer::create(&config).ok_or_else(|| {
            crate::AsrError::Backend(format!(
                "failed to create sherpa-onnx online recognizer for provider `{provider_id}`"
            ))
        })?;
        Ok(Self {
            recognizer: std::sync::Arc::new(recognizer),
        })
    }

    pub(crate) fn create_session(&self) -> Box<dyn crate::RecognitionSession> {
        Box::new(SherpaOnnxOnlineRecognitionSession {
            stream: self.recognizer.create_stream(),
            recognizer: std::sync::Arc::clone(&self.recognizer),
            pcm: None,
            events: Vec::new(),
            last_hypothesis: String::new(),
            finished: false,
            cancelled: false,
        })
    }
}

#[cfg(feature = "sherpa-onnx-backend")]
fn online_recognizer_config(
    plan: &SherpaOnnxOnlineRuntimePlan,
) -> sherpa_onnx::OnlineRecognizerConfig {
    let mut config = sherpa_onnx::OnlineRecognizerConfig::default();
    config.feat_config.sample_rate = plan.sample_rate;
    config.feat_config.feature_dim = plan.feature_dim;
    config.model_config.tokens = Some(display_path(&plan.tokens));
    config.model_config.num_threads = plan.num_threads;
    config.model_config.provider = Some(plan.provider.clone());
    config.model_config.debug = plan.debug;
    config.model_config.model_type.clone_from(&plan.model_type);
    config
        .model_config
        .modeling_unit
        .clone_from(&plan.modeling_unit);
    config.model_config.bpe_vocab = plan.bpe_vocab.as_deref().map(display_path);
    match &plan.layout {
        SherpaOnnxOnlineModelLayout::Transducer {
            encoder,
            decoder,
            joiner,
        } => {
            config.model_config.transducer = sherpa_onnx::OnlineTransducerModelConfig {
                encoder: Some(display_path(encoder)),
                decoder: Some(display_path(decoder)),
                joiner: Some(display_path(joiner)),
            };
        }
        SherpaOnnxOnlineModelLayout::Zipformer2Ctc { model } => {
            config.model_config.zipformer2_ctc = sherpa_onnx::OnlineZipformer2CtcModelConfig {
                model: Some(display_path(model)),
            };
        }
    }
    config.decoding_method = Some(plan.decoding_method.clone());
    config.max_active_paths = plan.max_active_paths;
    config.enable_endpoint = plan.enable_endpoint;
    config.rule1_min_trailing_silence = plan.rule1_min_trailing_silence;
    config.rule2_min_trailing_silence = plan.rule2_min_trailing_silence;
    config.rule3_min_utterance_length = plan.rule3_min_utterance_length;
    config.hotwords_file = plan.hotwords_file.as_deref().map(display_path);
    config.hotwords_score = plan.hotwords_score;
    config.ctc_fst_decoder_config = sherpa_onnx::OnlineCtcFstDecoderConfig {
        graph: plan.ctc_graph.as_deref().map(display_path),
        max_active: plan.ctc_max_active,
    };
    config.rule_fsts = plan.rule_fsts.as_deref().map(display_path);
    config.rule_fars = plan.rule_fars.as_deref().map(display_path);
    config.blank_penalty = plan.blank_penalty;
    config.hr = sherpa_onnx::HomophoneReplacerConfig {
        lexicon: plan.homophone_lexicon.as_deref().map(display_path),
        rule_fsts: plan.homophone_rule_fsts.as_deref().map(display_path),
    };
    config
}

#[cfg(feature = "sherpa-onnx-backend")]
struct SherpaOnnxOnlineRecognitionSession {
    recognizer: std::sync::Arc<sherpa_onnx::OnlineRecognizer>,
    stream: sherpa_onnx::OnlineStream,
    pcm: Option<vinput_audio::PcmSpec>,
    events: Vec<crate::RecognitionEvent>,
    last_hypothesis: String,
    finished: bool,
    cancelled: bool,
}

#[cfg(feature = "sherpa-onnx-backend")]
impl SherpaOnnxOnlineRecognitionSession {
    fn ensure_active(&self) -> Result<(), crate::AsrError> {
        if self.cancelled {
            Err(crate::AsrError::Cancelled)
        } else if self.finished {
            Err(crate::AsrError::AlreadyFinished)
        } else {
            Ok(())
        }
    }

    fn decode_available(&mut self, emit_partial: bool) {
        while self.recognizer.is_ready(&self.stream) {
            self.recognizer.decode(&self.stream);
        }
        if !emit_partial {
            return;
        }
        let Some(result) = self.recognizer.get_result(&self.stream) else {
            return;
        };
        let text = result.text.trim();
        if text.is_empty() || text == self.last_hypothesis {
            return;
        }
        text.clone_into(&mut self.last_hypothesis);
        self.events.push(crate::RecognitionEvent::PartialText {
            text: text.to_owned(),
        });
    }

    fn validate_pcm(&mut self, spec: vinput_audio::PcmSpec) -> Result<(), crate::AsrError> {
        if spec.channels != 1 {
            return Err(crate::AsrError::Backend(format!(
                "sherpa-onnx online recognition requires mono PCM, got {} channels",
                spec.channels
            )));
        }
        if let Some(existing) = self.pcm {
            if existing != spec {
                return Err(crate::AsrError::Backend(format!(
                    "sherpa-onnx PCM spec changed from {} Hz/{} channel(s) to {} Hz/{} channel(s)",
                    existing.sample_rate_hz, existing.channels, spec.sample_rate_hz, spec.channels
                )));
            }
        } else {
            self.pcm = Some(spec);
        }
        Ok(())
    }
}

#[cfg(feature = "sherpa-onnx-backend")]
impl crate::RecognitionSession for SherpaOnnxOnlineRecognitionSession {
    fn push_pcm(&mut self, pcm: &vinput_audio::PcmBuffer) -> Result<(), crate::AsrError> {
        self.ensure_active()?;
        self.validate_pcm(pcm.spec())?;
        let sample_rate = i32::try_from(pcm.spec().sample_rate_hz).map_err(|_| {
            crate::AsrError::Backend(format!(
                "sherpa-onnx sample rate {} does not fit i32",
                pcm.spec().sample_rate_hz
            ))
        })?;
        let samples = pcm
            .samples()
            .iter()
            .map(|sample| f32::from(*sample) / f32::from(i16::MAX))
            .collect::<Vec<_>>();
        self.stream.accept_waveform(sample_rate, &samples);
        self.decode_available(true);
        Ok(())
    }

    fn push_audio(&mut self, samples: &[i16]) -> Result<(), crate::AsrError> {
        self.push_pcm(&vinput_audio::PcmBuffer::at_default_rate(samples.to_vec()))
    }

    fn finish(&mut self) -> Result<(), crate::AsrError> {
        self.ensure_active()?;
        self.finished = true;
        self.stream.input_finished();
        self.decode_available(false);
        let result = self.recognizer.get_result(&self.stream).ok_or_else(|| {
            crate::AsrError::Backend("sherpa-onnx online recognizer returned no result".to_owned())
        })?;
        let text = result.text.trim().to_owned();
        if text.is_empty() {
            return Err(crate::AsrError::Backend(
                "sherpa-onnx online recognizer returned empty text".to_owned(),
            ));
        }
        self.events
            .push(crate::RecognitionEvent::FinalText { text });
        self.events.push(crate::RecognitionEvent::Completed);
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), crate::AsrError> {
        self.cancelled = true;
        self.events.clear();
        Ok(())
    }

    fn poll_events(&mut self) -> Result<Vec<crate::RecognitionEvent>, crate::AsrError> {
        Ok(std::mem::take(&mut self.events))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_model_file(root: &Path, name: &str) {
        fs::write(root.join(name), b"model").unwrap();
    }

    #[test]
    fn resolves_zipformer2_ctc_online_metadata() {
        let temp = tempfile::tempdir().unwrap();
        write_model_file(temp.path(), "model.int8.onnx");
        write_model_file(temp.path(), "tokens.txt");
        fs::write(
            temp.path().join("vinput-model.json"),
            serde_json::json!({
                "backend": "sherpa-streaming",
                "family": "zipformer2_ctc",
                "runtime": "online",
                "model": {
                    "tokens": "tokens.txt",
                    "num_threads": 2,
                    "provider": "cpu",
                    "debug": 1,
                    "zipformer2_ctc": {"model": "model.int8.onnx"}
                },
                "recognizer": {
                    "feat_config": {"sample_rate": 16000, "feature_dim": 80},
                    "decoding_method": "greedy_search",
                    "max_active_paths": 4,
                    "enable_endpoint": 0,
                    "ctc_fst_decoder_config": {"graph": "", "max_active": 3000}
                }
            })
            .to_string(),
        )
        .unwrap();
        let paths = SherpaOnnxModelPaths {
            model_dir: temp.path().to_owned(),
            hotwords_file: None,
        };

        assert!(metadata_requests_online(temp.path()).unwrap());
        let plan = resolve_online_runtime_plan(&paths).unwrap();
        assert_eq!(
            plan.layout,
            SherpaOnnxOnlineModelLayout::Zipformer2Ctc {
                model: temp.path().join("model.int8.onnx")
            }
        );
        assert_eq!(plan.tokens, temp.path().join("tokens.txt"));
        assert_eq!(plan.num_threads, 2);
        assert!(plan.debug);
        assert_eq!(plan.sample_rate, 16_000);
        assert_eq!(plan.ctc_max_active, 3_000);
    }

    #[test]
    fn resolves_transducer_assets_and_provider_hotwords_override() {
        let temp = tempfile::tempdir().unwrap();
        for name in [
            "encoder.onnx",
            "decoder.onnx",
            "joiner.onnx",
            "tokens.txt",
            "hotwords.txt",
        ] {
            write_model_file(temp.path(), name);
        }
        fs::write(
            temp.path().join("vinput-model.json"),
            serde_json::json!({
                "backend": "sherpa-streaming",
                "family": "transducer",
                "runtime": "online",
                "model": {
                    "tokens": "tokens.txt",
                    "transducer": {
                        "encoder": "encoder.onnx",
                        "decoder": "decoder.onnx",
                        "joiner": "joiner.onnx"
                    }
                },
                "recognizer": {
                    "hotwords_file": "",
                    "hotwords_score": 2.0
                }
            })
            .to_string(),
        )
        .unwrap();
        let hotwords = temp.path().join("hotwords.txt");
        let paths = SherpaOnnxModelPaths {
            model_dir: temp.path().to_owned(),
            hotwords_file: Some(hotwords.clone()),
        };

        let plan = resolve_online_runtime_plan(&paths).unwrap();
        assert!(matches!(
            plan.layout,
            SherpaOnnxOnlineModelLayout::Transducer { .. }
        ));
        assert_eq!(plan.hotwords_file, Some(hotwords));
        assert!((plan.hotwords_score - 2.0).abs() < f32::EPSILON);
    }
}

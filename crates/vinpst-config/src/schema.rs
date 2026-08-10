use std::{collections::HashMap, fmt};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vinpst_protocol::CandidateSource;

use crate::defaults::{
    default_active_scene, default_asr_provider, default_capture_device, default_duck_output_volume,
    default_input_gain, default_json_object, default_language, default_true,
    default_vad_min_silence_duration, default_vad_min_speech_duration, default_vad_speech_pad_ms,
    default_vad_threshold,
};

/// Forward-compatible extra key containing the installed managed script SHA-256.
pub const MANAGED_SCRIPT_REVISION_KEY: &str = "x-vinpst-managed-script-sha256";

/// Forward-compatible extra key identifying the script revision stored in the rollback artifact.
pub const MANAGED_SCRIPT_ROLLBACK_REVISION_KEY: &str = "x-vinpst-managed-script-rollback-sha256";

/// Built-in raw scene id used by the legacy project.
pub const RAW_SCENE_ID: &str = "__raw__";

/// Built-in command scene id used by the legacy project.
pub const COMMAND_SCENE_ID: &str = "__command__";

/// Legacy-compatible request timeout used when a scene omits `timeout_ms`.
pub const DEFAULT_SCENE_TIMEOUT_MS: u64 = 4_000;

/// Highest configuration schema version supported by this binary.
pub const CURRENT_CONFIG_VERSION: u32 = 1;

/// Complete config document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VinpstConfig {
    /// Config schema version.
    pub version: u32,
    /// Registry mirror settings.
    #[serde(default)]
    pub registry: RegistryConfig,
    /// Global daemon and UI defaults.
    #[serde(default)]
    pub global: GlobalConfig,
    /// ASR settings.
    #[serde(default)]
    pub asr: AsrConfig,
    /// LLM provider/adapter settings.
    #[serde(default)]
    pub llm: LlmConfig,
    /// Scene selection and definitions.
    #[serde(default)]
    pub scenes: ScenesConfig,
}

/// Compact config summary for CLI and diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VinpstConfigSummary {
    /// Whether validation succeeded.
    pub ok: bool,
    /// Config schema version.
    pub version: u32,
    /// Active scene id.
    pub active_scene: String,
    /// Active ASR provider id.
    pub active_provider: String,
    /// Number of configured scenes.
    pub scene_count: usize,
    /// Number of configured ASR providers.
    pub provider_count: usize,
    /// Number of configured registry mirrors.
    pub registry_mirror_count: usize,
}

/// Registry mirror settings.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct RegistryConfig {
    /// Ordered registry base URLs.
    #[serde(default)]
    pub base_urls: Vec<String>,
}

impl fmt::Debug for RegistryConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RegistryConfig")
            .field(
                "base_urls",
                &self
                    .base_urls
                    .iter()
                    .map(|url| crate::redact_url_for_diagnostics(url))
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

/// Global daemon/UI defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GlobalConfig {
    /// Default recognition language.
    #[serde(default = "default_language")]
    pub default_language: String,
    /// `PipeWire` target capture device.
    #[serde(default = "default_capture_device")]
    pub capture_device: String,
    /// Whether the default output sink should be ducked while recording.
    #[serde(default)]
    pub duck_output_while_recording: bool,
    /// Output-volume multiplier used while recording.
    #[serde(default = "default_duck_output_volume")]
    pub duck_output_volume: f32,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            default_language: default_language(),
            capture_device: default_capture_device(),
            duck_output_while_recording: false,
            duck_output_volume: default_duck_output_volume(),
        }
    }
}

/// ASR settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AsrConfig {
    /// Selected provider id.
    #[serde(default = "default_asr_provider")]
    pub active_provider: String,
    /// Whether captured audio should be normalized before recognition.
    #[serde(default = "default_true")]
    pub normalize_audio: bool,
    /// Input gain applied before ASR.
    #[serde(default = "default_input_gain")]
    pub input_gain: f32,
    /// VAD settings.
    #[serde(default)]
    pub vad: VadConfig,
    /// Known providers.
    #[serde(default)]
    pub providers: Vec<AsrProviderConfig>,
}

impl Default for AsrConfig {
    fn default() -> Self {
        Self {
            active_provider: default_asr_provider(),
            normalize_audio: true,
            input_gain: default_input_gain(),
            vad: VadConfig::default(),
            providers: Vec::new(),
        }
    }
}

/// VAD settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VadConfig {
    /// Whether VAD trimming is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Silero speech probability threshold.
    #[serde(default = "default_vad_threshold")]
    pub threshold: f32,
    /// Minimum accepted speech duration in seconds.
    #[serde(default = "default_vad_min_speech_duration")]
    pub min_speech_duration: f32,
    /// Minimum silence duration used to close a segment, in seconds.
    #[serde(default = "default_vad_min_silence_duration")]
    pub min_silence_duration: f32,
    /// Padding added before and after each detected speech segment.
    #[serde(default = "default_vad_speech_pad_ms")]
    pub speech_pad_ms: u32,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: default_vad_threshold(),
            min_speech_duration: default_vad_min_speech_duration(),
            min_silence_duration: default_vad_min_silence_duration(),
            speech_pad_ms: default_vad_speech_pad_ms(),
        }
    }
}

/// ASR provider type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AsrProviderKind {
    /// Local backend, usually sherpa-onnx.
    Local,
    /// Remote HTTP/WebSocket backend.
    Remote,
    /// External command backend.
    Command,
}

/// ASR provider config entry.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AsrProviderConfig {
    /// Stable provider id.
    pub id: String,
    /// Backend kind.
    #[serde(rename = "type")]
    pub kind: AsrProviderKind,
    /// Provider timeout in milliseconds.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Optional model id selected for this provider.
    #[serde(default)]
    pub model: Option<String>,
    /// Optional hotwords file for local ASR backends.
    #[serde(default)]
    pub hotwords_file: Option<String>,
    /// External command used by command ASR providers.
    #[serde(default)]
    pub command: Option<String>,
    /// Arguments passed to the external command ASR provider.
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables passed to the external command ASR provider.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Optional endpoint label or URL for remote ASR providers.
    #[serde(default)]
    pub endpoint: Option<String>,
}

impl fmt::Debug for AsrProviderConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AsrProviderConfig")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("timeout_ms", &self.timeout_ms)
            .field("has_model", &self.model.is_some())
            .field("has_hotwords_file", &self.hotwords_file.is_some())
            .field("has_command", &self.command.is_some())
            .field("args_count", &self.args.len())
            .field("env_count", &self.env.len())
            .field(
                "endpoint",
                &self
                    .endpoint
                    .as_deref()
                    .map(crate::redact_url_for_diagnostics),
            )
            .finish()
    }
}

/// LLM provider/adapter config.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
pub struct LlmConfig {
    /// Provider entries used by scene and command post-processing.
    #[serde(default)]
    pub providers: Vec<LlmProviderConfig>,
    /// Adapter process entries used by local/remote text adapters.
    #[serde(default)]
    pub adapters: Vec<LlmAdapterConfig>,
}

/// OpenAI-compatible or adapter-backed LLM provider config.
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LlmProviderConfig {
    /// Stable provider id.
    pub id: String,
    /// Base URL for OpenAI-compatible providers.
    #[serde(default)]
    pub base_url: String,
    /// API key or environment-reference expression.
    #[serde(default)]
    pub api_key: String,
    /// Optional default model name.
    #[serde(default)]
    pub model: Option<String>,
    /// Extra JSON body merged into provider requests.
    #[serde(default = "default_json_object")]
    pub extra_body: serde_json::Value,
    /// Forward-compatible unknown provider fields.
    #[serde(default, flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl fmt::Debug for LlmProviderConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LlmProviderConfig")
            .field("id", &self.id)
            .field(
                "base_url",
                &crate::redact_url_for_diagnostics(&self.base_url),
            )
            .field("has_api_key", &(!self.api_key.is_empty()))
            .field("model", &self.model)
            .field(
                "extra_body_keys",
                &self.extra_body.as_object().map(serde_json::Map::len),
            )
            .field("extra_keys", &self.extra.len())
            .finish()
    }
}

/// External text adapter process config.
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LlmAdapterConfig {
    /// Stable adapter id.
    pub id: String,
    /// Adapter executable path or command name.
    pub command: String,
    /// Arguments passed to the adapter process.
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables passed to the adapter process.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Optional working directory for the adapter process.
    #[serde(default)]
    pub working_dir: Option<String>,
    /// Forward-compatible unknown adapter fields.
    #[serde(default, flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl fmt::Debug for LlmAdapterConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LlmAdapterConfig")
            .field("id", &self.id)
            .field("has_command", &(!self.command.is_empty()))
            .field("args_count", &self.args.len())
            .field("env_count", &self.env.len())
            .field("has_working_dir", &self.working_dir.is_some())
            .field("extra_keys", &self.extra.len())
            .finish()
    }
}

/// Scene collection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScenesConfig {
    /// Selected scene id.
    #[serde(default = "default_active_scene")]
    pub active_scene: String,
    /// Known scenes.
    #[serde(default)]
    pub definitions: Vec<SceneDefinition>,
}

impl Default for ScenesConfig {
    fn default() -> Self {
        Self {
            active_scene: default_active_scene(),
            definitions: Vec::new(),
        }
    }
}

/// A post-processing scene definition.
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SceneDefinition {
    /// Stable scene id.
    pub id: String,
    /// Translation key or display label.
    pub label: String,
    /// Optional prompt template.
    #[serde(default)]
    pub prompt: Option<String>,
    /// LLM provider id used for post-processing.
    #[serde(default)]
    pub provider_id: Option<String>,
    /// Optional model override for this scene.
    #[serde(default)]
    pub model: Option<String>,
    /// Number of result candidates to ask the post-processor for.
    #[serde(default)]
    pub candidate_count: u8,
    /// Optional per-scene timeout in milliseconds.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Number of recent input context lines to include.
    #[serde(default)]
    pub context_lines: u8,
}

impl fmt::Debug for SceneDefinition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SceneDefinition")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("has_prompt", &self.prompt.is_some())
            .field("provider_id", &self.provider_id)
            .field("model", &self.model)
            .field("candidate_count", &self.candidate_count)
            .field("timeout_ms", &self.timeout_ms)
            .field("context_lines", &self.context_lines)
            .finish()
    }
}

impl SceneDefinition {
    /// Returns the explicit timeout or the legacy 4000 ms scene default.
    #[must_use]
    pub fn effective_timeout_ms(&self) -> u64 {
        self.timeout_ms.unwrap_or(DEFAULT_SCENE_TIMEOUT_MS)
    }

    /// Candidate source expected for this scene when no LLM is needed.
    #[must_use]
    pub fn default_candidate_source(&self) -> CandidateSource {
        if self.id == RAW_SCENE_ID {
            CandidateSource::Raw
        } else {
            CandidateSource::Llm
        }
    }
}

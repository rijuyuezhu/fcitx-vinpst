use std::path::PathBuf;

use thiserror::Error;

/// Config errors.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// JSON parsing failed.
    #[error("invalid config JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// Config schema is newer than this binary understands.
    #[error("unsupported config schema version {found}; this binary supports up to {supported}")]
    UnsupportedSchemaVersion {
        /// Version found in the config document.
        found: u32,
        /// Highest supported version.
        supported: u32,
    },
    /// Reading a config file failed.
    #[error("failed to read config file `{}`: {source}", path.display())]
    ReadFile {
        /// Config file path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Registry base URL is empty.
    #[error("invalid empty registry base URL")]
    InvalidRegistryBaseUrl(String),
    /// Registry base URL is duplicated.
    #[error("duplicate registry base URL `{0}`")]
    DuplicateRegistryBaseUrl(String),
    /// Default language is empty.
    #[error("invalid empty default language")]
    InvalidDefaultLanguage,
    /// Capture device is empty.
    #[error("invalid empty capture device")]
    InvalidCaptureDevice,
    /// Output ducking volume is outside the supported range.
    #[error("invalid duck_output_volume {0}; expected a finite value in 0.0..=1.0")]
    InvalidDuckOutputVolume(f32),
    /// Active scene is not listed in scene definitions.
    #[error("active scene `{0}` is not defined")]
    UnknownActiveScene(String),
    /// Active ASR provider id is empty.
    #[error("invalid empty active ASR provider id")]
    InvalidActiveAsrProviderId,
    /// Active ASR provider is not listed in provider definitions.
    #[error("active ASR provider `{0}` is not defined")]
    UnknownActiveAsrProvider(String),
    /// Empty scene id.
    #[error("invalid empty scene id")]
    InvalidSceneId(String),
    /// Empty scene label.
    #[error("invalid empty scene label for scene `{0}`")]
    InvalidSceneLabel(String),
    /// Duplicate scene id.
    #[error("duplicate scene id `{0}`")]
    DuplicateSceneId(String),
    /// Scene provider id is present but empty.
    #[error("scene `{0}` has an invalid empty provider id")]
    InvalidSceneProviderId(String),
    /// Scene model id is present but empty.
    #[error("scene `{0}` has an invalid empty model id")]
    InvalidSceneModelId(String),
    /// Scene prompt is present but empty.
    #[error("scene `{0}` has an invalid empty prompt")]
    InvalidScenePrompt(String),
    /// Scene provider id does not match a configured LLM provider.
    #[error("scene `{scene_id}` references unknown LLM provider `{provider_id}`")]
    UnknownSceneProviderId {
        /// Scene id.
        scene_id: String,
        /// Missing provider id.
        provider_id: String,
    },
    /// Scene timeout must be positive when configured.
    #[error("scene `{0}` has invalid timeout_ms 0")]
    InvalidSceneTimeoutMs(String),
    /// Scene asks for too many recent context lines.
    #[error("scene `{scene_id}` asks for {context_lines} context lines, max is 32")]
    TooManyContextLines {
        /// Scene id.
        scene_id: String,
        /// Requested context lines.
        context_lines: u8,
    },
    /// Empty ASR provider id.
    #[error("invalid empty ASR provider id")]
    InvalidAsrProviderId(String),
    /// Duplicate ASR provider id.
    #[error("duplicate ASR provider id `{0}`")]
    DuplicateAsrProviderId(String),
    /// ASR provider model id is present but empty.
    #[error("ASR provider `{0}` has an invalid empty model id")]
    InvalidAsrProviderModelId(String),
    /// ASR provider hotwords file is present but empty.
    #[error("ASR provider `{0}` has an invalid empty hotwords_file")]
    InvalidAsrProviderHotwordsFile(String),
    /// ASR provider command is present but empty for a non-command backend.
    #[error("ASR provider `{0}` has an invalid empty command")]
    InvalidAsrProviderCommand(String),
    /// ASR provider endpoint is present but empty.
    #[error("ASR provider `{0}` has an invalid empty endpoint")]
    InvalidAsrProviderEndpoint(String),
    /// ASR provider timeout must be positive when configured.
    #[error("ASR provider `{0}` has invalid timeout_ms 0")]
    InvalidAsrProviderTimeoutMs(String),
    /// Command ASR provider requires a command.
    #[error("command ASR provider `{0}` must configure a command")]
    InvalidCommandAsrProviderCommand(String),
    /// Remote ASR provider requires an endpoint.
    #[error("remote ASR provider `{0}` must configure an endpoint")]
    InvalidRemoteAsrProviderEndpoint(String),
    /// Provider environment contains an empty key.
    #[error("provider `{provider_id}` has an invalid environment key `{key}`")]
    InvalidProviderEnvKey {
        /// Provider id.
        provider_id: String,
        /// Invalid environment key.
        key: String,
    },
    /// VAD threshold is outside the supported strict range.
    #[error("invalid VAD threshold {0}; expected a finite value in 0.05..=0.95")]
    InvalidVadThreshold(f32),
    /// VAD minimum speech duration is outside the supported strict range.
    #[error("invalid VAD min_speech_duration {0}; expected a finite value in 0.05..=2.0")]
    InvalidVadMinSpeechDuration(f32),
    /// VAD minimum silence duration is outside the supported strict range.
    #[error("invalid VAD min_silence_duration {0}; expected a finite value in 0.05..=5.0")]
    InvalidVadMinSilenceDuration(f32),
    /// VAD speech padding exceeds the supported cap.
    #[error("invalid VAD speech_pad_ms {0}; max is 2000")]
    InvalidVadSpeechPadMs(u32),
    /// Empty LLM provider id.
    #[error("invalid empty LLM provider id")]
    InvalidLlmProviderId(String),
    /// Duplicate LLM provider id.
    #[error("duplicate LLM provider id `{0}`")]
    DuplicateLlmProviderId(String),
    /// LLM provider base URL is empty.
    #[error("LLM provider `{0}` must configure a base URL")]
    InvalidLlmProviderBaseUrl(String),
    /// LLM provider model id is present but empty.
    #[error("LLM provider `{0}` has an invalid empty model id")]
    InvalidLlmProviderModelId(String),
    /// LLM provider `extra_body` must be a JSON object.
    #[error("LLM provider `{0}` has invalid non-object extra_body")]
    InvalidLlmProviderExtraBody(String),
    /// Empty LLM adapter id.
    #[error("invalid empty LLM adapter id")]
    InvalidLlmAdapterId(String),
    /// Duplicate LLM adapter id.
    #[error("duplicate LLM adapter id `{0}`")]
    DuplicateLlmAdapterId(String),
    /// LLM adapter command is empty.
    #[error("LLM adapter `{0}` must configure a command")]
    InvalidLlmAdapterCommand(String),
    /// LLM adapter working directory is present but empty.
    #[error("LLM adapter `{0}` has an invalid empty working_dir")]
    InvalidLlmAdapterWorkingDir(String),
    /// LLM adapter environment contains an empty key.
    #[error("LLM adapter `{adapter_id}` has an invalid environment key `{key}`")]
    InvalidLlmAdapterEnvKey {
        /// Adapter id.
        adapter_id: String,
        /// Invalid environment key.
        key: String,
    },
    /// Candidate count is above the safety cap.
    #[error("scene `{scene_id}` asks for {candidate_count} candidates, max is 32")]
    TooManyCandidates {
        /// Scene id.
        scene_id: String,
        /// Requested candidate count.
        candidate_count: u8,
    },
}

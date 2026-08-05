//! Runtime error types.

use thiserror::Error;
use vinpst_asr::AsrError;
use vinpst_audio::AudioError;
use vinpst_protocol::ServiceStatus;

/// Runtime errors.
#[derive(Debug, Error)]
pub enum RuntimeError {
    /// Config failed validation.
    #[error("invalid config: {0}")]
    InvalidConfig(#[source] vinpst_config::ConfigError),
    /// Runtime cannot start a new session while busy.
    #[error("runtime is busy: {0}")]
    Busy(ServiceStatus),
    /// Stop was requested while not recording.
    #[error("runtime is not recording: {0}")]
    NotRecording(ServiceStatus),
    /// Recording reached stop without an active ASR session.
    #[error("runtime is missing an active ASR session")]
    MissingAsrSession,
    /// ASR backend/session failed.
    #[error("asr error: {0}")]
    Asr(#[source] AsrError),
    /// Installed model discovery failed.
    #[error("installed model discovery failed: {0}")]
    InstalledModels(#[source] vinpst_registry::InstalledModelError),
    /// Requested provider/model pair is not exposed by the configured menu.
    #[error("ASR target `{provider}` / `{model}` is not configured or installed")]
    UnknownAsrTarget {
        /// Requested provider id.
        provider: String,
        /// Requested model value.
        model: String,
    },
    /// Audio source failed.
    #[error("audio error: {0}")]
    Audio(#[source] AudioError),
    /// Result finishing failed.
    #[error("result finishing error: {0}")]
    Finish(#[source] vinpst_text::TextError),
    /// Requested text adapter is not configured.
    #[error("text adapter `{0}` is not configured")]
    TextAdapterNotConfigured(String),
    /// Requested text adapter is already managed by this runtime.
    #[error("text adapter `{0}` is already running")]
    TextAdapterAlreadyRunning(String),
    /// Text adapter process supervision failed.
    #[error("text adapter supervisor error: {0}")]
    TextAdapterSupervisor(#[source] vinpst_text::TextError),
    /// A daemon background task terminated before returning its result.
    #[error("background task failed: {0}")]
    BackgroundTask(String),
    /// The requested scene is not configured.
    #[error("scene `{0}` is not configured")]
    UnknownScene(String),
    /// Config serialization failed before persistence.
    #[error("failed to serialize config: {0}")]
    SerializeConfig(#[source] serde_json::Error),
    /// Config persistence failed.
    #[error("failed to persist config `{path}`: {source}")]
    PersistConfig {
        /// Path being written or published.
        path: std::path::PathBuf,
        /// Underlying filesystem failure.
        #[source]
        source: std::io::Error,
    },
}

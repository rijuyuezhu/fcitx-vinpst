//! ASR backend contract, deterministic mock, and backend skeletons.
//!
//! This crate mirrors the original C++ daemon's recognition contract at a Rust
//! trait boundary. Real backends such as sherpa-onnx and command execution
//! should implement these traits after their contracts are covered by tests.

mod command;
mod error;
mod factory;
mod mock;
mod payload;
mod sherpa;
mod sherpa_online;
mod sherpa_vad;
mod timeout;
mod traits;
mod unavailable;

pub use command::{
    CommandAsrBackend, CommandAsrRequest, CommandAsrResponse, CommandAsrRunner, CommandAsrSpec,
    LegacyCommandBatchRunner, LegacyCommandStreamingRunner, ProcessCommandAsrRunner,
    UnsupportedCommandAsrRunner, legacy_command_streaming_audio_line,
    legacy_command_streaming_finish_line, parse_legacy_command_streaming_line,
};
pub use error::AsrError;
pub use factory::AsrBackendFactory;
pub use mock::{MockAsrAudioLog, MockAsrAudioPush, MockAsrBackend};
pub use payload::events_to_payload;
#[cfg(feature = "sherpa-onnx-backend")]
pub use sherpa::SherpaOnnxBackend;
pub use sherpa::{
    SHERPA_ONNX_PROVIDER_ID, SherpaOnnxModelPathError, SherpaOnnxModelPaths,
    SherpaOnnxOfflineModelLayout, SherpaOnnxOfflineRuntimePlan, SherpaOnnxOfflineSettings,
    SherpaOnnxSpec,
};
pub use sherpa_online::{SherpaOnnxOnlineModelLayout, SherpaOnnxOnlineRuntimePlan};
pub use sherpa_vad::{SherpaOnnxVadModelSource, SherpaOnnxVadPlan, SherpaOnnxVadProbe};
pub use timeout::{AsrTimeoutEnforcement, AsrTimeoutProbe};
pub use traits::{
    AsrBackend, AudioDeliveryMode, BackendCapabilities, BackendDescriptor, RecognitionContext,
    RecognitionEvent, RecognitionSession,
};
pub use unavailable::UnavailableAsrBackend;

#[cfg(test)]
mod tests;

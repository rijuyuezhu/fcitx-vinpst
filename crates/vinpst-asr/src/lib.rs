//! ASR backend contracts and implementations shared by Vinpst runtimes.
//!
//! The crate provides local sherpa-onnx, supervised command, OpenAI-compatible
//! remote, deterministic mock, and unavailable-placeholder backends behind one
//! recognition-session contract, plus non-mutating capability diagnostics.

mod command;
mod command_streaming;
mod error;
mod factory;
mod mock;
mod payload;
mod remote;
mod sherpa;
mod sherpa_online;
mod sherpa_vad;
mod timeout;
mod traits;
mod unavailable;

#[cfg(test)]
pub(crate) use command::LegacyCommandStreamingRunner;
pub use command::{
    CommandAsrBackend, CommandAsrRequest, CommandAsrResponse, CommandAsrRunner, CommandAsrSpec,
    LegacyCommandBatchRunner, ProcessCommandAsrRunner, UnsupportedCommandAsrRunner,
    legacy_command_streaming_audio_line, legacy_command_streaming_finish_line,
    parse_legacy_command_streaming_line,
};
pub use command_streaming::LegacyCommandStreamingBackend;
pub use error::AsrError;
pub use factory::AsrBackendFactory;
pub use mock::{MockAsrAudioLog, MockAsrAudioPush, MockAsrBackend};
pub use payload::events_to_payload;
pub use remote::{
    RemoteAsrBackend, RemoteAsrRequest, RemoteAsrSpec, RemoteAsrTransport,
    ReqwestRemoteAsrTransport, build_openai_compatible_transcriptions_url, encode_pcm16le_wav,
    extract_openai_compatible_transcription,
};
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
    AsrBackend, AudioDeliveryMode, BackendCapabilities, BackendDescriptor,
    MIN_SAMPLES_FOR_RECOGNITION, RecognitionContext, RecognitionEvent, RecognitionSession,
};
pub use unavailable::UnavailableAsrBackend;

#[cfg(test)]
mod tests;

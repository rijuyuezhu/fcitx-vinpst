//! Stable protocol types shared by the daemon, CLI, GUI, and Fcitx5 frontend bridge.
//!
//! This crate keeps cross-process JSON and D-Bus types dependency-light. Legacy
//! wire compatibility is preserved where required, while Vinpst-only diagnostic
//! extensions use explicit sanitized summaries instead of exposing runtime secrets.

pub mod asr;
pub mod dbus;
pub mod recognition;
pub mod status;
pub mod text;

pub use asr::{AsrBackendState, RequestedAsrBackendStatus};
pub use recognition::{Candidate, CandidateSource, RecognitionPayload, RecognitionProtocolError};
pub use status::ServiceStatus;
pub use text::{TextAdapterState, TextAdapterSummary};

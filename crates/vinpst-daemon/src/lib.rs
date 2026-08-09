//! Daemon library pieces shared by the binary and integration tests.

pub mod dbus_service;
mod error_info;
pub mod remote;
pub mod runtime;

pub use dbus_service::VinpstDbusService;
pub use runtime::{RuntimeError, RuntimeState, StopRecordingReport};

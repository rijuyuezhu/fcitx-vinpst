//! Typed configuration contract for Vinpst.
//!
//! This crate owns schema parsing, field defaults, strict validation,
//! secret-safe diagnostics, user-path resolution, and atomic persistence so
//! CLI, daemon, and GUI callers share one configuration boundary.

mod config;
mod defaults;
mod diagnostics;
mod error;
mod persistence;
mod schema;
#[cfg(test)]
mod tests;
pub mod user_paths;
mod validation;

pub use diagnostics::redact_url_for_diagnostics;
pub use error::ConfigError;
pub use persistence::{
    ConfigWriteError, ConfigWriteReceipt, config_backup_path, resolve_symlink_write_target,
    write_config_file,
};
pub use schema::{
    AsrConfig, AsrProviderConfig, AsrProviderKind, COMMAND_SCENE_ID, CURRENT_CONFIG_VERSION,
    DEFAULT_SCENE_TIMEOUT_MS, GlobalConfig, LlmAdapterConfig, LlmConfig, LlmProviderConfig,
    RAW_SCENE_ID, RegistryConfig, SceneDefinition, ScenesConfig, VadConfig, VinpstConfig,
    VinpstConfigSummary,
};

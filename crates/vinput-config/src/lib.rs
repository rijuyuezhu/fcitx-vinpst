//! Configuration model and validation for vinput.
//!
//! The first implementation preserves the original `default-config.json` shape
//! and focuses on typed deserialization plus lightweight validation. Later
//! migrations can add versioned upgrades here without touching daemon code.

mod config;
mod defaults;
mod error;
mod schema;
#[cfg(test)]
mod tests;
mod validation;

pub use error::ConfigError;
pub use schema::{
    AsrConfig, AsrProviderConfig, AsrProviderKind, COMMAND_SCENE_ID, CURRENT_CONFIG_VERSION,
    GlobalConfig, LlmAdapterConfig, LlmConfig, LlmProviderConfig, RAW_SCENE_ID, RegistryConfig,
    SceneDefinition, ScenesConfig, VadConfig, VinputConfig, VinputConfigSummary,
};

use std::{fs, path::Path};

use serde::Deserialize;

#[derive(Deserialize)]
struct ConfigVersionEnvelope {
    version: u32,
}

use crate::{
    CURRENT_CONFIG_VERSION, ConfigError, SceneDefinition, VinpstConfig, VinpstConfigSummary,
    validation::validate_config,
};

impl VinpstConfig {
    /// Parses config from JSON.
    pub fn from_json_str(input: &str) -> Result<Self, ConfigError> {
        let envelope = serde_json::from_str::<ConfigVersionEnvelope>(input)?;
        validate_schema_version(envelope.version)?;
        Ok(serde_json::from_str::<Self>(input)?)
    }

    /// Reads and parses config from a JSON file.
    pub fn from_json_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let input = fs::read_to_string(path).map_err(|source| ConfigError::ReadFile {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_json_str(&input)
    }

    /// Parses the bundled upstream-compatible default config.
    pub fn bundled_default() -> Result<Self, ConfigError> {
        Self::from_json_str(include_str!("../../../data/default-config.json"))
    }

    /// Validates cross-field invariants that serde cannot express.
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.validate_schema_version()?;
        validate_config(self)?;
        Ok(())
    }

    fn validate_schema_version(&self) -> Result<(), ConfigError> {
        validate_schema_version(self.version)
    }

    /// Builds a compact summary for CLI and diagnostics.
    #[must_use]
    pub fn summary(&self) -> VinpstConfigSummary {
        VinpstConfigSummary {
            ok: true,
            version: self.version,
            active_scene: self.scenes.active_scene.clone(),
            active_provider: self.asr.active_provider.clone(),
            scene_count: self.scenes.definitions.len(),
            provider_count: self.asr.providers.len(),
            registry_mirror_count: self.registry.base_urls.len(),
        }
    }

    /// Returns the active scene definition, if it exists.
    #[must_use]
    pub fn active_scene(&self) -> Option<&SceneDefinition> {
        self.scenes
            .definitions
            .iter()
            .find(|scene| scene.id == self.scenes.active_scene)
    }
}

fn validate_schema_version(version: u32) -> Result<(), ConfigError> {
    if version != CURRENT_CONFIG_VERSION {
        return Err(ConfigError::UnsupportedSchemaVersion {
            found: version,
            supported: CURRENT_CONFIG_VERSION,
        });
    }
    Ok(())
}

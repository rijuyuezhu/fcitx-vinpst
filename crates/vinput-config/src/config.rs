use std::{fs, path::Path};

use crate::{
    CURRENT_CONFIG_VERSION, ConfigError, RAW_SCENE_ID, SceneDefinition, VinputConfig,
    VinputConfigSummary, defaults::ensure_builtin_scenes, validation::validate_config,
};

impl VinputConfig {
    /// Parses config from JSON.
    pub fn from_json_str(input: &str) -> Result<Self, ConfigError> {
        let config = serde_json::from_str::<Self>(input)?.normalized();
        config.validate_schema_version()?;
        Ok(config)
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

    /// Applies non-destructive defaults for optional sections.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        if self.version == 0 {
            self.version = 1;
        }
        if self.global.duck_output_volume.is_finite() {
            self.global.duck_output_volume = self.global.duck_output_volume.clamp(0.0, 1.0);
        }
        if self.scenes.active_scene.is_empty() {
            RAW_SCENE_ID.clone_into(&mut self.scenes.active_scene);
        }
        ensure_builtin_scenes(&mut self.scenes.definitions);
        self
    }

    /// Validates cross-field invariants that serde cannot express.
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.validate_schema_version()?;
        validate_config(self)?;
        Ok(())
    }

    fn validate_schema_version(&self) -> Result<(), ConfigError> {
        if self.version > CURRENT_CONFIG_VERSION {
            return Err(ConfigError::UnsupportedSchemaVersion {
                found: self.version,
                supported: CURRENT_CONFIG_VERSION,
            });
        }
        Ok(())
    }

    /// Builds a compact summary for CLI and diagnostics.
    #[must_use]
    pub fn summary(&self) -> VinputConfigSummary {
        VinputConfigSummary {
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

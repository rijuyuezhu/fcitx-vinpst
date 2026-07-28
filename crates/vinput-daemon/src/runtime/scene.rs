//! Scene state and active-scene persistence.

use vinput_config::{COMMAND_SCENE_ID, RAW_SCENE_ID};
use vinput_protocol::ServiceStatus;

use super::{RuntimeError, RuntimeState, config_io::persist_config_atomically};

/// Stable scene summary returned through the D-Bus extension.
pub(crate) type SceneStateTuple = (String, Vec<(String, String)>);

impl RuntimeState {
    /// Returns the current active scene and configured scene labels.
    pub(crate) fn scene_state(&self) -> SceneStateTuple {
        (
            self.config.scenes.active_scene.clone(),
            self.config
                .scenes
                .definitions
                .iter()
                .map(|scene| {
                    (
                        scene.id.clone(),
                        scene_display_label(&scene.id, &scene.label),
                    )
                })
                .collect(),
        )
    }

    /// Selects a configured scene and persists it when the daemon has an explicit config file.
    pub(crate) fn set_active_scene(&mut self, scene_id: &str) -> Result<bool, RuntimeError> {
        if self.status != ServiceStatus::Idle {
            return Err(RuntimeError::Busy(self.status));
        }
        if !self
            .config
            .scenes
            .definitions
            .iter()
            .any(|scene| scene.id == scene_id)
        {
            return Err(RuntimeError::UnknownScene(scene_id.to_owned()));
        }

        let mut updated = self.config.clone();
        scene_id.clone_into(&mut updated.scenes.active_scene);
        updated.validate().map_err(RuntimeError::InvalidConfig)?;

        let persisted = if let Some(path) = self.config_path.as_deref() {
            persist_config_atomically(path, &updated, "scene")?;
            true
        } else {
            false
        };
        scene_id.clone_into(&mut self.config.scenes.active_scene);
        Ok(persisted)
    }
}

fn scene_display_label(scene_id: &str, configured_label: &str) -> String {
    match configured_label {
        "__label_raw__" => "Raw".to_owned(),
        "__label_command__" => "Command".to_owned(),
        _ if configured_label.trim().is_empty() => match scene_id {
            RAW_SCENE_ID => "Raw".to_owned(),
            COMMAND_SCENE_ID => "Command".to_owned(),
            _ => scene_id.to_owned(),
        },
        _ => configured_label.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use vinput_asr::MockAsrBackend;
    use vinput_config::VinputConfig;

    #[test]
    fn scene_state_uses_display_labels_and_active_scene() {
        let runtime = RuntimeState::with_asr_backend(
            VinputConfig::bundled_default().unwrap(),
            Box::new(MockAsrBackend::buffered("final")),
        )
        .unwrap();

        let (active, scenes) = runtime.scene_state();
        assert_eq!(active, RAW_SCENE_ID);
        assert_eq!(
            scenes,
            [
                (RAW_SCENE_ID.to_owned(), "Raw".to_owned()),
                (COMMAND_SCENE_ID.to_owned(), "Command".to_owned()),
            ]
        );
    }

    #[test]
    fn active_scene_updates_runtime_without_explicit_config_path() {
        let mut runtime = RuntimeState::with_asr_backend(
            VinputConfig::bundled_default().unwrap(),
            Box::new(MockAsrBackend::buffered("final")),
        )
        .unwrap();

        assert!(!runtime.set_active_scene(COMMAND_SCENE_ID).unwrap());
        assert_eq!(runtime.scene_state().0, COMMAND_SCENE_ID);
    }

    #[test]
    fn active_scene_persists_explicit_config_atomically() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.json");
        let config = VinputConfig::bundled_default().unwrap();
        fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&config).unwrap()),
        )
        .unwrap();
        let mut runtime =
            RuntimeState::with_asr_backend(config, Box::new(MockAsrBackend::buffered("final")))
                .unwrap();
        runtime.set_config_path(Some(path.clone()));

        assert!(runtime.set_active_scene(COMMAND_SCENE_ID).unwrap());
        let persisted = VinputConfig::from_json_file(&path).unwrap();
        assert_eq!(persisted.scenes.active_scene, COMMAND_SCENE_ID);
        assert_eq!(runtime.scene_state().0, COMMAND_SCENE_ID);
    }

    #[test]
    fn active_scene_rejects_unknown_and_busy_changes() {
        let mut runtime = RuntimeState::with_asr_backend(
            VinputConfig::bundled_default().unwrap(),
            Box::new(MockAsrBackend::buffered("final")),
        )
        .unwrap();
        let error = runtime.set_active_scene("missing").unwrap_err();
        assert!(matches!(error, RuntimeError::UnknownScene(scene) if scene == "missing"));

        runtime.start_recording().unwrap();
        let error = runtime.set_active_scene(COMMAND_SCENE_ID).unwrap_err();
        assert!(matches!(
            error,
            RuntimeError::Busy(ServiceStatus::Recording)
        ));
    }
}

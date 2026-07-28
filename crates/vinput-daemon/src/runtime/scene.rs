//! Scene state and active-scene persistence.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use vinput_config::{COMMAND_SCENE_ID, RAW_SCENE_ID, VinputConfig};
use vinput_protocol::ServiceStatus;

use super::{RuntimeError, RuntimeState};

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
            persist_config_atomically(path, &updated)?;
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

fn persist_config_atomically(path: &Path, config: &VinputConfig) -> Result<(), RuntimeError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| RuntimeError::PersistConfig {
        path: path.to_path_buf(),
        source,
    })?;

    let contents = serde_json::to_string_pretty(config).map_err(RuntimeError::SerializeConfig)?;
    let temp_path = temporary_config_path(path);
    let write_result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .map_err(|source| RuntimeError::PersistConfig {
                path: temp_path.clone(),
                source,
            })?;
        if let Ok(metadata) = fs::metadata(path) {
            file.set_permissions(metadata.permissions())
                .map_err(|source| RuntimeError::PersistConfig {
                    path: temp_path.clone(),
                    source,
                })?;
        }
        file.write_all(contents.as_bytes())
            .and_then(|()| file.write_all(b"\n"))
            .and_then(|()| file.sync_all())
            .map_err(|source| RuntimeError::PersistConfig {
                path: temp_path.clone(),
                source,
            })?;
        fs::rename(&temp_path, path).map_err(|source| RuntimeError::PersistConfig {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result
}

fn temporary_config_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.json");
    path.with_file_name(format!(".{file_name}.scene-{}.tmp", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vinput_asr::MockAsrBackend;

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

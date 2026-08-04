//! Conflict-aware hotword content loading and atomic persistence.

use std::{
    fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use vinput_config::VinputConfig;

use crate::{
    ConfigDocument, ConfigSaveOutcome, ensure_config_save_allowed, query_daemon_snapshot,
    reload_asr_backend, save_updated_config_with_daemon,
};

const MAX_HOTWORD_FILE_BYTES: usize = 1024 * 1024;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct HotwordContentSnapshot {
    pub(super) existed: bool,
    pub(super) content: String,
}

impl fmt::Debug for HotwordContentSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HotwordContentSnapshot")
            .field("existed", &self.existed)
            .field("content", &"<redacted>")
            .finish()
    }
}

pub(super) fn save_hotword_path_with_daemon(
    document: &ConfigDocument,
    updated: &VinputConfig,
    prerequisite_path: Option<&Path>,
) -> Result<ConfigSaveOutcome, String> {
    with_prepared_hotword_file(prerequisite_path, || {
        save_updated_config_with_daemon(document, updated)
    })
}

fn with_prepared_hotword_file<T>(
    path: Option<&Path>,
    action: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let created_path = prepare_missing_hotword_file(path)?;
    match action() {
        Ok(outcome) => Ok(outcome),
        Err(error) => match rollback_created_hotword_file(created_path.as_deref()) {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(format!(
                "{error} Removing the hotword file created for this update also failed: {rollback_error}"
            )),
        },
    }
}

fn prepare_missing_hotword_file(path: Option<&Path>) -> Result<Option<PathBuf>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    if read_hotword_snapshot(path)?.existed {
        return Ok(None);
    }
    atomic_write_hotword_file(path, b"")?;
    Ok(Some(path.to_path_buf()))
}

fn rollback_created_hotword_file(path: Option<&Path>) -> Result<(), String> {
    let Some(path) = path else {
        return Ok(());
    };
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Remove newly created hotword file: {error}")),
    }
}

pub(super) fn read_hotword_snapshot(path: &Path) -> Result<HotwordContentSnapshot, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(
                "Configured hotword file is a symbolic link; edit it externally instead."
                    .to_owned(),
            );
        }
        Ok(metadata) if !metadata.is_file() => {
            return Err("Configured hotword path is not a regular file.".to_owned());
        }
        Ok(metadata) if metadata.len() > MAX_HOTWORD_FILE_BYTES as u64 => {
            return Err(format!(
                "Configured hotword file exceeds the {MAX_HOTWORD_FILE_BYTES}-byte GUI limit."
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(HotwordContentSnapshot {
                existed: false,
                content: String::new(),
            });
        }
        Err(error) => return Err(format!("Inspect configured hotword file: {error}")),
    }
    let bytes = fs::read(path).map_err(|error| format!("Read configured hotword file: {error}"))?;
    if bytes.len() > MAX_HOTWORD_FILE_BYTES {
        return Err(format!(
            "Configured hotword file exceeds the {MAX_HOTWORD_FILE_BYTES}-byte GUI limit."
        ));
    }
    let content = String::from_utf8(bytes)
        .map_err(|_| "Configured hotword file is not valid UTF-8.".to_owned())?;
    Ok(HotwordContentSnapshot {
        existed: true,
        content,
    })
}

pub(super) fn save_hotword_content_with_daemon(
    path: &Path,
    expected: &HotwordContentSnapshot,
    content: &str,
    validate_target: impl FnOnce() -> Result<(), String>,
) -> Result<String, String> {
    let daemon = query_daemon_snapshot();
    if let Ok(snapshot) = &daemon {
        ensure_config_save_allowed(snapshot)?;
    }
    validate_target()?;
    save_hotword_content_with_reload(path, expected, content, || match daemon {
        Ok(_) => reload_asr_backend().map(|()| "daemon config reload requested".to_owned()),
        Err(error) => Ok(format!("daemon reload skipped: {error}")),
    })
}

pub(super) fn save_hotword_content_with_reload(
    path: &Path,
    expected: &HotwordContentSnapshot,
    content: &str,
    reload: impl FnOnce() -> Result<String, String>,
) -> Result<String, String> {
    let current = read_hotword_snapshot(path)?;
    if &current != expected {
        return Err(
            "Configured hotword file changed outside the GUI; reload it before saving.".to_owned(),
        );
    }
    atomic_write_hotword_file(path, content.as_bytes())?;
    match reload() {
        Ok(reload_summary) => Ok(format!("Hotword content saved; {reload_summary}")),
        Err(error) => {
            let rollback = if expected.existed {
                atomic_write_hotword_file(path, expected.content.as_bytes())
            } else {
                match fs::remove_file(path) {
                    Ok(()) => Ok(()),
                    Err(remove_error) if remove_error.kind() == io::ErrorKind::NotFound => Ok(()),
                    Err(remove_error) => {
                        Err(format!("Remove newly created hotword file: {remove_error}"))
                    }
                }
            };
            Err(match rollback {
                Ok(()) => format!(
                    "Daemon reload failed after saving hotwords: {error}; previous content restored."
                ),
                Err(rollback_error) => format!(
                    "Daemon reload failed after saving hotwords: {error}; restoring previous content also failed: {rollback_error}"
                ),
            })
        }
    }
}

fn atomic_write_hotword_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > MAX_HOTWORD_FILE_BYTES {
        return Err(format!(
            "Hotword content exceeds the {MAX_HOTWORD_FILE_BYTES}-byte GUI limit."
        ));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| format!("Create hotword directory: {error}"))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| "Hotword path must name a file.".to_owned())?;
    let existing_permissions = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(
                "Configured hotword file is a symbolic link; edit it externally instead."
                    .to_owned(),
            );
        }
        Ok(metadata) if !metadata.is_file() => {
            return Err("Configured hotword path is not a regular file.".to_owned());
        }
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("Inspect configured hotword file: {error}")),
    };
    let mut temporary_path = None;
    let mut temporary_file = None;
    for attempt in 0..16_u8 {
        let candidate = parent.join(format!(
            ".{}.vinput-hotword-{}-{attempt}.tmp",
            file_name.to_string_lossy(),
            std::process::id()
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                temporary_path = Some(candidate);
                temporary_file = Some(file);
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(format!("Create temporary hotword file: {error}")),
        }
    }
    let temporary_path =
        temporary_path.ok_or_else(|| "Could not allocate a temporary hotword file.".to_owned())?;
    let mut temporary_file = temporary_file.expect("temporary path and file are created together");
    let result = (|| -> Result<(), String> {
        if let Some(permissions) = existing_permissions {
            temporary_file
                .set_permissions(permissions)
                .map_err(|error| format!("Preserve hotword file permissions: {error}"))?;
        }
        temporary_file
            .write_all(bytes)
            .and_then(|()| temporary_file.sync_all())
            .map_err(|error| format!("Write temporary hotword file: {error}"))?;
        drop(temporary_file);
        fs::rename(&temporary_path, path)
            .map_err(|error| format!("Publish hotword file atomically: {error}"))?;
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("Synchronize hotword directory: {error}"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_prerequisite_exists_before_commit() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("hotwords.txt");

        with_prepared_hotword_file(Some(&path), || {
            assert_eq!(fs::read_to_string(&path).expect("prepared file"), "");
            Ok(())
        })
        .expect("commit prepared file");

        assert!(path.is_file());
    }

    #[test]
    fn failed_commit_removes_only_new_prerequisite() {
        let directory = tempfile::tempdir().expect("temp dir");
        let missing = directory.path().join("missing.txt");
        let existing = directory.path().join("existing.txt");
        fs::write(&existing, "keep\n").expect("existing fixture");

        let missing_error = with_prepared_hotword_file(Some(&missing), || {
            assert!(missing.is_file());
            Err::<(), _>("fixture commit failure".to_owned())
        })
        .expect_err("rollback missing prerequisite");
        assert_eq!(missing_error, "fixture commit failure");
        assert!(!missing.exists());

        let existing_error = with_prepared_hotword_file(Some(&existing), || {
            Err::<(), _>("fixture commit failure".to_owned())
        })
        .expect_err("preserve existing prerequisite");
        assert_eq!(existing_error, "fixture commit failure");
        assert_eq!(
            fs::read_to_string(&existing).expect("existing content"),
            "keep\n"
        );
    }
}

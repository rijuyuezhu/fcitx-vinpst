//! Conflict-aware hotword content loading and atomic persistence.

use std::{
    fmt, fs,
    io::{self, Write},
    path::Path,
};

use vinput_config::VinputConfig;

use crate::{
    ConfigDocument, ConfigSaveOutcome, ensure_config_save_allowed,
    hotword_management::HotwordContentSaveOutcome, query_daemon_snapshot,
    reload_asr_backend_and_wait, save_updated_config_with_daemon,
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
    let created = prepare_missing_hotword_file(path)?;
    match action() {
        Ok(outcome) => Ok(outcome),
        Err(error) if created => Err(format!(
            "{error} The hotword file prepared for this update was preserved because rollback cannot safely exclude concurrent external changes."
        )),
        Err(error) => Err(error),
    }
}

fn prepare_missing_hotword_file(path: Option<&Path>) -> Result<bool, String> {
    let Some(path) = path else {
        return Ok(false);
    };
    if read_hotword_snapshot(path)?.existed {
        return Ok(false);
    }
    atomic_write_hotword_file(path, b"")?;
    Ok(true)
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
    active_provider_id: Option<&str>,
    validate_target: impl FnOnce() -> Result<(), String>,
) -> Result<HotwordContentSaveOutcome, String> {
    let daemon = query_daemon_snapshot();
    if let Ok(snapshot) = &daemon {
        ensure_config_save_allowed(snapshot)?;
    }
    validate_target()?;
    match active_provider_id {
        None => {
            save_hotword_content_with_reload(path, expected, content, || {
                Ok("inactive provider file updated; it will be used when the provider is activated".to_owned())
            })
        }
        Some(provider_id) => {
            save_hotword_content_with_reload(path, expected, content, || match daemon {
                Ok(_) => reload_asr_backend_and_wait(provider_id),
                Err(_) => Err(
                    "No reachable daemon was available to apply the saved hotword update."
                        .to_owned(),
                ),
            })
        }
    }
}

pub(super) fn save_hotword_content_with_reload(
    path: &Path,
    expected: &HotwordContentSnapshot,
    content: &str,
    reload: impl FnOnce() -> Result<String, String>,
) -> Result<HotwordContentSaveOutcome, String> {
    let current = read_hotword_snapshot(path)?;
    if &current != expected {
        return Err(
            "Configured hotword file changed outside the GUI; reload it before saving.".to_owned(),
        );
    }
    atomic_write_hotword_file(path, content.as_bytes())?;
    let reload = reload();
    let final_snapshot = read_hotword_snapshot(path);
    let mut activation_errors = Vec::new();
    if let Err(error) = &reload {
        activation_errors.push(error.clone());
    }
    let baseline = if let Ok(snapshot) = final_snapshot {
        if !snapshot.existed || snapshot.content != content {
            activation_errors.push(
                "The hotword file changed while the daemon was applying the update; reload the file before editing it again."
                    .to_owned(),
            );
        }
        Some(snapshot)
    } else {
        activation_errors.push(
            "The saved hotword file could not be verified after the daemon reload; reload the file before editing it again."
                .to_owned(),
        );
        None
    };
    let activation_error = (!activation_errors.is_empty()).then(|| {
        format!(
            "{} Automatic file rollback was skipped to avoid overwriting concurrent external updates.",
            activation_errors.join(" ")
        )
    });
    let summary = match reload {
        Ok(reload_summary) if activation_error.is_none() => {
            format!("Hotword content saved; {reload_summary}.")
        }
        Ok(_) | Err(_) => "Hotword content was saved to disk.".to_owned(),
    };
    Ok(HotwordContentSaveOutcome {
        summary,
        activation_error,
        baseline,
    })
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
    fn failed_commit_preserves_prepared_and_existing_files() {
        let directory = tempfile::tempdir().expect("temp dir");
        let missing = directory.path().join("missing.txt");
        let modified = directory.path().join("modified.txt");
        let existing = directory.path().join("existing.txt");
        fs::write(&existing, "keep\n").expect("existing fixture");

        let missing_error = with_prepared_hotword_file(Some(&missing), || {
            assert!(missing.is_file());
            Err::<(), _>("fixture commit failure".to_owned())
        })
        .expect_err("preserve missing prerequisite");
        assert!(missing_error.contains("prepared for this update was preserved"));
        assert_eq!(fs::read_to_string(&missing).expect("prepared content"), "");

        let modified_error = with_prepared_hotword_file(Some(&modified), || {
            fs::write(&modified, "external\n").expect("modify prepared file");
            Err::<(), _>("fixture commit failure".to_owned())
        })
        .expect_err("preserve externally modified prerequisite");
        assert!(modified_error.contains("prepared for this update was preserved"));
        assert_eq!(
            fs::read_to_string(&modified).expect("modified content"),
            "external\n"
        );

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
    #[test]
    fn content_save_is_atomic_conflict_aware_and_never_rolls_back_after_reload_failure() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("hotwords.txt");
        fs::write(&path, "alpha\n").expect("write fixture");
        let baseline = read_hotword_snapshot(&path).expect("read baseline");

        let outcome = save_hotword_content_with_reload(&path, &baseline, "beta\n", || {
            Ok("fixture reload".to_owned())
        })
        .expect("save content");
        assert!(outcome.summary.contains("fixture reload"));
        assert_eq!(outcome.activation_error, None);
        assert_eq!(
            outcome
                .baseline
                .as_ref()
                .map(|snapshot| snapshot.content.as_str()),
            Some("beta\n")
        );
        assert_eq!(fs::read_to_string(&path).expect("saved content"), "beta\n");

        let loaded = read_hotword_snapshot(&path).expect("read saved content");
        fs::write(&path, "external\n").expect("external update");
        let conflict = save_hotword_content_with_reload(&path, &loaded, "gamma\n", || {
            Ok("unreachable reload".to_owned())
        })
        .expect_err("reject external update");
        assert!(conflict.contains("changed outside"));
        assert_eq!(
            fs::read_to_string(&path).expect("external content"),
            "external\n"
        );

        let external = read_hotword_snapshot(&path).expect("read external content");
        let reload_outcome = save_hotword_content_with_reload(&path, &external, "delta\n", || {
            Err("fixture reload failure".to_owned())
        })
        .expect("preserve published content after reload failure");
        assert!(
            reload_outcome
                .activation_error
                .as_deref()
                .is_some_and(|error| error.contains("rollback was skipped"))
        );
        assert_eq!(
            fs::read_to_string(&path).expect("published content"),
            "delta\n"
        );

        let concurrent_baseline = read_hotword_snapshot(&path).expect("read concurrent baseline");
        let concurrent_outcome =
            save_hotword_content_with_reload(&path, &concurrent_baseline, "gui-write\n", || {
                fs::write(&path, "concurrent-write\n").expect("concurrent update");
                Err("fixture reload failure".to_owned())
            })
            .expect("preserve concurrent reload-window update");
        assert!(concurrent_outcome.activation_error.is_some());
        assert_eq!(
            concurrent_outcome
                .baseline
                .as_ref()
                .map(|snapshot| snapshot.content.as_str()),
            Some("concurrent-write\n")
        );
        assert_eq!(
            fs::read_to_string(&path).expect("concurrent content"),
            "concurrent-write\n"
        );

        let same_content_baseline =
            read_hotword_snapshot(&path).expect("read same-content baseline");
        let replacement = directory.path().join("replacement.txt");
        let same_content_outcome = save_hotword_content_with_reload(
            &path,
            &same_content_baseline,
            "gui-same-content\n",
            || {
                fs::write(&replacement, "gui-same-content\n").expect("replacement content");
                fs::rename(&replacement, &path).expect("replace with same content");
                Err("fixture reload failure".to_owned())
            },
        )
        .expect("detect same-content external replacement");
        assert!(same_content_outcome.activation_error.is_some());
        assert_eq!(
            fs::read_to_string(&path).expect("same replacement content"),
            "gui-same-content\n"
        );

        let missing_path = directory.path().join("new-hotwords.txt");
        let missing_baseline = read_hotword_snapshot(&missing_path).expect("read missing baseline");
        let missing_outcome = save_hotword_content_with_reload(
            &missing_path,
            &missing_baseline,
            "gui-create\n",
            || {
                fs::write(&missing_path, "concurrent-create\n").expect("concurrent create");
                Err("fixture reload failure".to_owned())
            },
        )
        .expect("preserve concurrent creation");
        assert!(missing_outcome.activation_error.is_some());
        assert_eq!(
            fs::read_to_string(&missing_path).expect("concurrent created content"),
            "concurrent-create\n"
        );
    }
}

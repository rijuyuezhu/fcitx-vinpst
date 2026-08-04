//! Conflict-aware hotword content loading and atomic persistence.

use std::{
    fmt, fs,
    io::{self, Write},
    os::unix::fs::MetadataExt,
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

#[derive(Clone, Copy, PartialEq, Eq)]
struct HotwordFileStamp {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

impl HotwordFileStamp {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.size(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_nanoseconds: metadata.ctime_nsec(),
        }
    }
}

struct PublishedHotwordFile {
    path: PathBuf,
    content: String,
    stamp: HotwordFileStamp,
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
    let created_file = prepare_missing_hotword_file(path)?;
    match action() {
        Ok(outcome) => Ok(outcome),
        Err(error) => match rollback_created_hotword_file(created_file.as_ref()) {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(format!(
                "{error} Removing the hotword file created for this update also failed: {rollback_error}"
            )),
        },
    }
}

fn prepare_missing_hotword_file(
    path: Option<&Path>,
) -> Result<Option<PublishedHotwordFile>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    if read_hotword_snapshot(path)?.existed {
        return Ok(None);
    }
    atomic_write_hotword_file(path, b"")?;
    capture_published_hotword_file(path, "").map(Some)
}

fn rollback_created_hotword_file(file: Option<&PublishedHotwordFile>) -> Result<(), String> {
    let Some(file) = file else {
        return Ok(());
    };
    if !published_hotword_file_is_current(file)? {
        return Err(
            "The prepared hotword file changed while the daemon was reloading; current content was preserved and rollback was skipped."
                .to_owned(),
        );
    }
    match fs::remove_file(&file.path) {
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
    let published = capture_published_hotword_file(path, content)?;
    match reload() {
        Ok(reload_summary) => Ok(format!("Hotword content saved; {reload_summary}")),
        Err(error) => {
            match published_hotword_file_is_current(&published) {
                Ok(true) => {}
                Ok(false) => {
                    return Err(format!(
                        "Daemon reload failed after saving hotwords: {error}; the hotword file changed again during reload, so current content was preserved and rollback was skipped."
                    ));
                }
                Err(verify_error) => {
                    return Err(format!(
                        "Daemon reload failed after saving hotwords: {error}; {verify_error}"
                    ));
                }
            }
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

fn capture_published_hotword_file(
    path: &Path,
    expected_content: &str,
) -> Result<PublishedHotwordFile, String> {
    let snapshot = read_hotword_snapshot(path)?;
    if !snapshot.existed || snapshot.content != expected_content {
        return Err(
            "The hotword file changed immediately after publication; current content was preserved."
                .to_owned(),
        );
    }
    let metadata = regular_hotword_metadata(path)?;
    Ok(PublishedHotwordFile {
        path: path.to_path_buf(),
        content: expected_content.to_owned(),
        stamp: HotwordFileStamp::from_metadata(&metadata),
    })
}

fn published_hotword_file_is_current(file: &PublishedHotwordFile) -> Result<bool, String> {
    let snapshot = match read_hotword_snapshot(&file.path) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            return Err(format!(
                "Could not verify the hotword file before rollback: {error}; rollback was skipped."
            ));
        }
    };
    if !snapshot.existed || snapshot.content != file.content {
        return Ok(false);
    }
    let metadata = match regular_hotword_metadata(&file.path) {
        Ok(metadata) => metadata,
        Err(error) => {
            return Err(format!(
                "Could not verify the hotword file version before rollback: {error}; rollback was skipped."
            ));
        }
    };
    Ok(HotwordFileStamp::from_metadata(&metadata) == file.stamp)
}

fn regular_hotword_metadata(path: &Path) -> Result<fs::Metadata, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Inspect published hotword file: {error}"))?;
    if metadata.file_type().is_symlink() {
        return Err("Published hotword file became a symbolic link.".to_owned());
    }
    if !metadata.is_file() {
        return Err("Published hotword path is no longer a regular file.".to_owned());
    }
    Ok(metadata)
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
        let modified = directory.path().join("modified.txt");
        let existing = directory.path().join("existing.txt");
        fs::write(&existing, "keep\n").expect("existing fixture");

        let missing_error = with_prepared_hotword_file(Some(&missing), || {
            assert!(missing.is_file());
            Err::<(), _>("fixture commit failure".to_owned())
        })
        .expect_err("rollback missing prerequisite");
        assert_eq!(missing_error, "fixture commit failure");
        assert!(!missing.exists());

        let modified_error = with_prepared_hotword_file(Some(&modified), || {
            fs::write(&modified, "external\n").expect("modify prepared file");
            Err::<(), _>("fixture commit failure".to_owned())
        })
        .expect_err("preserve externally modified prerequisite");
        assert!(modified_error.contains("rollback was skipped"));
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
    fn content_save_is_atomic_conflict_aware_and_rolls_back_reload_failures() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("hotwords.txt");
        fs::write(&path, "alpha\n").expect("write fixture");
        let baseline = read_hotword_snapshot(&path).expect("read baseline");

        let summary = save_hotword_content_with_reload(&path, &baseline, "beta\n", || {
            Ok("fixture reload".to_owned())
        })
        .expect("save content");
        assert!(summary.contains("fixture reload"));
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
        let reload_error = save_hotword_content_with_reload(&path, &external, "delta\n", || {
            Err("fixture reload failure".to_owned())
        })
        .expect_err("rollback reload failure");
        assert!(reload_error.contains("previous content restored"));
        assert_eq!(
            fs::read_to_string(&path).expect("restored content"),
            "external\n"
        );

        let concurrent_baseline = read_hotword_snapshot(&path).expect("read concurrent baseline");
        let concurrent_error =
            save_hotword_content_with_reload(&path, &concurrent_baseline, "gui-write\n", || {
                fs::write(&path, "concurrent-write\n").expect("concurrent update");
                Err("fixture reload failure".to_owned())
            })
            .expect_err("preserve concurrent reload-window update");
        assert!(concurrent_error.contains("rollback was skipped"));
        assert_eq!(
            fs::read_to_string(&path).expect("concurrent content"),
            "concurrent-write\n"
        );

        let same_content_baseline =
            read_hotword_snapshot(&path).expect("read same-content baseline");
        let replacement = directory.path().join("replacement.txt");
        let same_content_error = save_hotword_content_with_reload(
            &path,
            &same_content_baseline,
            "gui-same-content\n",
            || {
                fs::write(&replacement, "gui-same-content\n").expect("replacement content");
                fs::rename(&replacement, &path).expect("replace with same content");
                Err("fixture reload failure".to_owned())
            },
        )
        .expect_err("detect same-content external replacement");
        assert!(same_content_error.contains("rollback was skipped"));
        assert_eq!(
            fs::read_to_string(&path).expect("same replacement content"),
            "gui-same-content\n"
        );

        let missing_path = directory.path().join("new-hotwords.txt");
        let missing_baseline = read_hotword_snapshot(&missing_path).expect("read missing baseline");
        let missing_error = save_hotword_content_with_reload(
            &missing_path,
            &missing_baseline,
            "gui-create\n",
            || {
                fs::write(&missing_path, "concurrent-create\n").expect("concurrent create");
                Err("fixture reload failure".to_owned())
            },
        )
        .expect_err("preserve concurrent creation");
        assert!(missing_error.contains("rollback was skipped"));
        assert_eq!(
            fs::read_to_string(&missing_path).expect("concurrent created content"),
            "concurrent-create\n"
        );
    }
}

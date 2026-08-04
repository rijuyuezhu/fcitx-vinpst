//! Conflict-aware hotword content loading and atomic persistence.

use std::{
    fmt, fs,
    io::{self, Read, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use rustix::{
    fs::{CWD, RenameFlags, renameat_with},
    io::Errno,
};
use vinput_config::VinputConfig;
use xattr::FileExt as _;

use crate::{
    ConfigDocument, ConfigSaveOutcome, ensure_config_save_allowed,
    hotword_management::HotwordContentSaveOutcome, query_daemon_snapshot,
    reload_asr_backend_and_wait, save_updated_config_with_daemon,
};

const MAX_HOTWORD_FILE_BYTES: usize = 1024 * 1024;
static NEXT_HOTWORD_FILE_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct HotwordFileVersion {
    device: u64,
    inode: u64,
    size: u64,
    uid: u32,
    gid: u32,
    mode: u32,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

impl HotwordFileVersion {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.size(),
            uid: metadata.uid(),
            gid: metadata.gid(),
            mode: metadata.mode(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_nanoseconds: metadata.ctime_nsec(),
        }
    }

    fn matches_after_claim(self, expected: Self) -> bool {
        self.device == expected.device
            && self.inode == expected.inode
            && self.size == expected.size
            && self.uid == expected.uid
            && self.gid == expected.gid
            && self.mode == expected.mode
            && self.modified_seconds == expected.modified_seconds
            && self.modified_nanoseconds == expected.modified_nanoseconds
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct HotwordContentSnapshot {
    pub(super) existed: bool,
    pub(super) content: String,
    pub(super) version: Option<HotwordFileVersion>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HotwordPublishOutcome {
    previous_version_preserved: bool,
    durability_error: Option<String>,
}

impl fmt::Debug for HotwordContentSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HotwordContentSnapshot")
            .field("existed", &self.existed)
            .field("content", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl HotwordContentSnapshot {
    fn matches_after_claim(&self, expected: &Self) -> bool {
        self.existed == expected.existed
            && self.content == expected.content
            && self
                .version
                .zip(expected.version)
                .is_some_and(|(current, loaded)| current.matches_after_claim(loaded))
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

pub(super) fn ensure_hotword_path_update_current(
    path: &Path,
    updated: &VinputConfig,
) -> Result<(), String> {
    let persisted = ConfigDocument {
        path: path.to_path_buf(),
        from_disk: true,
        config: updated.clone(),
    };
    crate::ensure_config_document_current(&persisted).map_err(|error| {
        format!(
            "{error} The daemon application result was not accepted because the saved hotword path configuration changed during reload."
        )
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
    let metadata = match fs::symlink_metadata(path) {
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
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(HotwordContentSnapshot {
                existed: false,
                content: String::new(),
                version: None,
            });
        }
        Err(error) => return Err(format!("Inspect configured hotword file: {error}")),
    };
    let version = HotwordFileVersion::from_metadata(&metadata);
    let file =
        fs::File::open(path).map_err(|error| format!("Open configured hotword file: {error}"))?;
    let opened_version = HotwordFileVersion::from_metadata(
        &file
            .metadata()
            .map_err(|error| format!("Inspect opened hotword file: {error}"))?,
    );
    if opened_version != version {
        return Err(
            "Configured hotword file changed while it was being opened; reload it before editing."
                .to_owned(),
        );
    }
    ensure_no_hotword_extended_attributes(&file)?;
    let mut bytes = Vec::new();
    file.take((MAX_HOTWORD_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Read configured hotword file: {error}"))?;
    if bytes.len() > MAX_HOTWORD_FILE_BYTES {
        return Err(format!(
            "Configured hotword file exceeds the {MAX_HOTWORD_FILE_BYTES}-byte GUI limit."
        ));
    }
    let final_version = HotwordFileVersion::from_metadata(
        &fs::symlink_metadata(path)
            .map_err(|error| format!("Reinspect configured hotword file: {error}"))?,
    );
    if final_version != version {
        return Err(
            "Configured hotword file changed while it was being read; reload it before editing."
                .to_owned(),
        );
    }
    let content = String::from_utf8(bytes)
        .map_err(|_| "Configured hotword file is not valid UTF-8.".to_owned())?;
    Ok(HotwordContentSnapshot {
        existed: true,
        content,
        version: Some(version),
    })
}

pub(super) fn save_hotword_content_with_daemon(
    path: &Path,
    expected: &HotwordContentSnapshot,
    content: &str,
    active_provider_id: Option<&str>,
    mut validate_target: impl FnMut() -> Result<(), String>,
) -> Result<HotwordContentSaveOutcome, String> {
    let daemon = query_daemon_snapshot();
    if let Ok(snapshot) = &daemon {
        ensure_config_save_allowed(snapshot)?;
    }
    validate_target()?;
    let mut outcome = match active_provider_id {
        None => {
            save_hotword_content_with_reload(path, expected, content, || {
                Ok("inactive provider file updated; it will be used when the provider is activated".to_owned())
            })?
        }
        Some(provider_id) => {
            save_hotword_content_with_reload(path, expected, content, || match daemon {
                Ok(_) => reload_asr_backend_and_wait(provider_id),
                Err(_) => Err(
                    "No reachable daemon was available to apply the saved hotword update."
                        .to_owned(),
                ),
            })?
        }
    };
    if let Err(error) = validate_target() {
        append_activation_error(
            &mut outcome,
            format!(
                "{error} The daemon application result was not accepted because the configured hotword target changed after publication."
            ),
        );
    }
    Ok(outcome)
}

pub(super) fn save_hotword_content_with_reload(
    path: &Path,
    expected: &HotwordContentSnapshot,
    content: &str,
    reload: impl FnOnce() -> Result<String, String>,
) -> Result<HotwordContentSaveOutcome, String> {
    let publish = compare_and_swap_hotword_file(path, expected, content.as_bytes(), || {})?;
    let published_snapshot = read_hotword_snapshot(path);
    let reload = reload();
    let reload_failed = reload.is_err();
    let final_snapshot = read_hotword_snapshot(path);
    let mut activation_errors = publish.durability_error.into_iter().collect::<Vec<_>>();
    if published_snapshot.is_err() {
        activation_errors.push(
            "The published hotword file could not be versioned before daemon activation; reload the file before retrying activation."
                .to_owned(),
        );
    }
    if let Err(error) = &reload {
        activation_errors.push(error.clone());
    }
    let baseline = match (&published_snapshot, &final_snapshot) {
        (Ok(published), Ok(final_snapshot))
            if published == final_snapshot
                && final_snapshot.existed
                && final_snapshot.content == content =>
        {
            Some(final_snapshot.clone())
        }
        (Ok(_), Ok(_)) => {
            activation_errors.push(
                "The hotword file changed while the daemon was applying the update; reload the file before editing it again."
                    .to_owned(),
            );
            None
        }
        (_, Err(_)) => {
            activation_errors.push(
                "The saved hotword file could not be verified after the daemon reload; reload the file before editing it again."
                    .to_owned(),
            );
            None
        }
        (Err(_), Ok(_)) => None,
    };
    let retry_activation = reload_failed && baseline.is_some();
    let activation_error = (!activation_errors.is_empty()).then(|| {
        format!(
            "{} Automatic file rollback was skipped to avoid overwriting concurrent external updates.",
            activation_errors.join(" ")
        )
    });
    let recovery_summary = if publish.previous_version_preserved {
        " The previous version was preserved as an adjacent recovery file."
    } else {
        ""
    };
    let summary = match reload {
        Ok(reload_summary) if activation_error.is_none() => {
            format!("Hotword content saved; {reload_summary}.{recovery_summary}")
        }
        Ok(_) | Err(_) => format!("Hotword content was saved to disk.{recovery_summary}"),
    };
    Ok(HotwordContentSaveOutcome {
        summary,
        activation_error,
        baseline,
        retry_activation,
    })
}

fn append_activation_error(outcome: &mut HotwordContentSaveOutcome, error: String) {
    outcome.retry_activation = false;
    outcome.activation_error = Some(match outcome.activation_error.take() {
        Some(existing) => format!("{existing} {error}"),
        None => error,
    });
    "Hotword content was saved to disk.".clone_into(&mut outcome.summary);
}

fn atomic_write_hotword_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let expected = read_hotword_snapshot(path)?;
    compare_and_swap_hotword_file(path, &expected, bytes, || {}).map(|_| ())
}

fn ensure_no_hotword_extended_attributes(file: &fs::File) -> Result<(), String> {
    let mut attributes = file
        .list_xattr()
        .map_err(|error| format!("Inspect hotword file extended attributes: {error}"))?;
    if attributes.next().is_some() {
        return Err(
            "Configured hotword file has extended attributes or ACL metadata that the GUI cannot safely preserve; edit it externally instead."
                .to_owned(),
        );
    }
    Ok(())
}

fn prepare_temporary_hotword_metadata(
    temporary_file: &fs::File,
    expected: &HotwordContentSnapshot,
) -> Result<(), String> {
    ensure_no_hotword_extended_attributes(temporary_file)?;
    if !expected.existed {
        return Ok(());
    }
    let expected_version = expected.version.ok_or_else(|| {
        "Loaded hotword file metadata is unavailable; reload it before saving.".to_owned()
    })?;
    let metadata = temporary_file
        .metadata()
        .map_err(|error| format!("Inspect temporary hotword file metadata: {error}"))?;
    if metadata.uid() != expected_version.uid || metadata.gid() != expected_version.gid {
        return Err(
            "Configured hotword file ownership cannot be preserved by an atomic GUI replacement; edit it externally instead."
                .to_owned(),
        );
    }
    Ok(())
}

fn finalize_temporary_hotword_metadata(
    temporary_file: &fs::File,
    expected: &HotwordContentSnapshot,
) -> Result<(), String> {
    ensure_no_hotword_extended_attributes(temporary_file)?;
    if !expected.existed {
        return Ok(());
    }
    let expected_version = expected.version.ok_or_else(|| {
        "Loaded hotword file metadata is unavailable; reload it before saving.".to_owned()
    })?;
    temporary_file
        .set_permissions(fs::Permissions::from_mode(expected_version.mode & 0o7777))
        .map_err(|error| format!("Preserve hotword file mode after writing: {error}"))?;
    let prepared = temporary_file
        .metadata()
        .map_err(|error| format!("Verify temporary hotword file metadata: {error}"))?;
    if prepared.uid() != expected_version.uid
        || prepared.gid() != expected_version.gid
        || prepared.mode() & 0o7777 != expected_version.mode & 0o7777
    {
        return Err(
            "Configured hotword file ownership or mode could not be preserved exactly after writing; edit it externally instead."
                .to_owned(),
        );
    }
    Ok(())
}

fn compare_and_swap_hotword_file(
    path: &Path,
    expected: &HotwordContentSnapshot,
    bytes: &[u8],
    before_claim: impl FnOnce(),
) -> Result<HotwordPublishOutcome, String> {
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
    let (temporary_path, mut temporary_file) = create_sibling_file(parent, file_name, "tmp")?;
    let result = (|| -> Result<HotwordPublishOutcome, String> {
        prepare_temporary_hotword_metadata(&temporary_file, expected)?;
        temporary_file
            .write_all(bytes)
            .map_err(|error| format!("Write temporary hotword file: {error}"))?;
        finalize_temporary_hotword_metadata(&temporary_file, expected)?;
        temporary_file
            .sync_all()
            .map_err(|error| format!("Synchronize temporary hotword file: {error}"))?;
        drop(temporary_file);
        before_claim();
        if !expected.existed {
            publish_noreplace(&temporary_path, path).map_err(|error| {
                if error == Errno::EXIST {
                    "Configured hotword file was created outside the GUI before publication; the external file was preserved."
                        .to_owned()
                } else {
                    format!("Publish new hotword file without replacement: {error}")
                }
            })?;
            return Ok(finish_published_hotword(parent, false, sync_directory));
        }
        let recovery_path = claim_current_hotword_file(path, parent, file_name)?;
        synchronize_claim_or_restore(&recovery_path, path, parent, sync_directory)?;
        let claimed = read_claimed_hotword_or_restore(&recovery_path, path, parent)?;
        if !claimed.matches_after_claim(expected) {
            let restored = restore_claimed_file(&recovery_path, path, parent)?;
            return Err(if restored {
                "Configured hotword file changed outside the GUI before atomic publication; the external version was restored."
                    .to_owned()
            } else {
                "Configured hotword file changed outside the GUI before atomic publication; the current path and an adjacent recovery copy were both preserved."
                    .to_owned()
            });
        }
        if let Err(error) = publish_noreplace(&temporary_path, path) {
            let restored = restore_claimed_file(&recovery_path, path, parent)?;
            return Err(if error == Errno::EXIST {
                if restored {
                    "Configured hotword file was recreated outside the GUI during atomic publication; the external version was preserved and the loaded version was restored."
                        .to_owned()
                } else {
                    "Configured hotword file was recreated outside the GUI during atomic publication; the external version and an adjacent recovery copy were both preserved."
                        .to_owned()
                }
            } else {
                format!("Publish claimed hotword file without replacement: {error}")
            });
        }
        Ok(finish_published_hotword(parent, true, sync_directory))
    })();
    if temporary_path.exists() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

fn create_sibling_file(
    parent: &Path,
    file_name: &std::ffi::OsStr,
    suffix: &str,
) -> Result<(PathBuf, fs::File), String> {
    let transaction_id = next_hotword_file_id();
    for attempt in 0..64_u8 {
        let candidate = parent.join(format!(
            ".{}.vinput-hotword-{}-{transaction_id}-{attempt}.{suffix}",
            file_name.to_string_lossy(),
            std::process::id()
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(format!("Create sibling hotword file: {error}")),
        }
    }
    Err("Could not allocate a sibling hotword file.".to_owned())
}

fn claim_current_hotword_file(
    path: &Path,
    parent: &Path,
    file_name: &std::ffi::OsStr,
) -> Result<PathBuf, String> {
    let transaction_id = next_hotword_file_id();
    for attempt in 0..64_u8 {
        let candidate = parent.join(format!(
            ".{}.vinput-hotword-{}-{transaction_id}-{attempt}.recovery",
            file_name.to_string_lossy(),
            std::process::id()
        ));
        match publish_noreplace(path, &candidate) {
            Ok(()) => return Ok(candidate),
            Err(Errno::EXIST) => {}
            Err(Errno::NOENT) => {
                return Err(
                    "Configured hotword file disappeared before atomic publication; no file was overwritten."
                        .to_owned(),
                );
            }
            Err(error) => return Err(format!("Claim current hotword file: {error}")),
        }
    }
    Err("Could not allocate an adjacent hotword recovery file.".to_owned())
}

fn read_claimed_hotword_or_restore(
    recovery_path: &Path,
    path: &Path,
    parent: &Path,
) -> Result<HotwordContentSnapshot, String> {
    match read_hotword_snapshot(recovery_path) {
        Ok(snapshot) => Ok(snapshot),
        Err(validation_error) => match restore_claimed_file(recovery_path, path, parent) {
            Ok(true) => Err(format!(
                "{validation_error} The claimed external hotword target was restored to its configured path."
            )),
            Ok(false) => Err(format!(
                "{validation_error} The configured path was recreated externally, so it and the adjacent recovery copy were both preserved."
            )),
            Err(restore_error) => Err(format!(
                "{validation_error} Restoring the claimed external hotword target also failed: {restore_error}"
            )),
        },
    }
}

fn finish_published_hotword(
    parent: &Path,
    previous_version_preserved: bool,
    synchronize: impl FnOnce(&Path) -> Result<(), String>,
) -> HotwordPublishOutcome {
    let durability_error = synchronize(parent).err().map(|error| {
        format!(
            "The hotword file was published, but synchronizing its directory failed: {error}; durability and daemon activation were not confirmed."
        )
    });
    HotwordPublishOutcome {
        previous_version_preserved,
        durability_error,
    }
}

fn synchronize_claim_or_restore(
    recovery_path: &Path,
    path: &Path,
    parent: &Path,
    synchronize: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let Err(sync_error) = synchronize(parent) else {
        return Ok(());
    };
    match restore_claimed_file(recovery_path, path, parent) {
        Ok(true) => Err(format!(
            "Synchronizing the hotword directory after claiming the loaded file failed: {sync_error}; the loaded file was restored to its configured path."
        )),
        Ok(false) => Err(format!(
            "Synchronizing the hotword directory after claiming the loaded file failed: {sync_error}; an external file occupied the configured path, so it and the adjacent recovery copy were both preserved."
        )),
        Err(restore_error) => Err(format!(
            "Synchronizing the hotword directory after claiming the loaded file failed: {sync_error}; restoring the configured path also failed: {restore_error}"
        )),
    }
}

fn restore_claimed_file(recovery_path: &Path, path: &Path, parent: &Path) -> Result<bool, String> {
    match publish_noreplace(recovery_path, path) {
        Ok(()) => {
            sync_directory(parent)?;
            Ok(true)
        }
        Err(Errno::EXIST) => Ok(false),
        Err(error) => Err(format!(
            "Restore claimed hotword file while preserving external changes: {error}"
        )),
    }
}

fn next_hotword_file_id() -> u64 {
    NEXT_HOTWORD_FILE_ID.fetch_add(1, Ordering::Relaxed)
}

fn publish_noreplace(source: &Path, target: &Path) -> rustix::io::Result<()> {
    renameat_with(CWD, source, CWD, target, RenameFlags::NOREPLACE)
}

fn sync_directory(parent: &Path) -> Result<(), String> {
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("Synchronize hotword directory: {error}"))
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
    fn content_save_is_atomic_conflict_aware_and_retryable_after_reload_failure() {
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
        assert!(reload_outcome.retry_activation);
        assert_eq!(
            fs::read_to_string(&path).expect("published content"),
            "delta\n"
        );
    }

    #[test]
    fn content_save_disables_retry_after_reload_window_file_changes() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("hotwords.txt");
        fs::write(&path, "delta\n").expect("write fixture");
        let concurrent_baseline = read_hotword_snapshot(&path).expect("read concurrent baseline");
        let concurrent_outcome =
            save_hotword_content_with_reload(&path, &concurrent_baseline, "gui-write\n", || {
                fs::write(&path, "concurrent-write\n").expect("concurrent update");
                Err("fixture reload failure".to_owned())
            })
            .expect("preserve concurrent reload-window update");
        assert!(concurrent_outcome.activation_error.is_some());
        assert!(!concurrent_outcome.retry_activation);
        assert!(concurrent_outcome.baseline.is_none());

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
        assert!(!same_content_outcome.retry_activation);
        assert!(same_content_outcome.baseline.is_none());
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
        assert!(!missing_outcome.retry_activation);
        assert!(missing_outcome.baseline.is_none());
        assert_eq!(
            fs::read_to_string(&missing_path).expect("concurrent created content"),
            "concurrent-create\n"
        );
    }

    #[test]
    fn atomic_publication_claims_loaded_version_and_preserves_recovery() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("hotwords.txt");
        fs::write(&path, "alpha\n").expect("initial content");
        let baseline = read_hotword_snapshot(&path).expect("baseline");

        let published = compare_and_swap_hotword_file(&path, &baseline, b"beta\n", || {})
            .expect("publish loaded version");
        assert!(published.previous_version_preserved);
        assert_eq!(
            fs::read_to_string(&path).expect("published content"),
            "beta\n"
        );
        let recovery_files = fs::read_dir(directory.path())
            .expect("recovery directory")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|candidate| {
                candidate
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().ends_with(".recovery"))
            })
            .collect::<Vec<_>>();
        assert_eq!(recovery_files.len(), 1);
        assert_eq!(
            fs::read_to_string(&recovery_files[0]).expect("recovery content"),
            "alpha\n"
        );
    }

    #[test]
    fn hotword_snapshot_rejects_extended_attributes() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("hotwords.txt");
        let file = fs::File::create(&path).expect("hotword fixture");
        file.set_xattr("user.vinput-test", b"fixture")
            .expect("set fixture xattr");

        let error = read_hotword_snapshot(&path).expect_err("reject extended metadata");
        assert!(error.contains("extended attributes or ACL metadata"));
    }

    #[test]
    fn temporary_publication_rejects_owner_or_group_mismatch() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("hotwords.txt");
        fs::write(&path, "alpha\n").expect("hotword fixture");
        let mut baseline = read_hotword_snapshot(&path).expect("baseline");
        let (_, temporary_file) = create_sibling_file(
            directory.path(),
            path.file_name().expect("file name"),
            "tmp",
        )
        .expect("temporary file");
        prepare_temporary_hotword_metadata(&temporary_file, &baseline)
            .expect("matching owner and group");

        baseline.version.as_mut().expect("version").uid =
            baseline.version.expect("version").uid.wrapping_add(1);
        let error = prepare_temporary_hotword_metadata(&temporary_file, &baseline)
            .expect_err("reject ownership mismatch");
        assert!(error.contains("ownership cannot be preserved"));
    }

    #[test]
    fn atomic_publication_restores_special_mode_bits_after_writing() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("hotwords.txt");
        fs::write(&path, "alpha\n").expect("hotword fixture");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o6755))
            .expect("special mode fixture");
        let baseline = read_hotword_snapshot(&path).expect("baseline");

        compare_and_swap_hotword_file(&path, &baseline, b"beta\n", || {})
            .expect("publish with special mode");
        assert_eq!(
            fs::metadata(&path).expect("published metadata").mode() & 0o7777,
            0o6755
        );
    }

    #[test]
    fn atomic_publication_rejects_changes_after_preparation() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("hotwords.txt");
        fs::write(&path, "alpha\n").expect("initial content");
        let baseline = read_hotword_snapshot(&path).expect("baseline");

        let direct_write_error =
            compare_and_swap_hotword_file(&path, &baseline, b"gui-write\n", || {
                fs::write(&path, "external-write\n").expect("external write");
            })
            .expect_err("reject write after preparation");
        assert!(direct_write_error.contains("changed outside"));
        assert_eq!(
            fs::read_to_string(&path).expect("external content"),
            "external-write\n"
        );

        let same_content_baseline = read_hotword_snapshot(&path).expect("same baseline");
        let replacement = directory.path().join("same-content-replacement.txt");
        let same_content_error =
            compare_and_swap_hotword_file(&path, &same_content_baseline, b"gui-write\n", || {
                fs::write(&replacement, "external-write\n").expect("replacement content");
                fs::rename(&replacement, &path).expect("atomic external replacement");
            })
            .expect_err("reject same-content replacement after preparation");
        assert!(same_content_error.contains("changed outside"));
        assert_eq!(
            fs::read_to_string(&path).expect("same-content external file"),
            "external-write\n"
        );
    }

    #[test]
    fn post_publication_target_validation_marks_result_unapplied() {
        let mut outcome = HotwordContentSaveOutcome {
            summary: "Hotword content saved; daemon ASR backend applied.".to_owned(),
            activation_error: None,
            baseline: None,
            retry_activation: false,
        };
        append_activation_error(
            &mut outcome,
            "The configured hotword target changed after publication.".to_owned(),
        );
        assert_eq!(outcome.summary, "Hotword content was saved to disk.");
        assert!(!outcome.retry_activation);
        assert!(
            outcome
                .activation_error
                .as_deref()
                .is_some_and(|error| error.contains("target changed"))
        );
    }

    #[test]
    fn published_directory_sync_failure_is_reported_as_committed() {
        let outcome = finish_published_hotword(Path::new("."), true, |_| {
            Err("fixture final sync failure".to_owned())
        });
        assert!(outcome.previous_version_preserved);
        assert!(
            outcome
                .durability_error
                .as_deref()
                .is_some_and(|error| error.contains("was published"))
        );
    }

    #[test]
    fn claimed_symlink_validation_failure_restores_configured_path() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("hotwords.txt");
        let external = directory.path().join("external.txt");
        fs::write(&path, "alpha\n").expect("initial content");
        fs::write(&external, "external\n").expect("external target");
        let baseline = read_hotword_snapshot(&path).expect("baseline");

        let error = compare_and_swap_hotword_file(&path, &baseline, b"beta\n", || {
            fs::remove_file(&path).expect("remove loaded target");
            symlink(&external, &path).expect("external symlink replacement");
        })
        .expect_err("reject and restore claimed symlink");
        assert!(error.contains("symbolic link"));
        assert!(error.contains("restored to its configured path"));
        assert!(
            fs::symlink_metadata(&path)
                .expect("restored metadata")
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_to_string(&path).expect("restored symlink target"),
            "external\n"
        );
    }

    #[test]
    fn atomic_publication_rejects_permission_changes_after_loading() {
        use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("hotwords.txt");
        fs::write(&path, "alpha\n").expect("initial content");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("initial permissions");
        let baseline = read_hotword_snapshot(&path).expect("baseline");

        let error = compare_and_swap_hotword_file(&path, &baseline, b"beta\n", || {
            fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).expect("external chmod");
        })
        .expect_err("reject permission change after loading");
        assert!(error.contains("changed outside"));
        assert_eq!(
            fs::metadata(&path).expect("restored metadata").mode() & 0o777,
            0o640
        );
        assert_eq!(
            fs::read_to_string(&path).expect("restored content"),
            "alpha\n"
        );
    }

    #[test]
    fn path_reload_confirmation_rejects_external_config_changes() {
        let directory = tempfile::tempdir().expect("temp dir");
        let config_path = directory.path().join("config.json");
        let mut updated = VinputConfig::bundled_default().expect("bundled config");
        updated.asr.providers[0].hotwords_file = Some(
            directory
                .path()
                .join("old.txt")
                .to_string_lossy()
                .into_owned(),
        );
        fs::write(
            &config_path,
            serde_json::to_vec_pretty(&updated).expect("serialize updated config"),
        )
        .expect("write updated config");
        ensure_hotword_path_update_current(&config_path, &updated)
            .expect("unchanged path config is current");

        let mut superseding = updated.clone();
        superseding.asr.providers[0].hotwords_file = Some(
            directory
                .path()
                .join("new.txt")
                .to_string_lossy()
                .into_owned(),
        );
        fs::write(
            &config_path,
            serde_json::to_vec_pretty(&superseding).expect("serialize superseding config"),
        )
        .expect("write superseding config");
        let error = ensure_hotword_path_update_current(&config_path, &updated)
            .expect_err("reject superseding path config");
        assert!(error.contains("changed during reload"));
    }

    #[test]
    fn claim_sync_failure_restores_configured_path() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("hotwords.txt");
        fs::write(&path, "alpha\n").expect("initial content");
        let recovery = claim_current_hotword_file(
            &path,
            directory.path(),
            path.file_name().expect("file name"),
        )
        .expect("claim configured file");
        assert!(!path.exists());

        let error = synchronize_claim_or_restore(&recovery, &path, directory.path(), |_| {
            Err("fixture directory sync failure".to_owned())
        })
        .expect_err("sync failure must abort publication");
        assert!(error.contains("restored to its configured path"));
        assert_eq!(
            fs::read_to_string(&path).expect("restored content"),
            "alpha\n"
        );
        assert!(!recovery.exists());
    }

    #[test]
    fn repeated_publication_keeps_allocating_recovery_files() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("hotwords.txt");
        fs::write(&path, "revision-0\n").expect("initial revision");

        for revision in 1..=70_u8 {
            let baseline = read_hotword_snapshot(&path).expect("revision baseline");
            let content = format!("revision-{revision}\n");
            compare_and_swap_hotword_file(&path, &baseline, content.as_bytes(), || {})
                .expect("publish revision");
        }

        assert_eq!(
            fs::read_to_string(&path).expect("latest revision"),
            "revision-70\n"
        );
        let recovery_count = fs::read_dir(directory.path())
            .expect("recovery directory")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".recovery"))
            .count();
        assert_eq!(recovery_count, 70);
    }
}

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
    prepare_missing_hotword_file_with(path, || {})
}

fn prepare_missing_hotword_file_with(
    path: Option<&Path>,
    before_publish: impl FnOnce(),
) -> Result<bool, String> {
    let Some(path) = path else {
        return Ok(false);
    };
    let expected = read_hotword_snapshot(path)?;
    if expected.existed {
        return Ok(false);
    }
    before_publish();
    compare_and_swap_hotword_file(path, &expected, b"", || {}).map(|_| true)
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
    before_preserve: impl FnOnce(),
) -> Result<HotwordPublishOutcome, String> {
    compare_and_swap_hotword_file_with_exchange_hook(path, expected, bytes, before_preserve, || {})
}

fn compare_and_swap_hotword_file_with_exchange_hook(
    path: &Path,
    expected: &HotwordContentSnapshot,
    bytes: &[u8],
    before_preserve: impl FnOnce(),
    before_exchange: impl FnOnce(),
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
    let mut cleanup_temporary = true;
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
        let prepared = read_hotword_snapshot(&temporary_path)?;
        before_preserve();
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
        publish_existing_hotword(
            path,
            parent,
            &temporary_path,
            expected,
            &prepared,
            before_exchange,
            &mut cleanup_temporary,
        )
    })();
    if cleanup_temporary && temporary_path.exists() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

fn publish_existing_hotword(
    path: &Path,
    parent: &Path,
    temporary_path: &Path,
    expected: &HotwordContentSnapshot,
    prepared: &HotwordContentSnapshot,
    before_exchange: impl FnOnce(),
    cleanup_temporary: &mut bool,
) -> Result<HotwordPublishOutcome, String> {
    let file_name = path
        .file_name()
        .ok_or_else(|| "Hotword path must name a file.".to_owned())?;
    let recovery_path = preserve_current_hotword_file(path, parent, file_name)?;
    synchronize_recovery_or_remove(&recovery_path, parent, sync_directory)?;
    let preserved = read_preserved_hotword_or_remove(&recovery_path, parent)?;
    let current = read_current_hotword_or_preserve_recovery(path, &recovery_path)?;
    if !preserved.matches_after_claim(expected) || !current.matches_after_claim(expected) {
        remove_recovery_file(&recovery_path, parent)?;
        return Err(
            "Configured hotword file changed outside the GUI before atomic publication; the external version remained at the configured path."
                .to_owned(),
        );
    }

    before_exchange();
    if let Err(error) = exchange_paths(temporary_path, path) {
        let cleanup = remove_recovery_file(&recovery_path, parent);
        return Err(format!(
            "Atomically exchange the hotword file while keeping the configured path available: {error}. The recovery link was {}.",
            cleanup_description(&cleanup)
        ));
    }
    *cleanup_temporary = false;

    let published_matches = read_hotword_snapshot(path)
        .as_ref()
        .is_ok_and(|published| published.matches_after_claim(prepared));
    let displaced_matches = read_hotword_snapshot(temporary_path)
        .as_ref()
        .is_ok_and(|displaced| displaced.matches_after_claim(expected));
    if !published_matches || !displaced_matches {
        return handle_invalid_exchange(
            temporary_path,
            path,
            prepared,
            parent,
            &recovery_path,
            cleanup_temporary,
        );
    }

    let cleanup_error = fs::remove_file(temporary_path)
        .err()
        .map(|error| format!("Remove displaced hotword transaction file: {error}"));
    let mut outcome = finish_published_hotword(parent, true, sync_directory);
    if let Some(error) = cleanup_error {
        append_publish_durability_error(&mut outcome, error);
    }
    Ok(outcome)
}

fn handle_invalid_exchange(
    temporary_path: &Path,
    path: &Path,
    prepared: &HotwordContentSnapshot,
    parent: &Path,
    recovery_path: &Path,
    cleanup_temporary: &mut bool,
) -> Result<HotwordPublishOutcome, String> {
    match rollback_exchanged_hotword(temporary_path, path, prepared, parent) {
        Ok(true) => {
            *cleanup_temporary = true;
            remove_recovery_file(recovery_path, parent)?;
            Err(
                "Configured hotword file changed outside the GUI during atomic publication; the external version was restored to the configured path."
                    .to_owned(),
            )
        }
        Ok(false) => Err(
            "Configured hotword file changed during atomic publication and the published path was also changed externally; the current path, displaced file, and recovery link were all preserved."
                .to_owned(),
        ),
        Err(error) => Err(format!(
            "Configured hotword file changed during atomic publication, and restoring the atomic exchange failed: {error}. The configured path, displaced file, and recovery link were all preserved."
        )),
    }
}

fn cleanup_description(result: &Result<(), String>) -> &'static str {
    if result.is_ok() {
        "removed"
    } else {
        "preserved because cleanup failed"
    }
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

fn preserve_current_hotword_file(
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
        match fs::hard_link(path, &candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(
                    "Configured hotword file disappeared before its recovery link was created; no file was overwritten."
                        .to_owned(),
                );
            }
            Err(error) => return Err(format!("Preserve current hotword file: {error}")),
        }
    }
    Err("Could not allocate an adjacent hotword recovery file.".to_owned())
}

fn read_preserved_hotword_or_remove(
    recovery_path: &Path,
    parent: &Path,
) -> Result<HotwordContentSnapshot, String> {
    match read_hotword_snapshot(recovery_path) {
        Ok(snapshot) => Ok(snapshot),
        Err(validation_error) => {
            let cleanup = remove_recovery_file(recovery_path, parent);
            Err(format!(
                "{validation_error} The configured path remained in place and the invalid recovery link was {}.",
                if cleanup.is_ok() {
                    "removed"
                } else {
                    "preserved because cleanup failed"
                }
            ))
        }
    }
}

fn read_current_hotword_or_preserve_recovery(
    path: &Path,
    recovery_path: &Path,
) -> Result<HotwordContentSnapshot, String> {
    match read_hotword_snapshot(path) {
        Ok(snapshot) if snapshot.existed => Ok(snapshot),
        Ok(_) => Err(format!(
            "The configured hotword file disappeared after its recovery link was created; the previous version was preserved at the adjacent recovery path `{}`.",
            recovery_path.display()
        )),
        Err(error) => Err(format!(
            "{error} The configured path changed after its recovery link was created; the previous version was preserved at the adjacent recovery path `{}`.",
            recovery_path.display()
        )),
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

fn append_publish_durability_error(outcome: &mut HotwordPublishOutcome, error: String) {
    outcome.durability_error = Some(match outcome.durability_error.take() {
        Some(existing) => format!("{existing} {error}"),
        None => error,
    });
}

fn synchronize_recovery_or_remove(
    recovery_path: &Path,
    parent: &Path,
    synchronize: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let Err(sync_error) = synchronize(parent) else {
        return Ok(());
    };
    let cleanup = remove_recovery_file(recovery_path, parent);
    Err(format!(
        "Synchronizing the hotword directory after preserving the loaded file failed: {sync_error}; the configured path remained available and the recovery link was {}.",
        if cleanup.is_ok() {
            "removed"
        } else {
            "preserved because cleanup failed"
        }
    ))
}

fn remove_recovery_file(recovery_path: &Path, parent: &Path) -> Result<(), String> {
    match fs::remove_file(recovery_path) {
        Ok(()) => sync_directory(parent),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Remove hotword recovery link: {error}")),
    }
}

fn rollback_exchanged_hotword(
    temporary_path: &Path,
    path: &Path,
    prepared: &HotwordContentSnapshot,
    parent: &Path,
) -> Result<bool, String> {
    let Ok(current) = read_hotword_snapshot(path) else {
        return Ok(false);
    };
    if !current.matches_after_claim(prepared) {
        return Ok(false);
    }
    exchange_paths(temporary_path, path)
        .map_err(|error| format!("Rollback hotword atomic exchange: {error}"))?;
    sync_directory(parent)?;
    Ok(true)
}

fn next_hotword_file_id() -> u64 {
    NEXT_HOTWORD_FILE_ID.fetch_add(1, Ordering::Relaxed)
}

fn publish_noreplace(source: &Path, target: &Path) -> rustix::io::Result<()> {
    renameat_with(CWD, source, CWD, target, RenameFlags::NOREPLACE)
}

fn exchange_paths(left: &Path, right: &Path) -> rustix::io::Result<()> {
    renameat_with(CWD, left, CWD, right, RenameFlags::EXCHANGE)
}

fn sync_directory(parent: &Path) -> Result<(), String> {
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("Synchronize hotword directory: {error}"))
}

#[cfg(test)]
#[path = "hotword_persistence/tests.rs"]
mod tests;

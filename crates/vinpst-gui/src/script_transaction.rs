//! Transactional managed-script artifact preservation and revision metadata.

use std::{
    fs,
    io::{self, Read, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use vinpst_config::{
    MANAGED_SCRIPT_REVISION_KEY, MANAGED_SCRIPT_ROLLBACK_REVISION_KEY, VinpstConfig,
};
use vinpst_registry::{LiveScriptKind, managed_script_rollback_path, sha256_hex};

use crate::script_install::ScriptInstallOutcome;

static NEXT_SCRIPT_COPY_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub(crate) struct ManagedScriptRollback {
    script_path: PathBuf,
    rollback_path: PathBuf,
    revision: String,
}

impl ManagedScriptRollback {
    pub(crate) fn prepare(
        kind: LiveScriptKind,
        replacing: bool,
        script_path: &Path,
    ) -> Result<Option<Self>, String> {
        if kind != LiveScriptKind::LlmAdapter || !replacing {
            return Ok(None);
        }
        let (bytes, permissions) = read_regular_file(script_path).map_err(|error| {
            format!(
                "Read existing text adapter script `{}` before update: {error}",
                script_path.display()
            )
        })?;
        let rollback_path = managed_script_rollback_path(script_path);
        atomic_write_copy(&rollback_path, &bytes, permissions).map_err(|error| {
            format!(
                "Preserve existing text adapter script `{}` at `{}`: {error}",
                script_path.display(),
                rollback_path.display()
            )
        })?;
        Ok(Some(Self {
            script_path: script_path.to_path_buf(),
            rollback_path,
            revision: sha256_hex(&bytes),
        }))
    }

    fn restore(&self) -> Result<(), String> {
        let (bytes, permissions) = read_regular_file(&self.rollback_path).map_err(|error| {
            format!(
                "Read previous text adapter script `{}` before restore: {error}",
                self.rollback_path.display()
            )
        })?;
        if sha256_hex(&bytes) != self.revision {
            return Err(format!(
                "Previous text adapter script `{}` changed after rollback preparation; refusing to restore it.",
                self.rollback_path.display()
            ));
        }
        atomic_write_copy(&self.script_path, &bytes, permissions).map_err(|error| {
            format!(
                "Restore previous text adapter script `{}` from `{}`: {error}",
                self.script_path.display(),
                self.rollback_path.display()
            )
        })
    }
}

pub(crate) fn apply_managed_script_revision(
    config: &mut VinpstConfig,
    kind: LiveScriptKind,
    resource_id: &str,
    script_path: &Path,
    rollback: Option<&ManagedScriptRollback>,
) -> Result<(), String> {
    if kind != LiveScriptKind::LlmAdapter {
        return Ok(());
    }
    let (bytes, _) = read_regular_file(script_path).map_err(|error| {
        format!(
            "Read published text adapter script `{}` for revision: {error}",
            script_path.display()
        )
    })?;
    let adapter = config
        .llm
        .adapters
        .iter_mut()
        .find(|adapter| adapter.id == resource_id)
        .ok_or_else(|| {
            format!("Installed text adapter `{resource_id}` is missing from prepared config.")
        })?;
    adapter.extra.insert(
        MANAGED_SCRIPT_REVISION_KEY.to_owned(),
        serde_json::Value::String(sha256_hex(&bytes)),
    );
    match rollback {
        Some(rollback) => {
            adapter.extra.insert(
                MANAGED_SCRIPT_ROLLBACK_REVISION_KEY.to_owned(),
                serde_json::Value::String(rollback.revision.clone()),
            );
        }
        None => {
            adapter.extra.remove(MANAGED_SCRIPT_ROLLBACK_REVISION_KEY);
        }
    }
    Ok(())
}

pub(crate) fn failed_after_publication(
    error: String,
    rollback: Option<&ManagedScriptRollback>,
) -> ScriptInstallOutcome {
    match rollback {
        Some(rollback) => failed_with_script_restore(
            format!("{error} Retry the script update after resolving the failure."),
            Some(rollback),
        ),
        None => ScriptInstallOutcome::PublishedButConfigFailed { error },
    }
}

pub(crate) fn failed_with_script_restore(
    error: String,
    rollback: Option<&ManagedScriptRollback>,
) -> ScriptInstallOutcome {
    let Some(rollback) = rollback else {
        return ScriptInstallOutcome::Failed(error);
    };
    ScriptInstallOutcome::Failed(match rollback.restore() {
        Ok(()) => format!("{error} Previous managed adapter script restored."),
        Err(restore_error) => format!(
            "{error} Restoring the previous managed adapter script also failed: {restore_error}"
        ),
    })
}

fn read_regular_file(path: &Path) -> io::Result<(Vec<u8>, fs::Permissions)> {
    let before = fs::symlink_metadata(path)?;
    if !before.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "managed script is not a regular file",
        ));
    }
    let mut file = fs::File::open(path)?;
    let opened = file.metadata()?;
    if !same_file_version(&before, &opened) {
        return Err(io::Error::other(
            "managed script changed while it was being opened",
        ));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let after = fs::symlink_metadata(path)?;
    if !same_file_version(&before, &after) {
        return Err(io::Error::other(
            "managed script changed while it was being read",
        ));
    }
    Ok((bytes, before.permissions()))
}

fn same_file_version(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.size() == right.size()
        && left.uid() == right.uid()
        && left.gid() == right.gid()
        && left.mode() == right.mode()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
}

fn atomic_write_copy(
    destination: &Path,
    bytes: &[u8],
    permissions: fs::Permissions,
) -> io::Result<()> {
    if let Some(parent) = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("managed-script");
    let transaction_id = NEXT_SCRIPT_COPY_ID.fetch_add(1, Ordering::Relaxed);
    let (temporary, mut temporary_file) = (0..64_u8)
        .find_map(|attempt| {
            let candidate = parent.join(format!(
                ".{file_name}.vinpst-copy-{}-{transaction_id}-{attempt}.tmp",
                std::process::id()
            ));
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(file) => Some(Ok((candidate, file))),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => None,
                Err(error) => Some(Err(error)),
            }
        })
        .unwrap_or_else(|| {
            Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "could not allocate a unique managed-script copy file",
            ))
        })?;
    let result = (|| {
        temporary_file.write_all(bytes)?;
        temporary_file.set_permissions(permissions)?;
        temporary_file.sync_all()?;
        drop(temporary_file);
        fs::rename(&temporary, destination)?;
        fs::File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;

    #[test]
    fn rollback_copy_does_not_follow_legacy_pid_temporary_symlink() {
        let directory = tempfile::tempdir().expect("temp dir");
        let source = directory.path().join("adapter");
        let destination = directory.path().join("adapter.rollback");
        let victim = directory.path().join("victim");
        fs::write(&source, b"old adapter").expect("source");
        fs::write(&victim, b"victim").expect("victim");
        let mut planted = destination.as_os_str().to_os_string();
        planted.push(format!(".tmp-{}", std::process::id()));
        symlink(&victim, PathBuf::from(planted)).expect("plant old temporary symlink");

        let (bytes, permissions) = read_regular_file(&source).expect("read source");
        atomic_write_copy(&destination, &bytes, permissions).expect("safe copy");

        assert_eq!(fs::read(&destination).expect("destination"), b"old adapter");
        assert_eq!(fs::read(&victim).expect("victim unchanged"), b"victim");
    }

    #[test]
    fn rollback_restore_refuses_tampered_rollback_revision() {
        let directory = tempfile::tempdir().expect("temp dir");
        let script = directory.path().join("adapter");
        fs::write(&script, b"old adapter").expect("old adapter");
        let rollback = ManagedScriptRollback::prepare(LiveScriptKind::LlmAdapter, true, &script)
            .expect("prepare rollback")
            .expect("rollback");
        fs::write(&rollback.rollback_path, b"tampered rollback").expect("tamper rollback");
        fs::write(&script, b"new adapter").expect("published adapter");

        let error = rollback
            .restore()
            .expect_err("tampered rollback must fail closed");

        assert!(error.contains("changed after rollback preparation"));
        assert_eq!(
            fs::read(&script).expect("published remains"),
            b"new adapter"
        );
    }

    #[test]
    fn rollback_prepare_refuses_symlinked_managed_script() {
        let directory = tempfile::tempdir().expect("temp dir");
        let target = directory.path().join("target");
        let script = directory.path().join("adapter");
        fs::write(&target, b"target").expect("target");
        symlink(&target, &script).expect("symlink");

        let error = ManagedScriptRollback::prepare(LiveScriptKind::LlmAdapter, true, &script)
            .expect_err("symlink must fail closed");

        assert!(error.contains("not a regular file"));
    }
}

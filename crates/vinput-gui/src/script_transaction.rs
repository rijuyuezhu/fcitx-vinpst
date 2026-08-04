//! Transactional managed-script artifact preservation and revision metadata.

use std::{
    fs,
    path::{Path, PathBuf},
};

use vinput_config::{
    MANAGED_SCRIPT_REVISION_KEY, MANAGED_SCRIPT_ROLLBACK_REVISION_KEY, VinputConfig,
};
use vinput_registry::{LiveScriptKind, managed_script_rollback_path, sha256_hex};

use crate::script_install::ScriptInstallOutcome;

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
        let bytes = fs::read(script_path).map_err(|error| {
            format!(
                "Read existing text adapter script `{}` before update: {error}",
                script_path.display()
            )
        })?;
        let rollback_path = managed_script_rollback_path(script_path);
        atomic_copy_file(script_path, &rollback_path).map_err(|error| {
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
        atomic_copy_file(&self.rollback_path, &self.script_path).map_err(|error| {
            format!(
                "Restore previous text adapter script `{}` from `{}`: {error}",
                self.script_path.display(),
                self.rollback_path.display()
            )
        })
    }
}

pub(crate) fn apply_managed_script_revision(
    config: &mut VinputConfig,
    kind: LiveScriptKind,
    resource_id: &str,
    script_path: &Path,
    rollback: Option<&ManagedScriptRollback>,
) -> Result<(), String> {
    if kind != LiveScriptKind::LlmAdapter {
        return Ok(());
    }
    let bytes = fs::read(script_path).map_err(|error| {
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

fn atomic_copy_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    if let Some(parent) = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let mut temporary = destination.as_os_str().to_os_string();
    temporary.push(format!(".tmp-{}", std::process::id()));
    let temporary = PathBuf::from(temporary);
    let result = (|| {
        fs::copy(source, &temporary)?;
        fs::File::open(&temporary)?.sync_all()?;
        fs::rename(&temporary, destination)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

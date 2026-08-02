//! Registry staged-tree materialization boundary.
//!
//! This module publishes a fully prepared staging directory into a target
//! directory using renames, with a target-filesystem copy fallback for `EXDEV`,
//! and an explicit rollback path for replacements.
//! It does not download assets, extract archives, mutate configuration, or expose
//! user-facing install commands.

use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::RegistryOperationControl;

/// Result of materializing a staged registry tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedRegistryTree {
    /// Source staging path that was consumed by the rename.
    pub source_path: PathBuf,
    /// Final materialized target path.
    pub target_path: PathBuf,
    /// Whether an existing target directory was replaced.
    pub replaced_existing: bool,
}

/// Registry materialization errors.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum RegistryMaterializeError {
    /// The caller requested cooperative cancellation before publication completed.
    #[error("registry materialization cancelled for `{target}`")]
    Cancelled {
        /// Final target path.
        target: String,
    },
    /// Source staging tree does not exist.
    #[error("staged tree `{path}` does not exist")]
    SourceMissing {
        /// Source staging path.
        path: String,
    },
    /// Source exists but is not a directory.
    #[error("staged tree `{path}` is not a directory")]
    SourceNotDirectory {
        /// Source staging path.
        path: String,
    },
    /// Target equals source.
    #[error("materialization target `{path}` is the staged source")]
    TargetEqualsSource {
        /// Shared source/target path.
        path: String,
    },
    /// Target exists but is not a directory.
    #[error("materialization target `{path}` exists but is not a directory")]
    TargetNotDirectory {
        /// Target path.
        path: String,
    },
    /// Target parent directory could not be created.
    #[error("failed to create materialization parent for `{path}`: {message}")]
    CreateTargetParent {
        /// Target path.
        path: String,
        /// Sanitized I/O failure message.
        message: String,
    },
    /// Existing target could not be moved aside for replacement.
    #[error("failed to move existing target `{target}` aside to `{backup}`: {message}")]
    MoveExistingTarget {
        /// Existing target path.
        target: String,
        /// Temporary backup path.
        backup: String,
        /// Sanitized I/O failure message.
        message: String,
    },
    /// Staged source could not be published to target.
    #[error("failed to publish staged tree `{staged}` to `{target}`: {message}")]
    Publish {
        /// Source staging path.
        staged: String,
        /// Target path.
        target: String,
        /// Sanitized I/O failure message.
        message: String,
    },
    /// Replacement failed and the previous target could not be restored.
    #[error(
        "failed to publish staged tree `{staged}` to `{target}`: {publish_message}; rollback from `{backup}` failed: {rollback_message}"
    )]
    RollbackFailed {
        /// Source staging path.
        staged: String,
        /// Target path.
        target: String,
        /// Temporary backup path.
        backup: String,
        /// Sanitized publish failure.
        publish_message: String,
        /// Sanitized rollback failure.
        rollback_message: String,
    },
    /// The old target backup remained after successful replacement.
    #[error("failed to remove replaced target backup `{backup}`: {message}")]
    CleanupBackup {
        /// Temporary backup path.
        backup: String,
        /// Sanitized I/O failure message.
        message: String,
    },
}

/// Moves a staged directory into a target directory.
///
/// The operation normally consumes `source_path` with `rename`. If source and
/// target are on different filesystems, it first copies the validated tree to a
/// hidden sibling of the target and then atomically renames that sibling into
/// place. If the target directory exists, it is first moved to a same-directory
/// backup; publish failure attempts to roll that backup back into place.
pub fn materialize_staged_tree(
    source_path: impl AsRef<Path>,
    target_path: impl AsRef<Path>,
) -> Result<MaterializedRegistryTree, RegistryMaterializeError> {
    materialize_staged_tree_controlled(
        source_path,
        target_path,
        &RegistryOperationControl::default(),
    )
}

/// Controlled companion to [`materialize_staged_tree`].
pub fn materialize_staged_tree_controlled(
    source_path: impl AsRef<Path>,
    target_path: impl AsRef<Path>,
    control: &RegistryOperationControl,
) -> Result<MaterializedRegistryTree, RegistryMaterializeError> {
    let source_path = source_path.as_ref();
    let target_path = target_path.as_ref();

    check_cancelled(control, target_path)?;
    validate_materialize_paths(source_path, target_path)?;
    if let Some(parent) = target_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            RegistryMaterializeError::CreateTargetParent {
                path: display_path(target_path),
                message: sanitize_io_error(&error),
            }
        })?;
    }

    let replaced_existing = target_path.exists();
    let backup_path = materialize_backup_path(target_path);
    check_cancelled(control, target_path)?;
    if replaced_existing {
        fs::rename(target_path, &backup_path).map_err(|error| {
            RegistryMaterializeError::MoveExistingTarget {
                target: display_path(target_path),
                backup: display_path(&backup_path),
                message: sanitize_io_error(&error),
            }
        })?;
    }

    if let Err(error) = publish_staged_tree(source_path, target_path, control) {
        let publish_message = error.message();
        if replaced_existing {
            return match fs::rename(&backup_path, target_path) {
                Ok(()) => Err(error.into_registry_error(source_path, target_path)),
                Err(rollback_error) => Err(RegistryMaterializeError::RollbackFailed {
                    staged: display_path(source_path),
                    target: display_path(target_path),
                    backup: display_path(&backup_path),
                    publish_message,
                    rollback_message: sanitize_io_error(&rollback_error),
                }),
            };
        }
        return Err(error.into_registry_error(source_path, target_path));
    }

    if replaced_existing {
        fs::remove_dir_all(&backup_path).map_err(|error| {
            RegistryMaterializeError::CleanupBackup {
                backup: display_path(&backup_path),
                message: sanitize_io_error(&error),
            }
        })?;
    }

    Ok(MaterializedRegistryTree {
        source_path: source_path.to_owned(),
        target_path: target_path.to_owned(),
        replaced_existing,
    })
}

fn publish_staged_tree(
    source_path: &Path,
    target_path: &Path,
    control: &RegistryOperationControl,
) -> Result<(), PublishStagedTreeError> {
    check_cancelled(control, target_path).map_err(|_| PublishStagedTreeError::Cancelled)?;
    match fs::rename(source_path, target_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
            publish_cross_device(source_path, target_path, control)
        }
        Err(error) => Err(PublishStagedTreeError::Io(error)),
    }
}

fn publish_cross_device(
    source_path: &Path,
    target_path: &Path,
    control: &RegistryOperationControl,
) -> Result<(), PublishStagedTreeError> {
    let publish_path = materialize_publish_path(target_path);
    if let Err(error) = copy_directory_tree(source_path, &publish_path, control) {
        remove_dir_if_exists(&publish_path);
        return Err(error);
    }
    check_cancelled(control, target_path).map_err(|_| {
        remove_dir_if_exists(&publish_path);
        PublishStagedTreeError::Cancelled
    })?;
    if let Err(error) = fs::rename(&publish_path, target_path) {
        remove_dir_if_exists(&publish_path);
        return Err(PublishStagedTreeError::Io(error));
    }
    remove_dir_if_exists(source_path);
    Ok(())
}

fn copy_directory_tree(
    source_path: &Path,
    target_path: &Path,
    control: &RegistryOperationControl,
) -> Result<(), PublishStagedTreeError> {
    check_cancelled(control, target_path).map_err(|_| PublishStagedTreeError::Cancelled)?;
    fs::create_dir(target_path).map_err(PublishStagedTreeError::Io)?;
    for entry in fs::read_dir(source_path).map_err(PublishStagedTreeError::Io)? {
        check_cancelled(control, target_path).map_err(|_| PublishStagedTreeError::Cancelled)?;
        let entry = entry.map_err(PublishStagedTreeError::Io)?;
        let file_type = entry.file_type().map_err(PublishStagedTreeError::Io)?;
        let source_entry = entry.path();
        let target_entry = target_path.join(entry.file_name());
        if file_type.is_dir() {
            copy_directory_tree(&source_entry, &target_entry, control)?;
        } else if file_type.is_file() {
            copy_file(&source_entry, &target_entry, control)?;
        } else {
            return Err(PublishStagedTreeError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                "staged tree contains a non-file, non-directory entry",
            )));
        }
    }
    let permissions = fs::metadata(source_path)
        .map_err(PublishStagedTreeError::Io)?
        .permissions();
    fs::set_permissions(target_path, permissions).map_err(PublishStagedTreeError::Io)?;
    Ok(())
}

fn copy_file(
    source_path: &Path,
    target_path: &Path,
    control: &RegistryOperationControl,
) -> Result<(), PublishStagedTreeError> {
    let mut source = fs::File::open(source_path).map_err(PublishStagedTreeError::Io)?;
    let mut target = fs::File::create(target_path).map_err(PublishStagedTreeError::Io)?;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        check_cancelled(control, target_path).map_err(|_| PublishStagedTreeError::Cancelled)?;
        let count = source
            .read(&mut buffer)
            .map_err(PublishStagedTreeError::Io)?;
        if count == 0 {
            break;
        }
        target
            .write_all(&buffer[..count])
            .map_err(PublishStagedTreeError::Io)?;
    }
    let permissions = fs::metadata(source_path)
        .map_err(PublishStagedTreeError::Io)?
        .permissions();
    fs::set_permissions(target_path, permissions).map_err(PublishStagedTreeError::Io)?;
    Ok(())
}

#[derive(Debug)]
enum PublishStagedTreeError {
    Io(io::Error),
    Cancelled,
}

impl PublishStagedTreeError {
    fn message(&self) -> String {
        match self {
            Self::Io(error) => sanitize_io_error(error),
            Self::Cancelled => "registry operation cancelled".to_owned(),
        }
    }

    fn into_registry_error(
        self,
        source_path: &Path,
        target_path: &Path,
    ) -> RegistryMaterializeError {
        match self {
            Self::Io(error) => RegistryMaterializeError::Publish {
                staged: display_path(source_path),
                target: display_path(target_path),
                message: sanitize_io_error(&error),
            },
            Self::Cancelled => RegistryMaterializeError::Cancelled {
                target: display_path(target_path),
            },
        }
    }
}

fn remove_dir_if_exists(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

fn check_cancelled(
    control: &RegistryOperationControl,
    target_path: &Path,
) -> Result<(), RegistryMaterializeError> {
    control
        .check_cancelled()
        .map_err(|_| RegistryMaterializeError::Cancelled {
            target: display_path(target_path),
        })
}

fn validate_materialize_paths(
    source_path: &Path,
    target_path: &Path,
) -> Result<(), RegistryMaterializeError> {
    if source_path == target_path {
        return Err(RegistryMaterializeError::TargetEqualsSource {
            path: display_path(source_path),
        });
    }
    if !source_path.exists() {
        return Err(RegistryMaterializeError::SourceMissing {
            path: display_path(source_path),
        });
    }
    if !source_path.is_dir() {
        return Err(RegistryMaterializeError::SourceNotDirectory {
            path: display_path(source_path),
        });
    }
    if target_path.exists() && !target_path.is_dir() {
        return Err(RegistryMaterializeError::TargetNotDirectory {
            path: display_path(target_path),
        });
    }
    Ok(())
}

fn materialize_backup_path(target_path: &Path) -> PathBuf {
    let file_name = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("registry-materialized");
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    target_path.with_file_name(format!(
        ".{file_name}.backup.{}.{unique}",
        std::process::id()
    ))
}

fn materialize_publish_path(target_path: &Path) -> PathBuf {
    let file_name = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("registry-materialized");
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    target_path.with_file_name(format!(
        ".{file_name}.publish.{}.{unique}",
        std::process::id()
    ))
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

fn sanitize_io_error(error: &io::Error) -> String {
    error.kind().to_string()
}

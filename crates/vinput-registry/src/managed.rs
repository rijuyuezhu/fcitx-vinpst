//! Managed model path naming and deletion boundaries shared by CLI and GUI.

use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use thiserror::Error;

use crate::LiveModelEntry;

/// Builds the stable managed directory name used for a live registry model.
#[must_use]
pub fn managed_model_dir_name(model: &LiveModelEntry) -> String {
    let preferred = model
        .short_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&model.id);
    safe_path_component(preferred)
}

/// Replaces unsafe path characters with `-` and prevents hidden/empty names.
#[must_use]
pub fn safe_path_component(value: &str) -> String {
    let mut component = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    while component.starts_with('.') {
        component.remove(0);
    }
    while component.ends_with('.') {
        component.pop();
    }
    if component.is_empty() {
        "model".to_owned()
    } else {
        component
    }
}

/// Request for deleting one inactive directory beneath a managed model root.
#[derive(Debug, Clone)]
pub struct ManagedModelRemoveRequest<'a> {
    /// Root that owns model directories.
    pub model_root: &'a Path,
    /// Exact managed directory to remove.
    pub target_path: &'a Path,
    /// Model paths currently referenced by configured local ASR providers.
    pub active_model_paths: &'a [PathBuf],
}

/// Successful managed model deletion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedModelRemoveResult {
    /// Removed directory path.
    pub target_path: PathBuf,
}

/// Managed model deletion failures.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ManagedModelRemoveError {
    /// Target is not a descendant of the managed root.
    #[error("refusing to remove `{target}` because it is outside model root `{root}`")]
    OutsideRoot {
        /// Rejected target path.
        target: String,
        /// Managed root path.
        root: String,
    },
    /// The model root itself was selected.
    #[error("refusing to remove model root `{root}`; select a managed model directory")]
    RootTarget {
        /// Managed root path.
        root: String,
    },
    /// Relative target contains an unsafe path component.
    #[error("refusing unsafe model remove target `{target}`")]
    UnsafeTarget {
        /// Rejected target path.
        target: String,
    },
    /// Target does not exist.
    #[error("model remove target `{target}` does not exist")]
    Missing {
        /// Missing target path.
        target: String,
    },
    /// Target is not a directory.
    #[error("model remove target `{target}` is not a directory")]
    NotDirectory {
        /// Rejected target path.
        target: String,
    },
    /// Target is referenced by the active configuration.
    #[error("refusing to remove active model `{target}`")]
    Active {
        /// Active target path.
        target: String,
    },
    /// Metadata inspection failed.
    #[error("failed to inspect model remove target `{target}`: {message}")]
    Inspect {
        /// Target path being inspected.
        target: String,
        /// Sanitized I/O failure category.
        message: String,
    },
    /// Recursive removal failed.
    #[error("failed to remove model directory `{target}`: {message}")]
    Remove {
        /// Target path being removed.
        target: String,
        /// Sanitized I/O failure category.
        message: String,
    },
}

/// Validates and deletes one inactive managed model directory.
pub fn remove_managed_model(
    request: &ManagedModelRemoveRequest<'_>,
) -> Result<ManagedModelRemoveResult, ManagedModelRemoveError> {
    validate_managed_model_target(request.model_root, request.target_path)?;
    let metadata = fs::metadata(request.target_path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            ManagedModelRemoveError::Missing {
                target: display_path(request.target_path),
            }
        } else {
            ManagedModelRemoveError::Inspect {
                target: display_path(request.target_path),
                message: error.kind().to_string(),
            }
        }
    })?;
    if !metadata.is_dir() {
        return Err(ManagedModelRemoveError::NotDirectory {
            target: display_path(request.target_path),
        });
    }
    if request
        .active_model_paths
        .iter()
        .any(|active| active == request.target_path)
    {
        return Err(ManagedModelRemoveError::Active {
            target: display_path(request.target_path),
        });
    }
    fs::remove_dir_all(request.target_path).map_err(|error| ManagedModelRemoveError::Remove {
        target: display_path(request.target_path),
        message: error.kind().to_string(),
    })?;
    Ok(ManagedModelRemoveResult {
        target_path: request.target_path.to_path_buf(),
    })
}

/// Rejects paths outside the managed root, the root itself, and traversal components.
pub fn validate_managed_model_target(
    model_root: &Path,
    target_path: &Path,
) -> Result<(), ManagedModelRemoveError> {
    let relative =
        target_path
            .strip_prefix(model_root)
            .map_err(|_| ManagedModelRemoveError::OutsideRoot {
                target: display_path(target_path),
                root: display_path(model_root),
            })?;
    if relative.as_os_str().is_empty() {
        return Err(ManagedModelRemoveError::RootTarget {
            root: display_path(model_root),
        });
    }
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(ManagedModelRemoveError::UnsafeTarget {
            target: display_path(target_path),
        });
    }
    Ok(())
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_directory_name_prefers_safe_short_id() {
        let model = LiveModelEntry {
            id: "model.sherpa-onnx/example".to_owned(),
            short_id: Some(".unsafe name.".to_owned()),
            urls: vec!["https://example.invalid/model.tar.zst".to_owned()],
            sha256: None,
            size_bytes: None,
            language: None,
            title: None,
            description: None,
            vinput_model: None,
        };
        assert_eq!(managed_model_dir_name(&model), "unsafe-name");
    }

    #[test]
    fn delete_rejects_root_and_paths_outside_root() {
        let directory = tempfile::tempdir().expect("temp dir");
        let root = directory.path().join("models");
        fs::create_dir_all(&root).expect("model root");
        assert!(matches!(
            validate_managed_model_target(&root, &root),
            Err(ManagedModelRemoveError::RootTarget { .. })
        ));
        assert!(matches!(
            validate_managed_model_target(&root, directory.path()),
            Err(ManagedModelRemoveError::OutsideRoot { .. })
        ));
    }

    #[test]
    fn delete_rejects_active_model_without_mutation() {
        let directory = tempfile::tempdir().expect("temp dir");
        let root = directory.path().join("models");
        let target = root.join("active");
        fs::create_dir_all(&target).expect("model target");
        fs::write(target.join("model.onnx"), b"fixture").expect("fixture");
        let error = remove_managed_model(&ManagedModelRemoveRequest {
            model_root: &root,
            target_path: &target,
            active_model_paths: std::slice::from_ref(&target),
        })
        .expect_err("active model must be rejected");
        assert!(matches!(error, ManagedModelRemoveError::Active { .. }));
        assert!(target.exists());
    }

    #[test]
    fn delete_removes_inactive_managed_directory() {
        let directory = tempfile::tempdir().expect("temp dir");
        let root = directory.path().join("models");
        let target = root.join("inactive");
        fs::create_dir_all(&target).expect("model target");
        fs::write(target.join("model.onnx"), b"fixture").expect("fixture");
        let result = remove_managed_model(&ManagedModelRemoveRequest {
            model_root: &root,
            target_path: &target,
            active_model_paths: &[],
        })
        .expect("remove inactive model");
        assert_eq!(result.target_path, target);
        assert!(!result.target_path.exists());
        assert!(root.exists());
    }
}

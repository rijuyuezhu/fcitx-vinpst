//! Validated atomic persistence for typed vinpst configuration documents.

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use thiserror::Error;

use crate::{ConfigError, VinpstConfig};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Successful typed config write metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigWriteReceipt {
    /// Final config path.
    pub path: PathBuf,
    /// Backup path written before replacement, when requested.
    pub backup_path: Option<PathBuf>,
}

/// Errors produced while validating or atomically persisting a config.
#[derive(Debug, Error)]
pub enum ConfigWriteError {
    /// The candidate config failed typed validation.
    #[error("validate config before writing: {0}")]
    Validation(#[from] ConfigError),
    /// The destination symlink chain could not be resolved safely.
    #[error("resolve config write target `{path}`: {source}")]
    ResolveTarget {
        /// User-facing destination path.
        path: PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: io::Error,
    },
    /// The output parent directory could not be created.
    #[error("create config directory `{path}`: {source}")]
    CreateDirectory {
        /// Directory path.
        path: PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: io::Error,
    },
    /// The existing config could not be copied to the requested backup.
    #[error("backup config `{path}` to `{backup_path}`: {source}")]
    Backup {
        /// Existing config path.
        path: PathBuf,
        /// Backup destination.
        backup_path: PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: io::Error,
    },
    /// The validated config could not be serialized.
    #[error("serialize config: {0}")]
    Serialize(#[from] serde_json::Error),
    /// A unique temporary file could not be created.
    #[error("create temporary config beside `{path}`: {source}")]
    CreateTemporary {
        /// Final config path.
        path: PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: io::Error,
    },
    /// The temporary config could not be written or synchronized.
    #[error("write temporary config `{path}`: {source}")]
    WriteTemporary {
        /// Temporary file path.
        path: PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: io::Error,
    },
    /// The temporary file could not atomically replace the destination.
    #[error("rename temporary config `{temporary_path}` to `{path}`: {source}")]
    Rename {
        /// Temporary file path.
        temporary_path: PathBuf,
        /// Final config path.
        path: PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: io::Error,
    },
}

/// Returns the legacy-compatible adjacent backup path for a config file.
#[must_use]
pub fn config_backup_path(config_path: &Path) -> PathBuf {
    let mut backup = config_path.as_os_str().to_os_string();
    backup.push(".bak");
    PathBuf::from(backup)
}

/// Validates and atomically writes a typed config, optionally backing up the existing file first.
///
/// The destination parent is created when missing. The temporary file is created beside the
/// destination, synchronized, and renamed over the final path so readers never observe a partial
/// JSON document.
pub fn write_config_file(
    config: &VinpstConfig,
    output_path: &Path,
    backup_path: Option<&Path>,
) -> Result<ConfigWriteReceipt, ConfigWriteError> {
    config.validate()?;
    let resolved_output = resolve_symlink_write_target(output_path).map_err(|source| {
        ConfigWriteError::ResolveTarget {
            path: output_path.to_path_buf(),
            source,
        }
    })?;

    if let Some(parent) = resolved_output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|source| ConfigWriteError::CreateDirectory {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let mut contents = serde_json::to_string_pretty(config)?;
    contents.push('\n');

    let (temporary_path, mut temporary_file) = create_temporary_file(&resolved_output)?;
    if let Err(source) = temporary_file
        .write_all(contents.as_bytes())
        .and_then(|()| temporary_file.sync_all())
    {
        drop(temporary_file);
        let _ = fs::remove_file(&temporary_path);
        return Err(ConfigWriteError::WriteTemporary {
            path: temporary_path,
            source,
        });
    }
    drop(temporary_file);

    // Stage and synchronize the complete replacement before touching an
    // existing backup. This keeps a prior recovery point intact when the
    // destination directory cannot even accept the candidate transaction.
    if let Some(backup_path) = backup_path
        && let Err(source) = fs::copy(&resolved_output, backup_path)
            .and_then(|_| set_private_file_permissions(backup_path))
    {
        let _ = fs::remove_file(&temporary_path);
        return Err(ConfigWriteError::Backup {
            path: output_path.to_path_buf(),
            backup_path: backup_path.to_path_buf(),
            source,
        });
    }

    if let Err(source) = fs::rename(&temporary_path, &resolved_output) {
        let _ = fs::remove_file(&temporary_path);
        return Err(ConfigWriteError::Rename {
            temporary_path,
            path: output_path.to_path_buf(),
            source,
        });
    }

    Ok(ConfigWriteReceipt {
        path: output_path.to_path_buf(),
        backup_path: backup_path.map(Path::to_path_buf),
    })
}

/// Resolves a final symlink write target without requiring the leaf to exist.
///
/// Relative symlink targets are interpreted relative to the link parent and
/// symlink chains are bounded to the frozen implementation's 32 levels.
pub fn resolve_symlink_write_target(path: &Path) -> io::Result<PathBuf> {
    resolve_symlink_write_target_inner(path, 0)
}

fn resolve_symlink_write_target_inner(path: &Path, depth: usize) -> io::Result<PathBuf> {
    if depth >= 32 {
        return Err(io::Error::other(format!(
            "too many symlink levels while resolving `{}`",
            path.display()
        )));
    }

    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let target = fs::read_link(path)?;
            let target = if target.is_absolute() {
                target
            } else {
                path.parent()
                    .map_or(target.clone(), |parent| parent.join(target))
            };
            resolve_symlink_write_target_inner(&target, depth + 1)
        }
        Ok(_) => Ok(path.to_path_buf()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            else {
                return Ok(path.to_path_buf());
            };
            let resolved_parent = resolve_symlink_write_target_inner(parent, depth)?;
            Ok(path
                .file_name()
                .map_or(resolved_parent.clone(), |name| resolved_parent.join(name)))
        }
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn create_temporary_file(path: &Path) -> Result<(PathBuf, fs::File), ConfigWriteError> {
    for _ in 0..32 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut temporary = path.as_os_str().to_os_string();
        temporary.push(format!(".tmp-{}-{sequence}", std::process::id()));
        let temporary_path = PathBuf::from(temporary);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        match options.open(&temporary_path) {
            Ok(file) => return Ok((temporary_path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(source) => {
                return Err(ConfigWriteError::CreateTemporary {
                    path: path.to_path_buf(),
                    source,
                });
            }
        }
    }

    Err(ConfigWriteError::CreateTemporary {
        path: path.to_path_buf(),
        source: io::Error::new(
            io::ErrorKind::AlreadyExists,
            "exhausted temporary config name attempts",
        ),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::{PermissionsExt, symlink};

    use super::*;

    #[test]
    fn atomic_write_creates_parent_and_valid_json() {
        let directory = tempfile::tempdir().expect("create temp directory");
        let path = directory.path().join("nested/config.json");
        let mut config = VinpstConfig::bundled_default().expect("bundled config");
        config.global.default_language = "zh-CN".to_owned();

        let receipt = write_config_file(&config, &path, None).expect("write config");

        assert_eq!(receipt.path, path);
        assert_eq!(receipt.backup_path, None);
        let contents = fs::read_to_string(&path).expect("read written config");
        assert!(contents.ends_with('\n'));
        let loaded = VinpstConfig::from_json_file(&path).expect("parse written config");
        assert_eq!(loaded.global.default_language, "zh-CN");
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(&path)
                .expect("written config metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn atomic_write_preserves_requested_backup() {
        let directory = tempfile::tempdir().expect("create temp directory");
        let path = directory.path().join("config.json");
        let backup_path = config_backup_path(&path);
        fs::write(&path, "old-config\n").expect("write old config");
        #[cfg(unix)]
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644))
            .expect("set legacy config permissions");
        let config = VinpstConfig::bundled_default().expect("bundled config");

        let receipt =
            write_config_file(&config, &path, Some(&backup_path)).expect("replace config");

        assert_eq!(receipt.backup_path.as_deref(), Some(backup_path.as_path()));
        assert_eq!(
            fs::read_to_string(&backup_path).expect("read backup"),
            "old-config\n"
        );
        assert!(VinpstConfig::from_json_file(&path).is_ok());
        #[cfg(unix)]
        {
            assert_eq!(
                fs::metadata(&path)
                    .expect("replaced config metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(&backup_path)
                    .expect("backup metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        let temporary_prefix =
            format!("{}{}", path.file_name().unwrap().to_string_lossy(), ".tmp-");
        assert!(
            !fs::read_dir(directory.path())
                .expect("list temp directory")
                .filter_map(Result::ok)
                .any(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&temporary_prefix))
        );
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_preserves_relative_symlink_and_backs_up_target_contents() {
        let directory = tempfile::tempdir().expect("create temp directory");
        let real_dir = directory.path().join("dotfiles");
        fs::create_dir_all(&real_dir).expect("create dotfiles directory");
        let target_path = real_dir.join("config.json");
        let link_path = directory.path().join("config.json");
        let backup_path = config_backup_path(&link_path);
        let original = VinpstConfig::bundled_default().expect("bundled config");
        let mut original_text =
            serde_json::to_string_pretty(&original).expect("serialize original");
        original_text.push('\n');
        fs::write(&target_path, &original_text).expect("seed real config target");
        symlink("dotfiles/config.json", &link_path).expect("create relative config symlink");

        let mut updated = original;
        updated.global.default_language = "en-US".to_owned();
        let receipt = write_config_file(&updated, &link_path, Some(&backup_path))
            .expect("write through config symlink");

        assert_eq!(receipt.path, link_path);
        assert!(
            fs::symlink_metadata(&link_path)
                .expect("link metadata")
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_link(&link_path).expect("read config symlink"),
            PathBuf::from("dotfiles/config.json")
        );
        let loaded = VinpstConfig::from_json_file(&target_path).expect("load updated target");
        assert_eq!(loaded.global.default_language, "en-US");
        assert_eq!(
            fs::read_to_string(&backup_path).expect("read symlink-entry backup"),
            original_text
        );
    }

    #[cfg(unix)]
    #[test]
    fn candidate_staging_failure_preserves_existing_config_and_backup() {
        let directory = tempfile::tempdir().expect("create temp directory");
        let path = directory.path().join("config.json");
        let backup_path = config_backup_path(&path);
        fs::write(&path, "current-config\n").expect("write current config");
        fs::write(&backup_path, "previous-backup\n").expect("write previous backup");
        let config = VinpstConfig::bundled_default().expect("bundled config");

        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o500))
            .expect("make directory read-only");
        let result = write_config_file(&config, &path, Some(&backup_path));
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))
            .expect("restore directory permissions");

        assert!(matches!(
            result,
            Err(ConfigWriteError::CreateTemporary { .. })
        ));
        assert_eq!(
            fs::read_to_string(&path).expect("read current config"),
            "current-config\n"
        );
        assert_eq!(
            fs::read_to_string(&backup_path).expect("read previous backup"),
            "previous-backup\n"
        );
    }

    #[test]
    fn invalid_config_does_not_touch_existing_file_or_backup() {
        let directory = tempfile::tempdir().expect("create temp directory");
        let path = directory.path().join("config.json");
        let backup_path = config_backup_path(&path);
        fs::write(&path, "old-config\n").expect("write old config");
        let mut config = VinpstConfig::bundled_default().expect("bundled config");
        config.global.default_language.clear();

        let error = write_config_file(&config, &path, Some(&backup_path))
            .expect_err("invalid config must fail");

        assert!(matches!(error, ConfigWriteError::Validation(_)));
        assert_eq!(
            fs::read_to_string(&path).expect("read existing config"),
            "old-config\n"
        );
        assert!(!backup_path.exists());
    }
}

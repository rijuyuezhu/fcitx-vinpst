//! Shared atomic config persistence helpers.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use vinput_config::VinputConfig;

use super::RuntimeError;

pub(crate) fn persist_config_atomically(
    path: &Path,
    config: &VinputConfig,
    operation: &str,
) -> Result<(), RuntimeError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| RuntimeError::PersistConfig {
        path: path.to_path_buf(),
        source,
    })?;

    let contents = serde_json::to_string_pretty(config).map_err(RuntimeError::SerializeConfig)?;
    let temp_path = temporary_config_path(path, operation);
    let write_result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .map_err(|source| RuntimeError::PersistConfig {
                path: temp_path.clone(),
                source,
            })?;
        if let Ok(metadata) = fs::metadata(path) {
            file.set_permissions(metadata.permissions())
                .map_err(|source| RuntimeError::PersistConfig {
                    path: temp_path.clone(),
                    source,
                })?;
        }
        file.write_all(contents.as_bytes())
            .and_then(|()| file.write_all(b"\n"))
            .and_then(|()| file.sync_all())
            .map_err(|source| RuntimeError::PersistConfig {
                path: temp_path.clone(),
                source,
            })?;
        fs::rename(&temp_path, path).map_err(|source| RuntimeError::PersistConfig {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result
}

fn temporary_config_path(path: &Path, operation: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.json");
    path.with_file_name(format!(
        ".{file_name}.{operation}-{}.tmp",
        std::process::id()
    ))
}

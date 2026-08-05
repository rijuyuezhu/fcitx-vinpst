//! Installed model discovery for legacy and Rust managed layouts.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::LiveVinpstModelMetadata;

/// Metadata file materialized inside every managed model directory.
pub const INSTALLED_MODEL_METADATA_FILE: &str = "vinpst-model.json";

/// One installed model discovered under a managed model root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledModelInfo {
    /// Stable model id derived from the install layout.
    pub model_id: String,
    /// Concrete model directory used as the Rust provider model value.
    pub model_dir: PathBuf,
    /// Parsed metadata file path.
    pub metadata_path: PathBuf,
    /// Typed model metadata.
    pub metadata: LiveVinpstModelMetadata,
    /// Regular files below the model directory, relative to that directory.
    pub files: Vec<String>,
    /// Number of regular files below the model directory.
    pub file_count: usize,
}

impl InstalledModelInfo {
    /// Returns the concrete model directory as a lossy UTF-8 config value.
    #[must_use]
    pub fn config_model_value(&self) -> String {
        self.model_dir.to_string_lossy().into_owned()
    }

    /// Returns the full registry id when installed display metadata provides one.
    #[must_use]
    pub fn stable_model_id(&self) -> &str {
        self.metadata
            .display
            .as_ref()
            .and_then(|display| display.registry_id.as_deref())
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .unwrap_or(&self.model_id)
    }

    /// Resolves an installed registry title for the supplied locale candidates.
    #[must_use]
    pub fn display_title(&self, locale_candidates: &[String]) -> Option<&str> {
        self.metadata
            .display
            .as_ref()
            .and_then(|display| display.resolved_title(locale_candidates))
    }
}

/// Installed model discovery failures.
#[derive(Debug, Error)]
pub enum InstalledModelError {
    /// The requested model root exists but is not a directory.
    #[error("model root `{path}` is not a directory")]
    RootNotDirectory {
        /// Rejected model root.
        path: PathBuf,
    },
    /// A directory could not be enumerated.
    #[error("failed to read installed model directory `{path}`: {source}")]
    ReadDirectory {
        /// Directory being enumerated.
        path: PathBuf,
        /// Filesystem failure.
        source: io::Error,
    },
    /// A directory entry could not be read or inspected.
    #[error("failed to inspect installed model entry under `{path}`: {source}")]
    InspectEntry {
        /// Parent directory being enumerated.
        path: PathBuf,
        /// Filesystem failure.
        source: io::Error,
    },
    /// A requested model directory does not exist.
    #[error("installed model directory `{path}` does not exist")]
    MissingModelDirectory {
        /// Missing model directory.
        path: PathBuf,
    },
    /// A requested model path is not a directory.
    #[error("installed model path `{path}` is not a directory")]
    ModelPathNotDirectory {
        /// Rejected model path.
        path: PathBuf,
    },
    /// Installed metadata could not be read.
    #[error("failed to read installed model metadata `{path}`: {source}")]
    ReadMetadata {
        /// Metadata path.
        path: PathBuf,
        /// Filesystem failure.
        source: io::Error,
    },
    /// Installed metadata could not be parsed.
    #[error("failed to parse installed model metadata `{path}`: {source}")]
    ParseMetadata {
        /// Metadata path.
        path: PathBuf,
        /// JSON failure.
        source: serde_json::Error,
    },
}

/// Scans both supported managed install layouts.
///
/// Rust CLI installs currently use `<root>/<managed-name>/vinpst-model.json`.
/// The legacy project uses `<root>/<engine>/<name>/vinpst-model.json`, with a
/// stable id of `model.<engine>.<name>`. Discovery remains shallow so model
/// asset subdirectories are never mistaken for independently installed models.
pub fn scan_installed_models(
    model_root: &Path,
) -> Result<Vec<InstalledModelInfo>, InstalledModelError> {
    if !model_root.exists() {
        return Ok(Vec::new());
    }
    if !model_root.is_dir() {
        return Err(InstalledModelError::RootNotDirectory {
            path: model_root.to_path_buf(),
        });
    }

    let mut models = Vec::new();
    for entry in read_directory(model_root)? {
        let entry = entry.map_err(|source| InstalledModelError::InspectEntry {
            path: model_root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|source| InstalledModelError::InspectEntry {
                path: model_root.to_path_buf(),
                source,
            })?;
        if !file_type.is_dir() || hidden_component(&path) {
            continue;
        }

        if path.join(INSTALLED_MODEL_METADATA_FILE).is_file() {
            let model_id = path_component(&path);
            models.push(load_installed_model_info_with_id(&path, model_id)?);
            continue;
        }

        let engine = path_component(&path);
        for model_entry in read_directory(&path)? {
            let model_entry = model_entry.map_err(|source| InstalledModelError::InspectEntry {
                path: path.clone(),
                source,
            })?;
            let model_path = model_entry.path();
            let model_type =
                model_entry
                    .file_type()
                    .map_err(|source| InstalledModelError::InspectEntry {
                        path: path.clone(),
                        source,
                    })?;
            if !model_type.is_dir()
                || hidden_component(&model_path)
                || !model_path.join(INSTALLED_MODEL_METADATA_FILE).is_file()
            {
                continue;
            }
            let name = path_component(&model_path);
            models.push(load_installed_model_info_with_id(
                &model_path,
                format!("model.{engine}.{name}"),
            )?);
        }
    }
    models.sort_by(|left, right| {
        left.model_id
            .cmp(&right.model_id)
            .then_with(|| left.model_dir.cmp(&right.model_dir))
    });
    Ok(models)
}

/// Loads one concrete installed model directory.
pub fn load_installed_model_info(
    model_dir: &Path,
) -> Result<InstalledModelInfo, InstalledModelError> {
    load_installed_model_info_with_id(model_dir, path_component(model_dir))
}

fn load_installed_model_info_with_id(
    model_dir: &Path,
    model_id: String,
) -> Result<InstalledModelInfo, InstalledModelError> {
    if !model_dir.exists() {
        return Err(InstalledModelError::MissingModelDirectory {
            path: model_dir.to_path_buf(),
        });
    }
    if !model_dir.is_dir() {
        return Err(InstalledModelError::ModelPathNotDirectory {
            path: model_dir.to_path_buf(),
        });
    }
    let metadata_path = model_dir.join(INSTALLED_MODEL_METADATA_FILE);
    let metadata_text =
        fs::read_to_string(&metadata_path).map_err(|source| InstalledModelError::ReadMetadata {
            path: metadata_path.clone(),
            source,
        })?;
    let metadata =
        serde_json::from_str::<LiveVinpstModelMetadata>(&metadata_text).map_err(|source| {
            InstalledModelError::ParseMetadata {
                path: metadata_path.clone(),
                source,
            }
        })?;
    let files = collect_installed_model_files(model_dir)?;
    let file_count = files.len();
    Ok(InstalledModelInfo {
        model_id,
        model_dir: model_dir.to_path_buf(),
        metadata_path,
        metadata,
        files,
        file_count,
    })
}

fn collect_installed_model_files(model_dir: &Path) -> Result<Vec<String>, InstalledModelError> {
    let mut files = Vec::new();
    collect_installed_model_files_inner(model_dir, model_dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_installed_model_files_inner(
    root: &Path,
    current: &Path,
    files: &mut Vec<String>,
) -> Result<(), InstalledModelError> {
    for entry in read_directory(current)? {
        let entry = entry.map_err(|source| InstalledModelError::InspectEntry {
            path: current.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|source| InstalledModelError::InspectEntry {
                path: current.to_path_buf(),
                source,
            })?;
        if file_type.is_dir() {
            collect_installed_model_files_inner(root, &path, files)?;
        } else if file_type.is_file() {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            files.push(relative.to_string_lossy().into_owned());
        }
    }
    Ok(())
}

fn read_directory(path: &Path) -> Result<fs::ReadDir, InstalledModelError> {
    fs::read_dir(path).map_err(|source| InstalledModelError::ReadDirectory {
        path: path.to_path_buf(),
        source,
    })
}

fn path_component(path: &Path) -> String {
    path.file_name()
        .and_then(|component| component.to_str())
        .unwrap_or_default()
        .to_owned()
}

fn hidden_component(path: &Path) -> bool {
    path.file_name()
        .and_then(|component| component.to_str())
        .is_some_and(|component| component.starts_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_metadata(path: &Path, family: &str) {
        fs::create_dir_all(path).unwrap();
        fs::write(
            path.join(INSTALLED_MODEL_METADATA_FILE),
            format!(r#"{{"backend":"sherpa-offline","family":"{family}"}}"#),
        )
        .unwrap();
        fs::write(path.join("tokens.txt"), "token").unwrap();
    }

    #[test]
    fn scans_flat_rust_and_nested_legacy_layouts() {
        let temp = tempfile::tempdir().unwrap();
        write_metadata(&temp.path().join("flat-model"), "sense_voice");
        write_metadata(
            &temp.path().join("sherpa-onnx").join("moonshine-v1"),
            "moonshine",
        );
        fs::create_dir_all(temp.path().join("incomplete")).unwrap();
        write_metadata(&temp.path().join(".hidden"), "sense_voice");

        let models = scan_installed_models(temp.path()).unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].model_id, "flat-model");
        assert_eq!(models[1].model_id, "model.sherpa-onnx.moonshine-v1");
        assert_eq!(models[1].metadata.model_family(), Some("moonshine"));
        assert_eq!(
            models[1].files,
            vec!["tokens.txt", INSTALLED_MODEL_METADATA_FILE]
        );
    }

    #[test]
    fn missing_root_is_empty_and_file_root_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing");
        assert!(scan_installed_models(&missing).unwrap().is_empty());

        let file = temp.path().join("file");
        fs::write(&file, "x").unwrap();
        assert!(matches!(
            scan_installed_models(&file).unwrap_err(),
            InstalledModelError::RootNotDirectory { .. }
        ));
    }

    #[test]
    fn installed_display_metadata_exposes_registry_id_and_localized_title() {
        let temp = tempfile::tempdir().unwrap();
        let model = temp.path().join("managed-name");
        fs::create_dir_all(&model).unwrap();
        fs::write(
            model.join(INSTALLED_MODEL_METADATA_FILE),
            r#"{
              "backend":"sherpa-offline",
              "family":"moonshine",
              "display":{
                "registry_id":"model.sherpa-onnx.moonshine-v1",
                "localized_titles":{"zh_CN":"月光语音模型"}
              }
            }"#,
        )
        .unwrap();

        let models = scan_installed_models(temp.path()).unwrap();
        assert_eq!(
            models[0].stable_model_id(),
            "model.sherpa-onnx.moonshine-v1"
        );
        assert_eq!(
            models[0].display_title(&["zh_CN.UTF-8".to_owned()]),
            Some("月光语音模型")
        );
        assert_eq!(models[0].display_title(&["en_US".to_owned()]), None);
    }

    #[test]
    fn invalid_metadata_reports_the_concrete_path() {
        let temp = tempfile::tempdir().unwrap();
        let model = temp.path().join("broken");
        fs::create_dir_all(&model).unwrap();
        fs::write(model.join(INSTALLED_MODEL_METADATA_FILE), "not-json").unwrap();

        let error = scan_installed_models(temp.path()).unwrap_err();
        assert!(matches!(error, InstalledModelError::ParseMetadata { .. }));
        assert!(error.to_string().contains("broken/vinpst-model.json"));
    }
}

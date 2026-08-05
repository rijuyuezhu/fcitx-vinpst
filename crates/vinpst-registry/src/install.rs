//! Live model install orchestration boundary.
//!
//! This module connects the already-reviewable asset download, checksum, archive
//! staging, metadata writing, and materialization boundaries for one live model.
//! It intentionally does not mutate user configuration or select the active ASR
//! model.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::{
    ArchiveStagingError, AssetChecksumStatus, InstalledModelDisplayMetadata, LiveModelEntry,
    MaterializedRegistryTree, PlannedInstallAsset, RegistryAssetSource, RegistryAssetStagingError,
    RegistryEntryKind, RegistryMaterializeError, RegistryOperationControl,
    RegistryOperationProgress, StagedArchiveTree, StagedRegistryAsset,
    materialize_staged_tree_controlled, stage_archive_by_format_controlled,
    stage_planned_asset_controlled,
};

/// Request for installing one live registry model into a managed model directory.
#[derive(Debug, Clone)]
pub struct LiveModelInstallRequest<'a> {
    /// Live registry model entry selected by id or short id.
    pub model: &'a LiveModelEntry,
    /// Final managed model directory.
    pub model_dir: PathBuf,
    /// Temporary staging directory dedicated to this install attempt.
    pub staging_dir: PathBuf,
    /// Optional registry display metadata to persist with the runtime metadata.
    pub display: Option<InstalledModelDisplayMetadata>,
}

/// Successful live model install result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveModelInstallResult {
    /// Live model id that was installed.
    pub model_id: String,
    /// Optional live registry short id.
    pub short_id: Option<String>,
    /// Staged archive file downloaded and verified before extraction.
    pub staged_asset: StagedRegistryAsset,
    /// Archive extraction result.
    pub staged_archive: StagedArchiveTree,
    /// Source directory materialized into the managed model directory.
    pub materialize_source_path: PathBuf,
    /// Final materialization result.
    pub materialized: MaterializedRegistryTree,
    /// Metadata path written inside the materialized model directory.
    pub metadata_path: PathBuf,
}

/// Live model install orchestration errors.
#[derive(Debug, Error)]
pub enum LiveModelInstallError {
    /// The caller requested cooperative cancellation.
    #[error("live model install cancelled for `{id}`")]
    Cancelled {
        /// Live model id.
        id: String,
    },
    /// The live model has no `vinpst_model` metadata to materialize.
    #[error("live model `{id}` has no vinpst_model metadata")]
    MissingMetadata {
        /// Live model id.
        id: String,
    },
    /// The live model has no candidate download URLs.
    #[error("live model `{id}` has no candidate download URLs")]
    MissingDownloadUrl {
        /// Live model id.
        id: String,
    },
    /// The first model URL did not contain an archive filename.
    #[error("live model `{id}` has no archive file name in first URL")]
    MissingArchiveFileName {
        /// Live model id.
        id: String,
    },
    /// Asset download/checksum staging failed.
    #[error("failed to stage live model archive for `{id}`: {source}")]
    StageAsset {
        /// Live model id.
        id: String,
        /// Asset staging failure.
        source: Box<RegistryAssetStagingError>,
    },
    /// Archive extraction staging failed.
    #[error("failed to extract live model archive for `{id}`: {source}")]
    StageArchive {
        /// Live model id.
        id: String,
        /// Archive staging failure.
        source: Box<ArchiveStagingError>,
    },
    /// A stale extraction tree from an earlier attempt could not be removed.
    #[error("failed to reset live model extraction directory for `{id}` at `{path}`: {message}")]
    ResetExtractDir {
        /// Live model id.
        id: String,
        /// Extraction directory path.
        path: String,
        /// Sanitized I/O failure.
        message: String,
    },
    /// Extracted archive tree could not be inspected.
    #[error("failed to inspect extracted live model tree for `{id}`: {message}")]
    InspectExtractedTree {
        /// Live model id.
        id: String,
        /// Sanitized I/O failure.
        message: String,
    },
    /// vinpst-model.json metadata serialization failed.
    #[error("failed to serialize vinpst-model.json for `{id}`: {message}")]
    SerializeMetadata {
        /// Live model id.
        id: String,
        /// Serialization failure.
        message: String,
    },
    /// vinpst-model.json metadata write failed.
    #[error("failed to write vinpst-model.json for `{id}` at `{path}`: {message}")]
    WriteMetadata {
        /// Live model id.
        id: String,
        /// Metadata path.
        path: String,
        /// Sanitized I/O failure.
        message: String,
    },
    /// Final materialization failed.
    #[error("failed to materialize live model `{id}`: {source}")]
    Materialize {
        /// Live model id.
        id: String,
        /// Materialization failure.
        source: Box<RegistryMaterializeError>,
    },
}

/// Downloads, verifies, extracts, writes metadata, and materializes one live model.
///
/// The caller supplies an asset source so tests can inject local fixtures while
/// the CLI can use the reqwest-backed source. This function never mutates user
/// configuration.
pub fn install_live_model(
    source: &impl RegistryAssetSource,
    request: &LiveModelInstallRequest<'_>,
) -> Result<LiveModelInstallResult, LiveModelInstallError> {
    install_live_model_controlled(source, request, &RegistryOperationControl::default())
}

/// Controlled companion to [`install_live_model`].
pub fn install_live_model_controlled(
    source: &impl RegistryAssetSource,
    request: &LiveModelInstallRequest<'_>,
    control: &RegistryOperationControl,
) -> Result<LiveModelInstallResult, LiveModelInstallError> {
    let model = request.model;
    control.report(RegistryOperationProgress::Preparing);
    check_cancelled(control, model)?;
    let mut metadata = model
        .vinpst_model
        .as_ref()
        .ok_or_else(|| LiveModelInstallError::MissingMetadata {
            id: model.id.clone(),
        })?
        .clone();
    if let Some(display) = &request.display {
        metadata.display = Some(display.clone());
    }
    let archive_file_name = model_archive_file_name(model)?;
    let archive_path = request.staging_dir.join("archives").join(archive_file_name);
    let extract_dir = request.staging_dir.join("extract");

    let asset = live_model_planned_asset(model, archive_file_name);
    let staged_asset = stage_planned_asset_controlled(source, &asset, &archive_path, control)
        .map_err(|source| map_asset_error(model, source))?;
    check_cancelled(control, model)?;
    if extract_dir.exists() {
        fs::remove_dir_all(&extract_dir).map_err(|error| {
            LiveModelInstallError::ResetExtractDir {
                id: model.id.clone(),
                path: display_path(&extract_dir),
                message: sanitize_io_error(&error),
            }
        })?;
    }
    let staged_archive =
        stage_archive_by_format_controlled(&staged_asset.path, &extract_dir, control)
            .map_err(|source| map_archive_error(model, source))?;
    check_cancelled(control, model)?;

    let materialize_source =
        select_materialize_source(&staged_archive.path).map_err(|message| {
            LiveModelInstallError::InspectExtractedTree {
                id: model.id.clone(),
                message,
            }
        })?;
    let metadata_path_in_source = materialize_source.join("vinpst-model.json");
    control.report(RegistryOperationProgress::WritingMetadata);
    let metadata_json =
        serde_json::to_string_pretty(&metadata.to_raw_value().map_err(|error| {
            LiveModelInstallError::SerializeMetadata {
                id: model.id.clone(),
                message: error.to_string(),
            }
        })?)
        .map_err(|error| LiveModelInstallError::SerializeMetadata {
            id: model.id.clone(),
            message: error.to_string(),
        })?;
    fs::write(&metadata_path_in_source, format!("{metadata_json}\n")).map_err(|error| {
        LiveModelInstallError::WriteMetadata {
            id: model.id.clone(),
            path: display_path(&metadata_path_in_source),
            message: sanitize_io_error(&error),
        }
    })?;

    check_cancelled(control, model)?;
    control.report(RegistryOperationProgress::Publishing);
    let materialized =
        materialize_staged_tree_controlled(&materialize_source, &request.model_dir, control)
            .map_err(|source| map_materialize_error(model, source))?;
    control.report(RegistryOperationProgress::Completed);

    Ok(LiveModelInstallResult {
        model_id: model.id.clone(),
        short_id: model.short_id.clone(),
        staged_asset,
        staged_archive,
        materialize_source_path: materialize_source,
        metadata_path: request.model_dir.join("vinpst-model.json"),
        materialized,
    })
}

fn check_cancelled(
    control: &RegistryOperationControl,
    model: &LiveModelEntry,
) -> Result<(), LiveModelInstallError> {
    control
        .check_cancelled()
        .map_err(|_| LiveModelInstallError::Cancelled {
            id: model.id.clone(),
        })
}

fn map_asset_error(
    model: &LiveModelEntry,
    source: RegistryAssetStagingError,
) -> LiveModelInstallError {
    if matches!(source, RegistryAssetStagingError::Cancelled { .. }) {
        LiveModelInstallError::Cancelled {
            id: model.id.clone(),
        }
    } else {
        LiveModelInstallError::StageAsset {
            id: model.id.clone(),
            source: Box::new(source),
        }
    }
}

fn map_archive_error(model: &LiveModelEntry, source: ArchiveStagingError) -> LiveModelInstallError {
    if matches!(source, ArchiveStagingError::Cancelled { .. }) {
        LiveModelInstallError::Cancelled {
            id: model.id.clone(),
        }
    } else {
        LiveModelInstallError::StageArchive {
            id: model.id.clone(),
            source: Box::new(source),
        }
    }
}

fn map_materialize_error(
    model: &LiveModelEntry,
    source: RegistryMaterializeError,
) -> LiveModelInstallError {
    if matches!(source, RegistryMaterializeError::Cancelled { .. }) {
        LiveModelInstallError::Cancelled {
            id: model.id.clone(),
        }
    } else {
        LiveModelInstallError::Materialize {
            id: model.id.clone(),
            source: Box::new(source),
        }
    }
}

fn live_model_planned_asset(
    model: &LiveModelEntry,
    archive_file_name: &str,
) -> PlannedInstallAsset {
    PlannedInstallAsset {
        entry_kind: RegistryEntryKind::Model,
        entry_id: model.id.clone(),
        source_path: archive_file_name.to_owned(),
        target_path: archive_file_name.to_owned(),
        urls: model.urls.clone(),
        sha256: model.sha256.clone(),
        size_bytes: model.size_bytes,
        checksum_policy: if model.sha256.is_some() {
            crate::ChecksumPolicy::Sha256
        } else {
            crate::ChecksumPolicy::Missing
        },
    }
}

fn model_archive_file_name(model: &LiveModelEntry) -> Result<&str, LiveModelInstallError> {
    let first_url =
        model
            .urls
            .first()
            .ok_or_else(|| LiveModelInstallError::MissingDownloadUrl {
                id: model.id.clone(),
            })?;
    let file_name = first_url
        .rsplit('/')
        .next()
        .unwrap_or(first_url)
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    if file_name.is_empty() {
        Err(LiveModelInstallError::MissingArchiveFileName {
            id: model.id.clone(),
        })
    } else {
        Ok(file_name)
    }
}

fn select_materialize_source(extract_dir: &Path) -> Result<PathBuf, String> {
    let mut entries = fs::read_dir(extract_dir)
        .map_err(|error| sanitize_io_error(&error))?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_none_or(|name| !name.starts_with('.'))
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    if entries.len() == 1 {
        let only = entries.remove(0);
        if only
            .file_type()
            .map_err(|error| sanitize_io_error(&error))?
            .is_dir()
        {
            return Ok(only.path());
        }
    }
    Ok(extract_dir.to_owned())
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

fn sanitize_io_error(error: &io::Error) -> String {
    error.kind().to_string()
}

impl LiveModelInstallResult {
    /// Returns true when the installed archive checksum was verified with SHA-256.
    #[must_use]
    pub fn checksum_verified(&self) -> bool {
        matches!(
            self.staged_asset.checksum,
            AssetChecksumStatus::VerifiedSha256(_)
        )
    }
}

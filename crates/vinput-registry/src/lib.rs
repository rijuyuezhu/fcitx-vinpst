//! Registry manifest models, URL resolution helpers, and staged asset boundaries.
//!
//! Registry side-effect boundaries can download/verify one planned asset, extract
//! a staged tar, tar.zst, or tar.bz2 archive into a temporary tree, and materialize
//! a prepared staged directory with local filesystem renames. Configuration mutation and
//! user-facing install commands are intentionally still outside this crate.

mod archive;
mod asset;
mod cache;
mod checksum;
mod error;
mod fetch;
mod install;
mod installed;
mod live;
mod materialize;
mod plan;
mod schema;
mod staging;

pub use archive::{
    ArchiveEntryKind, ArchiveFormat, ArchiveSafetyError, ArchiveStagingError, StagedArchiveTree,
    checked_archive_entry_target, stage_archive_by_format, stage_tar_archive,
    stage_tar_bz2_archive, stage_tar_zst_archive,
};
pub use asset::{
    AssetChecksumStatus, RegistryAssetFetchFailure, RegistryAssetSource, RegistryAssetStagingError,
    ReqwestRegistryAssetSource, StagedRegistryAsset, stage_planned_asset,
};
pub use cache::{
    RegistryCacheError, RegistryCachedFetchError, RegistryTextCache,
    fetch_registry_index_with_cache,
};
pub use checksum::{
    RegistrySha256Error, sha256_hex, verify_sha256_bytes, verify_sha256_file, verify_sha256_reader,
};
pub use error::RegistryError;
pub use fetch::{
    RegistryFetchError, RegistryFetchFailure, RegistryTextSource, ReqwestRegistryTextSource,
    fetch_registry_index_from_mirrors,
};
pub use install::{
    LiveModelInstallError, LiveModelInstallRequest, LiveModelInstallResult, install_live_model,
};
pub use installed::{
    INSTALLED_MODEL_METADATA_FILE, InstalledModelError, InstalledModelInfo,
    load_installed_model_info, scan_installed_models,
};
pub use live::{
    InstalledModelDisplayMetadata, LiveModelEntry, LiveModelFamily, LiveModelRegistry,
    LiveRegistryI18n, LiveVinputModelMetadata,
};
pub use materialize::{
    MaterializedRegistryTree, RegistryMaterializeError, materialize_staged_tree,
};
pub use plan::{
    AssetPlanSummary, ChecksumPolicy, InstallPlan, InstallPlanSummary, PlannedAsset,
    PlannedInstallAsset, RegistryEntryKind,
};
pub use schema::{AdapterEntry, AssetEntry, ModelEntry, RegistryIndex, RegistryIndexSummary};
pub use staging::{
    ArchiveStagingPathError, ArchiveStagingPaths, plan_archive_staging_paths,
    plan_archive_staging_paths_for_plan,
};

#[cfg(test)]
mod tests;

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
mod managed;
mod materialize;
mod operation;
mod plan;
mod provider_script;
mod schema;
mod script;
mod staging;

pub use archive::{
    ArchiveEntryKind, ArchiveFormat, ArchiveSafetyError, ArchiveStagingError, StagedArchiveTree,
    checked_archive_entry_target, stage_archive_by_format, stage_archive_by_format_controlled,
    stage_tar_archive, stage_tar_bz2_archive, stage_tar_zst_archive,
};
pub use asset::{
    AssetChecksumStatus, RegistryAssetFetchFailure, RegistryAssetSource, RegistryAssetStagingError,
    ReqwestRegistryAssetSource, StagedRegistryAsset, stage_planned_asset,
    stage_planned_asset_controlled,
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
    install_live_model_controlled,
};
pub use installed::{
    INSTALLED_MODEL_METADATA_FILE, InstalledModelError, InstalledModelInfo,
    load_installed_model_info, scan_installed_models,
};
pub use live::{
    InstalledModelDisplayMetadata, LiveModelEntry, LiveModelFamily, LiveModelRegistry,
    LiveRegistryI18n, LiveVinputModelMetadata, detect_preferred_registry_locale,
    normalize_registry_locale, select_preferred_registry_locale,
};
pub use managed::{
    ManagedModelRemoveError, ManagedModelRemoveRequest, ManagedModelRemoveResult,
    managed_model_dir_name, remove_managed_model, safe_path_component,
    validate_managed_model_target,
};
pub use materialize::{
    MaterializedRegistryTree, RegistryMaterializeError, materialize_staged_tree,
    materialize_staged_tree_controlled,
};
pub use operation::{
    RegistryOperationCancelled, RegistryOperationControl, RegistryOperationProgress,
};
pub use plan::{
    AssetPlanSummary, ChecksumPolicy, InstallPlan, InstallPlanSummary, PlannedAsset,
    PlannedInstallAsset, RegistryEntryKind,
};
pub use provider_script::{
    ProviderEditorCommand, ProviderScriptEditError, ProviderScriptEditOutcome,
    ProviderScriptEditPlan, ProviderScriptResolutionContext, prepare_provider_script_edit,
    prepare_provider_script_edit_with, resolve_editable_provider_script,
    resolve_editable_provider_script_with,
};
pub use schema::{AdapterEntry, AssetEntry, ModelEntry, RegistryIndex, RegistryIndexSummary};
pub use script::{
    AsrProviderMaterialization, AsrProviderMaterializationError, LiveScriptEntry,
    LiveScriptEnvSpec, LiveScriptInstallError, LiveScriptInstallResult, LiveScriptKind,
    LiveScriptRegistry, LiveScriptRegistryError, LlmAdapterMaterialization,
    LlmAdapterMaterializationError, install_live_script, install_live_script_controlled,
    managed_script_relative_path, managed_script_rollback_path, materialize_asr_provider,
    materialize_llm_adapter,
};
pub use staging::{
    ArchiveStagingPathError, ArchiveStagingPaths, plan_archive_staging_paths,
    plan_archive_staging_paths_for_plan,
};

#[cfg(test)]
mod tests;

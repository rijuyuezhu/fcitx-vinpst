//! Live provider/adapter script registry and managed installation helpers.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use vinpst_config::{AsrProviderConfig, AsrProviderKind, LlmAdapterConfig};

use crate::live::LiveRegistryI18n;
use crate::{
    AssetChecksumStatus, ChecksumPolicy, PlannedInstallAsset, RegistryAssetSource,
    RegistryAssetStagingError, RegistryEntryKind, RegistryOperationControl, StagedRegistryAsset,
    stage_planned_asset_controlled,
};

/// Registry category for one managed script.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveScriptKind {
    /// External command ASR provider.
    AsrProvider,
    /// External command text adapter.
    LlmAdapter,
}

impl LiveScriptKind {
    const fn id_prefix(self) -> &'static str {
        match self {
            Self::AsrProvider => "provider",
            Self::LlmAdapter => "adapter",
        }
    }

    const fn registry_entry_kind(self) -> RegistryEntryKind {
        match self {
            Self::AsrProvider => RegistryEntryKind::Provider,
            Self::LlmAdapter => RegistryEntryKind::Adapter,
        }
    }
}

/// Environment variable requested by a registry script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LiveScriptEnvSpec {
    /// Environment variable name.
    pub name: String,
    /// Whether the registry marks the variable as required.
    #[serde(default)]
    pub required: bool,
}

/// One entry from `registry/providers.json` or `registry/adapters.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LiveScriptEntry {
    /// Stable machine id such as `adapter.mtranserver.proxy`.
    pub id: String,
    /// Optional CLI-friendly selector.
    #[serde(default)]
    pub short_id: Option<String>,
    /// Whether an ASR provider uses the streaming command protocol.
    #[serde(default)]
    pub stream: bool,
    /// Executable used to launch the downloaded script.
    pub command: String,
    /// Ordered mirror URLs for the script body.
    pub script_urls: Vec<String>,
    /// Optional user-facing README URL.
    #[serde(default)]
    pub readme_url: Option<String>,
    /// Environment variables understood by the script.
    #[serde(default)]
    pub envs: Vec<LiveScriptEnvSpec>,
}

impl LiveScriptEntry {
    /// User-facing selector, preferring the registry short id when present.
    #[must_use]
    pub fn display_id(&self) -> &str {
        non_empty(self.short_id.as_deref()).unwrap_or(self.id.as_str())
    }

    /// Resolves the localized display title, then falls back to `short_id` or full id.
    #[must_use]
    pub fn resolved_title(&self, i18n: Option<&LiveRegistryI18n>) -> String {
        i18n.and_then(|map| map.get(&format!("{}.title", self.id)))
            .map_or_else(|| self.display_id().to_owned(), str::to_owned)
    }

    /// Resolves the localized display description.
    #[must_use]
    pub fn resolved_description(&self, i18n: Option<&LiveRegistryI18n>) -> Option<String> {
        i18n.and_then(|map| map.get(&format!("{}.description", self.id)))
            .map(str::to_owned)
    }
}

/// Current upstream script registry document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LiveScriptRegistry {
    /// Registry schema version.
    pub version: u32,
    /// Script entries in upstream order.
    pub items: Vec<LiveScriptEntry>,
}

impl LiveScriptRegistry {
    /// Parses and validates one provider or adapter registry.
    pub fn from_json_str(
        input: &str,
        kind: LiveScriptKind,
    ) -> Result<Self, LiveScriptRegistryError> {
        let registry: Self = serde_json::from_str(input)?;
        registry.validate(kind)?;
        Ok(registry)
    }

    /// Validates one provider or adapter registry.
    pub fn validate(&self, kind: LiveScriptKind) -> Result<(), LiveScriptRegistryError> {
        if self.version == 0 {
            return Err(LiveScriptRegistryError::InvalidVersion);
        }
        let mut ids = HashSet::new();
        let mut short_ids = HashSet::new();
        for entry in &self.items {
            validate_script_entry(entry, kind)?;
            if !ids.insert(entry.id.as_str()) {
                return Err(LiveScriptRegistryError::DuplicateId(entry.id.clone()));
            }
            if let Some(short_id) = non_empty(entry.short_id.as_deref())
                && !short_ids.insert(short_id)
            {
                return Err(LiveScriptRegistryError::DuplicateShortId(
                    short_id.to_owned(),
                ));
            }
        }
        Ok(())
    }

    /// Resolves a full id or `short_id` within one registry category.
    #[must_use]
    pub fn entry_by_id_or_short_id(
        &self,
        selector: &str,
        kind: LiveScriptKind,
    ) -> Option<&LiveScriptEntry> {
        self.items.iter().find(|entry| {
            entry.id == selector
                || entry.short_id.as_deref() == Some(selector)
                    && managed_script_relative_path(kind, &entry.id).is_ok()
        })
    }
}

/// Result of downloading and publishing one managed script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveScriptInstallResult {
    /// Registry id that was installed.
    pub id: String,
    /// Optional short selector.
    pub short_id: Option<String>,
    /// Final executable script path.
    pub script_path: PathBuf,
    /// Checksum status inherited from the generic staging boundary.
    pub checksum: AssetChecksumStatus,
}

/// Planned adapter configuration and overwrite classification.
#[derive(Debug, Clone, PartialEq)]
pub struct LlmAdapterMaterialization {
    /// Adapter entry to write into configuration.
    pub adapter: LlmAdapterConfig,
    /// Whether an existing managed adapter is being updated.
    pub replacing_managed: bool,
}

/// Planned command ASR provider configuration and overwrite classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrProviderMaterialization {
    /// Provider entry to write into configuration.
    pub provider: AsrProviderConfig,
    /// Whether an existing managed provider is being updated.
    pub replacing_managed: bool,
}

/// Script registry validation errors.
#[derive(Debug, Error)]
pub enum LiveScriptRegistryError {
    /// JSON decoding failed.
    #[error("failed to parse script registry JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// Registry schema version is invalid.
    #[error("script registry version must be greater than zero")]
    InvalidVersion,
    /// A registry id is duplicated.
    #[error("duplicate script registry id `{0}`")]
    DuplicateId(String),
    /// A registry short id is duplicated.
    #[error("duplicate script registry short_id `{0}`")]
    DuplicateShortId(String),
    /// Entry fields are invalid.
    #[error("invalid script registry entry `{id}`: {message}")]
    InvalidEntry {
        /// Registry entry id.
        id: String,
        /// Validation failure.
        message: String,
    },
    /// Resource id cannot be mapped to a managed path.
    #[error("invalid managed script id `{0}`")]
    InvalidResourceId(String),
    /// Resource id belongs to another registry category.
    #[error("managed script id `{id}` does not belong to `{expected}` registry")]
    WrongResourceKind {
        /// Registry entry id.
        id: String,
        /// Expected leading resource segment.
        expected: &'static str,
    },
}

/// Managed script installation errors.
#[derive(Debug, Error)]
pub enum LiveScriptInstallError {
    /// Registry metadata is invalid.
    #[error(transparent)]
    Registry(#[from] LiveScriptRegistryError),
    /// Generic asset staging failed.
    #[error(transparent)]
    Stage(#[from] RegistryAssetStagingError),
    /// Executable permissions could not be applied.
    #[error("failed to mark managed script `{path}` executable: {message}")]
    Permissions {
        /// Installed script path.
        path: String,
        /// Sanitized filesystem failure.
        message: String,
    },
}

/// Adapter materialization errors.
#[derive(Debug, Error)]
pub enum LlmAdapterMaterializationError {
    /// Registry metadata is invalid for an adapter.
    #[error(transparent)]
    Registry(#[from] LiveScriptRegistryError),
    /// An existing user-defined adapter must not be overwritten.
    #[error("refusing to overwrite user-defined text adapter `{0}`")]
    UserDefinedAdapter(String),
}

/// ASR provider materialization errors.
#[derive(Debug, Error)]
pub enum AsrProviderMaterializationError {
    /// Registry metadata is invalid for a provider.
    #[error(transparent)]
    Registry(#[from] LiveScriptRegistryError),
    /// An existing user-defined provider must not be overwritten.
    #[error("refusing to overwrite user-defined ASR provider `{0}`")]
    UserDefinedProvider(String),
}

/// Returns the deterministic adjacent rollback artifact for a managed script.
#[must_use]
pub fn managed_script_rollback_path(script_path: impl AsRef<Path>) -> PathBuf {
    let script_path = script_path.as_ref();
    let mut rollback = script_path.as_os_str().to_os_string();
    rollback.push(".rollback");
    PathBuf::from(rollback)
}

/// Returns the legacy-compatible managed relative path for a registry id.
///
/// The first segment names the resource kind, the second segment is a directory,
/// and all remaining dot-separated segments form the script filename.
pub fn managed_script_relative_path(
    kind: LiveScriptKind,
    id: &str,
) -> Result<PathBuf, LiveScriptRegistryError> {
    let segments = split_resource_id(id)?;
    if segments[0] != kind.id_prefix() {
        return Err(LiveScriptRegistryError::WrongResourceKind {
            id: id.to_owned(),
            expected: kind.id_prefix(),
        });
    }
    Ok(PathBuf::from(&segments[1]).join(&segments[2]))
}

/// Downloads a registry script into its managed root and marks it executable.
pub fn install_live_script(
    source: &impl RegistryAssetSource,
    kind: LiveScriptKind,
    entry: &LiveScriptEntry,
    script_root: impl AsRef<Path>,
) -> Result<LiveScriptInstallResult, LiveScriptInstallError> {
    install_live_script_controlled(
        source,
        kind,
        entry,
        script_root,
        &RegistryOperationControl::default(),
    )
}

/// Controlled companion to [`install_live_script`].
pub fn install_live_script_controlled(
    source: &impl RegistryAssetSource,
    kind: LiveScriptKind,
    entry: &LiveScriptEntry,
    script_root: impl AsRef<Path>,
    control: &RegistryOperationControl,
) -> Result<LiveScriptInstallResult, LiveScriptInstallError> {
    validate_script_entry(entry, kind)?;
    let relative_path = managed_script_relative_path(kind, &entry.id)?;
    let output_path = script_root.as_ref().join(&relative_path);
    let asset = PlannedInstallAsset {
        entry_kind: kind.registry_entry_kind(),
        entry_id: entry.id.clone(),
        source_path: relative_path.to_string_lossy().into_owned(),
        target_path: output_path.to_string_lossy().into_owned(),
        urls: entry.script_urls.clone(),
        sha256: None,
        size_bytes: None,
        checksum_policy: ChecksumPolicy::Missing,
    };
    let staged = stage_planned_asset_controlled(source, &asset, &output_path, control)?;
    if let Err(error) = mark_executable(&staged.path) {
        let _ = fs::remove_file(&staged.path);
        return Err(LiveScriptInstallError::Permissions {
            path: staged.path.display().to_string(),
            message: error.to_string(),
        });
    }
    Ok(install_result(entry, staged))
}

/// Builds an adapter config entry while preserving existing env values and
/// refusing to replace an adapter that is not already bound to the expected
/// managed script path.
pub fn materialize_llm_adapter(
    entry: &LiveScriptEntry,
    script_path: impl AsRef<Path>,
    existing: Option<&LlmAdapterConfig>,
) -> Result<LlmAdapterMaterialization, LlmAdapterMaterializationError> {
    validate_script_entry(entry, LiveScriptKind::LlmAdapter)?;
    let script_path = script_path.as_ref().to_string_lossy().into_owned();
    let mut adapter = match existing {
        Some(existing) => {
            if existing.args.as_slice() != [script_path.as_str()] {
                return Err(LlmAdapterMaterializationError::UserDefinedAdapter(
                    entry.id.clone(),
                ));
            }
            existing.clone()
        }
        None => LlmAdapterConfig {
            id: entry.id.clone(),
            command: entry.command.clone(),
            args: vec![script_path.clone()],
            env: HashMap::new(),
            working_dir: None,
            extra: HashMap::new(),
        },
    };
    adapter.id.clone_from(&entry.id);
    adapter.command.clone_from(&entry.command);
    adapter.args = vec![script_path];
    for env in &entry.envs {
        adapter.env.entry(env.name.clone()).or_default();
    }
    Ok(LlmAdapterMaterialization {
        adapter,
        replacing_managed: existing.is_some(),
    })
}

/// Builds a command ASR provider while preserving existing environment values
/// and refusing to replace non-command or non-managed providers.
pub fn materialize_asr_provider(
    entry: &LiveScriptEntry,
    script_path: impl AsRef<Path>,
    existing: Option<&AsrProviderConfig>,
) -> Result<AsrProviderMaterialization, AsrProviderMaterializationError> {
    validate_script_entry(entry, LiveScriptKind::AsrProvider)?;
    let script_path = script_path.as_ref().to_string_lossy().into_owned();
    let mut provider = match existing {
        Some(existing) => {
            if existing.kind != AsrProviderKind::Command
                || existing.args.as_slice() != [script_path.as_str()]
            {
                return Err(AsrProviderMaterializationError::UserDefinedProvider(
                    entry.id.clone(),
                ));
            }
            existing.clone()
        }
        None => AsrProviderConfig {
            id: entry.id.clone(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(60_000),
            model: None,
            hotwords_file: None,
            command: Some(entry.command.clone()),
            args: vec![script_path.clone()],
            env: HashMap::new(),
            endpoint: None,
        },
    };
    provider.id.clone_from(&entry.id);
    provider.kind = AsrProviderKind::Command;
    provider.command = Some(entry.command.clone());
    provider.args = vec![script_path];
    if provider.timeout_ms.unwrap_or(0) == 0 {
        provider.timeout_ms = Some(60_000);
    }
    for env in &entry.envs {
        provider.env.entry(env.name.clone()).or_default();
    }
    Ok(AsrProviderMaterialization {
        provider,
        replacing_managed: existing.is_some(),
    })
}

fn validate_script_entry(
    entry: &LiveScriptEntry,
    kind: LiveScriptKind,
) -> Result<(), LiveScriptRegistryError> {
    managed_script_relative_path(kind, &entry.id)?;
    if entry.command.trim().is_empty() {
        return invalid_entry(entry, "command cannot be empty");
    }
    if entry.script_urls.is_empty() || entry.script_urls.iter().any(|url| url.trim().is_empty()) {
        return invalid_entry(entry, "script_urls must contain non-empty URLs");
    }
    if entry
        .short_id
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return invalid_entry(entry, "short_id cannot be empty");
    }
    if kind == LiveScriptKind::AsrProvider && entry.stream != entry.id.ends_with(".streaming") {
        return invalid_entry(
            entry,
            "stream must match the provider id `.streaming` suffix",
        );
    }
    let mut env_names = HashSet::new();
    for env in &entry.envs {
        if env.name.trim().is_empty() {
            return invalid_entry(entry, "environment variable name cannot be empty");
        }
        if !env_names.insert(env.name.as_str()) {
            return invalid_entry(entry, "environment variable names must be unique");
        }
    }
    Ok(())
}

fn invalid_entry<T>(
    entry: &LiveScriptEntry,
    message: impl Into<String>,
) -> Result<T, LiveScriptRegistryError> {
    Err(LiveScriptRegistryError::InvalidEntry {
        id: entry.id.clone(),
        message: message.into(),
    })
}

fn split_resource_id(id: &str) -> Result<[String; 3], LiveScriptRegistryError> {
    if id.is_empty() || id == "." || id == ".." || id.contains(['/', '\\']) {
        return Err(LiveScriptRegistryError::InvalidResourceId(id.to_owned()));
    }
    let raw = id.split('.').collect::<Vec<_>>();
    if raw.len() < 3
        || raw
            .iter()
            .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
    {
        return Err(LiveScriptRegistryError::InvalidResourceId(id.to_owned()));
    }
    Ok([raw[0].to_owned(), raw[1].to_owned(), raw[2..].join(".")])
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

fn install_result(entry: &LiveScriptEntry, staged: StagedRegistryAsset) -> LiveScriptInstallResult {
    LiveScriptInstallResult {
        id: entry.id.clone(),
        short_id: entry.short_id.clone(),
        script_path: staged.path,
        checksum: staged.checksum,
    }
}

#[cfg(unix)]
fn mark_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(permissions.mode() | 0o111);
    fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
fn mark_executable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

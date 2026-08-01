mod catalog;
mod install;
mod remove;
mod support;
mod use_model;

use catalog::{handle_model_list_command, print_model_info};
use install::print_model_install_plan;
use remove::print_model_remove_plan;
use use_model::print_model_use_preview;

use crate::{
    ArchiveFormat, AsrProviderKind, Context, Duration, InstalledModelInfo, LiveModelEntry,
    LiveModelFamily, LiveModelInstallRequest, LiveModelInstallResult, LiveModelRegistry,
    LiveRegistryI18n, LoadedLiveI18n, ModelCommand, Path, PathBuf, RegistryConfig,
    ReqwestRegistryAssetSource, ReqwestRegistryTextSource, VinputConfig, config_backup_path, dbus,
    default_model_install_staging_root, default_model_root, fetch_text_from_mirrors, fs,
    install_live_model, live_registry_urls, load_config_file, load_live_i18n,
    load_registry_installed_model_info, reload_asr_backend_via_dbus, same_path_text,
    scan_installed_models, write_config_in_place, write_config_output,
};

#[allow(clippy::too_many_lines)]
pub(crate) fn handle_model_command(command: ModelCommand) -> anyhow::Result<()> {
    match command {
        ModelCommand::List {
            available,
            installed,
            model_root,
            registry,
            i18n,
            config,
            locale,
            json,
        } => handle_model_list_command(&ModelListOwnedRequest {
            available,
            installed,
            model_root,
            registry,
            i18n,
            config,
            locale,
            json_output: json,
        }),
        ModelCommand::Info {
            id,
            installed,
            model_root,
            registry,
            i18n,
            config,
            locale,
            json,
        } => print_model_info(ModelInfoRequest {
            id_or_short_id: &id,
            installed,
            model_root: model_root.as_deref(),
            registry_path: registry.as_deref(),
            i18n_path: i18n.as_deref(),
            config_path: config.as_ref(),
            locale: &locale,
            json_output: json,
        }),
        ModelCommand::Install {
            id,
            registry,
            i18n,
            config,
            locale,
            model_root,
            staging_root,
            dry_run,
            json,
        } => print_model_install_plan(ModelInstallPlanRequest {
            id_or_short_id: &id,
            registry_path: registry.as_deref(),
            i18n_path: i18n.as_deref(),
            config_path: config.as_ref(),
            locale: &locale,
            model_root: model_root.as_deref(),
            staging_root: staging_root.as_deref(),
            dry_run,
            json_output: json,
        }),
        ModelCommand::Use {
            selector,
            installed,
            registry,
            i18n,
            config,
            locale,
            provider,
            output,
            in_place,
            model_root,
            reload_daemon,
            dry_run,
            json,
        } => print_model_use_preview(ModelUseRequest {
            selector: &selector,
            installed,
            registry_path: registry.as_deref(),
            i18n_path: i18n.as_deref(),
            config_path: config.as_ref(),
            locale: &locale,
            provider: provider.as_deref(),
            output_path: output.as_deref(),
            in_place,
            model_root: model_root.as_deref(),
            reload_daemon,
            dry_run,
            json_output: json,
        }),
        ModelCommand::Remove {
            selector,
            installed,
            registry,
            i18n,
            config,
            locale,
            model_root,
            dry_run,
            yes,
            json,
        } => print_model_remove_plan(ModelRemoveRequest {
            selector: &selector,
            installed,
            registry_path: registry.as_deref(),
            i18n_path: i18n.as_deref(),
            config_path: config.as_ref(),
            locale: &locale,
            model_root: model_root.as_deref(),
            dry_run,
            yes,
            json_output: json,
        }),
    }
}

struct ModelListOwnedRequest {
    available: bool,
    installed: bool,
    model_root: Option<PathBuf>,
    registry: Option<PathBuf>,
    i18n: Option<PathBuf>,
    config: Option<PathBuf>,
    locale: String,
    json_output: bool,
}

struct LoadedLiveModelRegistry {
    registry: LiveModelRegistry,
    source_json: serde_json::Value,
    source_label: String,
    remote_base_url: Option<String>,
}

#[derive(Clone, Copy)]
struct ModelListRequest<'a> {
    available: bool,
    installed: bool,
    model_root: Option<&'a Path>,
    registry_path: Option<&'a Path>,
    i18n_path: Option<&'a Path>,
    config_path: Option<&'a PathBuf>,
    locale: &'a str,
    json_output: bool,
}

#[derive(Clone, Copy)]
struct ModelInfoRequest<'a> {
    id_or_short_id: &'a str,
    installed: bool,
    model_root: Option<&'a Path>,
    registry_path: Option<&'a Path>,
    i18n_path: Option<&'a Path>,
    config_path: Option<&'a PathBuf>,
    locale: &'a str,
    json_output: bool,
}

#[derive(Clone, Copy)]
struct ModelInstallPlanRequest<'a> {
    id_or_short_id: &'a str,
    registry_path: Option<&'a Path>,
    i18n_path: Option<&'a Path>,
    config_path: Option<&'a PathBuf>,
    locale: &'a str,
    model_root: Option<&'a Path>,
    staging_root: Option<&'a Path>,
    dry_run: bool,
    json_output: bool,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct ModelRemoveRequest<'a> {
    selector: &'a str,
    installed: bool,
    registry_path: Option<&'a Path>,
    i18n_path: Option<&'a Path>,
    config_path: Option<&'a PathBuf>,
    locale: &'a str,
    model_root: Option<&'a Path>,
    dry_run: bool,
    yes: bool,
    json_output: bool,
}

struct ModelRemovePlan {
    selector: String,
    selector_kind: String,
    model_root: PathBuf,
    target_path: PathBuf,
    exists: bool,
    is_dir: bool,
    resolved_model_id: Option<String>,
    resolved_short_id: Option<String>,
    resolved_title: Option<String>,
    removed: bool,
}

struct ModelRemoveResolution {
    target_path: PathBuf,
    selector_kind: String,
    resolved_model_id: Option<String>,
    resolved_short_id: Option<String>,
    resolved_title: Option<String>,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct ModelUseRequest<'a> {
    selector: &'a str,
    installed: bool,
    registry_path: Option<&'a Path>,
    i18n_path: Option<&'a Path>,
    config_path: Option<&'a PathBuf>,
    locale: &'a str,
    provider: Option<&'a str>,
    output_path: Option<&'a Path>,
    in_place: bool,
    model_root: Option<&'a Path>,
    reload_daemon: bool,
    dry_run: bool,
    json_output: bool,
}

#[allow(clippy::pedantic)]
struct ModelUsePreview {
    config_path: Option<PathBuf>,
    provider_id: String,
    provider_kind: AsrProviderKind,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    reload_daemon: bool,
    reloaded_daemon: bool,
    wrote_config: bool,
    before_active_provider: String,
    before_model: Option<String>,
    after_active_provider: String,
    after_model: String,
    selector_kind: String,
    selector: String,
    resolved_model_id: Option<String>,
    resolved_short_id: Option<String>,
    resolved_title: Option<String>,
}

#[derive(Clone)]
enum ModelUseWriteTarget {
    DryRun,
    Output(PathBuf),
    InPlace {
        config_path: PathBuf,
        backup_path: PathBuf,
    },
}

impl ModelUseWriteTarget {
    fn output_path(&self) -> Option<PathBuf> {
        match self {
            Self::DryRun => None,
            Self::Output(path) => Some(path.clone()),
            Self::InPlace { config_path, .. } => Some(config_path.clone()),
        }
    }

    fn backup_path(&self) -> Option<PathBuf> {
        match self {
            Self::InPlace { backup_path, .. } => Some(backup_path.clone()),
            Self::DryRun | Self::Output(_) => None,
        }
    }

    fn in_place(&self) -> bool {
        matches!(self, Self::InPlace { .. })
    }
}

struct ModelUseResolution {
    model_value: String,
    selector_kind: String,
    resolved_model_id: Option<String>,
    resolved_short_id: Option<String>,
    resolved_title: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct ModelSupport {
    supported: bool,
    reason: &'static str,
}

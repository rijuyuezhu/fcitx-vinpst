use std::path::PathBuf;

use anyhow::Context;
use vinpst_config::user_paths;

pub(crate) fn user_activation_service_path() -> anyhow::Result<PathBuf> {
    user_paths::user_activation_service_path()
        .context("resolve user D-Bus activation path: HOME and XDG_DATA_HOME are both unavailable")
}

pub(crate) fn default_config_path() -> anyhow::Result<PathBuf> {
    user_paths::default_config_path()
        .context("resolve user config path: HOME and XDG_CONFIG_HOME are both unavailable")
}

pub(crate) fn default_fcitx_config_path() -> anyhow::Result<PathBuf> {
    user_paths::default_fcitx_config_path()
        .context("resolve Fcitx config path: HOME and XDG_CONFIG_HOME are both unavailable")
}

pub(crate) fn user_data_home() -> anyhow::Result<PathBuf> {
    user_paths::user_data_home()
        .context("resolve data home: HOME and XDG_DATA_HOME are both unavailable")
}

pub(crate) fn user_systemd_unit_dir() -> anyhow::Result<PathBuf> {
    user_paths::user_systemd_unit_dir().context(
        "resolve systemd user unit directory: HOME and XDG_CONFIG_HOME are both unavailable",
    )
}

pub(crate) fn default_model_root() -> anyhow::Result<PathBuf> {
    user_paths::default_model_root()
        .context("resolve model root: HOME and XDG_DATA_HOME are both unavailable")
}

pub(crate) fn default_provider_root() -> anyhow::Result<PathBuf> {
    user_paths::default_provider_root()
        .context("resolve provider root: HOME and XDG_DATA_HOME are both unavailable")
}

pub(crate) fn default_adapter_root() -> anyhow::Result<PathBuf> {
    user_paths::default_adapter_root()
        .context("resolve adapter root: HOME and XDG_DATA_HOME are both unavailable")
}

pub(crate) fn default_cache_root() -> anyhow::Result<PathBuf> {
    user_paths::default_cache_root()
        .context("resolve cache root: HOME and XDG_CACHE_HOME are both unavailable")
}

pub(crate) fn default_model_install_staging_root() -> anyhow::Result<PathBuf> {
    user_paths::default_model_install_staging_root()
        .context("resolve model staging root: HOME and XDG_CACHE_HOME are both unavailable")
}

pub(crate) fn user_home() -> anyhow::Result<PathBuf> {
    user_paths::user_home().context("resolve user home: HOME is unavailable")
}

pub(crate) fn quote_exec_arg(value: &str) -> String {
    if value.is_empty() {
        return "''".to_owned();
    }
    if value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-' | b':' | b'=')
    }) {
        return value.to_owned();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

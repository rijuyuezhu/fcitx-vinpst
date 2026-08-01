use std::path::PathBuf;

use anyhow::Context;

pub(crate) fn user_activation_service_path() -> anyhow::Result<PathBuf> {
    Ok(user_data_home()?
        .join("dbus-1")
        .join("services")
        .join("org.fcitx.Vinput.service"))
}

pub(crate) fn default_config_path() -> anyhow::Result<PathBuf> {
    Ok(user_config_home()?.join("fcitx-vinput").join("config.json"))
}

pub(crate) fn user_config_home() -> anyhow::Result<PathBuf> {
    match std::env::var_os("XDG_CONFIG_HOME") {
        Some(value) if !value.is_empty() => Ok(PathBuf::from(value)),
        _ => Ok(user_home()?.join(".config")),
    }
}

pub(crate) fn user_data_home() -> anyhow::Result<PathBuf> {
    match std::env::var_os("XDG_DATA_HOME") {
        Some(value) if !value.is_empty() => Ok(PathBuf::from(value)),
        _ => Ok(user_home()?.join(".local/share")),
    }
}

pub(crate) fn user_cache_home() -> anyhow::Result<PathBuf> {
    match std::env::var_os("XDG_CACHE_HOME") {
        Some(value) if !value.is_empty() => Ok(PathBuf::from(value)),
        _ => Ok(user_home()?.join(".cache")),
    }
}

pub(crate) fn default_model_root() -> anyhow::Result<PathBuf> {
    Ok(user_data_home()?.join("fcitx-vinput").join("models"))
}

pub(crate) fn default_provider_root() -> anyhow::Result<PathBuf> {
    Ok(user_data_home()?.join("fcitx-vinput").join("providers"))
}

pub(crate) fn default_adapter_root() -> anyhow::Result<PathBuf> {
    Ok(user_data_home()?.join("fcitx-vinput").join("adapters"))
}

pub(crate) fn default_cache_root() -> anyhow::Result<PathBuf> {
    Ok(user_cache_home()?.join("fcitx-vinput"))
}

pub(crate) fn default_model_install_staging_root() -> anyhow::Result<PathBuf> {
    Ok(user_cache_home()?
        .join("fcitx-vinput")
        .join("model-install"))
}

pub(crate) fn user_home() -> anyhow::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .context("resolve user path: HOME is unset and XDG_DATA_HOME is unset")?;
    Ok(PathBuf::from(home))
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

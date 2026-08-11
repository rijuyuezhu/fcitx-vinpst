//! Canonical current-user filesystem roots for Vinpst-owned state.

use std::{ffi::OsStr, path::PathBuf};

const PRODUCT_DIR: &str = "fcitx-vinpst";
const ACTIVATION_SERVICE_FILE: &str = "org.fcitx.Vinpst.service";

/// Returns the current user's home directory when `HOME` is non-empty.
#[must_use]
pub fn user_home() -> Option<PathBuf> {
    non_empty_env_path("HOME")
}

/// Returns the current user's XDG configuration home with the standard HOME fallback.
#[must_use]
pub fn user_config_home() -> Option<PathBuf> {
    resolve_xdg_home(
        std::env::var_os("XDG_CONFIG_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
        ".config",
    )
}

/// Returns the current user's XDG data home with the standard HOME fallback.
#[must_use]
pub fn user_data_home() -> Option<PathBuf> {
    resolve_xdg_home(
        std::env::var_os("XDG_DATA_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
        ".local/share",
    )
}

/// Returns the current user's XDG cache home with the standard HOME fallback.
#[must_use]
pub fn user_cache_home() -> Option<PathBuf> {
    resolve_xdg_home(
        std::env::var_os("XDG_CACHE_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
        ".cache",
    )
}

/// Returns the canonical Vinpst user config path.
#[must_use]
pub fn default_config_path() -> Option<PathBuf> {
    Some(user_config_home()?.join(PRODUCT_DIR).join("config.json"))
}

/// Returns the canonical Fcitx addon config path for Vinpst.
#[must_use]
pub fn default_fcitx_config_path() -> Option<PathBuf> {
    Some(
        user_config_home()?
            .join("fcitx5")
            .join("conf")
            .join("vinpst.conf"),
    )
}

/// Returns the canonical Vinpst managed model root.
#[must_use]
pub fn default_model_root() -> Option<PathBuf> {
    Some(user_data_home()?.join(PRODUCT_DIR).join("models"))
}

/// Returns the canonical Vinpst managed command-ASR provider root.
#[must_use]
pub fn default_provider_root() -> Option<PathBuf> {
    Some(user_data_home()?.join(PRODUCT_DIR).join("providers"))
}

/// Returns the canonical Vinpst managed text-adapter root.
#[must_use]
pub fn default_adapter_root() -> Option<PathBuf> {
    Some(user_data_home()?.join(PRODUCT_DIR).join("adapters"))
}

/// Returns the canonical Vinpst cache root used by registry and install workflows.
#[must_use]
pub fn default_cache_root() -> Option<PathBuf> {
    Some(user_cache_home()?.join(PRODUCT_DIR))
}

/// Returns the canonical model-install staging root.
#[must_use]
pub fn default_model_install_staging_root() -> Option<PathBuf> {
    Some(default_cache_root()?.join("model-install"))
}

/// Returns the canonical GUI startup-notification read-state path.
#[must_use]
pub fn default_read_notifications_path() -> Option<PathBuf> {
    Some(default_cache_root()?.join("read_notifications"))
}

/// Returns the current user's systemd unit directory.
#[must_use]
pub fn user_systemd_unit_dir() -> Option<PathBuf> {
    Some(user_config_home()?.join("systemd").join("user"))
}

/// Returns the current user's Vinpst D-Bus activation service path.
#[must_use]
pub fn user_activation_service_path() -> Option<PathBuf> {
    Some(
        user_data_home()?
            .join("dbus-1")
            .join("services")
            .join(ACTIVATION_SERVICE_FILE),
    )
}

fn non_empty_env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn resolve_xdg_home(xdg: Option<&OsStr>, home: Option<&OsStr>, fallback: &str) -> Option<PathBuf> {
    xdg.filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            home.filter(|value| !value.is_empty())
                .map(|value| PathBuf::from(value).join(fallback))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdg_home_prefers_non_empty_explicit_value() {
        assert_eq!(
            resolve_xdg_home(
                Some(OsStr::new("/xdg/config")),
                Some(OsStr::new("/home/demo")),
                ".config",
            ),
            Some(PathBuf::from("/xdg/config"))
        );
    }

    #[test]
    fn xdg_home_uses_home_fallback_for_missing_or_empty_override() {
        for xdg in [None, Some(OsStr::new(""))] {
            assert_eq!(
                resolve_xdg_home(xdg, Some(OsStr::new("/home/demo")), ".local/share"),
                Some(PathBuf::from("/home/demo/.local/share"))
            );
        }
    }

    #[test]
    fn xdg_home_is_unavailable_without_xdg_or_home() {
        assert_eq!(resolve_xdg_home(None, None, ".cache"), None);
        assert_eq!(
            resolve_xdg_home(Some(OsStr::new("")), Some(OsStr::new("")), ".cache"),
            None
        );
    }
}

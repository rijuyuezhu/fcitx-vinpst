//! Managed ASR model storage and registry operations for the GUI.

use std::{
    env,
    path::{Path, PathBuf},
    time::Duration,
};

use vinput_config::{AsrProviderKind, VinputConfig};
use vinput_registry::{
    InstalledModelInfo, LiveModelInstallError, LiveModelInstallRequest, LiveModelRegistry,
    ManagedModelRemoveRequest, RegistryOperationControl, RegistryOperationProgress,
    RegistryTextSource, ReqwestRegistryAssetSource, ReqwestRegistryTextSource,
    install_live_model_controlled, managed_model_dir_name, remove_managed_model,
    scan_installed_models,
};

use crate::{GuiLocale, model_install::ModelInstallOutcome};

/// Returns the managed ASR model root used by CLI and GUI workflows.
pub fn default_model_root() -> Result<PathBuf, String> {
    Ok(user_data_home()?.join("fcitx-vinput").join("models"))
}

fn default_model_staging_root() -> Result<PathBuf, String> {
    Ok(user_cache_home()?
        .join("fcitx-vinput")
        .join("model-install"))
}

fn user_data_home() -> Result<PathBuf, String> {
    match env::var_os("XDG_DATA_HOME") {
        Some(value) if !value.is_empty() => Ok(PathBuf::from(value)),
        _ => Ok(user_home()?.join(".local/share")),
    }
}

fn user_cache_home() -> Result<PathBuf, String> {
    match env::var_os("XDG_CACHE_HOME") {
        Some(value) if !value.is_empty() => Ok(PathBuf::from(value)),
        _ => Ok(user_home()?.join(".cache")),
    }
}

fn user_home() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is required to locate managed model storage".to_owned())
}

pub(crate) fn load_installed_models() -> Result<Vec<InstalledModelInfo>, String> {
    let root = default_model_root()?;
    scan_installed_models(&root).map_err(|error| error.to_string())
}

pub(crate) fn install_registry_model_controlled(
    config: &VinputConfig,
    selector: &str,
    control: &RegistryOperationControl,
    locale: GuiLocale,
) -> ModelInstallOutcome {
    control.report(RegistryOperationProgress::ResolvingRegistry);
    if control.is_cancelled() {
        return ModelInstallOutcome::Cancelled;
    }
    let registry = match fetch_live_model_registry(config) {
        Ok(registry) => registry,
        Err(_) if control.is_cancelled() => return ModelInstallOutcome::Cancelled,
        Err(error) => return ModelInstallOutcome::Failed(error),
    };
    if control.is_cancelled() {
        return ModelInstallOutcome::Cancelled;
    }
    let model = registry
        .model_by_id_or_short_id(selector)
        .ok_or_else(|| format!("Unknown registry model id or short id `{selector}`."));
    let model = match model {
        Ok(model) => model,
        Err(error) => return ModelInstallOutcome::Failed(error),
    };
    let model_name = managed_model_dir_name(model);
    let model_dir = match default_model_root() {
        Ok(root) => root.join(&model_name),
        Err(error) => return ModelInstallOutcome::Failed(error),
    };
    let staging_dir = match default_model_staging_root() {
        Ok(root) => root.join(&model_name),
        Err(error) => return ModelInstallOutcome::Failed(error),
    };
    let source = ReqwestRegistryAssetSource::with_timeout(Duration::from_secs(300));
    let installed = install_live_model_controlled(
        &source,
        &LiveModelInstallRequest {
            model,
            model_dir,
            staging_dir: staging_dir.clone(),
            display: Some(model.installed_display_metadata(&config.global.default_language, None)),
        },
        control,
    );
    let installed = match installed {
        Ok(installed) => installed,
        Err(LiveModelInstallError::Cancelled { .. }) => {
            remove_staging_dir(&staging_dir);
            return ModelInstallOutcome::Cancelled;
        }
        Err(error) => {
            remove_staging_dir(&staging_dir);
            return ModelInstallOutcome::Failed(format!("Model installation failed: {error}"));
        }
    };
    ModelInstallOutcome::Installed(locale.model_installed(
        &model.resolved_title(None),
        &model_name,
        installed.checksum_verified(),
    ))
}

fn remove_staging_dir(path: &Path) {
    let _ = std::fs::remove_dir_all(path);
}

fn fetch_live_model_registry(config: &VinputConfig) -> Result<LiveModelRegistry, String> {
    let source = ReqwestRegistryTextSource::with_timeout(Duration::from_secs(30));
    fetch_live_model_registry_from(config, &source)
}

fn fetch_live_model_registry_from(
    config: &VinputConfig,
    source: &impl RegistryTextSource,
) -> Result<LiveModelRegistry, String> {
    let urls = config
        .registry
        .base_urls
        .iter()
        .map(|base| format!("{}/registry/models.json", base.trim_end_matches('/')))
        .collect::<Vec<_>>();
    if urls.is_empty() {
        return Err("No registry mirrors are configured.".to_owned());
    }
    let mut failure_count = 0;
    for url in &urls {
        match source.fetch_registry_text(url) {
            Ok(text) => {
                return LiveModelRegistry::from_json_str(&text)
                    .map_err(|error| format!("Registry model catalog is invalid: {error}"));
            }
            Err(_) => failure_count += 1,
        }
    }
    Err(format!(
        "All {failure_count} configured registry mirrors failed."
    ))
}

pub(crate) fn remove_installed_model(
    config: &VinputConfig,
    target_path: &Path,
    locale: GuiLocale,
) -> Result<String, String> {
    let model_root = default_model_root()?;
    let active_model_paths = config
        .asr
        .providers
        .iter()
        .filter(|provider| provider.kind == AsrProviderKind::Local)
        .filter_map(|provider| provider.model.as_deref())
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    remove_managed_model(&ManagedModelRemoveRequest {
        model_root: &model_root,
        target_path,
        active_model_paths: &active_model_paths,
    })
    .map_err(|error| error.to_string())?;
    let directory = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("managed model");
    Ok(locale.model_removed(directory))
}

pub(crate) fn model_is_active(config: &VinputConfig, model_dir: &Path) -> bool {
    config.asr.providers.iter().any(|provider| {
        provider.kind == AsrProviderKind::Local
            && provider
                .model
                .as_deref()
                .is_some_and(|model| Path::new(model) == model_dir)
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::*;

    #[derive(Default)]
    struct StubRegistryTextSource {
        responses: HashMap<String, Result<String, String>>,
    }

    impl RegistryTextSource for StubRegistryTextSource {
        fn fetch_registry_text(&self, url: &str) -> Result<String, String> {
            self.responses
                .get(url)
                .cloned()
                .unwrap_or_else(|| Err("missing fixture".to_owned()))
        }
    }

    #[test]
    fn registry_model_fetch_uses_mirror_fallback_without_leaking_urls() {
        let mut config = VinputConfig::bundled_default().expect("bundled config");
        let first = "https://user:super-secret@first.invalid".to_owned();
        let second = "https://second.invalid".to_owned();
        config.registry.base_urls = vec![first.clone(), second.clone()];
        let model_json = json!({
            "version": 1,
            "items": [{
                "id": "model.test.fixture",
                "short_id": "fixture",
                "urls": ["https://assets.invalid/fixture.tar.zst"]
            }]
        })
        .to_string();
        let source = StubRegistryTextSource {
            responses: HashMap::from([
                (
                    format!("{first}/registry/models.json"),
                    Err("connection failed".to_owned()),
                ),
                (format!("{second}/registry/models.json"), Ok(model_json)),
            ]),
        };

        let registry = fetch_live_model_registry_from(&config, &source).expect("mirror fallback");
        assert!(registry.model_by_id_or_short_id("fixture").is_some());

        let failed = StubRegistryTextSource::default();
        let error =
            fetch_live_model_registry_from(&config, &failed).expect_err("all mirrors should fail");
        assert_eq!(error, "All 2 configured registry mirrors failed.");
        assert!(!error.contains("super-secret"));
        assert!(!error.contains("first.invalid"));
    }

    #[test]
    fn active_model_detection_matches_only_local_provider_paths() {
        let mut config = VinputConfig::bundled_default().expect("bundled config");
        let model_dir = PathBuf::from("/managed/models/active");
        let provider = config
            .asr
            .providers
            .iter_mut()
            .find(|provider| provider.kind == AsrProviderKind::Local)
            .expect("local provider");
        provider.model = Some(model_dir.to_string_lossy().into_owned());
        assert!(model_is_active(&config, &model_dir));
        assert!(!model_is_active(
            &config,
            Path::new("/managed/models/inactive")
        ));
    }
}

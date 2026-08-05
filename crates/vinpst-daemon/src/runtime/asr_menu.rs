//! ASR provider/model menu state and selection helpers.

use std::{collections::HashSet, path::Path};

use vinpst_config::{AsrProviderKind, VinpstConfig};
use vinpst_registry::InstalledModelInfo;

use super::RuntimeState;

/// Provider-only ASR menu state exposed through the first Rust D-Bus extension.
pub(crate) type AsrMenuStateTuple = (
    String,
    String,
    String,
    bool,
    String,
    Vec<(String, String, String)>,
);

/// Provider/model ASR menu state exposed through the additive Rust D-Bus extension.
pub(crate) type AsrTargetMenuStateTuple = (
    String,
    String,
    String,
    String,
    bool,
    String,
    Vec<(String, String, String, String)>,
);

/// Provider/model display state with stable ids, localized titles, and concrete values.
pub(crate) type AsrDisplayMenuStateTuple = (
    String,
    String,
    String,
    String,
    bool,
    String,
    Vec<(String, String, String, String, String)>,
);

impl RuntimeState {
    /// Returns configured providers plus target/effective reload state.
    pub(crate) fn asr_menu_state(&self) -> AsrMenuStateTuple {
        let state = self.asr_backend_state();
        (
            state.target_provider_id,
            state.effective_provider_id,
            state.effective_model_id,
            state.reload_in_progress,
            state.last_error,
            self.config
                .asr
                .providers
                .iter()
                .map(|provider| {
                    (
                        provider.id.clone(),
                        provider_kind_label(&provider.kind).to_owned(),
                        provider.model.clone().unwrap_or_default(),
                    )
                })
                .collect(),
        )
    }

    /// Returns provider/model rows plus target/effective reload state.
    pub(crate) fn asr_target_menu_state(
        &self,
        installed_models: &[InstalledModelInfo],
    ) -> AsrTargetMenuStateTuple {
        let state = self.asr_backend_state();
        (
            state.target_provider_id,
            state.target_model_id,
            state.effective_provider_id,
            state.effective_model_id,
            state.reload_in_progress,
            state.last_error,
            target_menu_items(&self.config, installed_models),
        )
    }

    /// Returns localized provider/model rows plus target/effective reload state.
    pub(crate) fn asr_display_menu_state(
        &self,
        installed_models: &[InstalledModelInfo],
        locale_candidates: &[String],
    ) -> AsrDisplayMenuStateTuple {
        let state = self.asr_backend_state();
        (
            state.target_provider_id,
            state.target_model_id,
            state.effective_provider_id,
            state.effective_model_id,
            state.reload_in_progress,
            state.last_error,
            display_menu_items(&self.config, installed_models, locale_candidates),
        )
    }
}

/// Selects a configured provider in an owned config snapshot.
pub(crate) fn select_asr_provider(
    config: VinpstConfig,
    provider_id: &str,
) -> Result<VinpstConfig, vinpst_asr::AsrError> {
    select_asr_target(config, provider_id, None)
}

/// Selects a configured provider and optional concrete model value.
pub(crate) fn select_asr_target(
    mut config: VinpstConfig,
    provider_id: &str,
    model_value: Option<&str>,
) -> Result<VinpstConfig, vinpst_asr::AsrError> {
    let Some(provider) = config
        .asr
        .providers
        .iter_mut()
        .find(|provider| provider.id == provider_id)
    else {
        return Err(vinpst_asr::AsrError::UnknownProvider(
            provider_id.to_owned(),
        ));
    };
    if let Some(model_value) = model_value.filter(|value| !value.trim().is_empty()) {
        provider.model = Some(model_value.to_owned());
    }
    provider_id.clone_into(&mut config.asr.active_provider);
    Ok(config)
}

fn target_menu_items(
    config: &VinpstConfig,
    installed_models: &[InstalledModelInfo],
) -> Vec<(String, String, String, String)> {
    let mut items = Vec::new();
    for provider in &config.asr.providers {
        let kind = provider_kind_label(&provider.kind).to_owned();
        let configured_model = provider.model.clone().unwrap_or_default();
        if provider.kind == AsrProviderKind::Local && !installed_models.is_empty() {
            for model in installed_models {
                items.push((
                    provider.id.clone(),
                    kind.clone(),
                    model.model_id.clone(),
                    model.config_model_value(),
                ));
            }
            if !configured_model.is_empty()
                && !installed_models
                    .iter()
                    .any(|model| model.model_dir == Path::new(&configured_model))
            {
                items.push((
                    provider.id.clone(),
                    kind,
                    configured_model_label(&configured_model),
                    configured_model,
                ));
            }
        } else {
            let item_id = if configured_model.is_empty() {
                provider.id.clone()
            } else {
                configured_model_label(&configured_model)
            };
            items.push((provider.id.clone(), kind, item_id, configured_model));
        }
    }
    items
}

fn display_menu_items(
    config: &VinpstConfig,
    installed_models: &[InstalledModelInfo],
    locale_candidates: &[String],
) -> Vec<(String, String, String, String, String)> {
    let mut items = Vec::new();
    for provider in &config.asr.providers {
        let kind = provider_kind_label(&provider.kind).to_owned();
        let configured_model = provider.model.clone().unwrap_or_default();
        if provider.kind == AsrProviderKind::Local && !installed_models.is_empty() {
            for model in installed_models {
                let item_id = model.stable_model_id().to_owned();
                let display_title = model
                    .display_title(locale_candidates)
                    .unwrap_or(&item_id)
                    .to_owned();
                items.push((
                    provider.id.clone(),
                    kind.clone(),
                    item_id,
                    display_title,
                    model.config_model_value(),
                ));
            }
            if !configured_model.is_empty()
                && !installed_models
                    .iter()
                    .any(|model| model.model_dir == Path::new(&configured_model))
            {
                let item_id = configured_model_label(&configured_model);
                items.push((
                    provider.id.clone(),
                    kind,
                    item_id.clone(),
                    item_id,
                    configured_model,
                ));
            }
        } else {
            let item_id = if configured_model.is_empty() {
                provider.id.clone()
            } else {
                configured_model_label(&configured_model)
            };
            items.push((
                provider.id.clone(),
                kind,
                item_id.clone(),
                item_id,
                configured_model,
            ));
        }
    }
    items
}

/// Returns ordered locale preferences from the daemon process environment.
pub(crate) fn locale_candidates_from_environment() -> Vec<String> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    for name in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
        let Some(value) = std::env::var_os(name) else {
            continue;
        };
        for locale in value.to_string_lossy().split(':') {
            let locale = locale.trim();
            if !locale.is_empty() && seen.insert(locale.to_owned()) {
                candidates.push(locale.to_owned());
            }
        }
    }
    candidates
}

fn configured_model_label(model: &str) -> String {
    Path::new(model)
        .file_name()
        .and_then(|component| component.to_str())
        .filter(|component| !component.is_empty())
        .unwrap_or(model)
        .to_owned()
}

fn provider_kind_label(kind: &AsrProviderKind) -> &'static str {
    match kind {
        AsrProviderKind::Local => "local",
        AsrProviderKind::Remote => "remote",
        AsrProviderKind::Command => "command",
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs};

    use super::*;
    use vinpst_asr::MockAsrBackend;
    use vinpst_config::{AsrProviderConfig, AsrProviderKind};
    use vinpst_registry::scan_installed_models;

    fn provider(id: &str, kind: AsrProviderKind, model: Option<&str>) -> AsrProviderConfig {
        AsrProviderConfig {
            id: id.to_owned(),
            kind,
            timeout_ms: None,
            model: model.map(ToOwned::to_owned),
            hotwords_file: None,
            command: None,
            args: Vec::new(),
            env: HashMap::new(),
            endpoint: None,
        }
    }

    #[test]
    fn asr_menu_state_reports_target_effective_and_provider_rows() {
        let mut config = VinpstConfig::bundled_default().unwrap();
        let mut command = provider("cmd", AsrProviderKind::Command, Some("cmd-model"));
        command.command = Some("cat".to_owned());
        config.asr.providers.push(command);
        let runtime =
            RuntimeState::with_asr_backend(config, Box::new(MockAsrBackend::buffered("final")))
                .unwrap();

        let state = runtime.asr_menu_state();
        assert_eq!(state.0, "sherpa-onnx");
        assert_eq!(state.1, "mock");
        assert_eq!(state.2, "mock-buffered");
        assert!(!state.3);
        assert!(state.4.is_empty());
        assert_eq!(state.5[0].0, "sherpa-onnx");
        assert_eq!(state.5[0].1, "local");
        assert_eq!(
            state.5[1],
            (
                "cmd".to_owned(),
                "command".to_owned(),
                "cmd-model".to_owned()
            )
        );
    }

    #[test]
    fn target_menu_expands_local_providers_with_flat_and_legacy_models() {
        let temp = tempfile::tempdir().unwrap();
        for path in [
            temp.path().join("flat"),
            temp.path().join("sherpa-onnx").join("moonshine"),
        ] {
            fs::create_dir_all(&path).unwrap();
            fs::write(
                path.join("vinpst-model.json"),
                r#"{"backend":"sherpa-offline","family":"moonshine"}"#,
            )
            .unwrap();
        }
        let installed = scan_installed_models(temp.path()).unwrap();
        let mut config = VinpstConfig::bundled_default().unwrap();
        config.asr.providers.push(provider(
            "remote",
            AsrProviderKind::Remote,
            Some("remote-model"),
        ));
        config.asr.providers.last_mut().unwrap().endpoint = Some("https://example.test".to_owned());
        let runtime =
            RuntimeState::with_asr_backend(config, Box::new(MockAsrBackend::buffered("final")))
                .unwrap();

        let state = runtime.asr_target_menu_state(&installed);
        assert_eq!(state.0, "sherpa-onnx");
        assert_eq!(state.2, "mock");
        assert!(state.6.iter().any(|item| item.2 == "flat"));
        assert!(
            state
                .6
                .iter()
                .any(|item| item.2 == "model.sherpa-onnx.moonshine")
        );
        assert!(state.6.iter().any(|item| {
            item.0 == "remote" && item.2 == "remote-model" && item.3 == "remote-model"
        }));
    }

    #[test]
    fn display_menu_uses_installed_registry_ids_and_localized_titles() {
        let temp = tempfile::tempdir().unwrap();
        let model = temp.path().join("managed-name");
        fs::create_dir_all(&model).unwrap();
        fs::write(
            model.join("vinpst-model.json"),
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
        let installed = scan_installed_models(temp.path()).unwrap();
        let config = VinpstConfig::bundled_default().unwrap();
        let runtime =
            RuntimeState::with_asr_backend(config, Box::new(MockAsrBackend::buffered("final")))
                .unwrap();

        let state = runtime.asr_display_menu_state(&installed, &["zh_CN.UTF-8".to_owned()]);
        assert_eq!(state.0, "sherpa-onnx");
        assert_eq!(state.2, "mock");
        assert!(state.6.iter().any(|item| {
            item.0 == "sherpa-onnx"
                && item.1 == "local"
                && item.2 == "model.sherpa-onnx.moonshine-v1"
                && item.3 == "月光语音模型"
                && item.4 == model.to_string_lossy()
        }));

        let fallback = runtime.asr_display_menu_state(&installed, &["en_US".to_owned()]);
        assert!(fallback.6.iter().any(|item| {
            item.2 == "model.sherpa-onnx.moonshine-v1" && item.3 == "model.sherpa-onnx.moonshine-v1"
        }));
    }

    #[test]
    fn target_selection_updates_provider_model_and_rejects_unknown_ids() {
        let mut config = VinpstConfig::bundled_default().unwrap();
        config
            .asr
            .providers
            .push(provider("mock", AsrProviderKind::Local, None));

        let selected = select_asr_target(config.clone(), "mock", Some("/models/new")).unwrap();
        assert_eq!(selected.asr.active_provider, "mock");
        assert_eq!(
            selected.asr.providers[1].model.as_deref(),
            Some("/models/new")
        );
        let error = select_asr_target(config, "missing", None).unwrap_err();
        assert!(matches!(error, vinpst_asr::AsrError::UnknownProvider(id) if id == "missing"));
    }
}

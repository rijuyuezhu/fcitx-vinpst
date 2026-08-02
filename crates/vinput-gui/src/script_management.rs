//! Managed provider and adapter installation transactions for the GUI.

use std::{env, path::PathBuf, time::Duration};

use vinput_config::VinputConfig;
use vinput_registry::{
    LiveScriptKind, LiveScriptRegistry, RegistryOperationControl, RegistryOperationProgress,
    RegistryTextSource, ReqwestRegistryAssetSource, ReqwestRegistryTextSource,
    install_live_script_controlled, managed_script_relative_path, materialize_asr_provider,
    materialize_llm_adapter,
};

use crate::{
    ConfigDocument, ensure_config_mutation_allowed, save_updated_config_with_daemon,
    script_install::ScriptInstallOutcome,
};

pub(crate) fn install_registry_script_controlled(
    document: &ConfigDocument,
    kind: LiveScriptKind,
    selector: &str,
    control: &RegistryOperationControl,
) -> ScriptInstallOutcome {
    let root = match default_script_root(kind) {
        Ok(root) => root,
        Err(error) => return ScriptInstallOutcome::Failed(error),
    };
    let registry_source = ReqwestRegistryTextSource::with_timeout(Duration::from_secs(30));
    let asset_source = ReqwestRegistryAssetSource::with_timeout(Duration::from_secs(120));
    install_registry_script_from_sources(
        document,
        kind,
        selector,
        control,
        &registry_source,
        &asset_source,
        &root,
    )
}

fn install_registry_script_from_sources(
    document: &ConfigDocument,
    kind: LiveScriptKind,
    selector: &str,
    control: &RegistryOperationControl,
    registry_source: &impl RegistryTextSource,
    asset_source: &impl vinput_registry::RegistryAssetSource,
    root: &std::path::Path,
) -> ScriptInstallOutcome {
    control.report(RegistryOperationProgress::ResolvingRegistry);
    if control.is_cancelled() {
        return ScriptInstallOutcome::Cancelled;
    }
    if let Err(error) = ensure_config_mutation_allowed(document) {
        return ScriptInstallOutcome::Failed(error);
    }
    let registry =
        match fetch_live_script_registry_from(&document.config, kind, control, registry_source) {
            Ok(registry) => registry,
            Err(_) if control.is_cancelled() => return ScriptInstallOutcome::Cancelled,
            Err(error) => return ScriptInstallOutcome::Failed(error),
        };
    let Some(entry) = registry.entry_by_id_or_short_id(selector, kind).cloned() else {
        return ScriptInstallOutcome::Failed(format!(
            "Unknown {} registry id or short id `{selector}`.",
            resource_label(kind)
        ));
    };
    let script_path = match managed_script_relative_path(kind, &entry.id) {
        Ok(path) => root.join(path),
        Err(error) => return ScriptInstallOutcome::Failed(error.to_string()),
    };
    let (updated, replacing) =
        match materialize_config(&document.config, kind, &entry, &script_path) {
            Ok(value) => value,
            Err(error) => return ScriptInstallOutcome::Failed(error),
        };
    if let Err(error) = updated.validate() {
        return ScriptInstallOutcome::Failed(format!(
            "Validate installed {} configuration: {error}",
            resource_label(kind)
        ));
    }
    if control.is_cancelled() {
        return ScriptInstallOutcome::Cancelled;
    }

    let installed = match install_live_script_controlled(asset_source, kind, &entry, root, control)
    {
        Ok(installed) => installed,
        Err(_) if control.is_cancelled() => return ScriptInstallOutcome::Cancelled,
        Err(error) => {
            return ScriptInstallOutcome::Failed(format!(
                "{} installation failed: {error}",
                resource_title(kind)
            ));
        }
    };
    if installed.script_path != script_path {
        return ScriptInstallOutcome::Failed(format!(
            "Installed script path `{}` did not match planned path `{}`.",
            installed.script_path.display(),
            script_path.display()
        ));
    }

    control.report(RegistryOperationProgress::UpdatingConfiguration);
    let saved = match save_updated_config_with_daemon(document, &updated) {
        Ok(saved) => saved,
        Err(error) => {
            return ScriptInstallOutcome::Failed(format!(
                "Script installed at {}, but configuration update failed: {error}",
                script_path.display()
            ));
        }
    };
    control.report(RegistryOperationProgress::Completed);
    let action = if replacing { "Updated" } else { "Installed" };
    ScriptInstallOutcome::Installed(format!(
        "{action} {} `{}` at {}; {}.",
        resource_label(kind),
        entry.id,
        script_path.display(),
        saved.daemon_reload
    ))
}

fn fetch_live_script_registry_from(
    config: &VinputConfig,
    kind: LiveScriptKind,
    control: &RegistryOperationControl,
    source: &impl RegistryTextSource,
) -> Result<LiveScriptRegistry, String> {
    let filename = match kind {
        LiveScriptKind::AsrProvider => "providers.json",
        LiveScriptKind::LlmAdapter => "adapters.json",
    };
    let urls = config
        .registry
        .base_urls
        .iter()
        .map(|base| format!("{}/registry/{filename}", base.trim_end_matches('/')))
        .collect::<Vec<_>>();
    if urls.is_empty() {
        return Err("No registry mirrors are configured.".to_owned());
    }
    let mut failure_count = 0;
    for url in &urls {
        if control.is_cancelled() {
            return Err("Registry request cancelled.".to_owned());
        }
        match source.fetch_registry_text(url) {
            Ok(text) => {
                return LiveScriptRegistry::from_json_str(&text, kind).map_err(|error| {
                    format!(
                        "{} registry catalog is invalid: {error}",
                        resource_title(kind)
                    )
                });
            }
            Err(_) => failure_count += 1,
        }
    }
    Err(format!(
        "All {failure_count} configured {} registry mirrors failed.",
        resource_label(kind)
    ))
}

fn materialize_config(
    config: &VinputConfig,
    kind: LiveScriptKind,
    entry: &vinput_registry::LiveScriptEntry,
    script_path: &std::path::Path,
) -> Result<(VinputConfig, bool), String> {
    let mut updated = config.clone();
    match kind {
        LiveScriptKind::AsrProvider => {
            let existing = updated
                .asr
                .providers
                .iter()
                .find(|provider| provider.id == entry.id);
            let materialized = materialize_asr_provider(entry, script_path, existing)
                .map_err(|error| error.to_string())?;
            let replacing = materialized.replacing_managed;
            if let Some(index) = updated
                .asr
                .providers
                .iter()
                .position(|provider| provider.id == entry.id)
            {
                updated.asr.providers[index] = materialized.provider;
            } else {
                updated.asr.providers.push(materialized.provider);
            }
            Ok((updated, replacing))
        }
        LiveScriptKind::LlmAdapter => {
            let existing = updated
                .llm
                .adapters
                .iter()
                .find(|adapter| adapter.id == entry.id);
            let materialized = materialize_llm_adapter(entry, script_path, existing)
                .map_err(|error| error.to_string())?;
            let replacing = materialized.replacing_managed;
            if let Some(index) = updated
                .llm
                .adapters
                .iter()
                .position(|adapter| adapter.id == entry.id)
            {
                updated.llm.adapters[index] = materialized.adapter;
            } else {
                updated.llm.adapters.push(materialized.adapter);
            }
            Ok((updated, replacing))
        }
    }
}

fn default_script_root(kind: LiveScriptKind) -> Result<PathBuf, String> {
    let data_home = match env::var_os("XDG_DATA_HOME") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => env::var_os("HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| "HOME is required to locate managed script storage".to_owned())?
            .join(".local/share"),
    };
    Ok(data_home.join("fcitx-vinput").join(match kind {
        LiveScriptKind::AsrProvider => "providers",
        LiveScriptKind::LlmAdapter => "adapters",
    }))
}

pub(crate) const fn resource_label(kind: LiveScriptKind) -> &'static str {
    match kind {
        LiveScriptKind::AsrProvider => "ASR provider",
        LiveScriptKind::LlmAdapter => "text adapter",
    }
}

const fn resource_title(kind: LiveScriptKind) -> &'static str {
    match kind {
        LiveScriptKind::AsrProvider => "ASR provider",
        LiveScriptKind::LlmAdapter => "Text adapter",
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::*;
    use vinput_registry::{LiveScriptEntry, RegistryAssetSource};

    struct FixtureTextSource(&'static str);

    impl RegistryTextSource for FixtureTextSource {
        fn fetch_registry_text(&self, _url: &str) -> Result<String, String> {
            Ok(self.0.to_owned())
        }
    }

    struct FixtureAssetSource(&'static [u8]);

    impl RegistryAssetSource for FixtureAssetSource {
        fn fetch_asset(&self, _url: &str, destination: &Path) -> Result<(), String> {
            fs::write(destination, self.0).map_err(|error| error.to_string())
        }
    }

    #[test]
    fn provider_materialization_adds_managed_command_entry() {
        let config = VinputConfig::bundled_default().expect("bundled config");
        let entry = LiveScriptEntry {
            id: "provider.fixture.batch".to_owned(),
            short_id: Some("fixture".to_owned()),
            stream: false,
            command: "python3".to_owned(),
            script_urls: vec!["https://example.invalid/provider.py".to_owned()],
            readme_url: None,
            envs: Vec::new(),
        };

        let (updated, replacing) = materialize_config(
            &config,
            LiveScriptKind::AsrProvider,
            &entry,
            std::path::Path::new("/tmp/provider.py"),
        )
        .expect("materialize provider");

        assert!(!replacing);
        assert!(
            updated.asr.providers.iter().any(|provider| {
                provider.id == entry.id && provider.args == ["/tmp/provider.py"]
            })
        );
    }

    #[test]
    fn adapter_materialization_adds_managed_command_entry() {
        let config = VinputConfig::bundled_default().expect("bundled config");
        let entry = LiveScriptEntry {
            id: "adapter.fixture.command".to_owned(),
            short_id: Some("fixture".to_owned()),
            stream: false,
            command: "python3".to_owned(),
            script_urls: vec!["https://example.invalid/adapter.py".to_owned()],
            readme_url: None,
            envs: Vec::new(),
        };

        let (updated, replacing) = materialize_config(
            &config,
            LiveScriptKind::LlmAdapter,
            &entry,
            std::path::Path::new("/tmp/adapter.py"),
        )
        .expect("materialize adapter");

        assert!(!replacing);
        assert!(
            updated
                .llm
                .adapters
                .iter()
                .any(|adapter| { adapter.id == entry.id && adapter.args == ["/tmp/adapter.py"] })
        );
    }

    #[test]
    fn provider_install_publishes_script_and_validated_config() {
        let directory = tempfile::tempdir().expect("temp dir");
        let document = ConfigDocument {
            path: directory.path().join("config.json"),
            from_disk: false,
            config: VinputConfig::bundled_default().expect("bundled config"),
        };
        let registry = FixtureTextSource(
            r#"{
                "version": 1,
                "items": [{
                    "id": "provider.fixture.batch",
                    "short_id": "fixture",
                    "stream": false,
                    "command": "python3",
                    "script_urls": ["https://example.invalid/provider.py"],
                    "envs": [{"name": "TOKEN", "required": true}]
                }]
            }"#,
        );
        let asset = FixtureAssetSource(b"#!/usr/bin/env python3\nprint('ok')\n");
        let root = directory.path().join("providers");

        let outcome = install_registry_script_from_sources(
            &document,
            LiveScriptKind::AsrProvider,
            "fixture",
            &RegistryOperationControl::default(),
            &registry,
            &asset,
            &root,
        );

        assert!(matches!(outcome, ScriptInstallOutcome::Installed(_)));
        assert!(root.join("fixture/batch").is_file());
        let config = VinputConfig::from_json_file(&document.path).expect("saved config");
        let provider = config
            .asr
            .providers
            .iter()
            .find(|provider| provider.id == "provider.fixture.batch")
            .expect("installed provider");
        assert_eq!(
            provider.args,
            [root.join("fixture/batch").display().to_string()]
        );
        assert_eq!(provider.env.get("TOKEN").map(String::as_str), Some(""));
    }

    #[test]
    fn adapter_install_publishes_script_and_validated_config() {
        let directory = tempfile::tempdir().expect("temp dir");
        let document = ConfigDocument {
            path: directory.path().join("config.json"),
            from_disk: false,
            config: VinputConfig::bundled_default().expect("bundled config"),
        };
        let registry = FixtureTextSource(
            r#"{
                "version": 1,
                "items": [{
                    "id": "adapter.fixture.command",
                    "short_id": "fixture",
                    "command": "python3",
                    "script_urls": ["https://example.invalid/adapter.py"]
                }]
            }"#,
        );
        let asset = FixtureAssetSource(b"#!/usr/bin/env python3\nprint('ok')\n");
        let root = directory.path().join("adapters");

        let outcome = install_registry_script_from_sources(
            &document,
            LiveScriptKind::LlmAdapter,
            "fixture",
            &RegistryOperationControl::default(),
            &registry,
            &asset,
            &root,
        );

        assert!(matches!(outcome, ScriptInstallOutcome::Installed(_)));
        assert!(root.join("fixture/command").is_file());
        let config = VinputConfig::from_json_file(&document.path).expect("saved config");
        let adapter = config
            .llm
            .adapters
            .iter()
            .find(|adapter| adapter.id == "adapter.fixture.command")
            .expect("installed adapter");
        assert_eq!(
            adapter.args,
            [root.join("fixture/command").display().to_string()]
        );
    }
}

//! Recovery for scripts published before their config entry could be committed.

use vinpst_config::VinpstConfig;
use vinpst_registry::LiveScriptKind;

use crate::{
    ConfigDocument, ConfigSaveOutcome, GuiLocale, GuiText, ScriptInstallOutcome,
    ensure_config_mutation_allowed, save_updated_config_with_daemon,
    script_install::ScriptInstallPlan,
    script_management::{
        apply_plan_environment, materialize_config, resource_label, validate_plan_environment,
    },
    script_transaction::apply_managed_script_revision,
};

pub(crate) fn recover_registry_script_config(
    document: &ConfigDocument,
    plan: &ScriptInstallPlan,
    locale: GuiLocale,
) -> ScriptInstallOutcome {
    recover_registry_script_config_with_save(
        document,
        plan,
        locale,
        save_updated_config_with_daemon,
    )
}

fn recover_registry_script_config_with_save(
    document: &ConfigDocument,
    plan: &ScriptInstallPlan,
    locale: GuiLocale,
    save: impl FnOnce(&ConfigDocument, &VinpstConfig) -> Result<ConfigSaveOutcome, String>,
) -> ScriptInstallOutcome {
    let result = (|| {
        ensure_config_mutation_allowed(document)?;
        validate_plan_environment(plan)?;
        inspect_published_script(plan)?;
        let (mut updated, _) =
            materialize_config(&document.config, plan.kind, &plan.entry, &plan.script_path)?;
        apply_plan_environment(&mut updated, plan);
        apply_managed_script_revision(
            &mut updated,
            plan.kind,
            &plan.entry.id,
            &plan.script_path,
            None,
        )?;
        updated.validate().map_err(|error| {
            format!(
                "Validate recovered {} configuration: {error}",
                resource_label(plan.kind)
            )
        })?;
        save(document, &updated)
    })();

    match result {
        Ok(saved) => {
            let resource = locale.text(match plan.kind {
                LiveScriptKind::AsrProvider => GuiText::AsrProviderResource,
                LiveScriptKind::LlmAdapter => GuiText::TextAdapterResource,
            });
            ScriptInstallOutcome::Installed(locale.script_configuration_completed(
                resource,
                &plan.entry.id,
                &plan.script_path.display().to_string(),
                &saved.daemon_reload,
            ))
        }
        Err(error) => ScriptInstallOutcome::PublishedButConfigFailed { error },
    }
}

fn inspect_published_script(plan: &ScriptInstallPlan) -> Result<(), String> {
    match std::fs::symlink_metadata(&plan.script_path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(()),
        Ok(_) => Err(format!(
            "Published script path `{}` is no longer a regular file.",
            plan.script_path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(format!(
            "Published script `{}` is missing; run installation again instead of config recovery.",
            plan.script_path.display()
        )),
        Err(error) => Err(format!(
            "Inspect published script `{}`: {error}",
            plan.script_path.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script_install::ScriptEnvironmentValue;
    use crate::script_management::install_registry_script_from_source_and_save;
    use vinpst_registry::{
        LiveScriptEntry, LiveScriptKind, RegistryAssetSource, RegistryOperationControl,
    };

    struct FixtureAssetSource(&'static [u8]);

    impl RegistryAssetSource for FixtureAssetSource {
        fn fetch_asset(&self, _url: &str, destination: &std::path::Path) -> Result<(), String> {
            std::fs::write(destination, self.0).map_err(|error| error.to_string())
        }
    }

    fn plan(script_path: std::path::PathBuf) -> ScriptInstallPlan {
        ScriptInstallPlan {
            kind: LiveScriptKind::AsrProvider,
            selector: "fixture".to_owned(),
            entry: LiveScriptEntry {
                id: "provider.fixture.batch".to_owned(),
                short_id: Some("fixture".to_owned()),
                stream: false,
                command: "python3".to_owned(),
                script_urls: vec!["https://example.invalid/provider.py".to_owned()],
                readme_url: None,
                envs: vec![vinpst_registry::LiveScriptEnvSpec {
                    name: "TOKEN".to_owned(),
                    required: true,
                }],
            },
            script_root: script_path.parent().expect("script parent").to_path_buf(),
            script_path,
            environment: vec![ScriptEnvironmentValue {
                name: "TOKEN".to_owned(),
                required: true,
                value: "super-secret".to_owned(),
            }],
        }
    }

    fn managed_plan(root: &std::path::Path) -> ScriptInstallPlan {
        let mut plan = plan(root.join("fixture/batch"));
        plan.script_root = root.to_path_buf();
        plan
    }

    fn managed_adapter_plan(root: &std::path::Path) -> ScriptInstallPlan {
        ScriptInstallPlan {
            kind: LiveScriptKind::LlmAdapter,
            selector: "fixture".to_owned(),
            entry: LiveScriptEntry {
                id: "adapter.fixture.command".to_owned(),
                short_id: Some("fixture".to_owned()),
                stream: false,
                command: "python3".to_owned(),
                script_urls: vec!["https://example.invalid/adapter.py".to_owned()],
                readme_url: None,
                envs: Vec::new(),
            },
            script_root: root.to_path_buf(),
            script_path: root.join("fixture/command"),
            environment: Vec::new(),
        }
    }

    #[test]
    fn published_script_failure_can_recover_without_redownload() {
        let directory = tempfile::tempdir().expect("temp dir");
        let document = ConfigDocument {
            path: directory.path().join("config.json"),
            from_disk: false,
            config: VinpstConfig::bundled_default().expect("bundled config"),
        };
        let plan = managed_plan(&directory.path().join("providers"));
        let source = FixtureAssetSource(b"#!/usr/bin/env python3\nprint('published')\n");

        let failed = install_registry_script_from_source_and_save(
            &document,
            &plan,
            &RegistryOperationControl::default(),
            &source,
            GuiLocale::EnUs,
            |_, _| Err("permission denied".to_owned()),
        );

        assert!(matches!(
            failed,
            ScriptInstallOutcome::PublishedButConfigFailed { error }
                if error == "permission denied"
        ));
        assert!(plan.script_path.is_file());
        assert!(!document.path.exists());
        let published = std::fs::read_to_string(&plan.script_path).expect("published script");

        let recovered = recover_registry_script_config(&document, &plan, GuiLocale::EnUs);

        assert!(matches!(recovered, ScriptInstallOutcome::Installed(_)));
        assert_eq!(
            std::fs::read_to_string(&plan.script_path).expect("reused script"),
            published
        );
        assert!(document.path.is_file());
    }

    #[test]
    fn recovery_reuses_existing_script_and_commits_config() {
        let directory = tempfile::tempdir().expect("temp dir");
        let script_path = directory.path().join("provider.py");
        std::fs::write(&script_path, "print('ok')\n").expect("write script");
        let document = ConfigDocument {
            path: directory.path().join("config.json"),
            from_disk: false,
            config: VinpstConfig::bundled_default().expect("bundled config"),
        };

        let outcome =
            recover_registry_script_config(&document, &plan(script_path.clone()), GuiLocale::EnUs);

        assert!(matches!(outcome, ScriptInstallOutcome::Installed(_)));
        assert_eq!(
            std::fs::read_to_string(&script_path).expect("read script"),
            "print('ok')\n"
        );
        let saved = VinpstConfig::from_json_file(&document.path).expect("saved config");
        let provider = saved
            .asr
            .providers
            .iter()
            .find(|provider| provider.id == "provider.fixture.batch")
            .expect("provider");
        assert_eq!(
            provider.env.get("TOKEN").map(String::as_str),
            Some("super-secret")
        );
    }

    #[test]
    fn adapter_recovery_persists_published_script_revision() {
        let directory = tempfile::tempdir().expect("temp dir");
        let root = directory.path().join("adapters");
        let plan = managed_adapter_plan(&root);
        std::fs::create_dir_all(plan.script_path.parent().expect("script parent"))
            .expect("create script parent");
        let bytes = b"#!/usr/bin/env python3\nprint('recovered')\n";
        std::fs::write(&plan.script_path, bytes).expect("write published adapter");
        let document = ConfigDocument {
            path: directory.path().join("config.json"),
            from_disk: false,
            config: VinpstConfig::bundled_default().expect("bundled config"),
        };

        let outcome = recover_registry_script_config(&document, &plan, GuiLocale::EnUs);
        assert!(matches!(outcome, ScriptInstallOutcome::Installed(_)));
        let saved = VinpstConfig::from_json_file(&document.path).expect("saved config");
        let adapter = saved
            .llm
            .adapters
            .iter()
            .find(|adapter| adapter.id == plan.entry.id)
            .expect("recovered adapter");
        assert_eq!(
            adapter
                .extra
                .get(vinpst_config::MANAGED_SCRIPT_REVISION_KEY)
                .and_then(serde_json::Value::as_str),
            Some(vinpst_registry::sha256_hex(bytes).as_str())
        );
    }

    #[test]
    fn recovery_refuses_missing_or_non_regular_script() {
        let directory = tempfile::tempdir().expect("temp dir");
        let missing = directory.path().join("missing.py");
        let document = ConfigDocument {
            path: directory.path().join("config.json"),
            from_disk: false,
            config: VinpstConfig::bundled_default().expect("bundled config"),
        };

        let missing_outcome =
            recover_registry_script_config(&document, &plan(missing.clone()), GuiLocale::EnUs);
        assert!(matches!(
            missing_outcome,
            ScriptInstallOutcome::PublishedButConfigFailed { error }
                if error.contains("is missing")
        ));

        std::fs::create_dir(&missing).expect("create directory");
        let directory_outcome =
            recover_registry_script_config(&document, &plan(missing), GuiLocale::EnUs);
        assert!(matches!(
            directory_outcome,
            ScriptInstallOutcome::PublishedButConfigFailed { error }
                if error.contains("no longer a regular file")
        ));
    }

    #[test]
    fn recovery_failure_debug_never_exposes_environment_values() {
        let directory = tempfile::tempdir().expect("temp dir");
        let script_path = directory.path().join("provider.py");
        std::fs::write(&script_path, "print('ok')\n").expect("write script");
        let outcome = recover_registry_script_config_with_save(
            &ConfigDocument {
                path: directory.path().join("config.json"),
                from_disk: false,
                config: VinpstConfig::bundled_default().expect("bundled config"),
            },
            &plan(script_path),
            GuiLocale::EnUs,
            |_, _| Err("fixture failure".to_owned()),
        );

        assert!(!format!("{outcome:?}").contains("super-secret"));
    }
}

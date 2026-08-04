use std::path::Path;

use super::*;
use vinput_config::MANAGED_SCRIPT_REVISION_KEY;
use vinput_registry::{RegistryAssetSource, RegistryTextSource, sha256_hex};

struct FixtureTextSource(&'static str);

impl RegistryTextSource for FixtureTextSource {
    fn fetch_registry_text(&self, _url: &str) -> Result<String, String> {
        Ok(self.0.to_owned())
    }
}

struct FixtureAssetSource(&'static [u8]);

impl RegistryAssetSource for FixtureAssetSource {
    fn fetch_asset(&self, _url: &str, destination: &Path) -> Result<(), String> {
        std::fs::write(destination, self.0).map_err(|error| error.to_string())
    }
}

fn prepared_plan(outcome: ScriptPrepareOutcome) -> ScriptInstallPlan {
    match outcome {
        ScriptPrepareOutcome::Prepared(plan) => *plan,
        other => panic!("expected prepared plan, got {other:?}"),
    }
}

#[test]
fn adapter_update_changes_managed_script_revision() {
    let directory = tempfile::tempdir().expect("temp dir");
    let config_path = directory.path().join("config.json");
    let root = directory.path().join("adapters");
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
    let first_document = ConfigDocument {
        path: config_path.clone(),
        from_disk: false,
        config: VinputConfig::bundled_default().expect("bundled config"),
    };
    let first_plan = prepared_plan(prepare_registry_script_from_source(
        &first_document,
        LiveScriptKind::LlmAdapter,
        "fixture",
        &RegistryOperationControl::default(),
        &registry,
        &root,
    ));
    let first_bytes = b"#!/usr/bin/env python3\nprint('first')\n";
    let first = install_registry_script_from_source(
        &first_document,
        &first_plan,
        &RegistryOperationControl::default(),
        &FixtureAssetSource(first_bytes),
    );
    assert!(matches!(first, ScriptInstallOutcome::Installed(_)));
    let first_config = VinputConfig::from_json_file(&config_path).expect("first config");
    let first_revision = first_config.llm.adapters[0].extra[MANAGED_SCRIPT_REVISION_KEY]
        .as_str()
        .expect("first revision")
        .to_owned();

    let second_document = ConfigDocument {
        path: config_path.clone(),
        from_disk: true,
        config: first_config,
    };
    let second_plan = prepared_plan(prepare_registry_script_from_source(
        &second_document,
        LiveScriptKind::LlmAdapter,
        "fixture",
        &RegistryOperationControl::default(),
        &registry,
        &root,
    ));
    let second_bytes = b"#!/usr/bin/env python3\nprint('second')\n";
    let second = install_registry_script_from_source(
        &second_document,
        &second_plan,
        &RegistryOperationControl::default(),
        &FixtureAssetSource(second_bytes),
    );
    assert!(matches!(second, ScriptInstallOutcome::Installed(_)));
    let second_config = VinputConfig::from_json_file(&config_path).expect("second config");
    let second_revision = second_config.llm.adapters[0].extra[MANAGED_SCRIPT_REVISION_KEY]
        .as_str()
        .expect("second revision");

    assert_eq!(first_revision, sha256_hex(first_bytes));
    assert_eq!(second_revision, sha256_hex(second_bytes));
    assert_ne!(first_revision, second_revision);
    let adapter = &second_config.llm.adapters[0];
    assert_eq!(
        adapter
            .extra
            .get(vinput_config::MANAGED_SCRIPT_ROLLBACK_REVISION_KEY)
            .and_then(serde_json::Value::as_str),
        Some(first_revision.as_str())
    );
    assert_eq!(
        std::fs::read(vinput_registry::managed_script_rollback_path(
            &second_plan.script_path
        ))
        .expect("rollback script"),
        first_bytes
    );
}

#[test]
fn rejected_adapter_update_restores_previous_script() {
    let directory = tempfile::tempdir().expect("temp dir");
    let config_path = directory.path().join("config.json");
    let root = directory.path().join("adapters");
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
    let initial_document = ConfigDocument {
        path: config_path.clone(),
        from_disk: false,
        config: VinputConfig::bundled_default().expect("bundled config"),
    };
    let initial_plan = prepared_plan(prepare_registry_script_from_source(
        &initial_document,
        LiveScriptKind::LlmAdapter,
        "fixture",
        &RegistryOperationControl::default(),
        &registry,
        &root,
    ));
    let old_bytes = b"#!/usr/bin/env python3\nprint('old')\n";
    let initial = install_registry_script_from_source(
        &initial_document,
        &initial_plan,
        &RegistryOperationControl::default(),
        &FixtureAssetSource(old_bytes),
    );
    assert!(matches!(initial, ScriptInstallOutcome::Installed(_)));
    let installed_config = VinputConfig::from_json_file(&config_path).expect("installed config");
    let update_document = ConfigDocument {
        path: config_path,
        from_disk: true,
        config: installed_config,
    };
    let update_plan = prepared_plan(prepare_registry_script_from_source(
        &update_document,
        LiveScriptKind::LlmAdapter,
        "fixture",
        &RegistryOperationControl::default(),
        &registry,
        &root,
    ));
    let new_bytes = b"#!/usr/bin/env python3\nprint('new')\n";

    let outcome = install_registry_script_from_source_and_save(
        &update_document,
        &update_plan,
        &RegistryOperationControl::default(),
        &FixtureAssetSource(new_bytes),
        |_, _| Err("daemon reload rejected replacement".to_owned()),
    );

    assert!(matches!(
        outcome,
        ScriptInstallOutcome::Failed(ref error)
            if error.contains("Previous managed adapter script restored")
    ));
    assert_eq!(
        std::fs::read(&update_plan.script_path).expect("restored canonical script"),
        old_bytes
    );
    assert_eq!(
        std::fs::read(vinput_registry::managed_script_rollback_path(
            &update_plan.script_path
        ))
        .expect("retained rollback script"),
        old_bytes
    );
}

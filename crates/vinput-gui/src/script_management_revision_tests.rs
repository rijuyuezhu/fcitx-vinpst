use std::path::Path;

use super::*;
use vinput_registry::{RegistryAssetSource, RegistryTextSource};

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
}

//! Integration coverage for live provider/adapter script registries.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use vinput_config::LlmAdapterConfig;
use vinput_registry::{
    AssetChecksumStatus, LiveScriptKind, LiveScriptRegistry, LlmAdapterMaterializationError,
    RegistryAssetSource, install_live_script, managed_script_relative_path,
    materialize_llm_adapter,
};

const ADAPTER_REGISTRY: &str = r#"
{
  "version": 1,
  "items": [
    {
      "id": "adapter.mtranserver.proxy",
      "short_id": "mtran-proxy",
      "command": "python3",
      "script_urls": ["memory://missing", "memory://entry.py"],
      "readme_url": "https://example.test/README.md",
      "envs": [
        {"name": "MTRAN_URL", "required": false},
        {"name": "MTRAN_TOKEN", "required": true}
      ]
    }
  ]
}
"#;

struct MemoryAssetSource;

impl RegistryAssetSource for MemoryAssetSource {
    fn fetch_asset(&self, url: &str, destination: &Path) -> Result<(), String> {
        if url == "memory://entry.py" {
            fs::write(destination, b"#!/usr/bin/env python3\nprint('ok')\n")
                .map_err(|error| error.to_string())
        } else {
            Err("fixture mirror unavailable".to_owned())
        }
    }
}

#[test]
fn parses_current_adapter_registry_and_resolves_short_id() {
    let registry = LiveScriptRegistry::from_json_str(ADAPTER_REGISTRY, LiveScriptKind::LlmAdapter)
        .expect("parse adapter registry");
    let entry = registry
        .entry_by_id_or_short_id("mtran-proxy", LiveScriptKind::LlmAdapter)
        .expect("resolve short id");

    assert_eq!(entry.id, "adapter.mtranserver.proxy");
    assert_eq!(entry.command, "python3");
    assert_eq!(entry.envs.len(), 2);
    assert_eq!(
        managed_script_relative_path(LiveScriptKind::LlmAdapter, &entry.id).expect("managed path"),
        PathBuf::from("mtranserver/proxy")
    );
}

#[test]
fn rejects_provider_entries_in_adapter_registry() {
    let input = ADAPTER_REGISTRY.replace("adapter.mtranserver.proxy", "provider.mtranserver.proxy");
    let error = LiveScriptRegistry::from_json_str(&input, LiveScriptKind::LlmAdapter)
        .expect_err("provider id should be rejected");
    assert!(
        error
            .to_string()
            .contains("does not belong to `adapter` registry")
    );
}

#[test]
fn installs_script_with_mirror_fallback_and_executable_permission() {
    let registry = LiveScriptRegistry::from_json_str(ADAPTER_REGISTRY, LiveScriptKind::LlmAdapter)
        .expect("parse adapter registry");
    let entry = &registry.items[0];
    let temp = tempfile::tempdir().expect("temporary root");

    let result = install_live_script(
        &MemoryAssetSource,
        LiveScriptKind::LlmAdapter,
        entry,
        temp.path(),
    )
    .expect("install managed script");

    assert_eq!(result.script_path, temp.path().join("mtranserver/proxy"));
    assert_eq!(result.checksum, AssetChecksumStatus::Missing);
    assert!(
        fs::read_to_string(&result.script_path)
            .expect("read installed script")
            .contains("print('ok')")
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = fs::metadata(&result.script_path)
            .expect("script metadata")
            .permissions()
            .mode();
        assert_ne!(mode & 0o111, 0);
    }
}

#[test]
fn materializes_new_adapter_with_blank_registry_envs() {
    let registry = LiveScriptRegistry::from_json_str(ADAPTER_REGISTRY, LiveScriptKind::LlmAdapter)
        .expect("parse adapter registry");
    let script_path = Path::new("/tmp/vinput/adapters/mtranserver/proxy");

    let outcome = materialize_llm_adapter(&registry.items[0], script_path, None)
        .expect("materialize new adapter");

    assert!(!outcome.replacing_managed);
    assert_eq!(outcome.adapter.id, "adapter.mtranserver.proxy");
    assert_eq!(outcome.adapter.command, "python3");
    assert_eq!(
        outcome.adapter.args,
        vec![script_path.to_string_lossy().into_owned()]
    );
    assert_eq!(outcome.adapter.env["MTRAN_URL"], "");
    assert_eq!(outcome.adapter.env["MTRAN_TOKEN"], "");
}

#[test]
fn updates_managed_adapter_without_losing_env_or_extra_fields() {
    let registry = LiveScriptRegistry::from_json_str(ADAPTER_REGISTRY, LiveScriptKind::LlmAdapter)
        .expect("parse adapter registry");
    let script_path = Path::new("/tmp/vinput/adapters/mtranserver/proxy");
    let existing = LlmAdapterConfig {
        id: "adapter.mtranserver.proxy".to_owned(),
        command: "old-python".to_owned(),
        args: vec![script_path.to_string_lossy().into_owned()],
        env: HashMap::from([
            ("MTRAN_TOKEN".to_owned(), "preserve-me".to_owned()),
            ("CUSTOM".to_owned(), "also-preserve".to_owned()),
        ]),
        working_dir: Some("/tmp/work".to_owned()),
        extra: HashMap::from([("future".to_owned(), serde_json::json!(true))]),
    };

    let outcome = materialize_llm_adapter(&registry.items[0], script_path, Some(&existing))
        .expect("update managed adapter");

    assert!(outcome.replacing_managed);
    assert_eq!(outcome.adapter.command, "python3");
    assert_eq!(outcome.adapter.env["MTRAN_TOKEN"], "preserve-me");
    assert_eq!(outcome.adapter.env["MTRAN_URL"], "");
    assert_eq!(outcome.adapter.env["CUSTOM"], "also-preserve");
    assert_eq!(outcome.adapter.working_dir.as_deref(), Some("/tmp/work"));
    assert_eq!(outcome.adapter.extra["future"], serde_json::json!(true));
}

#[test]
fn refuses_to_replace_user_defined_adapter() {
    let registry = LiveScriptRegistry::from_json_str(ADAPTER_REGISTRY, LiveScriptKind::LlmAdapter)
        .expect("parse adapter registry");
    let existing = LlmAdapterConfig {
        id: "adapter.mtranserver.proxy".to_owned(),
        command: "custom-adapter".to_owned(),
        args: vec!["--serve".to_owned()],
        env: HashMap::new(),
        working_dir: None,
        extra: HashMap::new(),
    };

    let error = materialize_llm_adapter(
        &registry.items[0],
        "/tmp/vinput/adapters/mtranserver/proxy",
        Some(&existing),
    )
    .expect_err("custom adapter should not be overwritten");

    assert!(matches!(
        error,
        LlmAdapterMaterializationError::UserDefinedAdapter(_)
    ));
}

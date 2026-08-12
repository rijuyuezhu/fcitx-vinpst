//! Integration coverage for live provider/adapter script registries.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use vinpst_config::{AsrProviderConfig, AsrProviderKind, LlmAdapterConfig};
use vinpst_registry::{
    AsrProviderMaterializationError, AssetChecksumStatus, LiveRegistryI18n, LiveScriptKind,
    LiveScriptRegistry, LlmAdapterMaterializationError, RegistryAssetSource, install_live_script,
    managed_script_relative_path, materialize_asr_provider, materialize_llm_adapter,
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

const PROVIDER_REGISTRY: &str = r#"
{
  "version": 1,
  "items": [
    {
      "id": "provider.openai-compatible.streaming",
      "short_id": "oai-stream",
      "stream": true,
      "command": "python3",
      "script_urls": ["memory://entry.py"],
      "readme_url": "https://example.test/provider/README.md",
      "envs": [
        {"name": "VINPST_ASR_API_KEY", "required": true},
        {"name": "VINPST_ASR_MODEL", "required": false}
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
fn resolves_script_display_text_from_flat_i18n_map() {
    let registry = LiveScriptRegistry::from_json_str(ADAPTER_REGISTRY, LiveScriptKind::LlmAdapter)
        .expect("parse adapter registry");
    let i18n = LiveRegistryI18n::from_json_str(
        r#"{
          "adapter.mtranserver.proxy.title":"MTranServer 代理",
          "adapter.mtranserver.proxy.description":"本地代理描述"
        }"#,
    )
    .expect("parse script i18n");

    assert_eq!(
        registry.items[0].resolved_title(Some(&i18n)),
        "MTranServer 代理"
    );
    assert_eq!(
        registry.items[0]
            .resolved_description(Some(&i18n))
            .as_deref(),
        Some("本地代理描述")
    );
}

#[test]
fn script_display_falls_back_to_short_id_then_full_id() {
    let registry = LiveScriptRegistry::from_json_str(ADAPTER_REGISTRY, LiveScriptKind::LlmAdapter)
        .expect("parse adapter registry");
    let empty = LiveRegistryI18n::from_json_str(
        r#"{
          "adapter.mtranserver.proxy.title":"   ",
          "adapter.mtranserver.proxy.description":""
        }"#,
    )
    .expect("parse empty script i18n");

    assert_eq!(
        registry.items[0].resolved_title(Some(&empty)),
        "mtran-proxy"
    );
    assert_eq!(registry.items[0].resolved_description(Some(&empty)), None);

    let mut entry = registry.items[0].clone();
    entry.short_id = None;
    assert_eq!(entry.resolved_title(None), "adapter.mtranserver.proxy");
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
    let script_path = Path::new("/tmp/vinpst/adapters/mtranserver/proxy");

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
fn updates_managed_adapter_without_losing_env_or_revision_fields() {
    let registry = LiveScriptRegistry::from_json_str(ADAPTER_REGISTRY, LiveScriptKind::LlmAdapter)
        .expect("parse adapter registry");
    let script_path = Path::new("/tmp/vinpst/adapters/mtranserver/proxy");
    let existing = LlmAdapterConfig {
        id: "adapter.mtranserver.proxy".to_owned(),
        command: "old-python".to_owned(),
        args: vec!["/tmp/vinpst/adapters/mtranserver/./proxy".to_owned()],
        env: HashMap::from([
            ("MTRAN_TOKEN".to_owned(), "preserve-me".to_owned()),
            ("CUSTOM".to_owned(), "also-preserve".to_owned()),
        ]),
        working_dir: Some("/tmp/work".to_owned()),
        managed_script_sha256: Some("current-revision".to_owned()),
        managed_script_rollback_sha256: Some("rollback-revision".to_owned()),
    };

    let outcome = materialize_llm_adapter(&registry.items[0], script_path, Some(&existing))
        .expect("update managed adapter");

    assert!(outcome.replacing_managed);
    assert_eq!(outcome.adapter.command, "python3");
    assert_eq!(outcome.adapter.env["MTRAN_TOKEN"], "preserve-me");
    assert_eq!(outcome.adapter.env["MTRAN_URL"], "");
    assert_eq!(outcome.adapter.env["CUSTOM"], "also-preserve");
    assert_eq!(outcome.adapter.working_dir.as_deref(), Some("/tmp/work"));
    assert_eq!(
        outcome.adapter.managed_script_sha256.as_deref(),
        Some("current-revision")
    );
    assert_eq!(
        outcome.adapter.managed_script_rollback_sha256.as_deref(),
        Some("rollback-revision")
    );
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
        managed_script_sha256: None,
        managed_script_rollback_sha256: None,
    };

    let error = materialize_llm_adapter(
        &registry.items[0],
        "/tmp/vinpst/adapters/mtranserver/proxy",
        Some(&existing),
    )
    .expect_err("custom adapter should not be overwritten");

    assert!(matches!(
        error,
        LlmAdapterMaterializationError::UserDefinedAdapter(_)
    ));
}

#[test]
fn parses_current_provider_registry_and_maps_multi_segment_id() {
    let registry =
        LiveScriptRegistry::from_json_str(PROVIDER_REGISTRY, LiveScriptKind::AsrProvider)
            .expect("parse provider registry");
    let entry = registry
        .entry_by_id_or_short_id("oai-stream", LiveScriptKind::AsrProvider)
        .expect("resolve provider short id");

    assert!(entry.stream);
    assert_eq!(entry.id, "provider.openai-compatible.streaming");
    assert_eq!(
        managed_script_relative_path(LiveScriptKind::AsrProvider, &entry.id)
            .expect("managed provider path"),
        PathBuf::from("openai-compatible/streaming")
    );
}

#[test]
fn rejects_provider_stream_flag_that_disagrees_with_id() {
    let input = PROVIDER_REGISTRY.replace("\"stream\": true", "\"stream\": false");
    let error = LiveScriptRegistry::from_json_str(&input, LiveScriptKind::AsrProvider)
        .expect_err("stream mismatch should fail");
    assert!(error.to_string().contains("`.streaming` suffix"));
}

#[test]
fn materializes_new_command_provider_with_legacy_timeout_and_envs() {
    let registry =
        LiveScriptRegistry::from_json_str(PROVIDER_REGISTRY, LiveScriptKind::AsrProvider)
            .expect("parse provider registry");
    let script_path = Path::new("/tmp/vinpst/providers/openai-compatible/streaming");

    let outcome = materialize_asr_provider(&registry.items[0], script_path, None)
        .expect("materialize provider");

    assert!(!outcome.replacing_managed);
    assert_eq!(outcome.provider.kind, AsrProviderKind::Command);
    assert_eq!(outcome.provider.timeout_ms, Some(60_000));
    assert_eq!(outcome.provider.command.as_deref(), Some("python3"));
    assert_eq!(
        outcome.provider.args,
        vec![script_path.to_string_lossy().into_owned()]
    );
    assert_eq!(outcome.provider.env["VINPST_ASR_API_KEY"], "");
    assert_eq!(outcome.provider.env["VINPST_ASR_MODEL"], "");
}

#[test]
fn updates_managed_provider_without_losing_env_or_timeout() {
    let registry =
        LiveScriptRegistry::from_json_str(PROVIDER_REGISTRY, LiveScriptKind::AsrProvider)
            .expect("parse provider registry");
    let script_path = Path::new("/tmp/vinpst/providers/openai-compatible/streaming");
    let existing = AsrProviderConfig {
        id: "provider.openai-compatible.streaming".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: Some(12_345),
        model: Some("preserve-model".to_owned()),
        hotwords_file: None,
        command: Some("old-python".to_owned()),
        args: vec!["/tmp/vinpst/providers/openai-compatible/./streaming".to_owned()],
        env: HashMap::from([
            ("VINPST_ASR_API_KEY".to_owned(), "preserve-me".to_owned()),
            ("CUSTOM".to_owned(), "also-preserve".to_owned()),
        ]),
        endpoint: None,
    };

    let outcome = materialize_asr_provider(&registry.items[0], script_path, Some(&existing))
        .expect("update managed provider");

    assert!(outcome.replacing_managed);
    assert_eq!(outcome.provider.timeout_ms, Some(12_345));
    assert_eq!(outcome.provider.model.as_deref(), Some("preserve-model"));
    assert_eq!(outcome.provider.env["VINPST_ASR_API_KEY"], "preserve-me");
    assert_eq!(outcome.provider.env["VINPST_ASR_MODEL"], "");
    assert_eq!(outcome.provider.env["CUSTOM"], "also-preserve");
}

#[test]
fn refuses_to_replace_non_managed_provider() {
    let registry =
        LiveScriptRegistry::from_json_str(PROVIDER_REGISTRY, LiveScriptKind::AsrProvider)
            .expect("parse provider registry");
    let existing = AsrProviderConfig {
        id: "provider.openai-compatible.streaming".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("custom".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: HashMap::new(),
        endpoint: None,
    };

    let error = materialize_asr_provider(
        &registry.items[0],
        "/tmp/vinpst/providers/openai-compatible/streaming",
        Some(&existing),
    )
    .expect_err("local provider should not be overwritten");

    assert!(matches!(
        error,
        AsrProviderMaterializationError::UserDefinedProvider(_)
    ));
}

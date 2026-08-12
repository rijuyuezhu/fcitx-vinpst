//! Managed provider and adapter removal transaction tests.

use std::fs;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use vinpst_config::VinpstConfig;
use vinpst_registry::LiveScriptEntry;

use super::*;

#[test]
fn managed_provider_removal_updates_config_and_deletes_script() {
    let directory = tempfile::tempdir().expect("temp dir");
    let root = directory.path().join("providers");
    let entry = LiveScriptEntry {
        id: "provider.fixture.batch".to_owned(),
        short_id: Some("fixture".to_owned()),
        stream: false,
        command: "python3".to_owned(),
        script_urls: vec!["https://example.invalid/provider.py".to_owned()],
        readme_url: None,
        envs: Vec::new(),
    };
    let (config, _) = materialize_config(
        &VinpstConfig::bundled_default().expect("bundled config"),
        LiveScriptKind::AsrProvider,
        &entry,
        &root.join("fixture/batch"),
    )
    .expect("materialize provider");
    let document = ConfigDocument {
        path: directory.path().join("config.json"),
        from_disk: false,
        config,
    };
    fs::create_dir_all(root.join("fixture")).expect("provider dir");
    fs::write(root.join("fixture/batch"), b"provider").expect("provider script");

    let summary = remove_managed_script_entry_from_root(
        &document,
        LiveScriptKind::AsrProvider,
        &entry.id,
        &root,
        GuiLocale::EnUs,
    )
    .expect("remove provider");

    assert!(summary.contains("Removed ASR provider"));
    assert!(!root.join("fixture/batch").exists());
    let saved = VinpstConfig::from_json_file(&document.path).expect("saved config");
    assert!(
        saved
            .asr
            .providers
            .iter()
            .all(|provider| provider.id != entry.id)
    );
}

#[cfg(unix)]
#[test]
fn failed_provider_config_commit_preserves_managed_script() {
    let directory = tempfile::tempdir().expect("temp dir");
    let root = directory.path().join("providers");
    let entry = LiveScriptEntry {
        id: "provider.fixture.batch".to_owned(),
        short_id: Some("fixture".to_owned()),
        stream: false,
        command: "python3".to_owned(),
        script_urls: vec!["https://example.invalid/provider.py".to_owned()],
        readme_url: None,
        envs: Vec::new(),
    };
    let (config, _) = materialize_config(
        &VinpstConfig::bundled_default().expect("bundled config"),
        LiveScriptKind::AsrProvider,
        &entry,
        &root.join("fixture/batch"),
    )
    .expect("materialize provider");
    let config_path = directory.path().join("config/config.json");
    fs::create_dir_all(config_path.parent().expect("config parent")).expect("config dir");
    let original = serde_json::to_vec_pretty(&config).expect("serialize config");
    fs::write(&config_path, &original).expect("write config");
    let document = ConfigDocument {
        path: config_path.clone(),
        from_disk: true,
        config,
    };
    let script_path = root.join("fixture/batch");
    fs::create_dir_all(script_path.parent().expect("provider dir")).expect("provider dir");
    fs::write(&script_path, b"provider").expect("provider script");

    let config_parent = config_path.parent().expect("config parent");
    fs::set_permissions(config_parent, fs::Permissions::from_mode(0o500))
        .expect("read-only config dir");
    let result = remove_managed_script_entry_from_root(
        &document,
        LiveScriptKind::AsrProvider,
        &entry.id,
        &root,
        GuiLocale::EnUs,
    );
    fs::set_permissions(config_parent, fs::Permissions::from_mode(0o700))
        .expect("restore config dir");

    let error = result.expect_err("config commit must fail");
    assert!(error.contains("Save configuration"));
    assert_eq!(fs::read(&config_path).expect("read config"), original);
    assert_eq!(fs::read(&script_path).expect("read script"), b"provider");
    assert!(!config_path.with_extension("json.bak").exists());
}

#[test]
fn active_managed_provider_removal_is_rejected_without_mutation() {
    let directory = tempfile::tempdir().expect("temp dir");
    let root = directory.path().join("providers");
    let entry = LiveScriptEntry {
        id: "provider.fixture.batch".to_owned(),
        short_id: Some("fixture".to_owned()),
        stream: false,
        command: "python3".to_owned(),
        script_urls: vec!["https://example.invalid/provider.py".to_owned()],
        readme_url: None,
        envs: Vec::new(),
    };
    let (mut config, _) = materialize_config(
        &VinpstConfig::bundled_default().expect("bundled config"),
        LiveScriptKind::AsrProvider,
        &entry,
        &root.join("fixture/batch"),
    )
    .expect("materialize provider");
    config.asr.active_provider.clone_from(&entry.id);
    let document = ConfigDocument {
        path: directory.path().join("config.json"),
        from_disk: false,
        config,
    };
    fs::create_dir_all(root.join("fixture")).expect("provider dir");
    fs::write(root.join("fixture/batch"), b"provider").expect("provider script");

    let error = remove_managed_script_entry_from_root(
        &document,
        LiveScriptKind::AsrProvider,
        &entry.id,
        &root,
        GuiLocale::EnUs,
    )
    .expect_err("active provider must be rejected");

    assert!(error.contains("Active ASR provider"));
    assert!(root.join("fixture/batch").exists());
    assert!(!document.path.exists());
}

#[test]
fn user_defined_adapter_removal_is_rejected_without_mutation() {
    let directory = tempfile::tempdir().expect("temp dir");
    let root = directory.path().join("adapters");
    let mut config = VinpstConfig::bundled_default().expect("bundled config");
    config.llm.adapters.push(vinpst_config::LlmAdapterConfig {
        id: "adapter.user.command".to_owned(),
        command: "python3".to_owned(),
        args: vec!["/tmp/user-adapter.py".to_owned()],
        env: std::collections::HashMap::default(),
        working_dir: None,
        managed_script_sha256: None,
        managed_script_rollback_sha256: None,
    });
    let document = ConfigDocument {
        path: directory.path().join("config.json"),
        from_disk: false,
        config,
    };

    let error = remove_managed_script_entry_from_root(
        &document,
        LiveScriptKind::LlmAdapter,
        "adapter.user.command",
        &root,
        GuiLocale::EnUs,
    )
    .expect_err("user-defined adapter must be rejected");

    assert!(error.contains("not a managed registry adapter"));
    assert!(!document.path.exists());
}

#[test]
fn managed_adapter_removal_updates_config_and_deletes_script() {
    let directory = tempfile::tempdir().expect("temp dir");
    let root = directory.path().join("adapters");
    let entry = LiveScriptEntry {
        id: "adapter.fixture.command".to_owned(),
        short_id: Some("fixture".to_owned()),
        stream: false,
        command: "python3".to_owned(),
        script_urls: vec!["https://example.invalid/adapter.py".to_owned()],
        readme_url: None,
        envs: Vec::new(),
    };
    let (config, _) = materialize_config(
        &VinpstConfig::bundled_default().expect("bundled config"),
        LiveScriptKind::LlmAdapter,
        &entry,
        &root.join("fixture/command"),
    )
    .expect("materialize adapter");
    let document = ConfigDocument {
        path: directory.path().join("config.json"),
        from_disk: false,
        config,
    };
    fs::create_dir_all(root.join("fixture")).expect("adapter dir");
    let script_path = root.join("fixture/command");
    let rollback_path = managed_script_rollback_path(&script_path);
    fs::write(&script_path, b"adapter").expect("adapter script");
    fs::write(&rollback_path, b"previous adapter").expect("rollback script");

    let summary = remove_managed_script_entry_from_root(
        &document,
        LiveScriptKind::LlmAdapter,
        &entry.id,
        &root,
        GuiLocale::EnUs,
    )
    .expect("remove adapter");

    assert!(summary.contains("Removed text adapter"));
    assert!(!script_path.exists());
    assert!(!rollback_path.exists());
    let saved = VinpstConfig::from_json_file(&document.path).expect("saved config");
    assert!(
        saved
            .llm
            .adapters
            .iter()
            .all(|adapter| adapter.id != entry.id)
    );
}

#[cfg(unix)]
#[test]
fn failed_adapter_config_commit_preserves_managed_script_and_rollback() {
    let directory = tempfile::tempdir().expect("temp dir");
    let root = directory.path().join("adapters");
    let entry = LiveScriptEntry {
        id: "adapter.fixture.command".to_owned(),
        short_id: Some("fixture".to_owned()),
        stream: false,
        command: "python3".to_owned(),
        script_urls: vec!["https://example.invalid/adapter.py".to_owned()],
        readme_url: None,
        envs: Vec::new(),
    };
    let (config, _) = materialize_config(
        &VinpstConfig::bundled_default().expect("bundled config"),
        LiveScriptKind::LlmAdapter,
        &entry,
        &root.join("fixture/command"),
    )
    .expect("materialize adapter");
    let config_path = directory.path().join("config/config.json");
    fs::create_dir_all(config_path.parent().expect("config parent")).expect("config dir");
    let original = serde_json::to_vec_pretty(&config).expect("serialize config");
    fs::write(&config_path, &original).expect("write config");
    let document = ConfigDocument {
        path: config_path.clone(),
        from_disk: true,
        config,
    };
    let script_path = root.join("fixture/command");
    let rollback_path = managed_script_rollback_path(&script_path);
    fs::create_dir_all(script_path.parent().expect("adapter dir")).expect("adapter dir");
    fs::write(&script_path, b"adapter").expect("adapter script");
    fs::write(&rollback_path, b"previous adapter").expect("rollback script");

    let config_parent = config_path.parent().expect("config parent");
    fs::set_permissions(config_parent, fs::Permissions::from_mode(0o500))
        .expect("read-only config dir");
    let result = remove_managed_script_entry_from_root(
        &document,
        LiveScriptKind::LlmAdapter,
        &entry.id,
        &root,
        GuiLocale::EnUs,
    );
    fs::set_permissions(config_parent, fs::Permissions::from_mode(0o700))
        .expect("restore config dir");

    let error = result.expect_err("config commit must fail");
    assert!(error.contains("Save configuration"));
    assert_eq!(fs::read(&config_path).expect("read config"), original);
    assert_eq!(fs::read(&script_path).expect("read script"), b"adapter");
    assert_eq!(
        fs::read(&rollback_path).expect("read rollback"),
        b"previous adapter"
    );
    assert!(!config_path.with_extension("json.bak").exists());
}

use std::{collections::HashMap, fs};

use super::*;

#[test]
fn bundled_snapshot_is_redacted_and_has_legacy_pages() {
    let snapshot = headless_snapshot(Some(Path::new("/missing/config.json")), false)
        .expect("build offline GUI snapshot");
    assert_eq!(snapshot["application"], "vinput-gui");
    assert_eq!(
        snapshot["pages"],
        json!(["Control", "Resources", "LLM", "Hotwords"])
    );
    assert_eq!(snapshot["daemon"]["skipped"], true);
    assert!(!snapshot.to_string().contains("api_key"));
}

#[test]
fn resource_filter_matches_provider_and_scene_rows() {
    let config = VinputConfig::bundled_default().expect("bundled config");
    assert!(
        filtered_asr_rows(&config, "sherpa")
            .iter()
            .any(|row| row.contains("sherpa-onnx"))
    );
    assert!(
        filtered_scene_rows(&config, "raw")
            .iter()
            .any(|row| row.contains("__raw__"))
    );
}

#[test]
fn adapter_rows_never_expose_commands_or_environment() {
    let mut config = VinputConfig::bundled_default().expect("bundled config");
    config.llm.adapters.push(vinput_config::LlmAdapterConfig {
        id: "safe-adapter".to_owned(),
        command: "helper --token super-secret".to_owned(),
        args: vec!["--api-key".to_owned(), "another-secret".to_owned()],
        env: HashMap::from([("TOKEN".to_owned(), "env-secret".to_owned())]),
        working_dir: None,
        extra: HashMap::new(),
    });

    let rows = llm_adapter_rows(&config).join("\n");
    assert_eq!(rows, "safe-adapter · command adapter");
    assert!(!rows.contains("secret"));
    assert!(!rows.contains("token"));
}

#[test]
fn disk_config_is_validated_before_display() {
    let directory = tempfile::tempdir().expect("create temp dir");
    let path = directory.path().join("config.json");
    let mut config = VinputConfig::bundled_default().expect("bundled config");
    config.global.default_language = "zh-CN".to_owned();
    fs::write(
        &path,
        serde_json::to_vec_pretty(&config).expect("serialize config"),
    )
    .expect("write config");

    let loaded = load_config_document(Some(&path)).expect("load config");
    assert!(loaded.from_disk);
    assert_eq!(loaded.config.global.default_language, "zh-CN");
}

#[test]
fn config_draft_applies_every_editable_field() {
    let mut config = VinputConfig::bundled_default().expect("bundled config");
    let provider = config
        .asr
        .providers
        .last()
        .expect("bundled provider")
        .id
        .clone();
    let scene = config
        .scenes
        .definitions
        .last()
        .expect("bundled scene")
        .id
        .clone();
    let mut draft = ConfigDraft::from_config(&config);
    draft.default_language = "zh-CN".to_owned();
    draft.capture_device = "test-source".to_owned();
    draft.duck_output_while_recording = true;
    draft.duck_output_volume = 0.4;
    draft.vad_enabled = false;
    draft.vad_threshold = 0.65;
    draft.active_provider.clone_from(&provider);
    draft.active_scene.clone_from(&scene);

    draft.apply_to(&mut config);

    config.validate().expect("validate edited config");
    assert_eq!(config.global.default_language, "zh-CN");
    assert_eq!(config.global.capture_device, "test-source");
    assert!(config.global.duck_output_while_recording);
    assert!((config.global.duck_output_volume - 0.4).abs() < f32::EPSILON);
    assert!(!config.asr.vad.enabled);
    assert!((config.asr.vad.threshold - 0.65).abs() < f32::EPSILON);
    assert_eq!(config.asr.active_provider, provider);
    assert_eq!(config.scenes.active_scene, scene);
}

#[test]
fn config_draft_creates_missing_user_file_without_backup() {
    let directory = tempfile::tempdir().expect("create temp dir");
    let path = directory.path().join("nested/config.json");
    let config = VinputConfig::bundled_default().expect("bundled config");
    let document = ConfigDocument {
        path: path.clone(),
        from_disk: false,
        config,
    };
    let mut draft = ConfigDraft::from_config(&document.config);
    draft.default_language = "zh-CN".to_owned();

    let outcome = persist_config_draft(&document, &draft).expect("create user config");

    assert_eq!(outcome.path, path);
    assert_eq!(outcome.backup_path, None);
    assert_eq!(
        VinputConfig::from_json_file(&outcome.path)
            .expect("load created config")
            .global
            .default_language,
        "zh-CN"
    );
}

#[test]
fn config_draft_replaces_existing_file_with_backup() {
    let directory = tempfile::tempdir().expect("create temp dir");
    let path = directory.path().join("config.json");
    let config = VinputConfig::bundled_default().expect("bundled config");
    write_config_file(&config, &path, None).expect("write original config");
    let document = load_config_document(Some(&path)).expect("load original config");
    let mut draft = ConfigDraft::from_config(&document.config);
    draft.capture_device = "replacement-source".to_owned();

    let outcome = persist_config_draft(&document, &draft).expect("replace user config");

    let backup_path = config_backup_path(&path);
    assert_eq!(outcome.backup_path.as_deref(), Some(backup_path.as_path()));
    assert_eq!(
        VinputConfig::from_json_file(&path)
            .expect("load replaced config")
            .global
            .capture_device,
        "replacement-source"
    );
    assert_eq!(
        VinputConfig::from_json_file(&backup_path)
            .expect("load backup config")
            .global
            .capture_device,
        config.global.capture_device
    );
}

#[test]
fn config_draft_rejects_external_changes_without_overwrite() {
    let directory = tempfile::tempdir().expect("create temp dir");
    let path = directory.path().join("config.json");
    let config = VinputConfig::bundled_default().expect("bundled config");
    write_config_file(&config, &path, None).expect("write original config");
    let document = load_config_document(Some(&path)).expect("load original config");
    let mut external = config.clone();
    external.global.capture_device = "external-source".to_owned();
    write_config_file(&external, &path, None).expect("write external update");
    let mut draft = ConfigDraft::from_config(&document.config);
    draft.capture_device = "gui-source".to_owned();

    let error =
        persist_config_draft(&document, &draft).expect_err("external update must block GUI save");

    assert!(error.contains("changed on disk"));
    assert_eq!(
        VinputConfig::from_json_file(&path)
            .expect("load preserved external config")
            .global
            .capture_device,
        "external-source"
    );
    assert!(!config_backup_path(&path).exists());
}

#[test]
fn config_save_guard_requires_idle_daemon_without_active_session() {
    let idle = DaemonSnapshot {
        status: "idle".to_owned(),
        runtime: json!({"active_session": false}),
    };
    assert!(ensure_config_save_allowed(&idle).is_ok());

    let recording = DaemonSnapshot {
        status: "recording".to_owned(),
        runtime: json!({"active_session": true}),
    };
    assert!(ensure_config_save_allowed(&recording).is_err());

    let inconsistent = DaemonSnapshot {
        status: "idle".to_owned(),
        runtime: json!({"active_session": true}),
    };
    assert!(ensure_config_save_allowed(&inconsistent).is_err());
}

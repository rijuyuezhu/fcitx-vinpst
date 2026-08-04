use std::collections::HashMap;

use super::*;

fn provider(id: &str, kind: AsrProviderKind) -> AsrProviderConfig {
    let endpoint =
        (kind == AsrProviderKind::Remote).then(|| "https://example.invalid/asr".to_owned());
    let command = (kind == AsrProviderKind::Command).then(|| "/bin/true".to_owned());
    AsrProviderConfig {
        id: id.to_owned(),
        kind,
        timeout_ms: None,
        model: None,
        hotwords_file: None,
        command,
        args: Vec::new(),
        env: HashMap::new(),
        endpoint,
    }
}

#[test]
fn provider_options_include_only_hotword_capable_backends() {
    let mut config = VinputConfig::bundled_default().expect("bundled config");
    config.asr.providers = vec![
        provider("local", AsrProviderKind::Local),
        provider("remote", AsrProviderKind::Remote),
        provider("command", AsrProviderKind::Command),
    ];
    config.asr.active_provider = "remote".to_owned();

    let options = hotword_provider_options(&config);
    assert_eq!(
        options
            .iter()
            .map(HotwordProviderSelection::id)
            .collect::<Vec<_>>(),
        vec!["local", "command"]
    );
    assert_eq!(
        HotwordEditorState::from_config(&config, None)
            .selected_provider
            .as_deref(),
        Some("local")
    );
}

#[test]
fn path_mutation_sets_clears_and_rejects_remote_providers() {
    let mut config = VinputConfig::bundled_default().expect("bundled config");
    config.asr.providers = vec![
        provider("local", AsrProviderKind::Local),
        provider("remote", AsrProviderKind::Remote),
    ];
    config.asr.active_provider = "local".to_owned();

    let updated =
        update_hotword_path(&config, "local", Some("  words.txt  ")).expect("set hotword path");
    assert_eq!(
        updated.asr.providers[0].hotwords_file.as_deref(),
        Some("words.txt")
    );
    let cleared = update_hotword_path(&updated, "local", None).expect("clear hotword path");
    assert_eq!(cleared.asr.providers[0].hotwords_file, None);
    assert!(update_hotword_path(&config, "remote", Some("words.txt")).is_err());
}

#[test]
fn content_path_refuses_cross_process_relative_ambiguity() {
    let mut config = VinputConfig::bundled_default().expect("bundled config");
    let mut local = provider("local", AsrProviderKind::Local);
    local.model = Some("/managed-models/paraformer".to_owned());
    local.hotwords_file = Some("hotwords.txt".to_owned());
    let mut command = provider("command", AsrProviderKind::Command);
    command.hotwords_file = Some("relative-command-hotwords.txt".to_owned());
    config.asr.providers = vec![local, command];
    config.asr.active_provider = "local".to_owned();

    assert_eq!(
        resolved_hotword_content_path(&config, "local").expect("resolve local hotwords"),
        Some(PathBuf::from("/managed-models/paraformer/hotwords.txt"))
    );
    config.asr.providers[0].model = Some("paraformer".to_owned());
    let local_error = resolved_hotword_content_path(&config, "local")
        .expect_err("relative local model and hotword are ambiguous");
    assert!(local_error.contains("daemon process environment"));

    config.asr.providers[0].hotwords_file = Some("/tmp/local-hotwords.txt".to_owned());
    assert_eq!(
        resolved_hotword_content_path(&config, "local").expect("absolute local hotwords"),
        Some(PathBuf::from("/tmp/local-hotwords.txt"))
    );

    let command_error = resolved_hotword_content_path(&config, "command")
        .expect_err("relative command path is external");
    assert!(command_error.contains("external command"));

    config.asr.providers[1].hotwords_file = Some("/tmp/command-hotwords.txt".to_owned());
    assert_eq!(
        resolved_hotword_content_path(&config, "command")
            .expect("resolve absolute command hotwords"),
        Some(PathBuf::from("/tmp/command-hotwords.txt"))
    );
}

#[test]
fn content_save_rejects_external_config_target_changes_before_write() {
    let directory = tempfile::tempdir().expect("temp dir");
    let config_path = directory.path().join("config.json");
    let old_path = directory.path().join("old-hotwords.txt");
    let new_path = directory.path().join("new-hotwords.txt");
    fs::write(&old_path, "alpha\n").expect("old hotwords fixture");

    let mut config = VinputConfig::bundled_default().expect("bundled config");
    let mut local = provider("local", AsrProviderKind::Local);
    local.model = Some(
        directory
            .path()
            .join("model")
            .to_string_lossy()
            .into_owned(),
    );
    local.hotwords_file = Some(old_path.to_string_lossy().into_owned());
    config.asr.providers = vec![local];
    config.asr.active_provider = "local".to_owned();
    fs::write(
        &config_path,
        serde_json::to_vec_pretty(&config).expect("serialize config"),
    )
    .expect("write config");
    let document = ConfigDocument {
        path: config_path.clone(),
        from_disk: true,
        config: config.clone(),
    };
    let baseline = read_hotword_snapshot(&old_path).expect("read hotwords");

    config.asr.providers[0].hotwords_file = Some(new_path.to_string_lossy().into_owned());
    fs::write(
        &config_path,
        serde_json::to_vec_pretty(&config).expect("serialize external config"),
    )
    .expect("write external config");

    let error = save_hotword_content_for_document(
        &document,
        "local",
        &old_path,
        &baseline,
        "should-not-write\n",
    )
    .expect_err("reject external config change");
    assert!(error.contains("changed on disk"));
    assert_eq!(
        fs::read_to_string(&old_path).expect("old content"),
        "alpha\n"
    );
    assert!(!new_path.exists());
}

#[test]
fn resetting_temporary_edits_preserves_pending_activation() {
    let mut config = VinputConfig::bundled_default().expect("bundled config");
    config.asr.providers[0].hotwords_file = Some("/tmp/hotwords.txt".to_owned());
    let mut editor = HotwordEditorState::from_config(&config, None);
    editor.pending_activation = Some(PendingHotwordActivation::for_config(
        config.asr.active_provider.clone(),
    ));
    editor.path_input = "/tmp/temporary-edit.txt".to_owned();
    assert!(editor.path_is_dirty());

    editor.reset_changes();
    assert!(!editor.path_is_dirty());
    assert!(editor.pending_activation.is_some());

    editor.loaded_path = editor.content_path.clone();
    editor.baseline = Some(HotwordContentSnapshot {
        existed: true,
        content: "alpha\n".to_owned(),
        version: None,
    });
    editor.content = text_editor::Content::with_text("temporary content\n");
    assert!(editor.content_is_dirty());
    editor.reset_changes();
    assert!(!editor.content_is_dirty());
    assert!(editor.pending_activation.is_some());
}

#[test]
fn hotword_messages_redact_paths_and_loaded_content() {
    let path_message = HotwordMessage::PathChanged(SecretInput::new(
        "/home/user/private/hotwords.txt".to_owned(),
    ));
    assert!(!format!("{path_message:?}").contains("/home/user"));

    let loaded = LoadedHotwordContent {
        provider_id: "local".to_owned(),
        path: PathBuf::from("/home/user/private/hotwords.txt"),
        snapshot: HotwordContentSnapshot {
            existed: true,
            content: "private phrase".to_owned(),
            version: None,
        },
    };
    let message = HotwordMessage::ContentLoaded {
        operation_id: 7,
        result: Ok(loaded),
    };
    let debug = format!("{message:?}");
    assert!(!debug.contains("private phrase"));
    assert!(!debug.contains("/home/user"));

    let retry_message = HotwordMessage::ActivationRetried {
        operation_id: 8,
        result: Err("config /home/user/private/config.json changed".to_owned()),
    };
    assert!(!format!("{retry_message:?}").contains("/home/user"));

    let load_error = HotwordMessage::ContentLoaded {
        operation_id: 9,
        result: Err("read /home/user/private/hotwords.txt failed".to_owned()),
    };
    let save_error = HotwordMessage::ContentSaved {
        operation_id: 10,
        result: Err("config /home/user/private/config.json changed".to_owned()),
    };
    let mutation_error = HotwordMessage::MutationFinished(Err(
        "save /home/user/private/config.json failed".to_owned(),
    ));
    for message in [
        format!("{load_error:?}"),
        format!("{save_error:?}"),
        format!("{mutation_error:?}"),
    ] {
        assert!(!message.contains("/home/user"));
    }
}

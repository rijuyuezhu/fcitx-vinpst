use super::*;

#[test]
fn process_command_text_runner_writes_request_and_reads_response() {
    let mut capture_path = std::env::temp_dir();
    capture_path.push(format!(
        "vinpst-command-text-request-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            r#"cat > "$TEXT_REQUEST"; printf '%s\n' '{"text":"polished final"}'"#.to_owned(),
        ],
        env: std::collections::HashMap::from([(
            "TEXT_REQUEST".to_owned(),
            capture_path.to_string_lossy().into_owned(),
        )]),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    };

    let payload = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: Some("selection"),
    })
    .unwrap();
    assert_eq!(payload.commit_text, "polished final");

    let request: CommandTextRequest =
        serde_json::from_str(&std::fs::read_to_string(&capture_path).unwrap()).unwrap();
    std::fs::remove_file(&capture_path).unwrap();
    assert_eq!(request.adapter_id, "cmd-adapter");
    assert_eq!(request.raw_text, "raw text");
    assert_eq!(request.selected_text.as_deref(), Some("selection"));
    assert_eq!(request.scene.id, "polish");
    assert_eq!(request.scene.prompt.as_deref(), Some("polish"));
}

#[test]
fn process_command_text_runner_times_out_and_kills_descendants() {
    let directory = tempfile::tempdir().unwrap();
    let child_pid_path = directory.path().join("child.pid");
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        timeout_ms: Some(100),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            r#"sleep 30 & echo $! > "$CHILD_PID"; cat >/dev/null; wait"#.to_owned(),
        ],
        env: std::collections::HashMap::from([(
            "CHILD_PID".to_owned(),
            child_pid_path.to_string_lossy().into_owned(),
        )]),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    };

    let started = std::time::Instant::now();
    let error = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap_err();
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert_eq!(
        error,
        TextError::AdapterFailed("text adapter `cmd-adapter` timed out after 100 ms".to_owned())
    );

    let child_pid = std::fs::read_to_string(&child_pid_path)
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();
    let child_status = std::path::PathBuf::from(format!("/proc/{child_pid}/status"));
    let descendant_is_runnable = || {
        std::fs::read_to_string(&child_status).is_ok_and(|status| {
            status
                .lines()
                .find(|line| line.starts_with("State:"))
                .is_none_or(|line| !line.contains("Z (zombie)") && !line.contains("X (dead)"))
        })
    };
    for _ in 0..100 {
        if !descendant_is_runnable() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        !descendant_is_runnable(),
        "timed-out text adapter descendant {child_pid} remained runnable"
    );
}

#[test]
fn process_command_text_runner_times_out_while_helper_ignores_stdin() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        timeout_ms: Some(100),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec!["-c".to_owned(), "sleep 30".to_owned()],
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    };
    let raw_text = "x".repeat(1024 * 1024);

    let started = std::time::Instant::now();
    let error = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: &raw_text,
        scene: &prompted,
        selected_text: None,
    })
    .unwrap_err();

    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert_eq!(
        error,
        TextError::AdapterFailed("text adapter `cmd-adapter` timed out after 100 ms".to_owned())
    );
}

#[test]
fn process_command_text_runner_drains_large_stderr_without_deadlock() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        timeout_ms: Some(2_000),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            r#"cat >/dev/null; yes x | head -c 262144 >&2; printf '%s\n' '{"text":"drained final"}'"#
                .to_owned(),
        ],
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    };

    let payload = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap();

    assert_eq!(payload.commit_text, "drained final");
}

#[test]
fn process_command_text_runner_rejects_oversized_stdout() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        timeout_ms: Some(5_000),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            r"cat >/dev/null; head -c 1100000 /dev/zero | tr '\0' x; sleep 30".to_owned(),
        ],
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    };

    let started = std::time::Instant::now();
    let error = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap_err();

    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert_eq!(
        error,
        TextError::AdapterFailed(
            "text adapter `cmd-adapter` stdout exceeds 1048576-byte limit".to_owned()
        )
    );
}

#[test]
fn process_command_text_runner_rejects_oversized_stderr() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        timeout_ms: Some(5_000),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            r"cat >/dev/null; head -c 1100000 /dev/zero | tr '\0' x >&2; sleep 30".to_owned(),
        ],
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    };

    let started = std::time::Instant::now();
    let error = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap_err();

    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert_eq!(
        error,
        TextError::AdapterFailed(
            "text adapter `cmd-adapter` stderr exceeds 1048576-byte limit".to_owned()
        )
    );
}

#[test]
fn process_command_text_runner_reports_nonzero_exit() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            "cat >/dev/null; echo adapter boom >&2; exit 7".to_owned(),
        ],
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    };

    let error = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap_err();

    assert!(matches!(
        error,
        TextError::AdapterFailed(message)
            if message.contains("exited with") && message.contains("adapter boom")
    ));
}

#[test]
fn process_command_text_runner_reports_missing_program() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: format!("vinpst-missing-text-adapter-{}", std::process::id()),
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    };

    let error = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap_err();

    assert!(matches!(
        error,
        TextError::AdapterFailed(message)
            if message.contains("failed to spawn text adapter `cmd-adapter`")
    ));
}

#[test]
fn process_command_text_runner_rejects_bad_json() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            "cat >/dev/null; printf not-json".to_owned(),
        ],
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    };

    let error = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap_err();

    assert!(matches!(
        error,
        TextError::AdapterFailed(message)
            if message.contains("failed to decode text adapter response")
    ));
}

#[test]
fn process_command_text_runner_maps_helper_error_response() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            r#"cat >/dev/null; printf '%s\n' '{"error":"adapter failed"}'"#.to_owned(),
        ],
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    };

    let error = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap_err();

    assert_eq!(error, TextError::AdapterFailed("adapter failed".to_owned()));
}

#[test]
fn process_command_text_runner_reads_payload_response() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
            id: "cmd-adapter".to_owned(),
            command: "sh".to_owned(),
            args: vec![
                "-c".to_owned(),
                r#"cat >/dev/null; printf '%s\n' '{"payload":{"commit_text":"payload final","candidates":[{"text":"payload final","source":"llm"}]}}'"#.to_owned(),
            ],
            env: std::collections::HashMap::default(),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        };

    let payload = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap();

    assert_eq!(payload.commit_text, "payload final");
    assert_eq!(payload.candidates[0].text, "payload final");
    assert_eq!(payload.candidates[0].source.to_string(), "llm");
}

#[test]
fn process_command_text_runner_reports_early_exit() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            "echo early adapter boom >&2; exit 9".to_owned(),
        ],
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    };

    let error = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap_err();

    assert!(matches!(
        error,
        TextError::AdapterFailed(message)
            if message.contains("exited with")
                && message.contains("early adapter boom")
                && !message.contains("failed to write")
    ));
}

#[test]
fn process_command_text_runner_reports_empty_stderr_exit_cleanly() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec!["-c".to_owned(), "cat >/dev/null; exit 7".to_owned()],
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    };

    let error = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap_err();

    assert!(matches!(
        error,
        TextError::AdapterFailed(message)
            if message.contains("exited with") && !message.ends_with(':')
    ));
}

#[test]
fn process_command_text_runner_uses_working_dir() {
    let mut work_dir = std::env::temp_dir();
    work_dir.push(format!(
        "vinpst-command-text-workdir-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    std::fs::create_dir(&work_dir).unwrap();
    let mut capture_path = std::env::temp_dir();
    capture_path.push(format!(
        "vinpst-command-text-cwd-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            r#"pwd > "$TEXT_CWD"; cat >/dev/null; printf '%s\n' '{"text":"cwd final"}'"#.to_owned(),
        ],
        env: std::collections::HashMap::from([(
            "TEXT_CWD".to_owned(),
            capture_path.to_string_lossy().into_owned(),
        )]),
        working_dir: Some(work_dir.to_string_lossy().into_owned()),
        extra: std::collections::HashMap::default(),
    };

    let payload = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap();

    assert_eq!(payload.commit_text, "cwd final");
    assert_eq!(
        std::fs::read_to_string(&capture_path).unwrap().trim(),
        work_dir.to_string_lossy()
    );
    std::fs::remove_file(&capture_path).unwrap();
    std::fs::remove_dir(&work_dir).unwrap();
}

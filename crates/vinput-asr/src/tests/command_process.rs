use super::*;

fn process_is_runnable(pid: u32) -> bool {
    let status_path = format!("/proc/{pid}/status");
    std::fs::read_to_string(status_path).is_ok_and(|status| {
        status
            .lines()
            .find(|line| line.starts_with("State:"))
            .is_none_or(|line| !line.contains("Z (zombie)") && !line.contains("X (dead)"))
    })
}

fn wait_until_process_stops_running(pid: u32) {
    for _ in 0..100 {
        if !process_is_runnable(pid) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!process_is_runnable(pid), "process {pid} remained runnable");
}

#[test]
fn legacy_command_batch_runner_writes_raw_little_endian_pcm() {
    let script_path = write_temp_script(
        "vinput-legacy-command-asr",
        r"
import struct
import sys
samples = [value[0] for value in struct.iter_unpack('<h', sys.stdin.buffer.read())]
sys.stdout.write('|'.join(str(sample) for sample in samples))
",
    );
    let spec = CommandAsrSpec {
        provider_id: "cmd".to_owned(),
        command: "python3".to_owned(),
        args: vec![script_path.to_string_lossy().into_owned()],
        env: std::collections::HashMap::default(),
        model_id: None,
        hotwords_file: None,
        timeout_ms: Some(1_000),
    };
    let request = CommandAsrRequest::from_spec(
        &spec,
        RecognitionContext::normal("raw", Some("zh".to_owned())),
        vec![1, -2, 258],
    );

    let events = LegacyCommandBatchRunner
        .recognize(&spec, &request)
        .expect("legacy runner should decode helper output");
    std::fs::remove_file(script_path).unwrap();

    assert_eq!(
        events,
        vec![
            RecognitionEvent::FinalText {
                text: "1|-2|258".to_owned()
            },
            RecognitionEvent::Completed,
        ]
    );
}

#[test]
fn legacy_command_batch_runner_reports_nonzero_stderr() {
    let spec = CommandAsrSpec {
        provider_id: "cmd".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            "cat >/dev/null; echo batch boom >&2; exit 7".to_owned(),
        ],
        env: std::collections::HashMap::default(),
        model_id: None,
        hotwords_file: None,
        timeout_ms: Some(1_000),
    };
    let request = CommandAsrRequest::from_spec(
        &spec,
        RecognitionContext::normal("raw", None),
        vec![1, 2, 3],
    );

    let error = LegacyCommandBatchRunner
        .recognize(&spec, &request)
        .unwrap_err();
    assert!(matches!(
        error,
        AsrError::Backend(message)
            if message.contains("command ASR provider `cmd`")
                && message.contains("exited with")
                && message.contains("batch boom")
    ));
}

#[test]
fn legacy_command_batch_runner_times_out_slow_helpers() {
    let spec = CommandAsrSpec {
        provider_id: "cmd".to_owned(),
        command: "sh".to_owned(),
        args: vec!["-c".to_owned(), "cat >/dev/null; sleep 1".to_owned()],
        env: std::collections::HashMap::default(),
        model_id: None,
        hotwords_file: None,
        timeout_ms: Some(25),
    };
    let request = CommandAsrRequest::from_spec(
        &spec,
        RecognitionContext::normal("raw", None),
        vec![1, 2, 3],
    );

    let error = LegacyCommandBatchRunner
        .recognize(&spec, &request)
        .unwrap_err();
    assert!(matches!(
        error,
        AsrError::Backend(message)
            if message.contains("command ASR provider `cmd`")
                && message.contains("timed out after 25 ms")
    ));
}

#[test]
fn legacy_command_batch_runner_drains_large_stderr_without_deadlock() {
    let spec = CommandAsrSpec {
        provider_id: "cmd".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            r"cat >/dev/null; yes x | head -c 262144 >&2; printf final".to_owned(),
        ],
        env: std::collections::HashMap::default(),
        model_id: None,
        hotwords_file: None,
        timeout_ms: None,
    };
    let request = CommandAsrRequest::from_spec(
        &spec,
        RecognitionContext::normal("raw", None),
        vec![1, 2, 3],
    );

    let events = LegacyCommandBatchRunner.recognize(&spec, &request).unwrap();
    assert_eq!(
        events,
        vec![
            RecognitionEvent::FinalText {
                text: "final".to_owned()
            },
            RecognitionEvent::Completed,
        ]
    );
}

#[test]
fn legacy_command_batch_runner_rejects_empty_stdout() {
    let spec = CommandAsrSpec {
        provider_id: "cmd".to_owned(),
        command: "sh".to_owned(),
        args: vec!["-c".to_owned(), "cat >/dev/null".to_owned()],
        env: std::collections::HashMap::default(),
        model_id: None,
        hotwords_file: None,
        timeout_ms: Some(1_000),
    };
    let request = CommandAsrRequest::from_spec(
        &spec,
        RecognitionContext::normal("raw", None),
        vec![1, 2, 3],
    );

    let error = LegacyCommandBatchRunner
        .recognize(&spec, &request)
        .unwrap_err();
    assert!(matches!(
        error,
        AsrError::Backend(message)
            if message.contains("legacy command ASR provider `cmd` returned no text")
    ));
}

#[test]
fn legacy_command_streaming_audio_line_encodes_little_endian_pcm() {
    let line = legacy_command_streaming_audio_line(&[1, -2, 258], true);
    let value: serde_json::Value = serde_json::from_str(&line).unwrap();

    assert_eq!(value["type"], "audio");
    assert_eq!(value["audio_base64"], "AQD+/wIB");
    assert_eq!(value["commit"], true);
}

#[test]
fn legacy_command_streaming_finish_line_matches_control_event() {
    let value: serde_json::Value =
        serde_json::from_str(&legacy_command_streaming_finish_line()).unwrap();

    assert_eq!(value, serde_json::json!({"type": "finish"}));
}

#[test]
fn legacy_command_streaming_line_parser_maps_known_events() {
    assert_eq!(
        parse_legacy_command_streaming_line(r#"{"type":"partial","text":" hello "}"#).unwrap(),
        vec![RecognitionEvent::PartialText {
            text: "hello".to_owned()
        }]
    );
    assert_eq!(
        parse_legacy_command_streaming_line(r#"{"type":"final","text":" done "}"#).unwrap(),
        vec![RecognitionEvent::FinalText {
            text: "done".to_owned()
        }]
    );
    assert_eq!(
        parse_legacy_command_streaming_line(
            r#"{"type":"final_timestamps","text":" timed final ","timestamps":[1]}"#,
        )
        .unwrap(),
        vec![RecognitionEvent::FinalText {
            text: "timed final".to_owned()
        }]
    );
    assert_eq!(
        parse_legacy_command_streaming_line(r#"{"type":"error","message":" boom "}"#).unwrap(),
        vec![RecognitionEvent::Error {
            message: "boom".to_owned()
        }]
    );
    assert_eq!(
        parse_legacy_command_streaming_line(r#"{"type":"closed"}"#).unwrap(),
        vec![RecognitionEvent::Completed]
    );
}

#[test]
fn legacy_command_streaming_line_parser_ignores_noop_events() {
    for line in [
        "",
        "   ",
        r#"{"type":"session_started"}"#,
        r#"{"type":"partial","text":""}"#,
        r#"{"type":"final","text":""}"#,
        r#"{"type":"unknown","text":"ignored"}"#,
    ] {
        assert!(
            parse_legacy_command_streaming_line(line)
                .unwrap()
                .is_empty(),
            "line should not yield events: {line}"
        );
    }
}

#[test]
fn legacy_command_streaming_line_parser_rejects_invalid_json() {
    let error = parse_legacy_command_streaming_line("not json").unwrap_err();
    assert!(matches!(
        error,
        AsrError::Backend(message) if message.contains("invalid streaming provider JSON")
    ));
}

#[test]
fn legacy_command_streaming_line_parser_defaults_blank_error_message() {
    assert_eq!(
        parse_legacy_command_streaming_line(r#"{"type":"error","message":""}"#).unwrap(),
        vec![RecognitionEvent::Error {
            message: "failed.".to_owned()
        }]
    );
}

#[test]
fn legacy_command_streaming_runner_sends_audio_and_finish_lines() {
    let script_path = write_temp_script(
        "vinput-legacy-command-streaming-asr",
        r"
import base64
import json
import struct
import sys
lines = [json.loads(line) for line in sys.stdin if line.strip()]
audio = base64.b64decode(lines[0]['audio_base64'])
samples = [value[0] for value in struct.iter_unpack('<h', audio)]
print(json.dumps({'type':'partial','text':'partial'}))
print(json.dumps({'type':'final','text':'|'.join(str(sample) for sample in samples)}))
print(json.dumps({'type':'closed'}))
assert lines[0]['type'] == 'audio'
assert lines[0]['commit'] is True
assert lines[1]['type'] == 'finish'
",
    );
    let spec = CommandAsrSpec {
        provider_id: "cmd.streaming".to_owned(),
        command: "python3".to_owned(),
        args: vec![script_path.to_string_lossy().into_owned()],
        env: std::collections::HashMap::default(),
        model_id: None,
        hotwords_file: None,
        timeout_ms: Some(1_000),
    };
    let request = CommandAsrRequest::from_spec(
        &spec,
        RecognitionContext::normal("raw", Some("zh".to_owned())),
        vec![1, -2, 258],
    );

    let events = LegacyCommandStreamingRunner
        .recognize(&spec, &request)
        .expect("legacy streaming runner should parse helper events");
    std::fs::remove_file(script_path).unwrap();

    assert_eq!(
        events,
        vec![
            RecognitionEvent::PartialText {
                text: "partial".to_owned()
            },
            RecognitionEvent::FinalText {
                text: "1|-2|258".to_owned()
            },
            RecognitionEvent::Completed,
        ]
    );
}

#[test]
fn legacy_command_streaming_runner_deduplicates_repeated_partials() {
    let script_path = write_temp_script(
        "vinput-legacy-command-streaming-dedupe",
        r"
import json
import sys
for _ in sys.stdin:
    pass
print(json.dumps({'type':'partial','text':'same'}))
print(json.dumps({'type':'partial','text':'same'}))
print(json.dumps({'type':'partial','text':'next'}))
print(json.dumps({'type':'final','text':'done'}))
print(json.dumps({'type':'closed'}))
",
    );
    let spec = CommandAsrSpec {
        provider_id: "cmd.streaming".to_owned(),
        command: "python3".to_owned(),
        args: vec![script_path.to_string_lossy().into_owned()],
        env: std::collections::HashMap::default(),
        model_id: None,
        hotwords_file: None,
        timeout_ms: Some(1_000),
    };
    let request = CommandAsrRequest::from_spec(
        &spec,
        RecognitionContext::normal("raw", Some("zh".to_owned())),
        vec![1],
    );

    let events = LegacyCommandStreamingRunner
        .recognize(&spec, &request)
        .expect("legacy streaming runner should deduplicate repeated partials");
    std::fs::remove_file(script_path).unwrap();

    assert_eq!(
        events,
        vec![
            RecognitionEvent::PartialText {
                text: "same".to_owned()
            },
            RecognitionEvent::PartialText {
                text: "next".to_owned()
            },
            RecognitionEvent::FinalText {
                text: "done".to_owned()
            },
            RecognitionEvent::Completed,
        ]
    );
}

#[test]
fn legacy_command_streaming_runner_reports_nonzero_stderr() {
    let spec = CommandAsrSpec {
        provider_id: "cmd.streaming".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            "cat >/dev/null; echo streaming boom >&2; exit 7".to_owned(),
        ],
        env: std::collections::HashMap::default(),
        model_id: None,
        hotwords_file: None,
        timeout_ms: Some(1_000),
    };
    let request = CommandAsrRequest::from_spec(
        &spec,
        RecognitionContext::normal("raw", None),
        vec![1, 2, 3],
    );

    let error = LegacyCommandStreamingRunner
        .recognize(&spec, &request)
        .unwrap_err();
    assert!(matches!(
        error,
        AsrError::Backend(message)
            if message.contains("cmd.streaming")
                && message.contains("exited with")
                && message.contains("streaming boom")
    ));
}

#[test]
fn legacy_command_streaming_runner_times_out_slow_helpers() {
    let spec = CommandAsrSpec {
        provider_id: "cmd.streaming".to_owned(),
        command: "sh".to_owned(),
        args: vec!["-c".to_owned(), "cat >/dev/null; sleep 1".to_owned()],
        env: std::collections::HashMap::default(),
        model_id: None,
        hotwords_file: None,
        timeout_ms: Some(25),
    };
    let request = CommandAsrRequest::from_spec(
        &spec,
        RecognitionContext::normal("raw", None),
        vec![1, 2, 3],
    );

    let error = LegacyCommandStreamingRunner
        .recognize(&spec, &request)
        .unwrap_err();
    assert!(matches!(
        error,
        AsrError::Backend(message)
            if message.contains("command ASR provider `cmd.streaming`")
                && message.contains("timed out after 25 ms")
    ));
}

#[test]
fn legacy_command_streaming_runner_cleans_descendants_after_direct_exit() {
    let directory = tempfile::tempdir().unwrap();
    let child_pid_path = directory.path().join("child.pid");
    let spec = CommandAsrSpec {
        provider_id: "cmd.streaming".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            r#"cat >/dev/null; sleep 30 & echo $! > "$CHILD_PID"; printf '%s\n' '{"type":"final","text":"stream final"}'"#
                .to_owned(),
        ],
        env: std::collections::HashMap::from([(
            "CHILD_PID".to_owned(),
            child_pid_path.to_string_lossy().into_owned(),
        )]),
        model_id: None,
        hotwords_file: None,
        timeout_ms: None,
    };
    let request = CommandAsrRequest::from_spec(
        &spec,
        RecognitionContext::normal("raw", None),
        vec![1, 2, 3],
    );

    let started = std::time::Instant::now();
    let events = LegacyCommandStreamingRunner
        .recognize(&spec, &request)
        .unwrap();
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert_eq!(
        events,
        vec![
            RecognitionEvent::FinalText {
                text: "stream final".to_owned()
            },
            RecognitionEvent::Completed,
        ]
    );
    let child_pid = std::fs::read_to_string(child_pid_path)
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();
    wait_until_process_stops_running(child_pid);
}

#[test]
fn legacy_command_streaming_runner_rejects_empty_stdout() {
    let spec = CommandAsrSpec {
        provider_id: "cmd.streaming".to_owned(),
        command: "sh".to_owned(),
        args: vec!["-c".to_owned(), "cat >/dev/null".to_owned()],
        env: std::collections::HashMap::default(),
        model_id: None,
        hotwords_file: None,
        timeout_ms: Some(1_000),
    };
    let request = CommandAsrRequest::from_spec(
        &spec,
        RecognitionContext::normal("raw", None),
        vec![1, 2, 3],
    );

    let error = LegacyCommandStreamingRunner
        .recognize(&spec, &request)
        .unwrap_err();
    assert!(matches!(
        error,
        AsrError::Backend(message)
            if message.contains("legacy command streaming provider returned no events")
    ));
}

#[test]
fn process_command_asr_runner_maps_partial_and_final_response() {
    let provider = AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: Some(1_000),
        model: None,
        hotwords_file: None,
        command: Some("sh".to_owned()),
        args: vec![
            "-c".to_owned(),
            r#"cat >/dev/null; printf '%s
' '{"partial_text":"listening","text":"final"}'"#
                .to_owned(),
        ],
        env: std::collections::HashMap::default(),
        endpoint: None,
    };

    let backend = CommandAsrBackend::with_config(&provider, ProcessCommandAsrRunner).unwrap();
    let mut session = backend
        .create_session(RecognitionContext::normal("raw", None))
        .expect("process runner should create a buffering session");
    session.finish().unwrap();

    let events = session.poll_events().unwrap();
    assert_eq!(
        events,
        vec![
            RecognitionEvent::PartialText {
                text: "listening".to_owned()
            },
            RecognitionEvent::FinalText {
                text: "final".to_owned()
            },
            RecognitionEvent::Completed,
        ]
    );
    assert_eq!(events_to_payload(&events).unwrap().commit_text, "final");
}

#[test]
fn process_command_asr_runner_writes_request_and_reads_response() {
    let mut capture_path = std::env::temp_dir();
    capture_path.push(format!(
        "vinput-command-asr-request-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    let provider = AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: Some(2_500),
        model: Some("paraformer".to_owned()),
        hotwords_file: Some("/tmp/hotwords.txt".to_owned()),
        command: Some("sh".to_owned()),
        args: vec![
            "-c".to_owned(),
            r#"cat > "$ASR_REQUEST"; printf '%s\n' '{"text":"process final"}'"#.to_owned(),
        ],
        env: std::collections::HashMap::from([(
            "ASR_REQUEST".to_owned(),
            capture_path.to_string_lossy().into_owned(),
        )]),
        endpoint: None,
    };

    let backend = CommandAsrBackend::with_config(&provider, ProcessCommandAsrRunner).unwrap();
    let mut session = backend
        .create_session(RecognitionContext::command(
            "__command__",
            Some("zh".to_owned()),
            "selected text",
        ))
        .expect("process runner should create a buffering session");
    let pcm = PcmBuffer::with_spec(
        PcmSpec {
            sample_rate_hz: 8_000,
            channels: 1,
        },
        vec![10, -20, 30],
    )
    .unwrap();
    session.push_pcm(&pcm).unwrap();
    session.finish().unwrap();
    let payload = events_to_payload(&session.poll_events().unwrap()).unwrap();
    assert_eq!(payload.commit_text, "process final");

    let request: CommandAsrRequest =
        serde_json::from_str(&std::fs::read_to_string(&capture_path).unwrap()).unwrap();
    std::fs::remove_file(&capture_path).unwrap();
    assert_eq!(request.provider_id, "cmd");
    assert_eq!(request.model_id.as_deref(), Some("paraformer"));
    assert_eq!(request.hotwords_file.as_deref(), Some("/tmp/hotwords.txt"));
    assert_eq!(request.timeout_ms, Some(2_500));
    assert_eq!(request.pcm.sample_rate_hz, 8_000);
    assert_eq!(request.pcm.channels, 1);
    assert!(request.context.command_mode);
    assert_eq!(
        request.context.selected_text.as_deref(),
        Some("selected text")
    );
    assert_eq!(request.samples, [10, -20, 30]);
}

#[test]
fn process_command_asr_runner_reports_spawn_failure() {
    let provider = AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: None,
        model: None,
        hotwords_file: None,
        command: Some(format!("vinput-missing-command-{}", std::process::id())),
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };

    let backend = CommandAsrBackend::with_config(&provider, ProcessCommandAsrRunner).unwrap();
    let mut session = backend
        .create_session(RecognitionContext::normal("raw", None))
        .expect("process runner should create a buffering session");
    let error = session.finish().unwrap_err();
    assert!(matches!(
        error,
        AsrError::Backend(message)
            if message.contains("failed to spawn command ASR provider `cmd`")
    ));
}

#[test]
fn process_command_asr_runner_times_out_slow_helpers() {
    let provider = AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: Some(25),
        model: None,
        hotwords_file: None,
        command: Some("sh".to_owned()),
        args: vec!["-c".to_owned(), "cat >/dev/null; sleep 1".to_owned()],
        env: std::collections::HashMap::default(),
        endpoint: None,
    };

    let backend = CommandAsrBackend::with_config(&provider, ProcessCommandAsrRunner).unwrap();
    let mut session = backend
        .create_session(RecognitionContext::normal("raw", None))
        .expect("process runner should create a buffering session");
    let error = session.finish().unwrap_err();
    assert!(matches!(
        error,
        AsrError::Backend(message) if message.contains("timed out after 25 ms")
    ));
}

#[test]
fn process_command_asr_runner_times_out_while_helper_ignores_large_stdin() {
    let spec = CommandAsrSpec {
        provider_id: "cmd".to_owned(),
        command: "sh".to_owned(),
        args: vec!["-c".to_owned(), "sleep 30".to_owned()],
        env: std::collections::HashMap::default(),
        model_id: None,
        hotwords_file: None,
        timeout_ms: Some(100),
    };
    let request = CommandAsrRequest::from_spec(
        &spec,
        RecognitionContext::normal("raw", None),
        vec![1; 600_000],
    );

    let started = std::time::Instant::now();
    let error = ProcessCommandAsrRunner
        .recognize(&spec, &request)
        .unwrap_err();
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert!(matches!(
        error,
        AsrError::Backend(message) if message == "command ASR provider `cmd` timed out after 100 ms"
    ));
}

#[test]
fn process_command_asr_runner_times_out_and_kills_descendants() {
    let directory = tempfile::tempdir().unwrap();
    let child_pid_path = directory.path().join("child.pid");
    let spec = CommandAsrSpec {
        provider_id: "cmd".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            r#"sleep 30 & echo $! > "$CHILD_PID"; cat >/dev/null; wait"#.to_owned(),
        ],
        env: std::collections::HashMap::from([(
            "CHILD_PID".to_owned(),
            child_pid_path.to_string_lossy().into_owned(),
        )]),
        model_id: None,
        hotwords_file: None,
        timeout_ms: Some(100),
    };
    let request = CommandAsrRequest::from_spec(
        &spec,
        RecognitionContext::normal("raw", None),
        vec![1, 2, 3],
    );

    let error = ProcessCommandAsrRunner
        .recognize(&spec, &request)
        .unwrap_err();
    assert!(matches!(
        error,
        AsrError::Backend(message) if message == "command ASR provider `cmd` timed out after 100 ms"
    ));
    let child_pid = std::fs::read_to_string(child_pid_path)
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();
    wait_until_process_stops_running(child_pid);
}

#[test]
fn process_command_asr_runner_rejects_oversized_stdout() {
    let spec = CommandAsrSpec {
        provider_id: "cmd".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            r"cat >/dev/null; head -c 1100000 /dev/zero | tr '\0' x; sleep 30".to_owned(),
        ],
        env: std::collections::HashMap::default(),
        model_id: None,
        hotwords_file: None,
        timeout_ms: None,
    };
    let request = CommandAsrRequest::from_spec(
        &spec,
        RecognitionContext::normal("raw", None),
        vec![1, 2, 3],
    );

    let started = std::time::Instant::now();
    let error = ProcessCommandAsrRunner
        .recognize(&spec, &request)
        .unwrap_err();
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert!(matches!(
        error,
        AsrError::Backend(message)
            if message == "command ASR provider `cmd` stdout exceeds 1048576-byte limit"
    ));
}

#[test]
fn process_command_asr_runner_rejects_oversized_stderr() {
    let spec = CommandAsrSpec {
        provider_id: "cmd".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            r"cat >/dev/null; head -c 1100000 /dev/zero | tr '\0' x >&2; sleep 30".to_owned(),
        ],
        env: std::collections::HashMap::default(),
        model_id: None,
        hotwords_file: None,
        timeout_ms: None,
    };
    let request = CommandAsrRequest::from_spec(
        &spec,
        RecognitionContext::normal("raw", None),
        vec![1, 2, 3],
    );

    let started = std::time::Instant::now();
    let error = ProcessCommandAsrRunner
        .recognize(&spec, &request)
        .unwrap_err();
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert!(matches!(
        error,
        AsrError::Backend(message)
            if message == "command ASR provider `cmd` stderr exceeds 1048576-byte limit"
    ));
}

#[test]
fn process_command_asr_runner_reports_early_nonzero_exit() {
    let provider = AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: None,
        model: None,
        hotwords_file: None,
        command: Some("sh".to_owned()),
        args: vec!["-c".to_owned(), "echo early boom >&2; exit 9".to_owned()],
        env: std::collections::HashMap::default(),
        endpoint: None,
    };

    let backend = CommandAsrBackend::with_config(&provider, ProcessCommandAsrRunner).unwrap();
    let mut session = backend
        .create_session(RecognitionContext::normal("raw", None))
        .expect("process runner should create a buffering session");
    let error = session.finish().unwrap_err();
    assert!(matches!(
        error,
        AsrError::Backend(message)
            if message.contains("exited with")
                && message.contains("early boom")
                && !message.contains("failed to write")
    ));
}

#[test]
fn process_command_asr_runner_reports_nonzero_exit() {
    let provider = AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: None,
        model: None,
        hotwords_file: None,
        command: Some("sh".to_owned()),
        args: vec![
            "-c".to_owned(),
            "cat >/dev/null; echo boom >&2; exit 7".to_owned(),
        ],
        env: std::collections::HashMap::default(),
        endpoint: None,
    };

    let backend = CommandAsrBackend::with_config(&provider, ProcessCommandAsrRunner).unwrap();
    let mut session = backend
        .create_session(RecognitionContext::normal("raw", None))
        .expect("process runner should create a buffering session");
    let error = session.finish().unwrap_err();
    assert!(matches!(
        error,
        AsrError::Backend(message) if message.contains("exited with") && message.contains("boom")
    ));
}

#[test]
fn process_command_asr_runner_rejects_invalid_json_response() {
    let provider = AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: None,
        model: None,
        hotwords_file: None,
        command: Some("sh".to_owned()),
        args: vec![
            "-c".to_owned(),
            "cat >/dev/null; printf not-json".to_owned(),
        ],
        env: std::collections::HashMap::default(),
        endpoint: None,
    };

    let backend = CommandAsrBackend::with_config(&provider, ProcessCommandAsrRunner).unwrap();
    let mut session = backend
        .create_session(RecognitionContext::normal("raw", None))
        .expect("process runner should create a buffering session");
    let error = session.finish().unwrap_err();
    assert!(matches!(
        error,
        AsrError::Backend(message) if message.contains("failed to decode command ASR response")
    ));
}

#[test]
fn process_command_asr_runner_rejects_missing_final_text_response() {
    let provider = AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: None,
        model: None,
        hotwords_file: None,
        command: Some("sh".to_owned()),
        args: vec!["-c".to_owned(), "cat >/dev/null; printf '{}'".to_owned()],
        env: std::collections::HashMap::default(),
        endpoint: None,
    };

    let backend = CommandAsrBackend::with_config(&provider, ProcessCommandAsrRunner).unwrap();
    let mut session = backend
        .create_session(RecognitionContext::normal("raw", None))
        .expect("process runner should create a buffering session");
    let error = session.finish().unwrap_err();
    assert!(matches!(
        error,
        AsrError::Backend(message) if message.contains("missing final text")
    ));
}

#[test]
fn command_asr_response_accepts_failure_alias() {
    let response: CommandAsrResponse =
        serde_json::from_str(r#"{"failure":"legacy failed"}"#).unwrap();
    let events = response.into_events().unwrap();
    assert_eq!(
        events_to_payload(&events).unwrap().commit_text,
        "legacy failed"
    );
}

#[test]
fn command_asr_response_rejects_missing_final_text() {
    let error = CommandAsrResponse::default().into_events().unwrap_err();
    assert!(matches!(
        error,
        AsrError::Backend(message) if message.contains("missing final text")
    ));
}

#[test]
fn command_asr_response_ignores_empty_partial_text() {
    let events = CommandAsrResponse {
        partial_text: Some(String::new()),
        text: Some("final".to_owned()),
        error: None,
    }
    .into_events()
    .unwrap();

    assert_eq!(
        events,
        vec![
            RecognitionEvent::FinalText {
                text: "final".to_owned()
            },
            RecognitionEvent::Completed,
        ]
    );
}

#[test]
fn command_asr_response_rejects_blank_final_text() {
    let error = CommandAsrResponse {
        text: Some("   	".to_owned()),
        ..CommandAsrResponse::default()
    }
    .into_events()
    .unwrap_err();
    assert!(matches!(
        error,
        AsrError::Backend(message) if message.contains("missing final text")
    ));
}

#[test]
fn command_asr_response_ignores_blank_partial_and_error_text() {
    let events = CommandAsrResponse {
        partial_text: Some("   ".to_owned()),
        text: Some("final".to_owned()),
        error: Some("   ".to_owned()),
    }
    .into_events()
    .unwrap();

    assert_eq!(
        events,
        vec![
            RecognitionEvent::FinalText {
                text: "final".to_owned()
            },
            RecognitionEvent::Completed,
        ]
    );
}

#[test]
fn command_asr_response_error_takes_priority_over_final_text() {
    let events = CommandAsrResponse {
        partial_text: Some("listening".to_owned()),
        text: Some("final".to_owned()),
        error: Some("asr failed".to_owned()),
    }
    .into_events()
    .unwrap();

    assert_eq!(
        events,
        vec![
            RecognitionEvent::PartialText {
                text: "listening".to_owned()
            },
            RecognitionEvent::Error {
                message: "asr failed".to_owned()
            },
            RecognitionEvent::Completed,
        ]
    );
    assert_eq!(
        events_to_payload(&events).unwrap().commit_text,
        "asr failed"
    );
}

#[test]
fn process_command_asr_runner_maps_failure_response() {
    let provider = AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: None,
        model: None,
        hotwords_file: None,
        command: Some("sh".to_owned()),
        args: vec![
            "-c".to_owned(),
            r#"cat >/dev/null; printf '%s
' '{"error":"asr failed"}'"#
                .to_owned(),
        ],
        env: std::collections::HashMap::default(),
        endpoint: None,
    };

    let backend = CommandAsrBackend::with_config(&provider, ProcessCommandAsrRunner).unwrap();
    let mut session = backend
        .create_session(RecognitionContext::normal("raw", None))
        .expect("process runner should create a buffering session");
    session.finish().unwrap();
    let events = session.poll_events().unwrap();
    assert_eq!(
        events_to_payload(&events).unwrap().commit_text,
        "asr failed"
    );
}

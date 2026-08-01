use super::{
    AsrBackend, AsrBackendFactory, AsrError, AudioDeliveryMode, CommandAsrBackend,
    CommandAsrRequest, CommandAsrResponse, CommandAsrRunner, CommandAsrSpec,
    LegacyCommandBatchRunner, LegacyCommandStreamingRunner, MockAsrAudioLog, MockAsrAudioPush,
    MockAsrBackend, ProcessCommandAsrRunner, RecognitionContext, RecognitionEvent,
    SherpaOnnxModelPathError, SherpaOnnxOfflineModelLayout, SherpaOnnxOfflineRuntimePlan,
    SherpaOnnxSpec, events_to_payload, legacy_command_streaming_audio_line,
    legacy_command_streaming_finish_line, parse_legacy_command_streaming_line,
};
use vinput_audio::{PcmBuffer, PcmSpec};
use vinput_config::{AsrConfig, AsrProviderConfig, AsrProviderKind};

mod command_process;

fn write_temp_script(prefix: &str, body: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "{}-{}-{}.py",
        prefix,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    std::fs::write(&path, body).expect("write temporary script");
    path
}

#[derive(Debug, Clone, Copy)]
struct FinalTextCommandRunner;

impl CommandAsrRunner for FinalTextCommandRunner {
    fn recognize(
        &self,
        spec: &CommandAsrSpec,
        request: &CommandAsrRequest,
    ) -> Result<Vec<RecognitionEvent>, AsrError> {
        CommandAsrResponse {
            text: Some(format!(
                "{}:{}:{}",
                spec.command,
                request.context.scene_id,
                request.samples.len()
            )),
            ..CommandAsrResponse::default()
        }
        .into_events()
    }
}

#[derive(Debug, Clone, Copy)]
struct ConfigEchoCommandRunner;

#[derive(Debug, Clone, Copy)]
struct PcmEchoCommandRunner;

impl CommandAsrRunner for PcmEchoCommandRunner {
    fn recognize(
        &self,
        _spec: &CommandAsrSpec,
        request: &CommandAsrRequest,
    ) -> Result<Vec<RecognitionEvent>, AsrError> {
        CommandAsrResponse {
            text: Some(format!(
                "{}|{}|{}",
                request.pcm.sample_rate_hz,
                request.pcm.channels,
                request.samples.len()
            )),
            ..CommandAsrResponse::default()
        }
        .into_events()
    }
}

impl CommandAsrRunner for ConfigEchoCommandRunner {
    fn recognize(
        &self,
        spec: &CommandAsrSpec,
        request: &CommandAsrRequest,
    ) -> Result<Vec<RecognitionEvent>, AsrError> {
        let scene_id = request.context.scene_id.clone();
        let language = request.context.language.clone().unwrap_or_default();
        let env_value = spec
            .env
            .get("ASR_MODE")
            .map(String::as_str)
            .unwrap_or_default();
        Ok(vec![
            RecognitionEvent::FinalText {
                text: format!(
                    "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                    request.provider_id,
                    spec.command,
                    spec.args.join(","),
                    env_value,
                    request.model_id.as_deref().unwrap_or_default(),
                    request.hotwords_file.as_deref().unwrap_or_default(),
                    request.timeout_ms.unwrap_or_default(),
                    scene_id,
                    language,
                    request.samples.len(),
                ),
            },
            RecognitionEvent::Completed,
        ])
    }
}

#[test]
fn recognition_context_marks_command_sessions() {
    let context = super::RecognitionContext::command("__command__", Some("zh".to_owned()), "text");
    assert!(context.command_mode);
    assert_eq!(context.scene_id, "__command__");
    assert_eq!(context.language.as_deref(), Some("zh"));
    assert_eq!(context.selected_text.as_deref(), Some("text"));
}

#[test]
fn mock_buffered_backend_emits_final_text_on_finish() {
    let backend = MockAsrBackend::buffered("hello");
    let descriptor = backend.describe();
    assert_eq!(
        descriptor.capabilities.delivery_mode,
        AudioDeliveryMode::Buffered
    );

    let mut session = backend
        .create_session(RecognitionContext::normal("__raw__", None))
        .unwrap();
    session.push_audio(&[1, 2, 3]).unwrap();
    assert!(session.poll_events().unwrap().is_empty());
    session.finish().unwrap();
    let events = session.poll_events().unwrap();
    assert_eq!(
        events,
        vec![
            RecognitionEvent::FinalText {
                text: "hello".to_owned()
            },
            RecognitionEvent::Completed
        ]
    );
    assert_eq!(events_to_payload(&events).unwrap().commit_text, "hello");
}

#[test]
fn mock_asr_audio_log_records_raw_audio_pushes() {
    let audio_log = MockAsrAudioLog::new();
    let backend = MockAsrBackend::streaming("partial", "final").with_audio_log(audio_log.clone());
    let mut session = backend
        .create_session(RecognitionContext::normal("default", None))
        .unwrap();

    session.push_audio(&[1, 2]).unwrap();
    session.push_audio(&[3]).unwrap();

    assert_eq!(
        audio_log.records(),
        vec![
            MockAsrAudioPush {
                sample_len: 2,
                pcm_spec: None,
            },
            MockAsrAudioPush {
                sample_len: 1,
                pcm_spec: None,
            },
        ]
    );
}

#[test]
fn mock_asr_audio_push_serializes_stable_fields() {
    let push = MockAsrAudioPush {
        sample_len: 4,
        pcm_spec: Some(PcmSpec {
            sample_rate_hz: 48_000,
            channels: 2,
        }),
    };

    let json = serde_json::to_value(push).unwrap();

    assert_eq!(json["sample_len"], 4);
    assert_eq!(json["pcm_spec"]["sample_rate_hz"], 48_000);
    assert_eq!(json["pcm_spec"]["channels"], 2);
}

#[test]
fn mock_asr_audio_log_records_pcm_push_metadata() {
    let audio_log = MockAsrAudioLog::new();
    let backend = MockAsrBackend::buffered("final").with_audio_log(audio_log.clone());
    let mut session = backend
        .create_session(RecognitionContext::normal("default", None))
        .unwrap();
    let spec = PcmSpec {
        sample_rate_hz: 48_000,
        channels: 2,
    };
    let pcm = PcmBuffer::with_spec(spec, vec![1, 2, 3, 4]).unwrap();

    session.push_pcm(&pcm).unwrap();

    assert_eq!(
        audio_log.records(),
        vec![MockAsrAudioPush {
            sample_len: 4,
            pcm_spec: Some(spec),
        }]
    );
}

#[test]
fn mock_streaming_backend_emits_partial_once() {
    let backend = MockAsrBackend::streaming("partial", "final");
    let mut session = backend
        .create_session(RecognitionContext::normal("__raw__", None))
        .unwrap();
    session.push_audio(&[1]).unwrap();
    assert_eq!(
        session.poll_events().unwrap(),
        vec![RecognitionEvent::PartialText {
            text: "partial".to_owned()
        }]
    );
    session.push_audio(&[2]).unwrap();
    assert!(session.poll_events().unwrap().is_empty());
}

#[test]
fn mock_streaming_backend_can_emit_final_before_finish() {
    let backend = MockAsrBackend::streaming_with_early_final("partial", "final");
    let mut session = backend
        .create_session(RecognitionContext::normal("__raw__", None))
        .unwrap();

    session.push_audio(&[1]).unwrap();
    let events = session.poll_events().unwrap();
    assert_eq!(
        events,
        vec![
            RecognitionEvent::PartialText {
                text: "partial".to_owned()
            },
            RecognitionEvent::FinalText {
                text: "final".to_owned()
            }
        ]
    );
    assert_eq!(events_to_payload(&events).unwrap().commit_text, "final");

    session.finish().unwrap();
    assert_eq!(
        session.poll_events().unwrap(),
        vec![RecognitionEvent::Completed]
    );
}

#[test]
fn error_event_maps_to_payload() {
    let payload = events_to_payload(&[RecognitionEvent::Error {
        message: "err".to_owned(),
    }])
    .unwrap();
    assert_eq!(payload.commit_text, "err");
}

#[test]
fn events_without_final_text_return_error() {
    let error = events_to_payload(&[RecognitionEvent::Completed]).unwrap_err();
    assert!(matches!(error, AsrError::Backend(message) if message.contains("without final text")));
}

#[test]
fn session_rejects_work_after_cancel() {
    let backend = MockAsrBackend::buffered("done");
    let mut session = backend
        .create_session(RecognitionContext::normal("__raw__", None))
        .unwrap();
    session.push_audio(&[1, 2]).unwrap();
    session.cancel().unwrap();

    assert!(session.poll_events().unwrap().is_empty());
    assert!(matches!(
        session.push_audio(&[3]).unwrap_err(),
        AsrError::Cancelled
    ));
    assert!(matches!(session.finish().unwrap_err(), AsrError::Cancelled));
}

#[test]
fn session_rejects_audio_after_finish() {
    let backend = MockAsrBackend::buffered("done");
    let mut session = backend
        .create_session(RecognitionContext::normal("__raw__", None))
        .unwrap();
    session.finish().unwrap();
    assert!(matches!(
        session.push_audio(&[1]).unwrap_err(),
        AsrError::AlreadyFinished
    ));
}

#[test]
fn backend_factory_builds_mock_provider() {
    let config = AsrConfig {
        active_provider: "mock".to_owned(),
        providers: vec![AsrProviderConfig {
            id: "mock".to_owned(),
            kind: AsrProviderKind::Local,
            timeout_ms: None,
            model: None,
            hotwords_file: None,
            command: None,
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            endpoint: None,
        }],
        ..AsrConfig::default()
    };

    let backend = AsrBackendFactory::build_active(&config).unwrap();
    assert_eq!(backend.describe().provider_id, "mock");
}

#[test]
fn backend_factory_reports_unknown_active_provider() {
    let config = AsrConfig {
        active_provider: "missing".to_owned(),
        providers: Vec::new(),
        ..AsrConfig::default()
    };

    let Err(error) = AsrBackendFactory::build_active(&config) else {
        panic!("missing provider should fail");
    };
    assert!(matches!(error, AsrError::UnknownProvider(id) if id == "missing"));
}

#[test]
fn backend_factory_reports_unselected_active_provider() {
    let config = AsrConfig {
        active_provider: String::new(),
        providers: Vec::new(),
        ..AsrConfig::default()
    };

    let Err(error) = AsrBackendFactory::build_active(&config) else {
        panic!("unselected provider should fail");
    };
    assert!(matches!(error, AsrError::NoActiveProvider));

    let state = AsrBackendFactory::state_for_config(&config);
    assert_eq!(state.target_provider_id, "");
    assert!(!state.has_effective_backend);
    assert_eq!(state.last_error, "no active ASR provider is configured");
}

#[test]
fn command_asr_spec_parses_provider_fields() {
    let provider = AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: Some(1_500),
        model: Some("paraformer".to_owned()),
        hotwords_file: Some("/tmp/hotwords.txt".to_owned()),
        command: Some(" helper ".to_owned()),
        args: vec!["--json".to_owned()],
        env: std::collections::HashMap::from([("RUST_LOG".to_owned(), "info".to_owned())]),
        endpoint: None,
    };

    let spec = CommandAsrSpec::try_from(&provider).unwrap();
    assert_eq!(spec.provider_id, "cmd");
    assert_eq!(spec.command, "helper");
    assert_eq!(spec.args, ["--json"]);
    assert_eq!(spec.env.get("RUST_LOG").map(String::as_str), Some("info"));
    assert_eq!(spec.model_id.as_deref(), Some("paraformer"));
    assert_eq!(spec.hotwords_file.as_deref(), Some("/tmp/hotwords.txt"));
    assert_eq!(spec.timeout_ms, Some(1_500));
}
#[test]
fn command_asr_request_serializes_metadata_context_and_audio() {
    let spec = CommandAsrSpec {
        provider_id: "cmd".to_owned(),
        command: "helper".to_owned(),
        args: vec!["--json".to_owned()],
        env: std::collections::HashMap::default(),
        model_id: Some("paraformer".to_owned()),
        hotwords_file: Some("/tmp/hotwords.txt".to_owned()),
        timeout_ms: Some(1_500),
    };
    let request = CommandAsrRequest::from_spec(
        &spec,
        RecognitionContext::command("__command__", Some("zh".to_owned()), "selected"),
        vec![1, -2, 3],
    );
    let value = serde_json::to_value(&request).unwrap();

    assert_eq!(value["provider_id"], "cmd");
    assert_eq!(value["model_id"], "paraformer");
    assert_eq!(value["hotwords_file"], "/tmp/hotwords.txt");
    assert_eq!(value["timeout_ms"], 1_500);
    assert_eq!(value["context"]["scene_id"], "__command__");
    assert_eq!(value["context"]["command_mode"], true);
    assert_eq!(value["context"]["selected_text"], "selected");
    assert_eq!(value["pcm"]["sample_rate_hz"], 16_000);
    assert_eq!(value["pcm"]["channels"], 1);
    assert_eq!(value["samples"], serde_json::json!([1, -2, 3]));
    assert_eq!(
        serde_json::from_value::<CommandAsrRequest>(value).unwrap(),
        request
    );
}

#[test]
fn command_asr_request_defaults_pcm_for_legacy_json() {
    let request: CommandAsrRequest = serde_json::from_str(
        r#"{
                "provider_id":"cmd",
                "context":{
                    "language":"zh",
                    "scene_id":"raw",
                    "command_mode":false
                },
                "samples":[1,2,3]
            }"#,
    )
    .unwrap();

    assert_eq!(request.pcm, PcmSpec::default());
    assert_eq!(request.samples, [1, 2, 3]);
}

#[test]
fn command_asr_request_preserves_explicit_pcm_spec() {
    let spec = CommandAsrSpec {
        provider_id: "cmd".to_owned(),
        command: "helper".to_owned(),
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        model_id: None,
        hotwords_file: None,
        timeout_ms: None,
    };
    let pcm = PcmSpec {
        sample_rate_hz: 48_000,
        channels: 2,
    };
    let request = CommandAsrRequest::from_spec_with_pcm(
        &spec,
        RecognitionContext::normal("raw", None),
        pcm,
        vec![1, 2, 3, 4],
    );

    assert_eq!(request.pcm, pcm);
    assert_eq!(request.samples, [1, 2, 3, 4]);
}

#[test]
fn command_asr_session_uses_pushed_pcm_metadata() {
    let backend = CommandAsrBackend::with_runner(
        CommandAsrSpec {
            provider_id: "cmd".to_owned(),
            command: "helper".to_owned(),
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            model_id: None,
            hotwords_file: None,
            timeout_ms: None,
        },
        PcmEchoCommandRunner,
    );
    let mut session = backend
        .create_session(RecognitionContext::normal("raw", None))
        .expect("command backend should create a buffering session");
    let pcm = PcmBuffer::with_spec(
        PcmSpec {
            sample_rate_hz: 48_000,
            channels: 2,
        },
        vec![1, 2, 3, 4],
    )
    .unwrap();

    session.push_pcm(&pcm).unwrap();
    session.finish().unwrap();

    let payload = events_to_payload(&session.poll_events().unwrap()).unwrap();
    assert_eq!(payload.commit_text, "48000|2|4");
}

#[test]
fn command_asr_session_rejects_mixed_pcm_metadata() {
    let backend = CommandAsrBackend::with_runner(
        CommandAsrSpec {
            provider_id: "cmd".to_owned(),
            command: "helper".to_owned(),
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            model_id: None,
            hotwords_file: None,
            timeout_ms: None,
        },
        PcmEchoCommandRunner,
    );
    let mut session = backend
        .create_session(RecognitionContext::normal("raw", None))
        .expect("command backend should create a buffering session");
    let first = PcmBuffer::with_spec(
        PcmSpec {
            sample_rate_hz: 48_000,
            channels: 2,
        },
        vec![1, 2],
    )
    .unwrap();
    let second = PcmBuffer::with_spec(
        PcmSpec {
            sample_rate_hz: 16_000,
            channels: 1,
        },
        vec![3],
    )
    .unwrap();

    session.push_pcm(&first).unwrap();
    let error = session.push_pcm(&second).unwrap_err();
    assert!(matches!(
        error,
        AsrError::Backend(message)
            if message.contains("PCM spec changed")
                && message.contains("48000 Hz/2")
                && message.contains("16000 Hz/1")
    ));
}

#[test]
fn backend_factory_command_spec_uses_same_parser() {
    let provider = AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: Some(1_500),
        model: Some("paraformer".to_owned()),
        hotwords_file: Some("/tmp/hotwords.txt".to_owned()),
        command: Some("helper".to_owned()),
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };

    let spec = AsrBackendFactory::command_spec(&provider).unwrap();
    assert_eq!(spec.provider_id, "cmd");
    assert_eq!(spec.command, "helper");
    assert_eq!(spec.model_id.as_deref(), Some("paraformer"));
    assert_eq!(spec.hotwords_file.as_deref(), Some("/tmp/hotwords.txt"));
    assert_eq!(spec.timeout_ms, Some(1_500));
}

#[test]
fn command_asr_spec_rejects_missing_command() {
    let provider = AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: None,
        model: None,
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };

    let error = CommandAsrSpec::try_from(&provider).unwrap_err();
    assert!(
        matches!(error, AsrError::Backend(message) if message.contains("must configure a command"))
    );
}

#[test]
fn command_asr_backend_describes_configured_provider() {
    let backend = CommandAsrBackend::new(CommandAsrSpec {
        provider_id: "cmd".to_owned(),
        command: "helper".to_owned(),
        args: vec!["--json".to_owned()],
        env: std::collections::HashMap::default(),
        model_id: Some("cmd-model".to_owned()),
        hotwords_file: None,
        timeout_ms: Some(1_000),
    });

    let descriptor = backend.describe();
    assert_eq!(descriptor.provider_id, "cmd");
    assert_eq!(descriptor.model_id, "cmd-model");
    assert_eq!(
        descriptor.capabilities.delivery_mode,
        AudioDeliveryMode::Buffered
    );
    assert_eq!(backend.spec().command, "helper");
}

#[test]
fn backend_factory_marks_streaming_command_provider_capabilities() {
    let provider = AsrProviderConfig {
        id: "cmd.streaming".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: Some(1_000),
        model: Some("cmd-model".to_owned()),
        hotwords_file: None,
        command: Some("helper".to_owned()),
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };

    let descriptor = AsrBackendFactory::build_provider(&provider)
        .expect("streaming command provider should build")
        .describe();

    assert_eq!(descriptor.provider_id, "cmd.streaming");
    assert_eq!(descriptor.model_id, "cmd-model");
    assert_eq!(
        descriptor.capabilities.delivery_mode,
        AudioDeliveryMode::Chunked
    );
    assert!(descriptor.capabilities.partial_results);
}

#[test]
fn command_asr_backend_delegates_to_injected_runner() {
    let backend = CommandAsrBackend::with_runner(
        CommandAsrSpec {
            provider_id: "cmd".to_owned(),
            command: "helper".to_owned(),
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            model_id: None,
            hotwords_file: None,
            timeout_ms: None,
        },
        FinalTextCommandRunner,
    );

    let mut session = backend
        .create_session(RecognitionContext::normal("raw", None))
        .expect("mock runner should create a session");
    session.push_audio(&[1, 2, 3]).unwrap();
    session.finish().unwrap();
    let payload = events_to_payload(&session.poll_events().unwrap()).unwrap();
    assert_eq!(payload.commit_text, "helper:raw:3");
}

#[test]
fn command_asr_backend_passes_config_to_injected_runner() {
    let backend = CommandAsrBackend::with_runner(
        CommandAsrSpec {
            provider_id: "cmd".to_owned(),
            command: "helper".to_owned(),
            args: vec!["--format".to_owned(), "json".to_owned()],
            env: std::collections::HashMap::from([("ASR_MODE".to_owned(), "fast".to_owned())]),
            model_id: Some("paraformer".to_owned()),
            hotwords_file: Some("/tmp/hotwords.txt".to_owned()),
            timeout_ms: Some(2_500),
        },
        ConfigEchoCommandRunner,
    );

    let mut session = backend
        .create_session(RecognitionContext::normal(
            "dictation",
            Some("zh".to_owned()),
        ))
        .expect("mock runner should create a session");
    session.finish().unwrap();

    let payload = events_to_payload(&session.poll_events().unwrap()).unwrap();
    assert_eq!(
        payload.commit_text,
        "cmd|helper|--format,json|fast|paraformer|/tmp/hotwords.txt|2500|dictation|zh|0"
    );
}

#[test]
fn command_asr_backend_builds_from_provider_config_with_runner() {
    let provider = AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: Some(2_500),
        model: Some("paraformer".to_owned()),
        hotwords_file: Some("/tmp/hotwords.txt".to_owned()),
        command: Some("helper".to_owned()),
        args: vec!["--format".to_owned(), "json".to_owned()],
        env: std::collections::HashMap::from([("ASR_MODE".to_owned(), "fast".to_owned())]),
        endpoint: None,
    };

    let backend = CommandAsrBackend::with_config(&provider, ConfigEchoCommandRunner)
        .expect("command provider config should build");
    assert_eq!(backend.spec().provider_id, "cmd");
    assert_eq!(backend.spec().model_id.as_deref(), Some("paraformer"));
    assert_eq!(
        backend.spec().hotwords_file.as_deref(),
        Some("/tmp/hotwords.txt")
    );

    let mut session = backend
        .create_session(RecognitionContext::normal(
            "dictation",
            Some("zh".to_owned()),
        ))
        .expect("mock runner should create a session");
    session.finish().unwrap();

    let payload = events_to_payload(&session.poll_events().unwrap()).unwrap();
    assert_eq!(
        payload.commit_text,
        "cmd|helper|--format,json|fast|paraformer|/tmp/hotwords.txt|2500|dictation|zh|0"
    );
}

#[test]
fn backend_factory_uses_legacy_streaming_protocol_for_streaming_command_provider() {
    let script_path = write_temp_script(
        "vinput-factory-legacy-streaming-asr",
        r"
import json
import sys
lines = [json.loads(line) for line in sys.stdin if line.strip()]
assert lines[0]['type'] == 'audio'
assert lines[1]['type'] == 'finish'
print(json.dumps({'type':'partial','text':'factory partial'}))
print(json.dumps({'type':'final','text':'factory final'}))
print(json.dumps({'type':'closed'}))
",
    );
    let provider = AsrProviderConfig {
        id: "cmd.streaming".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: Some(1_000),
        model: None,
        hotwords_file: None,
        command: Some("python3".to_owned()),
        args: vec![script_path.to_string_lossy().into_owned()],
        env: std::collections::HashMap::default(),
        endpoint: None,
    };

    let backend = AsrBackendFactory::build_provider(&provider).unwrap();
    let mut session = backend
        .create_session(RecognitionContext::normal("raw", None))
        .expect("legacy streaming command backend should create a session");
    session.push_audio(&[1, -2, 258]).unwrap();
    session.finish().unwrap();
    std::fs::remove_file(script_path).unwrap();

    assert_eq!(
        session.poll_events().unwrap(),
        vec![
            RecognitionEvent::PartialText {
                text: "factory partial".to_owned()
            },
            RecognitionEvent::FinalText {
                text: "factory final".to_owned()
            },
            RecognitionEvent::Completed,
        ]
    );
}

#[test]
fn backend_factory_uses_legacy_batch_protocol_for_command_provider() {
    let provider = AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: Some(1_000),
        model: None,
        hotwords_file: None,
        command: Some("sh".to_owned()),
        args: vec!["-c".to_owned(), "cat >/dev/null; printf final".to_owned()],
        env: std::collections::HashMap::default(),
        endpoint: None,
    };

    let backend = AsrBackendFactory::build_provider(&provider).unwrap();
    let mut session = backend
        .create_session(RecognitionContext::normal("raw", None))
        .expect("legacy command backend should create a session");
    session.push_audio(&[1, -2, 258]).unwrap();
    session.finish().unwrap();

    assert_eq!(
        session.poll_events().unwrap(),
        vec![
            RecognitionEvent::FinalText {
                text: "final".to_owned()
            },
            RecognitionEvent::Completed,
        ]
    );
}

#[test]
fn command_asr_backend_runner_is_not_implemented_yet() {
    let backend = CommandAsrBackend::new(CommandAsrSpec {
        provider_id: "cmd".to_owned(),
        command: "helper".to_owned(),
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        model_id: None,
        hotwords_file: None,
        timeout_ms: None,
    });

    let mut session = backend
        .create_session(RecognitionContext::normal("raw", None))
        .expect("command backend should create a buffering session");
    session.push_audio(&[1, 2, 3]).unwrap();
    let error = session.finish().unwrap_err();
    assert!(matches!(
        error,
        AsrError::Backend(message) if message.contains("runner is not implemented yet")
    ));
}

#[test]
fn command_asr_backend_with_config_describes_provider() {
    let provider = AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: Some(1_000),
        model: Some("cmd-model".to_owned()),
        hotwords_file: None,
        command: Some("helper".to_owned()),
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };

    let backend = CommandAsrBackend::with_config(&provider, ProcessCommandAsrRunner).unwrap();
    let descriptor = backend.describe();
    assert_eq!(descriptor.provider_id, "cmd");
    assert_eq!(descriptor.model_id, "cmd-model");
}

#[test]
fn sherpa_onnx_spec_preserves_local_provider_config() {
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: Some(12_000),
        model: Some("paraformer".to_owned()),
        hotwords_file: Some("hotwords.txt".to_owned()),
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };

    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    assert_eq!(spec.provider_id, "sherpa-onnx");
    assert_eq!(spec.model.as_deref(), Some("paraformer"));
    assert_eq!(spec.hotwords_file.as_deref(), Some("hotwords.txt"));
    assert_eq!(spec.timeout_ms, Some(12_000));
}

#[test]
fn sherpa_onnx_model_paths_resolve_relative_model_and_hotwords() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let model_dir = root.join("paraformer");
    std::fs::create_dir_all(&model_dir).unwrap();
    std::fs::write(model_dir.join("hotwords.txt"), b"hello 1.0\n").unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("paraformer".to_owned()),
        hotwords_file: Some("hotwords.txt".to_owned()),
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let paths = spec.resolve_model_paths(root).unwrap();

    assert_eq!(paths.model_dir, model_dir);
    assert_eq!(
        paths.hotwords_file,
        Some(paths.model_dir.join("hotwords.txt"))
    );
}

#[test]
fn sherpa_onnx_model_paths_accept_absolute_model_path() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let model_dir = root.join("absolute-model");
    std::fs::create_dir_all(&model_dir).unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some(model_dir.display().to_string()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let paths = spec.resolve_model_paths("ignored-root").unwrap();

    assert_eq!(paths.model_dir, model_dir);
    assert_eq!(paths.hotwords_file, None);
}

#[test]
fn sherpa_onnx_model_paths_serialize_resolved_paths() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let model_dir = root.join("paraformer");
    std::fs::create_dir_all(&model_dir).unwrap();
    std::fs::write(model_dir.join("hotwords.txt"), b"hello 1.0\n").unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("paraformer".to_owned()),
        hotwords_file: Some("hotwords.txt".to_owned()),
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();
    let paths = spec.resolve_model_paths(root).unwrap();

    let json = serde_json::to_value(&paths).unwrap();

    assert_eq!(json["model_dir"], model_dir.display().to_string());
    assert_eq!(
        json["hotwords_file"],
        model_dir.join("hotwords.txt").display().to_string()
    );
}

#[test]
fn sherpa_onnx_model_paths_reject_missing_model_config() {
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: None,
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let error = spec.resolve_model_paths("model-root").unwrap_err();

    assert_eq!(
        error,
        SherpaOnnxModelPathError::MissingModel {
            provider_id: "sherpa-onnx".to_owned(),
        }
    );
}

#[test]
fn sherpa_onnx_model_paths_reject_url_like_model_path() {
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("https://example.invalid/model".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let error = spec.resolve_model_paths("model-root").unwrap_err();

    assert_eq!(
        error,
        SherpaOnnxModelPathError::UrlLikePath {
            provider_id: "sherpa-onnx".to_owned(),
            path: "https://example.invalid/model".to_owned(),
        }
    );
}

#[test]
fn sherpa_onnx_model_paths_reject_file_model_path() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    std::fs::write(root.join("not-dir"), b"model").unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("not-dir".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let error = spec.resolve_model_paths(root).unwrap_err();

    assert!(matches!(
        error,
        SherpaOnnxModelPathError::ModelPathNotDirectory { .. }
    ));
}

#[test]
fn sherpa_onnx_model_paths_reject_missing_hotwords_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let model_dir = root.join("paraformer");
    std::fs::create_dir_all(&model_dir).unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("paraformer".to_owned()),
        hotwords_file: Some("missing.txt".to_owned()),
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let error = spec.resolve_model_paths(root).unwrap_err();

    assert!(matches!(
        error,
        SherpaOnnxModelPathError::MissingHotwordsFile { .. }
    ));
}

#[test]
fn sherpa_onnx_model_paths_accept_absolute_hotwords_path() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let model_dir = root.join("paraformer");
    let hotwords_file = root.join("shared-hotwords.txt");
    std::fs::create_dir_all(&model_dir).unwrap();
    std::fs::write(
        &hotwords_file,
        b"hello 1.0
",
    )
    .unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("paraformer".to_owned()),
        hotwords_file: Some(hotwords_file.display().to_string()),
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let paths = spec.resolve_model_paths(root).unwrap();

    assert_eq!(paths.model_dir, model_dir);
    assert_eq!(paths.hotwords_file, Some(hotwords_file));
}

#[test]
fn sherpa_onnx_model_paths_reject_empty_model_path() {
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("   ".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let error = spec.resolve_model_paths("model-root").unwrap_err();

    assert_eq!(
        error,
        SherpaOnnxModelPathError::EmptyModel {
            provider_id: "sherpa-onnx".to_owned(),
        }
    );
}

#[test]
fn sherpa_onnx_model_paths_reject_missing_model_directory() {
    let temp_dir = tempfile::tempdir().unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("missing-model".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let error = spec.resolve_model_paths(temp_dir.path()).unwrap_err();

    assert!(matches!(
        error,
        SherpaOnnxModelPathError::MissingModelDir { .. }
    ));
}

#[test]
fn sherpa_onnx_model_paths_reject_empty_hotwords_path() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    std::fs::create_dir_all(root.join("paraformer")).unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("paraformer".to_owned()),
        hotwords_file: Some("   ".to_owned()),
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let error = spec.resolve_model_paths(root).unwrap_err();

    assert_eq!(
        error,
        SherpaOnnxModelPathError::EmptyHotwords {
            provider_id: "sherpa-onnx".to_owned(),
        }
    );
}

#[test]
fn sherpa_onnx_model_paths_reject_url_like_hotwords_path() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    std::fs::create_dir_all(root.join("paraformer")).unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("paraformer".to_owned()),
        hotwords_file: Some("https://example.invalid/hotwords.txt".to_owned()),
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let error = spec.resolve_model_paths(root).unwrap_err();

    assert_eq!(
        error,
        SherpaOnnxModelPathError::UrlLikePath {
            provider_id: "sherpa-onnx".to_owned(),
            path: "https://example.invalid/hotwords.txt".to_owned(),
        }
    );
}

#[test]
fn sherpa_onnx_offline_runtime_plan_detects_sense_voice_layout() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let model_dir = root.join("sense-voice");
    std::fs::create_dir_all(&model_dir).unwrap();
    std::fs::write(model_dir.join("model.int8.onnx"), b"onnx").unwrap();
    std::fs::write(model_dir.join("tokens.txt"), b"<blank> 0\n").unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("sense-voice".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let plan = spec.resolve_offline_runtime_plan(root).unwrap();

    assert_eq!(plan.paths.model_dir, model_dir);
    assert_eq!(plan.layout_source, "files");
    assert_eq!(plan.metadata_path, None);
    assert_eq!(
        plan.layout,
        SherpaOnnxOfflineModelLayout::SenseVoice {
            model: plan.paths.model_dir.join("model.int8.onnx"),
            tokens: plan.paths.model_dir.join("tokens.txt"),
            language: "auto".to_owned(),
            use_itn: true,
        }
    );
}

#[test]
fn sherpa_onnx_offline_runtime_plan_uses_vinput_model_metadata() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let model_dir = root.join("sense-voice-meta");
    std::fs::create_dir_all(&model_dir).unwrap();
    std::fs::write(model_dir.join("custom.onnx"), b"onnx").unwrap();
    std::fs::write(
        model_dir.join("custom-tokens.txt"),
        b"<blank> 0
",
    )
    .unwrap();
    std::fs::write(
        model_dir.join("vinput-model.json"),
        r#"{
            "backend": "sherpa-offline",
            "family": "sense_voice",
            "language": "zh",
            "runtime": "offline",
            "model": {
                "tokens": "custom-tokens.txt",
                "sense_voice": {
                    "model": "custom.onnx",
                    "language": "zh",
                    "use_itn": false
                }
            }
        }"#,
    )
    .unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("sense-voice-meta".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let plan = spec.resolve_offline_runtime_plan(root).unwrap();

    assert_eq!(plan.layout_source, "metadata");
    assert_eq!(
        plan.metadata_path,
        Some(model_dir.join("vinput-model.json"))
    );
    assert_eq!(
        plan.layout,
        SherpaOnnxOfflineModelLayout::SenseVoice {
            model: model_dir.join("custom.onnx"),
            tokens: model_dir.join("custom-tokens.txt"),
            language: "zh".to_owned(),
            use_itn: false,
        }
    );
}

#[test]
fn sherpa_onnx_offline_runtime_plan_uses_transducer_metadata() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let model_dir = root.join("transducer");
    std::fs::create_dir_all(&model_dir).unwrap();
    for file_name in ["encoder.onnx", "decoder.onnx", "joiner.onnx", "tokens.txt"] {
        std::fs::write(model_dir.join(file_name), b"fixture").unwrap();
    }
    std::fs::write(
        model_dir.join("vinput-model.json"),
        r#"{
          "backend":"sherpa-offline",
          "family":"transducer",
          "runtime":"offline",
          "supports_hotwords":true,
          "model": {
            "tokens":"tokens.txt",
            "num_threads":2,
            "provider":"cpu",
            "transducer": {
              "encoder":"encoder.onnx",
              "decoder":"decoder.onnx",
              "joiner":"joiner.onnx"
            }
          },
          "recognizer": {
            "feat_config":{"sample_rate":16000,"feature_dim":80},
            "decoding_method":"modified_beam_search",
            "max_active_paths":4,
            "hotwords_score":1.5
          }
        }"#,
    )
    .unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("transducer".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let plan = spec.resolve_offline_runtime_plan(root).unwrap();

    assert_eq!(plan.layout_source, "metadata");
    assert_eq!(
        plan.layout,
        SherpaOnnxOfflineModelLayout::Transducer {
            encoder: model_dir.join("encoder.onnx"),
            decoder: model_dir.join("decoder.onnx"),
            joiner: model_dir.join("joiner.onnx"),
            tokens: model_dir.join("tokens.txt"),
        }
    );
    assert_eq!(plan.settings.num_threads, 2);
    assert_eq!(plan.settings.model_type.as_deref(), Some("transducer"));
    assert_eq!(plan.settings.decoding_method, "modified_beam_search");
    assert_eq!(plan.settings.max_active_paths, 4);
}

#[test]
fn sherpa_onnx_offline_runtime_plan_uses_dolphin_metadata() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let model_dir = root.join("dolphin");
    std::fs::create_dir_all(&model_dir).unwrap();
    std::fs::write(model_dir.join("model.int8.onnx"), b"onnx").unwrap();
    std::fs::write(
        model_dir.join("tokens.txt"),
        b"<blank> 0
",
    )
    .unwrap();
    std::fs::write(
        model_dir.join("vinput-model.json"),
        r#"{
          "backend":"sherpa-offline",
          "family":"dolphin",
          "runtime":"offline",
          "model": {
            "tokens":"tokens.txt",
            "num_threads":2,
            "provider":"cpu",
            "dolphin":{"model":"model.int8.onnx"}
          },
          "recognizer": {
            "feat_config":{"sample_rate":16000,"feature_dim":80},
            "decoding_method":"greedy_search",
            "max_active_paths":4
          }
        }"#,
    )
    .unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("dolphin".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let plan = spec.resolve_offline_runtime_plan(root).unwrap();

    assert_eq!(plan.layout_source, "metadata");
    assert_eq!(
        plan.metadata_path,
        Some(model_dir.join("vinput-model.json"))
    );
    assert_eq!(
        plan.layout,
        SherpaOnnxOfflineModelLayout::Dolphin {
            model: model_dir.join("model.int8.onnx"),
            tokens: model_dir.join("tokens.txt"),
        }
    );
    assert_eq!(plan.settings.num_threads, 2);
    assert_eq!(plan.settings.model_type.as_deref(), Some("dolphin"));
    assert_eq!(plan.settings.modeling_unit.as_deref(), Some("cjkchar"));
    assert_eq!(plan.settings.sample_rate, 16_000);
    assert_eq!(plan.settings.feature_dim, 80);
}

#[test]
fn sherpa_onnx_offline_runtime_plan_uses_paraformer_metadata() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let model_dir = root.join("paraformer");
    std::fs::create_dir_all(&model_dir).unwrap();
    for file_name in [
        "model.int8.onnx",
        "tokens.txt",
        "bpe.vocab",
        "lm.onnx",
        "hotwords.txt",
        "rules.fst",
        "rules.far",
        "lexicon.txt",
        "homophone.fst",
    ] {
        std::fs::write(model_dir.join(file_name), b"fixture").unwrap();
    }
    std::fs::write(
        model_dir.join("vinput-model.json"),
        r#"{
          "backend":"sherpa-offline",
          "family":"paraformer",
          "runtime":"offline",
          "model": {
            "tokens":"tokens.txt",
            "num_threads":3,
            "debug":1,
            "provider":"cuda",
            "model_type":"custom_paraformer",
            "modeling_unit":"bpe",
            "bpe_vocab":"bpe.vocab",
            "paraformer":{"model":"model.int8.onnx"}
          },
          "recognizer": {
            "feat_config":{"sample_rate":8000,"feature_dim":64},
            "lm_config":{"model":"lm.onnx","scale":0.25},
            "decoding_method":"modified_beam_search",
            "max_active_paths":8,
            "hotwords_file":"hotwords.txt",
            "hotwords_score":2.5,
            "rule_fsts":"rules.fst",
            "rule_fars":"rules.far",
            "blank_penalty":0.75,
            "hr":{"lexicon":"lexicon.txt","rule_fsts":"homophone.fst"}
          }
        }"#,
    )
    .unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("paraformer".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let plan = spec.resolve_offline_runtime_plan(root).unwrap();

    assert_paraformer_runtime_plan(&plan, &model_dir);
}

fn assert_paraformer_runtime_plan(
    plan: &SherpaOnnxOfflineRuntimePlan,
    model_dir: &std::path::Path,
) {
    assert_eq!(plan.layout_source, "metadata");
    assert_eq!(
        plan.metadata_path,
        Some(model_dir.join("vinput-model.json"))
    );
    assert_eq!(
        plan.layout,
        SherpaOnnxOfflineModelLayout::Paraformer {
            model: model_dir.join("model.int8.onnx"),
            tokens: model_dir.join("tokens.txt"),
        }
    );
    assert_eq!(plan.settings.num_threads, 3);
    assert_eq!(plan.settings.provider, "cuda");
    assert!(plan.settings.debug);
    assert_eq!(
        plan.settings.model_type.as_deref(),
        Some("custom_paraformer")
    );
    assert_eq!(plan.settings.modeling_unit.as_deref(), Some("bpe"));
    assert_eq!(plan.settings.bpe_vocab, Some(model_dir.join("bpe.vocab")));
    assert_eq!(plan.settings.sample_rate, 8000);
    assert_eq!(plan.settings.feature_dim, 64);
    assert_eq!(plan.settings.lm_model, Some(model_dir.join("lm.onnx")));
    assert!((plan.settings.lm_scale - 0.25).abs() < f32::EPSILON);
    assert_eq!(plan.settings.decoding_method, "modified_beam_search");
    assert_eq!(plan.settings.max_active_paths, 8);
    assert_eq!(
        plan.settings.hotwords_file,
        Some(model_dir.join("hotwords.txt"))
    );
    assert!((plan.settings.hotwords_score - 2.5).abs() < f32::EPSILON);
    assert_eq!(plan.settings.rule_fsts, Some(model_dir.join("rules.fst")));
    assert_eq!(plan.settings.rule_fars, Some(model_dir.join("rules.far")));
    assert!((plan.settings.blank_penalty - 0.75).abs() < f32::EPSILON);
    assert_eq!(
        plan.settings.homophone_lexicon,
        Some(model_dir.join("lexicon.txt"))
    );
    assert_eq!(
        plan.settings.homophone_rule_fsts,
        Some(model_dir.join("homophone.fst"))
    );
}

#[test]
fn sherpa_onnx_offline_runtime_plan_uses_qwen3_asr_metadata() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let model_dir = root.join("qwen3-asr");
    std::fs::create_dir_all(model_dir.join("tokenizer")).unwrap();
    for file_name in [
        "conv_frontend.onnx",
        "encoder.int8.onnx",
        "decoder.int8.onnx",
    ] {
        std::fs::write(model_dir.join(file_name), b"onnx").unwrap();
    }
    std::fs::write(
        model_dir.join("vinput-model.json"),
        r#"{
            "backend": "sherpa-offline",
            "family": "qwen3_asr",
            "runtime": "offline",
            "model": {
                "qwen3_asr": {
                    "conv_frontend": "conv_frontend.onnx",
                    "encoder": "encoder.int8.onnx",
                    "decoder": "decoder.int8.onnx",
                    "tokenizer": "tokenizer",
                    "max_total_len": 4096,
                    "max_new_tokens": 1024,
                    "temperature": 1.0,
                    "top_p": 0.9,
                    "seed": 0
                }
            }
        }"#,
    )
    .unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("qwen3-asr".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let plan = spec.resolve_offline_runtime_plan(root).unwrap();

    assert_eq!(plan.layout_source, "metadata");
    assert_eq!(
        plan.metadata_path,
        Some(model_dir.join("vinput-model.json"))
    );
    assert_eq!(
        plan.layout,
        SherpaOnnxOfflineModelLayout::Qwen3Asr {
            conv_frontend: model_dir.join("conv_frontend.onnx"),
            encoder: model_dir.join("encoder.int8.onnx"),
            decoder: model_dir.join("decoder.int8.onnx"),
            tokenizer: model_dir.join("tokenizer"),
            max_total_len: 4096,
            max_new_tokens: 1024,
            temperature: 1.0,
            top_p: 0.9,
            seed: 0,
            hotwords: None,
        }
    );
}

#[test]
fn sherpa_onnx_offline_runtime_plan_uses_moonshine_metadata() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let model_dir = root.join("moonshine");
    std::fs::create_dir_all(&model_dir).unwrap();
    for file_name in [
        "preprocess.onnx",
        "encode.int8.onnx",
        "uncached_decode.int8.onnx",
        "cached_decode.int8.onnx",
        "tokens.txt",
    ] {
        std::fs::write(model_dir.join(file_name), b"fixture").unwrap();
    }
    std::fs::write(
        model_dir.join("vinput-model.json"),
        r#"{
          "backend":"sherpa-offline",
          "family":"moonshine",
          "model_type":"moonshine_v1",
          "runtime":"offline",
          "model": {
            "moonshine": {
              "preprocessor":"preprocess.onnx",
              "encoder":"encode.int8.onnx",
              "uncached_decoder":"uncached_decode.int8.onnx",
              "cached_decoder":"cached_decode.int8.onnx"
            },
            "tokens":"tokens.txt"
          }
        }"#,
    )
    .unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("moonshine".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let plan = spec.resolve_offline_runtime_plan(root).unwrap();

    assert_eq!(
        plan.layout,
        SherpaOnnxOfflineModelLayout::MoonshineV1 {
            preprocessor: model_dir.join("preprocess.onnx"),
            encoder: model_dir.join("encode.int8.onnx"),
            uncached_decoder: model_dir.join("uncached_decode.int8.onnx"),
            cached_decoder: model_dir.join("cached_decode.int8.onnx"),
            tokens: model_dir.join("tokens.txt"),
        }
    );
    assert_eq!(plan.layout_source, "metadata");
}

#[test]
fn sherpa_onnx_offline_runtime_plan_uses_model_type_metadata_fallback() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let model_dir = root.join("sense-voice-legacy-meta");
    std::fs::create_dir_all(&model_dir).unwrap();
    std::fs::write(model_dir.join("model.int8.onnx"), b"onnx").unwrap();
    std::fs::write(
        model_dir.join("tokens.txt"),
        b"<blank> 0
",
    )
    .unwrap();
    std::fs::write(
        model_dir.join("vinput-model.json"),
        r#"{
            "model_type": "sense_voice",
            "language": "ja",
            "model": {
                "sense_voice": {
                    "use_itn": false
                }
            }
        }"#,
    )
    .unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("sense-voice-legacy-meta".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let plan = spec.resolve_offline_runtime_plan(root).unwrap();

    assert_eq!(plan.layout_source, "metadata");
    assert_eq!(
        plan.metadata_path,
        Some(model_dir.join("vinput-model.json"))
    );
    assert_eq!(
        plan.layout,
        SherpaOnnxOfflineModelLayout::SenseVoice {
            model: model_dir.join("model.int8.onnx"),
            tokens: model_dir.join("tokens.txt"),
            language: "ja".to_owned(),
            use_itn: false,
        }
    );
}

#[test]
fn sherpa_onnx_offline_runtime_plan_accepts_non_int8_sense_voice_model() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let model_dir = root.join("sense-voice");
    std::fs::create_dir_all(&model_dir).unwrap();
    std::fs::write(model_dir.join("model.onnx"), b"onnx").unwrap();
    std::fs::write(model_dir.join("tokens.txt"), b"<blank> 0\n").unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("sense-voice".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let plan = spec.resolve_offline_runtime_plan(root).unwrap();

    assert!(matches!(
        plan.layout,
        SherpaOnnxOfflineModelLayout::SenseVoice { model, .. }
            if model == model_dir.join("model.onnx")
    ));
}

#[test]
fn sherpa_onnx_offline_runtime_plan_rejects_invalid_vinput_model_metadata() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let model_dir = root.join("bad-meta");
    std::fs::create_dir_all(&model_dir).unwrap();
    std::fs::write(model_dir.join("vinput-model.json"), b"not json").unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("bad-meta".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let error = spec.resolve_offline_runtime_plan(root).unwrap_err();

    assert!(matches!(
        error,
        SherpaOnnxModelPathError::InvalidModelMetadata { path, message }
            if path.ends_with("vinput-model.json") && message.contains("expected")
    ));
}

#[test]
fn sherpa_onnx_offline_runtime_plan_prioritizes_unsupported_family_diagnostics() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let model_dir = root.join("future-family");
    std::fs::create_dir_all(&model_dir).unwrap();
    std::fs::write(
        model_dir.join("vinput-model.json"),
        r#"{
          "backend":"sherpa-offline",
          "family":"future_family",
          "runtime":"offline",
          "model":{"bpe_vocab":"missing.vocab"}
        }"#,
    )
    .unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("future-family".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let error = spec.resolve_offline_runtime_plan(root).unwrap_err();

    assert_eq!(
        error,
        SherpaOnnxModelPathError::UnsupportedOfflineFamily {
            path: model_dir.display().to_string(),
            family: "future_family".to_owned(),
        }
    );
}

#[test]
fn sherpa_onnx_offline_runtime_plan_rejects_unknown_layout() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    std::fs::create_dir_all(root.join("empty-model")).unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("empty-model".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let error = spec.resolve_offline_runtime_plan(root).unwrap_err();

    assert!(matches!(
        error,
        SherpaOnnxModelPathError::UnsupportedOfflineLayout { .. }
    ));
}

#[test]
fn sherpa_onnx_offline_runtime_plan_requires_tokens_for_sense_voice() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let model_dir = root.join("sense-voice");
    std::fs::create_dir_all(&model_dir).unwrap();
    std::fs::write(model_dir.join("model.int8.onnx"), b"onnx").unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("sense-voice".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let error = spec.resolve_offline_runtime_plan(root).unwrap_err();

    assert!(matches!(
        error,
        SherpaOnnxModelPathError::MissingTokensFile { .. }
    ));
}

#[test]
fn sherpa_onnx_model_paths_reject_directory_hotwords_path() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let model_dir = root.join("paraformer");
    std::fs::create_dir_all(model_dir.join("hotwords-dir")).unwrap();
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("paraformer".to_owned()),
        hotwords_file: Some("hotwords-dir".to_owned()),
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };
    let spec = SherpaOnnxSpec::from_provider(&provider).unwrap();

    let error = spec.resolve_model_paths(root).unwrap_err();

    assert!(matches!(
        error,
        SherpaOnnxModelPathError::HotwordsPathNotFile { .. }
    ));
}

#[cfg(not(feature = "sherpa-onnx-backend"))]
#[test]
fn backend_factory_reports_sherpa_onnx_runtime_unavailable() {
    let provider = AsrProviderConfig {
        id: "sherpa-onnx".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: None,
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };

    let Err(error) = AsrBackendFactory::build_provider(&provider) else {
        panic!("sherpa-onnx runtime should remain unavailable");
    };
    assert!(matches!(
        error,
        AsrError::Backend(message)
            if message == "sherpa-onnx runtime for provider `sherpa-onnx` is not enabled; build with feature `sherpa-onnx-backend`"
    ));
}

#[test]
fn backend_factory_reports_unimplemented_provider_kind() {
    let provider = AsrProviderConfig {
        id: "local-other".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: None,
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    };

    let Err(error) = AsrBackendFactory::build_provider(&provider) else {
        panic!("unsupported provider should fail");
    };
    assert!(matches!(
        error,
        AsrError::UnsupportedProviderKind { provider_id, kind }
            if provider_id == "local-other" && kind == "local"
    ));
}

#[cfg(not(feature = "sherpa-onnx-backend"))]
#[test]
fn backend_factory_state_reports_unavailable_provider() {
    let config = AsrConfig {
        active_provider: "sherpa-onnx".to_owned(),
        providers: vec![AsrProviderConfig {
            id: "sherpa-onnx".to_owned(),
            kind: AsrProviderKind::Local,
            timeout_ms: None,
            model: Some("paraformer".to_owned()),
            hotwords_file: None,
            command: None,
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            endpoint: None,
        }],
        ..AsrConfig::default()
    };

    let state = AsrBackendFactory::state_for_config(&config);
    assert_eq!(state.target_provider_id, "sherpa-onnx");
    assert_eq!(state.target_model_id, "paraformer");
    assert!(!state.has_effective_backend);
    assert!(state.last_error.contains("sherpa-onnx runtime"));
}

#[test]
fn backend_factory_state_preserves_remote_endpoint() {
    let config = AsrConfig {
        active_provider: "remote".to_owned(),
        providers: vec![AsrProviderConfig {
            id: "remote".to_owned(),
            kind: AsrProviderKind::Remote,
            timeout_ms: None,
            model: Some("cloud-model".to_owned()),
            hotwords_file: None,
            command: None,
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            endpoint: Some("https://asr.example.test".to_owned()),
        }],
        ..AsrConfig::default()
    };

    let state = AsrBackendFactory::state_for_config(&config);
    assert_eq!(state.target_provider_id, "remote");
    assert_eq!(state.target_model_id, "cloud-model");
    assert!(state.has_effective_backend);
    assert_eq!(state.effective_provider_id, "remote");
    assert_eq!(state.effective_model_id, "cloud-model");
    assert!(state.last_error.is_empty());
    assert_eq!(state.remote_endpoints, ["https://asr.example.test"]);
}

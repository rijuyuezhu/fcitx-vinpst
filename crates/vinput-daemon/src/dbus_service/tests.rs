use super::{AsrBackendStateTuple, LivePartialEmissionState, VinputDbusService};
use crate::RuntimeState;
use tokio::time::{Duration, sleep, timeout};
use vinput_asr::MockAsrBackend;
use vinput_config::{AsrProviderConfig, AsrProviderKind, LlmAdapterConfig, VinputConfig};
use vinput_protocol::{RecognitionPayload, TextAdapterState};

fn service() -> VinputDbusService {
    let config = VinputConfig::bundled_default().unwrap();
    VinputDbusService::new(RuntimeState::new(config).unwrap())
}

async fn wait_for_asr_reload(service: &VinputDbusService) -> AsrBackendStateTuple {
    timeout(Duration::from_secs(2), async {
        loop {
            let state = service.get_asr_backend_state().await;
            if !state.5 {
                return state;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("ASR reload should finish")
}

fn unique_adapter_runtime_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "vinput-daemon-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ))
}

fn reserve_remote_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("reserve remote lifecycle port")
        .local_addr()
        .expect("read reserved remote lifecycle address")
        .port()
}

fn remote_lifecycle_config(port: u16) -> VinputConfig {
    VinputConfig::from_json_str(
        &serde_json::to_string(&serde_json::json!({
            "version":1,
            "asr":{
                "active_provider":"provider.vinput.remote.streaming",
                "providers":[{
                    "id":"provider.vinput.remote.streaming",
                    "type":"command",
                    "command":"python3",
                    "args":["remote.py"],
                    "env":{
                        "VINPUT_ASR_API_KEY":"fixture-key",
                        "VINPUT_ASR_PORT":port.to_string(),
                        "VINPUT_ASR_DEBOUNCE_MS":"25"
                    }
                }]
            },
            "scenes":{
                "active_scene":"raw",
                "definitions":[{"id":"raw","label":"Raw","candidate_count":0}]
            }
        }))
        .expect("serialize remote lifecycle config"),
    )
    .expect("parse remote lifecycle config")
}

async fn remote_health_is_ready(port: u16) -> bool {
    reqwest::Client::builder()
        .timeout(Duration::from_millis(150))
        .build()
        .expect("build remote lifecycle health client")
        .get(format!("http://127.0.0.1:{port}/health"))
        .send()
        .await
        .is_ok_and(|response| response.status().is_success())
}

#[tokio::test]
async fn dbus_facade_exercises_normal_mock_flow() {
    let service = service();
    assert_eq!(service.get_status().await, "idle");
    assert_eq!(
        service.start_recording_state().await.unwrap().0,
        "recording"
    );
    let payload =
        RecognitionPayload::from_json_str(&service.stop_recording_payload("").await.unwrap().0)
            .unwrap();
    assert_eq!(payload.commit_text, "mock recognition result");
    assert_eq!(service.get_status().await, "idle");
}

#[tokio::test]
async fn dbus_facade_keeps_remote_asr_and_remote_text_endpoints_separate() {
    let mut config = VinputConfig::bundled_default().unwrap();
    config.asr.active_provider = "remote-asr".to_owned();
    config.asr.providers.push(AsrProviderConfig {
        id: "remote-asr".to_owned(),
        kind: AsrProviderKind::Remote,
        timeout_ms: Some(2_000),
        model: Some("remote-model".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::new(),
        endpoint: Some("https://asr.example.test/v1".to_owned()),
    });
    let service = VinputDbusService::new(
        RuntimeState::with_configured_asr(config).expect("create remote ASR runtime"),
    );

    let state = service.get_asr_backend_state().await;
    assert_eq!(state.0, "remote-asr");
    assert_eq!(state.2, "remote-asr");
    assert_eq!(state.7, ["https://asr.example.test/v1"]);

    let status: serde_json::Value =
        serde_json::from_str(&service.get_runtime_status().await.unwrap()).unwrap();
    assert_eq!(
        status["asr"]["remote_endpoints"],
        serde_json::json!(["https://asr.example.test/v1"])
    );
    assert_eq!(status["remote_text"]["endpoints"], serde_json::json!([]));
}

#[tokio::test]
async fn dbus_facade_reconciles_remote_service_on_config_reload() {
    let first_port = reserve_remote_port();
    let mut second_port = reserve_remote_port();
    while second_port == first_port {
        second_port = reserve_remote_port();
    }
    let root = unique_adapter_runtime_dir("remote-reload");
    std::fs::create_dir_all(&root).expect("create remote reload test directory");
    let config_path = root.join("config.json");
    let first_config = remote_lifecycle_config(first_port);
    std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&first_config).expect("serialize first remote config"),
    )
    .expect("write first remote config");
    let mut runtime = RuntimeState::new(first_config).expect("create remote runtime");
    runtime.set_config_path(Some(config_path.clone()));
    let service = VinputDbusService::new_with_remote_bind(
        runtime,
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
    );

    assert!(service.start_remote_text_service().await.unwrap());
    assert!(remote_health_is_ready(first_port).await);
    assert_eq!(
        service
            .remote_text_status()
            .await
            .local_addr
            .unwrap()
            .port(),
        first_port
    );

    let second_config = remote_lifecycle_config(second_port);
    std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&second_config).expect("serialize second remote config"),
    )
    .expect("write second remote config");
    service.reload_asr_backend().await.unwrap();
    assert!(!remote_health_is_ready(first_port).await);
    assert!(remote_health_is_ready(second_port).await);
    assert_eq!(
        service
            .remote_text_status()
            .await
            .local_addr
            .unwrap()
            .port(),
        second_port
    );

    let disabled = VinputConfig::bundled_default().expect("parse bundled config");
    std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&disabled).expect("serialize disabled remote config"),
    )
    .expect("write disabled remote config");
    service.reload_asr_backend().await.unwrap();
    assert!(!service.remote_text_status().await.running);
    assert!(!remote_health_is_ready(second_port).await);
    assert!(!service.shutdown_remote_text_service().await.unwrap());
    std::fs::remove_dir_all(root).expect("remove remote reload test directory");
}

#[tokio::test]
async fn dbus_facade_remote_bind_failure_drops_stale_listener() {
    let first_port = reserve_remote_port();
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").expect("occupy remote reload port");
    let occupied_port = occupied.local_addr().unwrap().port();
    let root = unique_adapter_runtime_dir("remote-bind-failure");
    std::fs::create_dir_all(&root).expect("create remote bind failure directory");
    let config_path = root.join("config.json");
    let first_config = remote_lifecycle_config(first_port);
    std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&first_config).expect("serialize first remote config"),
    )
    .expect("write first remote config");
    let mut runtime = RuntimeState::new(first_config).expect("create remote runtime");
    runtime.set_config_path(Some(config_path.clone()));
    let service = VinputDbusService::new_with_remote_bind(
        runtime,
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
    );

    service.start_remote_text_service().await.unwrap();
    assert!(remote_health_is_ready(first_port).await);
    let blocked_config = remote_lifecycle_config(occupied_port);
    std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&blocked_config).expect("serialize blocked remote config"),
    )
    .expect("write blocked remote config");
    let error = service
        .reload_asr_backend()
        .await
        .expect_err("occupied port should reject remote reload");
    assert!(error.to_string().contains("bind remote text service"));
    assert!(!service.remote_text_status().await.running);
    assert!(!remote_health_is_ready(first_port).await);

    drop(occupied);
    std::fs::remove_dir_all(root).expect("remove remote bind failure directory");
}

#[tokio::test]
async fn dbus_facade_provider_selection_starts_and_stops_remote_service() {
    let port = reserve_remote_port();
    let root = unique_adapter_runtime_dir("remote-provider-selection");
    std::fs::create_dir_all(&root).expect("create remote selection directory");
    let config_path = root.join("config.json");
    let mut config = remote_lifecycle_config(port);
    config.asr.providers.push(
        serde_json::from_value(serde_json::json!({
            "id":"mock",
            "type":"local",
            "model":"fixture-model"
        }))
        .expect("parse mock provider"),
    );
    "mock".clone_into(&mut config.asr.active_provider);
    std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&config).expect("serialize selection config"),
    )
    .expect("write selection config");
    let mut runtime = RuntimeState::new(config).expect("create selection runtime");
    runtime.set_config_path(Some(config_path.clone()));
    let service = VinputDbusService::new_with_remote_bind(
        runtime,
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
    );

    assert!(!service.start_remote_text_service().await.unwrap());
    assert!(!service.remote_text_status().await.running);
    assert!(
        service
            .set_active_asr_provider("provider.vinput.remote.streaming")
            .await
            .unwrap()
    );
    assert!(service.remote_text_status().await.running);
    assert!(remote_health_is_ready(port).await);

    assert!(service.set_active_asr_provider("mock").await.unwrap());
    assert!(!service.remote_text_status().await.running);
    assert!(!remote_health_is_ready(port).await);
    std::fs::remove_dir_all(root).expect("remove remote selection directory");
}

#[tokio::test]
async fn dbus_facade_defers_reload_while_recording() {
    let service = service();

    assert_eq!(
        service.start_recording_state().await.unwrap().0,
        "recording"
    );
    service.reload_asr_backend().await.unwrap();

    assert_eq!(service.get_status().await, "recording");
    assert!(service.get_asr_backend_state().await.5);
    let payload =
        RecognitionPayload::from_json_str(&service.stop_recording_payload("").await.unwrap().0)
            .unwrap();
    assert_eq!(payload.commit_text, "mock recognition result");
    let state = wait_for_asr_reload(&service).await;
    assert_eq!(state.2, "mock");
    assert_eq!(state.3, "mock-streaming");
    assert!(state.4.contains("Failed to reload ASR backend"));
}

#[tokio::test]
async fn dbus_facade_reload_rebuilds_configured_backend() {
    let mut config = VinputConfig::bundled_default().unwrap();
    config.asr.active_provider = "mock".to_owned();
    config.asr.providers.push(AsrProviderConfig {
        id: "mock".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: None,
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::new(),
        endpoint: None,
    });
    let runtime = RuntimeState::with_asr_backend(
        config,
        Box::new(MockAsrBackend::buffered("injected final")),
    )
    .unwrap();
    let service = VinputDbusService::new(runtime);

    assert_eq!(service.get_asr_backend_state().await.3, "mock-buffered");
    service.reload_asr_backend().await.unwrap();
    let state = wait_for_asr_reload(&service).await;
    assert_eq!(state.0, "mock");
    assert_eq!(state.2, "mock");
    assert_eq!(state.3, "mock-streaming");
    assert!(!state.5);
    assert!(state.4.is_empty());
}

#[tokio::test]
async fn dbus_facade_reload_synchronizes_scene_state_from_config_file() {
    let directory = tempfile::tempdir().unwrap();
    let config_path = directory.path().join("config.json");
    let mut config = VinputConfig::bundled_default().unwrap();
    config.asr.active_provider = "mock".to_owned();
    config.asr.providers.push(AsrProviderConfig {
        id: "mock".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: None,
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::new(),
        endpoint: None,
    });
    std::fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    let mut runtime = RuntimeState::with_asr_backend(
        config.clone(),
        Box::new(MockAsrBackend::buffered("injected final")),
    )
    .unwrap();
    runtime.set_config_path(Some(config_path.clone()));
    let service = VinputDbusService::new(runtime);

    config.scenes.active_scene = "meeting".to_owned();
    config
        .scenes
        .definitions
        .push(vinput_config::SceneDefinition {
            id: "meeting".to_owned(),
            label: "Meeting".to_owned(),
            prompt: None,
            provider_id: None,
            model: None,
            candidate_count: 0,
            timeout_ms: None,
            context_lines: 0,
        });
    config.validate().unwrap();
    std::fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();

    service.reload_asr_backend().await.unwrap();
    let scene_state = service.get_scene_state().await;
    assert_eq!(scene_state.0, "meeting");
    assert!(
        scene_state
            .1
            .contains(&("meeting".to_owned(), "Meeting".to_owned()))
    );
    let state = wait_for_asr_reload(&service).await;
    assert_eq!(state.2, "mock");
    assert!(!state.5);
}

#[tokio::test]
async fn dbus_facade_preserves_early_final_events() {
    let config = VinputConfig::bundled_default().unwrap();
    let runtime = RuntimeState::with_asr_backend(
        config,
        Box::new(MockAsrBackend::streaming_with_early_final(
            "early partial",
            "early final",
        )),
    )
    .unwrap();
    let service = VinputDbusService::new(runtime);

    assert_eq!(
        service.start_recording_state().await.unwrap().0,
        "recording"
    );
    let (payload_json, status, partial_text) = service.stop_recording_payload("").await.unwrap();
    let payload = RecognitionPayload::from_json_str(&payload_json).unwrap();

    assert_eq!(payload.commit_text, "early final");
    assert_eq!(partial_text.as_deref(), Some("early partial"));
    assert_eq!(status, "idle");
}

#[tokio::test]
async fn dbus_facade_exercises_timeout_mock_flow() {
    let mut config = VinputConfig::bundled_default().unwrap();
    config.scenes.active_scene = "timeout-scene".to_owned();
    config
        .scenes
        .definitions
        .push(vinput_config::SceneDefinition {
            id: "timeout-scene".to_owned(),
            label: "Timeout scene".to_owned(),
            prompt: None,
            provider_id: None,
            model: None,
            candidate_count: 0,
            timeout_ms: Some(2500),
            context_lines: 0,
        });
    let service = VinputDbusService::new(RuntimeState::new(config).unwrap());

    assert_eq!(
        service.start_recording_state().await.unwrap().0,
        "recording"
    );
    let payload =
        RecognitionPayload::from_json_str(&service.stop_recording_payload("").await.unwrap().0)
            .unwrap();
    assert_eq!(
        payload.commit_text,
        "mock postprocess result: mock recognition result"
    );
}

#[tokio::test]
async fn dbus_facade_exercises_command_mock_flow() {
    let service = service();
    assert_eq!(
        service
            .start_command_recording_state("selected text")
            .await
            .unwrap()
            .0,
        "recording"
    );
    let payload =
        RecognitionPayload::from_json_str(&service.stop_recording_payload("").await.unwrap().0)
            .unwrap();
    assert_eq!(
        payload.commit_text,
        "mock command result for: selected text"
    );
}

#[tokio::test]
async fn dbus_facade_handles_legacy_command_asr_stdout() {
    let mut config = VinputConfig::bundled_default().unwrap();
    config.asr.active_provider = "cmd".to_owned();
    config.asr.providers.push(AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: Some(1_000),
        model: Some("cmd-model".to_owned()),
        hotwords_file: None,
        command: Some("sh".to_owned()),
        args: vec![
            "-c".to_owned(),
            r"cat >/dev/null; printf '%s
' 'dbus final'"
                .to_owned(),
        ],
        env: std::collections::HashMap::new(),
        endpoint: None,
    });
    let service = VinputDbusService::new(RuntimeState::with_configured_asr(config).unwrap());

    assert_eq!(
        service.start_recording_state().await.unwrap().0,
        "recording"
    );
    let (payload_json, status, partial_text) = service.stop_recording_payload("").await.unwrap();
    let payload = RecognitionPayload::from_json_str(&payload_json).unwrap();

    assert_eq!(payload.commit_text, "dbus final");
    assert_eq!(status, "idle");
    assert!(partial_text.is_none());
}

#[tokio::test]
async fn dbus_facade_uses_configured_text_adapter() {
    let mut config = VinputConfig::bundled_default().unwrap();
    config.asr.active_provider = "mock".to_owned();
    config.asr.providers.push(AsrProviderConfig {
        id: "mock".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: None,
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::new(),
        endpoint: None,
    });
    config.scenes.active_scene = "needs-adapter".to_owned();
    config
        .scenes
        .definitions
        .push(vinput_config::SceneDefinition {
            id: "needs-adapter".to_owned(),
            label: "Needs adapter".to_owned(),
            prompt: Some("polish".to_owned()),
            provider_id: None,
            model: None,
            candidate_count: 1,
            timeout_ms: None,
            context_lines: 0,
        });
    config.llm.adapters.push(LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            r#"cat >/dev/null; printf '%s
' '{"text":"dbus configured final"}'"#
                .to_owned(),
        ],
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    });
    let service = VinputDbusService::new(RuntimeState::with_configured_backends(config).unwrap());

    assert_eq!(
        service.start_recording_state().await.unwrap().0,
        "recording"
    );
    let payload =
        RecognitionPayload::from_json_str(&service.stop_recording_payload("").await.unwrap().0)
            .unwrap();
    assert_eq!(payload.commit_text, "dbus configured final");
}

#[tokio::test]
async fn dbus_facade_preserves_remote_asr_endpoint_with_mock_runtime() {
    let mut config = VinputConfig::bundled_default().unwrap();
    config.asr.active_provider = "remote".to_owned();
    config.asr.providers.push(AsrProviderConfig {
        id: "remote".to_owned(),
        kind: AsrProviderKind::Remote,
        timeout_ms: None,
        model: Some("cloud".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: Some("https://asr.example.test".to_owned()),
    });
    let service = VinputDbusService::new(RuntimeState::new(config).unwrap());

    let state = service.get_asr_backend_state().await;
    assert_eq!(state.0, "remote");
    assert_eq!(state.1, "cloud");
    assert_eq!(state.2, "mock");
    assert_eq!(state.3, "mock-streaming");
    assert!(state.6);
    assert_eq!(state.7, ["https://asr.example.test"]);
}

#[tokio::test]
async fn dbus_facade_preserves_command_asr_metadata() {
    let mut config = VinputConfig::bundled_default().unwrap();
    config.asr.active_provider = "cmd".to_owned();
    config.asr.providers.push(AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: Some(1_500),
        model: Some("cmd-model".to_owned()),
        hotwords_file: Some("/tmp/hotwords.txt".to_owned()),
        command: Some("helper".to_owned()),
        args: vec!["--json".to_owned()],
        env: std::collections::HashMap::default(),
        endpoint: None,
    });
    let service = VinputDbusService::new(RuntimeState::new(config).unwrap());

    let state = service.get_asr_backend_state().await;
    assert!(state.6);
    assert_eq!(state.0, "cmd");
    assert_eq!(state.1, "cmd-model");
    assert_eq!(state.2, "mock");
    assert_eq!(state.3, "mock-streaming");
}

#[tokio::test]
async fn dbus_facade_supervises_configured_adapter() {
    let service = service();
    let start_error = service
        .start_adapter("mock-adapter")
        .await
        .expect_err("unconfigured adapter start should fail");
    assert!(
        start_error
            .to_string()
            .contains("text adapter `mock-adapter` is not configured")
    );
    let stop_error = service
        .stop_adapter("mock-adapter")
        .await
        .expect_err("unconfigured adapter stop should fail");
    assert!(
        stop_error
            .to_string()
            .contains("text adapter `mock-adapter` is not configured")
    );

    let runtime_dir = unique_adapter_runtime_dir("dbus-supervisor");
    let pid_path = runtime_dir.join("mock-adapter.pid");
    let mut config = VinputConfig::bundled_default().unwrap();
    config.llm.adapters.push(LlmAdapterConfig {
        id: "mock-adapter".to_owned(),
        command: "sleep".to_owned(),
        args: vec!["30".to_owned()],
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    });
    let runtime = RuntimeState::new(config)
        .unwrap()
        .with_adapter_runtime_paths(vinput_text::AdapterRuntimePaths::new(runtime_dir.clone()));
    let service = VinputDbusService::new(runtime);

    service.start_adapter("mock-adapter").await.unwrap();
    assert!(pid_path.exists());
    let duplicate_error = service
        .start_adapter("mock-adapter")
        .await
        .expect_err("duplicate adapter start should fail");
    assert!(
        duplicate_error
            .to_string()
            .contains("text adapter `mock-adapter` is already running")
    );
    service.stop_adapter("mock-adapter").await.unwrap();
    assert!(!pid_path.exists());
    service.stop_adapter("mock-adapter").await.unwrap();
    let _ = std::fs::remove_dir_all(runtime_dir);
}

#[tokio::test]
async fn dbus_facade_returns_text_adapter_state_json() {
    let mut config = VinputConfig::bundled_default().unwrap();
    config.llm.adapters.push(LlmAdapterConfig {
        id: "mock-adapter".to_owned(),
        command: "vinput-postprocess".to_owned(),
        args: vec!["--json".to_owned()],
        env: std::collections::HashMap::from([("TOKEN".to_owned(), "secret".to_owned())]),
        working_dir: Some("/tmp/adapter-work".to_owned()),
        extra: std::collections::HashMap::default(),
    });
    let service = VinputDbusService::new(RuntimeState::new(config).unwrap());
    let state_json = service.get_text_adapter_state().await.unwrap();
    let state: TextAdapterState = serde_json::from_str(&state_json).unwrap();
    assert!(!state_json.contains("TOKEN"));
    assert!(!state_json.contains("secret"));
    assert!(!state_json.contains("/tmp/adapter-work"));

    assert_eq!(state.adapter_count, 1);
    assert_eq!(state.adapter_ids, ["mock-adapter"]);
    assert_eq!(state.single_adapter_id.as_deref(), Some("mock-adapter"));
    assert_eq!(state.adapters[0].kind, "command");
    assert_eq!(state.adapters[0].args_count, 1);
    assert_eq!(state.adapters[0].env_count, 1);
    assert!(state.adapters[0].has_working_dir);
}

#[tokio::test]
async fn dbus_facade_returns_asr_state_tuple() {
    let service = service();
    let state = service.get_asr_backend_state().await;
    assert!(state.6);
    assert_eq!(state.0, "sherpa-onnx");
    assert_eq!(state.2, "mock");
    assert_eq!(state.3, "mock-streaming");
    assert!(state.4.is_empty());
}

#[tokio::test]
async fn dbus_facade_lists_and_selects_scenes() {
    let service = service();
    let state = service.get_scene_state().await;
    assert_eq!(state.0, vinput_config::RAW_SCENE_ID);
    assert_eq!(
        state.1,
        [
            (vinput_config::RAW_SCENE_ID.to_owned(), "Raw".to_owned()),
            (
                vinput_config::COMMAND_SCENE_ID.to_owned(),
                "Command".to_owned()
            ),
        ]
    );

    assert!(
        !service
            .set_active_scene(vinput_config::COMMAND_SCENE_ID)
            .await
            .unwrap()
    );
    assert_eq!(
        service.get_scene_state().await.0,
        vinput_config::COMMAND_SCENE_ID
    );
    assert!(service.set_active_scene("missing").await.is_err());
}

#[tokio::test]
async fn dbus_facade_lists_and_selects_asr_providers() {
    let mut config = VinputConfig::bundled_default().unwrap();
    config.asr.providers.push(AsrProviderConfig {
        id: "mock".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("mock-model".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::new(),
        endpoint: None,
    });
    let runtime = RuntimeState::with_asr_backend(
        config,
        Box::new(MockAsrBackend::buffered("injected final")),
    )
    .unwrap();
    let service = VinputDbusService::new(runtime);

    let before = service.get_asr_menu_state().await;
    assert_eq!(before.0, "sherpa-onnx");
    assert_eq!(before.1, "mock");
    assert_eq!(before.2, "mock-buffered");
    assert_eq!(before.5[1].0, "mock");

    assert!(!service.set_active_asr_provider("mock").await.unwrap());
    let after = wait_for_asr_reload(&service).await;
    assert_eq!(after.0, "mock");
    assert_eq!(after.2, "mock");
    assert_eq!(after.3, "mock-streaming");
    assert!(after.4.is_empty());
    assert!(service.set_active_asr_provider("missing").await.is_err());
}

#[test]
fn live_partial_generations_cancel_stale_pollers_and_return_last_emission() {
    let mut state = LivePartialEmissionState::default();
    let first = state.begin(Some("first".to_owned()));
    assert!(state.is_current(first));
    assert_eq!(state.last_emitted.as_deref(), Some("first"));

    assert_eq!(state.cancel().as_deref(), Some("first"));
    assert!(!state.is_current(first));
    assert!(state.last_emitted.is_none());

    let second = state.begin(None);
    assert!(state.is_current(second));
    assert_ne!(first, second);
}

#[tokio::test]
async fn recording_operations_wait_for_the_prior_transaction() {
    let service = service();
    let first = service.lock_recording_operation().await;
    let waiting_service = service.clone();
    let mut waiter = tokio::spawn(async move {
        let _second = waiting_service.lock_recording_operation().await;
    });

    assert!(
        timeout(Duration::from_millis(20), &mut waiter)
            .await
            .is_err()
    );
    drop(first);
    timeout(Duration::from_secs(1), waiter)
        .await
        .expect("recording transaction should resume after the prior operation")
        .expect("recording transaction task should finish");
}

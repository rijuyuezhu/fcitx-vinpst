//! Real local-socket coverage for the remote text HTTP/WebSocket runtime.

use std::{net::SocketAddr, time::Duration};

use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{
        Error as WebSocketError, Message,
        client::IntoClientRequest as _,
        http::{HeaderValue, header::AUTHORIZATION},
    },
};
use vinpst_config::VinpstConfig;
use vinpst_daemon::remote::{RemoteTextLifecycle, RemoteTextServer, remote_text_settings};

type ClientSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

fn remote_config(port: Option<u16>, debounce_ms: u64) -> VinpstConfig {
    remote_config_with_key(port, debounce_ms, "fixture-key")
}

fn remote_config_with_key(port: Option<u16>, debounce_ms: u64, api_key: &str) -> VinpstConfig {
    let mut env = json!({
        "VINPST_ASR_API_KEY":api_key,
        "VINPST_ASR_DEBOUNCE_MS":debounce_ms.to_string()
    });
    if let Some(port) = port {
        env["VINPST_ASR_PORT"] = port.to_string().into();
    }
    VinpstConfig::from_json_str(
        &serde_json::to_string(&json!({
            "version":1,
            "asr":{
                "active_provider":"provider.vinpst.remote.streaming",
                "providers":[{
                    "id":"provider.vinpst.remote.streaming",
                    "type":"command",
                    "command":"python3",
                    "args":["remote.py"],
                    "env":env
                }]
            },
            "scenes":{
                "active_scene":"raw",
                "definitions":[{"id":"raw","label":"Raw","candidate_count":0}]
            }
        }))
        .expect("serialize remote server config"),
    )
    .expect("parse remote server config")
}

fn remote_settings(debounce_ms: u64) -> vinpst_daemon::remote::RemoteTextServiceSettings {
    let config = remote_config(None, debounce_ms);
    remote_text_settings(&config)
        .expect("derive remote server settings")
        .expect("remote server should be enabled")
}

fn reserve_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("reserve loopback port")
        .local_addr()
        .expect("read reserved loopback address")
        .port()
}

async fn health_is_ready(port: u16) -> bool {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(150))
        .build()
        .expect("build lifecycle health client");
    client
        .get(format!("http://127.0.0.1:{port}/health"))
        .send()
        .await
        .is_ok_and(|response| response.status().is_success())
}

async fn start_server(debounce_ms: u64) -> RemoteTextServer {
    RemoteTextServer::bind(
        remote_settings(debounce_ms),
        "127.0.0.1:0"
            .parse::<SocketAddr>()
            .expect("parse loopback address"),
    )
    .await
    .expect("bind remote text server")
}

async fn receive_json(socket: &mut ClientSocket) -> Value {
    let message = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .expect("websocket message deadline")
        .expect("websocket should remain open")
        .expect("read websocket message");
    let Message::Text(text) = message else {
        panic!("expected text websocket message, got {message:?}");
    };
    serde_json::from_str(text.as_ref()).expect("parse websocket JSON")
}

async fn send_json(socket: &mut ClientSocket, value: Value) {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .expect("send websocket JSON");
}

async fn assert_socket_closes(socket: &mut ClientSocket) {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            match socket.next().await {
                None
                | Some(Err(WebSocketError::ConnectionClosed | WebSocketError::AlreadyClosed)) => {
                    return;
                }
                Some(Ok(Message::Close(_))) => return,
                Some(Ok(_)) => {}
                Some(Err(error)) => panic!("unexpected websocket shutdown error: {error}"),
            }
        }
    })
    .await
    .expect("remote service shutdown should close websocket promptly");
}

async fn connect_input(address: SocketAddr, api_key: &str) -> ClientSocket {
    let (mut socket, _) = connect_async(format!("ws://{address}/ws"))
        .await
        .expect("connect input websocket");
    send_json(&mut socket, json!({"type":"auth","api_key":api_key})).await;
    socket
}

async fn connect_output(address: SocketAddr, api_key: &str) -> ClientSocket {
    let mut request = format!("ws://{address}/v1/realtime")
        .into_client_request()
        .expect("build output websocket request");
    request.headers_mut().insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {api_key}")).expect("build authorization header"),
    );
    connect_async(request)
        .await
        .expect("connect output websocket")
        .0
}

#[tokio::test]
async fn serves_health_browser_and_favicon_assets() {
    let server = start_server(50).await;
    let address = server.local_addr();
    let client = reqwest::Client::new();

    let health = client
        .get(format!("http://{address}/health"))
        .send()
        .await
        .expect("request health")
        .error_for_status()
        .expect("health should succeed")
        .json::<Value>()
        .await
        .expect("parse health JSON");
    assert_eq!(health, json!({"ok":true}));

    let index = client
        .get(format!("http://{address}/"))
        .send()
        .await
        .expect("request browser page")
        .error_for_status()
        .expect("browser page should succeed")
        .text()
        .await
        .expect("read browser page");
    assert!(index.contains("Vinpst Remote"));
    assert!(index.contains("/ws"));
    assert!(index.contains("text_update"));
    assert!(index.contains("localStorage.setItem('vinpst_remote_api_key', entered)"));
    assert!(index.contains("outputStatus.textContent = 'output disconnected'"));
    assert!(index.contains("socket.onerror = () => socket.close()"));

    let favicon = client
        .get(format!("http://{address}/favicon.svg"))
        .send()
        .await
        .expect("request favicon")
        .error_for_status()
        .expect("favicon should succeed");
    assert_eq!(
        favicon
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/svg+xml")
    );

    server.shutdown().await.expect("shutdown remote server");
}

#[tokio::test]
async fn realtime_upgrade_requires_bearer_token() {
    let server = start_server(50).await;
    let address = server.local_addr();

    let result = connect_async(format!("ws://{address}/v1/realtime")).await;
    match result {
        Err(WebSocketError::Http(response)) => {
            assert_eq!(response.status(), 401);
        }
        other => panic!("expected HTTP authorization failure, got {other:?}"),
    }

    server.shutdown().await.expect("shutdown remote server");
}

#[tokio::test]
async fn browser_auth_and_single_input_ownership_match_legacy() {
    let server = start_server(50).await;
    let address = server.local_addr();

    let mut unauthorized = connect_input(address, "wrong-key").await;
    let error = receive_json(&mut unauthorized).await;
    assert_eq!(error["type"], "error");
    assert_eq!(error["message"], "Unauthorized.");

    let mut first = connect_input(address, "fixture-key").await;
    assert_eq!(receive_json(&mut first).await["type"], "auth_ok");
    let init = receive_json(&mut first).await;
    assert_eq!(init["type"], "init");
    assert_eq!(init["output_status"], "disconnected");

    let mut duplicate = connect_input(address, "fixture-key").await;
    assert_eq!(receive_json(&mut duplicate).await["type"], "auth_ok");
    let duplicate_error = receive_json(&mut duplicate).await;
    assert_eq!(duplicate_error["type"], "error");
    assert_eq!(
        duplicate_error["message"],
        "Input client already connected."
    );

    first.close(None).await.expect("close input websocket");
    duplicate
        .close(None)
        .await
        .expect("close duplicate input websocket");
    server.shutdown().await.expect("shutdown remote server");
}

#[tokio::test]
async fn websocket_runtime_emits_session_and_debounced_transcription_events() {
    let server = start_server(25).await;
    let address = server.local_addr();

    let mut input = connect_input(address, "fixture-key").await;
    assert_eq!(receive_json(&mut input).await["type"], "auth_ok");
    assert_eq!(receive_json(&mut input).await["type"], "init");

    let mut output = connect_output(address, "fixture-key").await;
    assert_eq!(receive_json(&mut input).await["type"], "output_connected");

    send_json(
        &mut output,
        json!({
            "type":"session.update",
            "session":{"input_audio_format":"pcm16"}
        }),
    )
    .await;
    let session = receive_json(&mut output).await;
    assert_eq!(session["type"], "session.updated");
    assert_eq!(session["session"]["input_audio_format"], "pcm16");

    send_json(
        &mut input,
        json!({"type":"text_update","text":"hello from browser"}),
    )
    .await;
    let committed = receive_json(&mut output).await;
    let delta = receive_json(&mut output).await;
    let completed = receive_json(&mut output).await;
    assert_eq!(committed["type"], "input_audio_buffer.committed");
    assert_eq!(
        delta["type"],
        "conversation.item.input_audio_transcription.delta"
    );
    assert_eq!(delta["delta"], "hello from browser");
    assert_eq!(
        completed["type"],
        "conversation.item.input_audio_transcription.completed"
    );
    assert_eq!(completed["transcript"], "hello from browser");
    assert_eq!(committed["item_id"], delta["item_id"]);
    assert_eq!(delta["item_id"], completed["item_id"]);

    output.close(None).await.expect("close output websocket");
    assert_eq!(
        receive_json(&mut input).await["type"],
        "output_disconnected"
    );
    input.close(None).await.expect("close input websocket");
    server.shutdown().await.expect("shutdown remote server");
}

#[tokio::test]
async fn server_shutdown_closes_authenticated_websocket_clients() {
    let server = start_server(25).await;
    let address = server.local_addr();

    let mut input = connect_input(address, "fixture-key").await;
    assert_eq!(receive_json(&mut input).await["type"], "auth_ok");
    assert_eq!(receive_json(&mut input).await["type"], "init");

    let mut output = connect_output(address, "fixture-key").await;
    assert_eq!(receive_json(&mut input).await["type"], "output_connected");

    tokio::time::timeout(Duration::from_secs(1), server.shutdown())
        .await
        .expect("remote server shutdown should not wait for websocket clients")
        .expect("shutdown remote server");
    assert_socket_closes(&mut input).await;
    assert_socket_closes(&mut output).await;
}

#[tokio::test]
async fn websocket_rejects_payload_over_frozen_two_mib_limit() {
    let server = start_server(25).await;
    let address = server.local_addr();
    let mut input = connect_input(address, "fixture-key").await;
    assert_eq!(receive_json(&mut input).await["type"], "auth_ok");
    assert_eq!(receive_json(&mut input).await["type"], "init");

    input
        .send(Message::Text("x".repeat(2 * 1024 * 1024 + 1).into()))
        .await
        .expect("send oversized websocket frame");
    assert_socket_closes(&mut input).await;
    server.shutdown().await.expect("shutdown remote server");
}

#[tokio::test]
async fn lifecycle_key_rotation_closes_old_clients_and_requires_new_key() {
    let port = reserve_port();
    let mut lifecycle = RemoteTextLifecycle::new("127.0.0.1".parse().unwrap());
    let first = remote_config_with_key(Some(port), 25, "old-key");
    assert!(lifecycle.reconcile_config(&first).await.unwrap());
    let address = lifecycle.status().local_addr.unwrap();

    let mut old_client = connect_input(address, "old-key").await;
    assert_eq!(receive_json(&mut old_client).await["type"], "auth_ok");
    assert_eq!(receive_json(&mut old_client).await["type"], "init");

    let rotated = remote_config_with_key(Some(port), 25, "new-key");
    assert!(lifecycle.reconcile_config(&rotated).await.unwrap());
    assert_socket_closes(&mut old_client).await;

    let mut rejected = connect_input(address, "old-key").await;
    let error = receive_json(&mut rejected).await;
    assert_eq!(error["type"], "error");
    assert_eq!(error["message"], "Unauthorized.");
    assert_socket_closes(&mut rejected).await;

    let mut current = connect_input(address, "new-key").await;
    assert_eq!(receive_json(&mut current).await["type"], "auth_ok");
    assert_eq!(receive_json(&mut current).await["type"], "init");
    current
        .close(None)
        .await
        .expect("close rotated input client");
    assert!(lifecycle.stop().await.unwrap());
}

#[tokio::test]
async fn lifecycle_starts_preserves_restarts_and_stops_from_config() {
    let first_port = reserve_port();
    let mut second_port = reserve_port();
    while second_port == first_port {
        second_port = reserve_port();
    }
    let mut lifecycle = RemoteTextLifecycle::new("127.0.0.1".parse().unwrap());

    let first = remote_config(Some(first_port), 25);
    assert!(lifecycle.reconcile_config(&first).await.unwrap());
    assert_eq!(lifecycle.status().local_addr.unwrap().port(), first_port);
    assert!(lifecycle.endpoints().is_empty());
    assert!(health_is_ready(first_port).await);
    assert!(!lifecycle.reconcile_config(&first).await.unwrap());

    let second = remote_config(Some(second_port), 50);
    assert!(lifecycle.reconcile_config(&second).await.unwrap());
    assert_eq!(lifecycle.status().local_addr.unwrap().port(), second_port);
    assert!(!health_is_ready(first_port).await);
    assert!(health_is_ready(second_port).await);

    let disabled = VinpstConfig::bundled_default().expect("parse bundled config");
    assert!(lifecycle.reconcile_config(&disabled).await.unwrap());
    assert!(!lifecycle.status().running);
    assert!(!health_is_ready(second_port).await);
    assert!(!lifecycle.stop().await.unwrap());
}

#[tokio::test]
async fn lifecycle_bind_failure_does_not_keep_stale_service() {
    let first_port = reserve_port();
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").expect("occupy loopback port");
    let occupied_port = occupied.local_addr().unwrap().port();
    let mut lifecycle = RemoteTextLifecycle::new("127.0.0.1".parse().unwrap());

    lifecycle
        .reconcile_config(&remote_config(Some(first_port), 25))
        .await
        .expect("start first lifecycle server");
    assert!(health_is_ready(first_port).await);

    let error = lifecycle
        .reconcile_config(&remote_config(Some(occupied_port), 25))
        .await
        .expect_err("occupied port should reject restart");
    assert!(error.to_string().contains("bind remote text service"));
    assert!(!lifecycle.status().running);
    assert!(!health_is_ready(first_port).await);
    drop(occupied);
}

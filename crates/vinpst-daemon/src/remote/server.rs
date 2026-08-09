//! Async HTTP/WebSocket runtime for the deterministic remote text protocol.

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use axum::{
    Json, Router,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use futures_util::{SinkExt as _, StreamExt as _};
use if_addrs::get_if_addrs;
use serde_json::{Value, json};
use thiserror::Error;
use tokio::{
    net::TcpListener,
    sync::{Mutex, mpsc, oneshot, watch},
    task::JoinHandle,
};

use super::{
    RemoteDebounceAction, RemoteProtocolEffects, RemoteTextProtocol, RemoteTextProtocolError,
    RemoteTextServiceSettings, RemoteTextSettingsError, remote_text_settings,
};
use vinpst_config::VinpstConfig;

const MAX_WEBSOCKET_MESSAGE_BYTES: usize = 2 * 1024 * 1024;

const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Vinpst Remote</title>
  <style>
    body { max-width: 48rem; margin: 2rem auto; padding: 0 1rem; font-family: system-ui, sans-serif; }
    textarea { width: 100%; min-height: 50vh; font: 1.1rem/1.5 system-ui, sans-serif; }
    .row { display: flex; justify-content: space-between; gap: 1rem; margin: .75rem 0; }
  </style>
</head>
<body>
  <h1>Vinpst Remote</h1>
  <div class="row"><span id="input">connecting</span><span id="output">output disconnected</span></div>
  <textarea id="editor" disabled placeholder="Connect Vinpst, then type here."></textarea>
  <div class="row"><span id="count">0 chars</span><button id="send" disabled>Send now</button></div>
  <script>
    const editor = document.getElementById('editor')
    const sendButton = document.getElementById('send')
    const inputStatus = document.getElementById('input')
    const outputStatus = document.getElementById('output')
    const count = document.getElementById('count')
    let socket
    let outputConnected = false
    let composing = false

    function apiKey() {
      const params = new URLSearchParams(location.hash.replace(/^#/, ''))
      const fromHash = params.get('key') || ''
      if (fromHash) {
        localStorage.setItem('vinpst_remote_api_key', fromHash)
        history.replaceState(null, '', location.pathname)
        return fromHash
      }
      const stored = localStorage.getItem('vinpst_remote_api_key') || ''
      if (stored) return stored
      const entered = prompt('API key') || ''
      if (entered) localStorage.setItem('vinpst_remote_api_key', entered)
      return entered
    }
    function updateEnabled() {
      const enabled = socket && socket.readyState === WebSocket.OPEN && outputConnected
      editor.disabled = !enabled
      sendButton.disabled = !enabled
      if (enabled) editor.focus()
    }
    function send(payload) {
      if (socket && socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify(payload))
    }
    function connect() {
      const scheme = location.protocol === 'https:' ? 'wss:' : 'ws:'
      inputStatus.textContent = 'input connecting'
      socket = new WebSocket(`${scheme}//${location.host}/ws`)
      socket.onopen = () => send({type: 'auth', api_key: apiKey()})
      socket.onmessage = event => {
        let message
        try { message = JSON.parse(event.data) } catch { return }
        if (message.type === 'auth_ok') inputStatus.textContent = 'input connected'
        if (message.type === 'init') outputConnected = message.output_status === 'connected'
        if (message.type === 'output_connected') outputConnected = true
        if (message.type === 'output_disconnected') {
          outputConnected = false
          editor.value = ''
          count.textContent = '0 chars'
        }
        if (message.type === 'error') {
          localStorage.removeItem('vinpst_remote_api_key')
          inputStatus.textContent = message.message || 'input failed'
        }
        outputStatus.textContent = outputConnected ? 'output connected' : 'output disconnected'
        updateEnabled()
      }
      socket.onclose = () => {
        inputStatus.textContent = 'input disconnected'
        outputConnected = false
        outputStatus.textContent = 'output disconnected'
        editor.value = ''
        count.textContent = '0 chars'
        updateEnabled()
        setTimeout(connect, 1000)
      }
      socket.onerror = () => socket.close()
    }
    editor.addEventListener('compositionstart', () => { composing = true })
    editor.addEventListener('compositionend', () => {
      composing = false
      count.textContent = `${editor.value.length} chars`
      send({type: 'text_update', text: editor.value})
    })
    editor.addEventListener('input', () => {
      count.textContent = `${editor.value.length} chars`
      if (!composing) send({type: 'text_update', text: editor.value})
    })
    sendButton.addEventListener('click', () => send({type: 'finalize'}))
    connect()
  </script>
</body>
</html>
"#;

const FAVICON_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" rx="12" fill="#1f9d87"/><path d="M32 12c-5 0-9 4-9 9v16c0 5 4 9 9 9s9-4 9-9V21c0-5-4-9-9-9z" fill="#fff"/></svg>"##;

/// Errors returned while binding, serving, or shutting down the remote text service.
#[derive(Debug, Error)]
pub enum RemoteTextServerError {
    /// TCP listener creation failed.
    #[error("bind remote text service: {0}")]
    Bind(#[source] std::io::Error),
    /// The HTTP server stopped with an I/O error.
    #[error("serve remote text service: {0}")]
    Serve(#[source] std::io::Error),
    /// The server task panicked or was cancelled unexpectedly.
    #[error("join remote text service task: {0}")]
    Join(#[source] tokio::task::JoinError),
}

/// A running HTTP/WebSocket remote text service with explicit graceful shutdown.
pub struct RemoteTextServer {
    local_addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    client_shutdown: watch::Sender<bool>,
    task: Option<JoinHandle<Result<(), RemoteTextServerError>>>,
}

impl RemoteTextServer {
    /// Binds the service to the supplied socket address.
    ///
    /// Port `0` is accepted for deterministic tests. Production callers should use
    /// `SocketAddr::new(bind_ip, settings.port)`.
    pub async fn bind(
        settings: RemoteTextServiceSettings,
        bind_addr: SocketAddr,
    ) -> Result<Self, RemoteTextServerError> {
        let listener = TcpListener::bind(bind_addr)
            .await
            .map_err(RemoteTextServerError::Bind)?;
        let local_addr = listener.local_addr().map_err(RemoteTextServerError::Bind)?;
        let (client_shutdown, _) = watch::channel(false);
        let state = Arc::new(RemoteServerState::new(settings, client_shutdown.clone()));
        let app = remote_router(state);
        let (shutdown, shutdown_rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .map_err(RemoteTextServerError::Serve)
        });
        Ok(Self {
            local_addr,
            shutdown: Some(shutdown),
            client_shutdown,
            task: Some(task),
        })
    }

    /// Binds on all IPv4 interfaces using the configured legacy port.
    pub async fn bind_configured(
        settings: RemoteTextServiceSettings,
    ) -> Result<Self, RemoteTextServerError> {
        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), settings.port);
        Self::bind(settings, bind_addr).await
    }

    /// Returns the actual listener address, including an operating-system-assigned test port.
    #[must_use]
    pub const fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Stops accepting clients, closes upgraded clients, and waits for the HTTP server task.
    pub async fn shutdown(mut self) -> Result<(), RemoteTextServerError> {
        let _ = self.client_shutdown.send(true);
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(task) = self.task.take() {
            task.await.map_err(RemoteTextServerError::Join)??;
        }
        Ok(())
    }
}

impl Drop for RemoteTextServer {
    fn drop(&mut self) {
        let _ = self.client_shutdown.send(true);
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// Errors returned while deriving or reconciling daemon-owned remote service state.
#[derive(Debug, Error)]
pub enum RemoteTextLifecycleError {
    /// Active-provider environment is invalid.
    #[error("resolve remote text service settings: {0}")]
    Settings(#[from] RemoteTextSettingsError),
    /// Starting, stopping, or joining the network runtime failed.
    #[error(transparent)]
    Server(#[from] RemoteTextServerError),
}

/// Redacted daemon-owned remote service state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteTextLifecycleStatus {
    /// Whether the service currently owns a listener.
    pub running: bool,
    /// Actual listener address when running.
    pub local_addr: Option<SocketAddr>,
}

struct ActiveRemoteTextServer {
    settings: RemoteTextServiceSettings,
    server: RemoteTextServer,
}

/// Owns one remote server and reconciles it against validated daemon config.
pub struct RemoteTextLifecycle {
    bind_ip: IpAddr,
    active: Option<ActiveRemoteTextServer>,
}

impl RemoteTextLifecycle {
    /// Creates an inactive lifecycle manager for the supplied bind address.
    #[must_use]
    pub const fn new(bind_ip: IpAddr) -> Self {
        Self {
            bind_ip,
            active: None,
        }
    }

    /// Starts, restarts, preserves, or stops the service to match `config`.
    ///
    /// Returns whether the owned network runtime changed.
    pub async fn reconcile_config(
        &mut self,
        config: &VinpstConfig,
    ) -> Result<bool, RemoteTextLifecycleError> {
        let desired = remote_text_settings(config)?;
        match (self.active.as_ref(), desired.as_ref()) {
            (None, None) => return Ok(false),
            (Some(active), Some(desired)) if active.settings == *desired => return Ok(false),
            _ => {}
        }

        self.stop().await?;
        let Some(settings) = desired else {
            return Ok(true);
        };
        let bind_addr = SocketAddr::new(self.bind_ip, settings.port);
        let server = RemoteTextServer::bind(settings.clone(), bind_addr).await?;
        self.active = Some(ActiveRemoteTextServer { settings, server });
        Ok(true)
    }

    /// Stops the active server, if any.
    pub async fn stop(&mut self) -> Result<bool, RemoteTextLifecycleError> {
        let Some(active) = self.active.take() else {
            return Ok(false);
        };
        active.server.shutdown().await?;
        Ok(true)
    }

    /// Returns redacted listener state without credentials or provider environment.
    #[must_use]
    pub fn status(&self) -> RemoteTextLifecycleStatus {
        RemoteTextLifecycleStatus {
            running: self.active.is_some(),
            local_addr: self
                .active
                .as_ref()
                .map(|active| active.server.local_addr()),
        }
    }

    /// Lists browser endpoints reachable through active non-loopback IPv4 interfaces.
    ///
    /// Interface enumeration is best-effort, matching the legacy daemon: an
    /// unavailable interface list produces no endpoints and does not affect the
    /// running listener.
    #[must_use]
    pub fn endpoints(&self) -> Vec<String> {
        let Some(active) = self.active.as_ref() else {
            return Vec::new();
        };
        let port = active.server.local_addr().port();
        match self.bind_ip {
            IpAddr::V4(ip) if ip.is_unspecified() => get_if_addrs().map_or_else(
                |_| Vec::new(),
                |interfaces| {
                    lan_http_endpoints(
                        port,
                        interfaces.into_iter().filter_map(|interface| {
                            let IpAddr::V4(ip) = interface.ip() else {
                                return None;
                            };
                            Some((ip, interface.is_oper_up(), interface.is_loopback()))
                        }),
                    )
                },
            ),
            IpAddr::V4(ip) if !ip.is_loopback() => {
                vec![format!("http://{ip}:{port}")]
            }
            _ => Vec::new(),
        }
    }
}

fn lan_http_endpoints(
    port: u16,
    interfaces: impl IntoIterator<Item = (Ipv4Addr, bool, bool)>,
) -> Vec<String> {
    let mut endpoints = interfaces
        .into_iter()
        .filter(|(_, is_up, is_loopback)| *is_up && !*is_loopback)
        .map(|(ip, _, _)| format!("http://{ip}:{port}"))
        .collect::<Vec<_>>();
    endpoints.sort_unstable();
    endpoints.dedup();
    endpoints
}

impl Default for RemoteTextLifecycle {
    fn default() -> Self {
        Self::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
    }
}

#[derive(Clone)]
struct RemoteClient {
    id: u64,
    sender: mpsc::UnboundedSender<Message>,
}

struct RemoteConnections {
    protocol: RemoteTextProtocol,
    input: Option<RemoteClient>,
    output: Option<RemoteClient>,
    next_client_id: u64,
    debounce_generation: u64,
}

impl Default for RemoteConnections {
    fn default() -> Self {
        Self {
            protocol: RemoteTextProtocol::default(),
            input: None,
            output: None,
            next_client_id: 1,
            debounce_generation: 0,
        }
    }
}

impl RemoteConnections {
    fn allocate_client_id(&mut self) -> u64 {
        let id = self.next_client_id;
        self.next_client_id = self.next_client_id.wrapping_add(1).max(1);
        id
    }

    fn dispatch(&mut self, effects: RemoteProtocolEffects) -> Option<u64> {
        send_json_values(self.input.as_ref(), effects.input_messages);
        send_json_values(self.output.as_ref(), effects.output_messages);
        match effects.debounce {
            RemoteDebounceAction::Unchanged => None,
            RemoteDebounceAction::Cancel => {
                self.debounce_generation = self.debounce_generation.wrapping_add(1);
                None
            }
            RemoteDebounceAction::Schedule => {
                self.debounce_generation = self.debounce_generation.wrapping_add(1);
                Some(self.debounce_generation)
            }
        }
    }
}

struct RemoteServerState {
    settings: RemoteTextServiceSettings,
    connections: Mutex<RemoteConnections>,
    shutdown: watch::Sender<bool>,
}

impl RemoteServerState {
    fn new(settings: RemoteTextServiceSettings, shutdown: watch::Sender<bool>) -> Self {
        Self {
            settings,
            connections: Mutex::new(RemoteConnections::default()),
            shutdown,
        }
    }

    async fn apply_protocol_transition(
        self: &Arc<Self>,
        transition: impl FnOnce(&mut RemoteTextProtocol) -> RemoteProtocolEffects,
    ) {
        let schedule_generation = {
            let mut connections = self.connections.lock().await;
            let effects = transition(&mut connections.protocol);
            connections.dispatch(effects)
        };
        if let Some(generation) = schedule_generation {
            self.spawn_debounce(generation);
        }
    }

    fn spawn_debounce(self: &Arc<Self>, generation: u64) {
        let state = Arc::clone(self);
        let delay = Duration::from_millis(self.settings.debounce_ms);
        let mut shutdown = self.shutdown.subscribe();
        tokio::spawn(async move {
            if *shutdown.borrow() {
                return;
            }
            tokio::select! {
                () = tokio::time::sleep(delay) => {}
                _ = shutdown.changed() => return,
            }
            let mut connections = state.connections.lock().await;
            if connections.debounce_generation != generation {
                return;
            }
            connections.debounce_generation = connections.debounce_generation.wrapping_add(1);
            let effects = connections.protocol.fire_debounce();
            connections.dispatch(effects);
        });
    }

    async fn disconnect_input(&self, client_id: u64) {
        let mut connections = self.connections.lock().await;
        if connections
            .input
            .as_ref()
            .is_none_or(|client| client.id != client_id)
        {
            return;
        }
        connections.input = None;
        let effects = connections.protocol.disconnect_input();
        connections.dispatch(effects);
    }

    async fn disconnect_output(&self, client_id: u64) {
        let mut connections = self.connections.lock().await;
        if connections
            .output
            .as_ref()
            .is_none_or(|client| client.id != client_id)
        {
            return;
        }
        connections.output = None;
        let effects = connections.protocol.disconnect_output();
        connections.dispatch(effects);
    }
}

fn remote_router(state: Arc<RemoteServerState>) -> Router {
    Router::new()
        .route("/", get(index_page))
        .route("/health", get(health))
        .route("/favicon.svg", get(favicon))
        .route("/ws", get(input_upgrade))
        .route("/v1/realtime", get(realtime_upgrade))
        .with_state(state)
}

async fn index_page() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn health() -> Json<Value> {
    Json(json!({"ok":true}))
}

async fn favicon() -> impl IntoResponse {
    (
        [
            (axum::http::header::CONTENT_TYPE, "image/svg+xml"),
            (axum::http::header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        FAVICON_SVG,
    )
}

async fn input_upgrade(
    State(state): State<Arc<RemoteServerState>>,
    websocket: WebSocketUpgrade,
) -> Response {
    websocket
        .max_message_size(MAX_WEBSOCKET_MESSAGE_BYTES)
        .max_frame_size(MAX_WEBSOCKET_MESSAGE_BYTES)
        .on_upgrade(move |socket| handle_input_socket(state, socket))
}

async fn realtime_upgrade(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    State(state): State<Arc<RemoteServerState>>,
    headers: HeaderMap,
    websocket: WebSocketUpgrade,
) -> Response {
    let bearer_token = bearer_token(&headers);
    if let Err(error) = state
        .settings
        .authorize_realtime(peer.ip(), bearer_token.as_deref())
    {
        return protocol_error_response(&error);
    }
    websocket
        .max_message_size(MAX_WEBSOCKET_MESSAGE_BYTES)
        .max_frame_size(MAX_WEBSOCKET_MESSAGE_BYTES)
        .on_upgrade(move |socket| handle_output_socket(state, socket))
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
}

fn protocol_error_response(error: &RemoteTextProtocolError) -> Response {
    match error {
        RemoteTextProtocolError::Unauthorized => {
            (StatusCode::UNAUTHORIZED, "Unauthorized.\n").into_response()
        }
        RemoteTextProtocolError::RealtimeEndpointLocalOnly => {
            (StatusCode::FORBIDDEN, "Realtime endpoint is local-only.\n").into_response()
        }
    }
}

async fn handle_input_socket(state: Arc<RemoteServerState>, socket: WebSocket) {
    let mut shutdown = state.shutdown.subscribe();
    if *shutdown.borrow() {
        return;
    }
    let (sink, mut stream) = socket.split();
    let (sender, receiver) = mpsc::unbounded_channel();
    let writer = tokio::spawn(websocket_writer(sink, receiver));

    let authentication = tokio::select! {
        _ = shutdown.changed() => None,
        message = stream.next() => message,
    };
    let Some(Ok(Message::Text(authentication))) = authentication else {
        drop(sender);
        let _ = writer.await;
        return;
    };
    let Ok(authentication) = serde_json::from_str::<Value>(authentication.as_str()) else {
        send_json(&sender, &json!({"type":"error","message":"Invalid JSON."}));
        drop(sender);
        let _ = writer.await;
        return;
    };
    let api_key = if authentication.get("type").and_then(Value::as_str) == Some("auth") {
        authentication
            .get("api_key")
            .and_then(Value::as_str)
            .unwrap_or_default()
    } else {
        ""
    };

    let (client_id, accepted, schedule_generation) = {
        let mut connections = state.connections.lock().await;
        let client_id = connections.allocate_client_id();
        let mut effects = connections.protocol.connect_input(&state.settings, api_key);
        let accepted = effects
            .input_messages
            .iter()
            .any(|message| message.get("type").and_then(Value::as_str) == Some("init"));
        send_json_values_to_sender(&sender, std::mem::take(&mut effects.input_messages));
        if accepted {
            connections.input = Some(RemoteClient {
                id: client_id,
                sender: sender.clone(),
            });
        }
        let schedule_generation = connections.dispatch(effects);
        (client_id, accepted, schedule_generation)
    };
    if let Some(generation) = schedule_generation {
        state.spawn_debounce(generation);
    }
    if !accepted {
        drop(sender);
        let _ = writer.await;
        return;
    }

    loop {
        let message = tokio::select! {
            _ = shutdown.changed() => break,
            message = stream.next() => message,
        };
        let Some(message) = message else {
            break;
        };
        let Ok(message) = message else {
            break;
        };
        match message {
            Message::Text(text) => {
                let Ok(event) = serde_json::from_str::<Value>(text.as_str()) else {
                    continue;
                };
                state
                    .apply_protocol_transition(|protocol| protocol.handle_input_event(&event))
                    .await;
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    state.disconnect_input(client_id).await;
    drop(sender);
    let _ = writer.await;
}

async fn handle_output_socket(state: Arc<RemoteServerState>, socket: WebSocket) {
    let mut shutdown = state.shutdown.subscribe();
    if *shutdown.borrow() {
        return;
    }
    let (sink, mut stream) = socket.split();
    let (sender, receiver) = mpsc::unbounded_channel();
    let writer = tokio::spawn(websocket_writer(sink, receiver));

    let (client_id, accepted, schedule_generation) = {
        let mut connections = state.connections.lock().await;
        let client_id = connections.allocate_client_id();
        let mut effects = connections.protocol.connect_output();
        let accepted = !effects
            .output_messages
            .iter()
            .any(|message| message.get("type").and_then(Value::as_str) == Some("error"));
        send_json_values_to_sender(&sender, std::mem::take(&mut effects.output_messages));
        if accepted {
            connections.output = Some(RemoteClient {
                id: client_id,
                sender: sender.clone(),
            });
        }
        let schedule_generation = connections.dispatch(effects);
        (client_id, accepted, schedule_generation)
    };
    if let Some(generation) = schedule_generation {
        state.spawn_debounce(generation);
    }
    if !accepted {
        drop(sender);
        let _ = writer.await;
        return;
    }

    loop {
        let message = tokio::select! {
            _ = shutdown.changed() => break,
            message = stream.next() => message,
        };
        let Some(message) = message else {
            break;
        };
        let Ok(message) = message else {
            break;
        };
        match message {
            Message::Text(text) => {
                let Ok(event) = serde_json::from_str::<Value>(text.as_str()) else {
                    continue;
                };
                state
                    .apply_protocol_transition(|protocol| protocol.handle_output_event(&event))
                    .await;
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    state.disconnect_output(client_id).await;
    drop(sender);
    let _ = writer.await;
}

async fn websocket_writer(
    mut sink: futures_util::stream::SplitSink<WebSocket, Message>,
    mut receiver: mpsc::UnboundedReceiver<Message>,
) {
    while let Some(message) = receiver.recv().await {
        if sink.send(message).await.is_err() {
            break;
        }
    }
    let _ = sink.close().await;
}

fn send_json_values(client: Option<&RemoteClient>, values: Vec<Value>) {
    let Some(client) = client else {
        return;
    };
    send_json_values_to_sender(&client.sender, values);
}

fn send_json_values_to_sender(sender: &mpsc::UnboundedSender<Message>, values: Vec<Value>) {
    for value in values {
        send_json(sender, &value);
    }
}

fn send_json(sender: &mpsc::UnboundedSender<Message>, value: &Value) {
    let _ = sender.send(Message::Text(value.to_string().into()));
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::lan_http_endpoints;

    #[test]
    fn lan_endpoints_match_legacy_ipv4_filtering_and_ordering() {
        let endpoints = lan_http_endpoints(
            8080,
            [
                (Ipv4Addr::new(192, 168, 1, 8), true, false),
                (Ipv4Addr::LOCALHOST, true, true),
                (Ipv4Addr::new(10, 0, 0, 4), false, false),
                (Ipv4Addr::new(192, 168, 1, 8), true, false),
                (Ipv4Addr::new(10, 0, 0, 9), true, false),
            ],
        );

        assert_eq!(
            endpoints,
            ["http://10.0.0.9:8080", "http://192.168.1.8:8080"]
        );
    }
}

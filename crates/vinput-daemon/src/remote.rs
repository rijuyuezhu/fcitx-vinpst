//! Deterministic protocol and configuration core for legacy remote text input.

mod server;

pub use server::{RemoteTextServer, RemoteTextServerError};

use std::{fmt, net::IpAddr};

use serde_json::{Value, json};
use thiserror::Error;
use vinput_config::{AsrProviderKind, VinputConfig};

/// Legacy command-provider id that enables the remote text service.
pub const REMOTE_TEXT_PROVIDER_ID: &str = "provider.vinput.remote.streaming";

const DEFAULT_PORT: u16 = 8080;
const DEFAULT_DEBOUNCE_MS: u64 = 1500;

/// Validated remote text service settings derived from the active provider.
#[derive(Clone, PartialEq, Eq)]
pub struct RemoteTextServiceSettings {
    /// TCP listen port.
    pub port: u16,
    /// Text-finalization debounce interval in milliseconds.
    pub debounce_ms: u64,
    api_key: String,
}

impl fmt::Debug for RemoteTextServiceSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RemoteTextServiceSettings")
            .field("port", &self.port)
            .field("debounce_ms", &self.debounce_ms)
            .field("api_key_configured", &!self.api_key.is_empty())
            .finish()
    }
}

impl RemoteTextServiceSettings {
    /// Compares an input API key without an early-exit byte mismatch.
    #[must_use]
    pub fn validates_api_key(&self, candidate: &str) -> bool {
        constant_time_eq(self.api_key.as_bytes(), candidate.as_bytes())
    }

    /// Validates the local-only `OpenAI` Realtime-compatible endpoint request.
    pub fn authorize_realtime(
        &self,
        peer_address: IpAddr,
        bearer_token: Option<&str>,
    ) -> Result<(), RemoteTextProtocolError> {
        if !peer_address.is_loopback() {
            return Err(RemoteTextProtocolError::RealtimeEndpointLocalOnly);
        }
        if !bearer_token.is_some_and(|token| self.validates_api_key(token)) {
            return Err(RemoteTextProtocolError::Unauthorized);
        }
        Ok(())
    }
}

/// Derives legacy remote text settings when its command provider is active.
pub fn remote_text_settings(
    config: &VinputConfig,
) -> Result<Option<RemoteTextServiceSettings>, RemoteTextSettingsError> {
    let Some(provider) = config
        .asr
        .providers
        .iter()
        .find(|provider| provider.id == config.asr.active_provider)
    else {
        return Ok(None);
    };
    if provider.kind != AsrProviderKind::Command || provider.id != REMOTE_TEXT_PROVIDER_ID {
        return Ok(None);
    }

    let explicit_port = trimmed_env(provider, "VINPUT_ASR_PORT");
    let port = if let Some(port) = explicit_port {
        parse_port(&port)?
    } else {
        trimmed_env(provider, "VINPUT_ASR_URL")
            .as_deref()
            .and_then(port_from_websocket_url)
            .unwrap_or(DEFAULT_PORT)
    };
    let debounce_ms = trimmed_env(provider, "VINPUT_ASR_DEBOUNCE_MS")
        .map_or(Ok(DEFAULT_DEBOUNCE_MS), |value| parse_debounce(&value))?;
    let api_key = trimmed_env(provider, "VINPUT_ASR_API_KEY")
        .filter(|value| !value.is_empty())
        .ok_or(RemoteTextSettingsError::MissingApiKey)?;

    Ok(Some(RemoteTextServiceSettings {
        port,
        debounce_ms,
        api_key,
    }))
}

fn trimmed_env(provider: &vinput_config::AsrProviderConfig, name: &str) -> Option<String> {
    provider.env.get(name).map(|value| value.trim().to_owned())
}

fn parse_port(value: &str) -> Result<u16, RemoteTextSettingsError> {
    value
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or_else(|| RemoteTextSettingsError::InvalidPort(value.trim().to_owned()))
}

fn parse_debounce(value: &str) -> Result<u64, RemoteTextSettingsError> {
    value
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| RemoteTextSettingsError::InvalidDebounce(value.trim().to_owned()))
}

fn port_from_websocket_url(url: &str) -> Option<u16> {
    let text = url.trim();
    let authority = text
        .split_once("://")?
        .1
        .split('/')
        .next()
        .unwrap_or_default();
    if authority.starts_with('[') {
        let close = authority.find(']')?;
        return authority.get(close + 1..)?.strip_prefix(':')?.parse().ok();
    }
    authority.rsplit_once(':')?.1.parse().ok()
}

fn constant_time_eq(expected: &[u8], candidate: &[u8]) -> bool {
    if expected.len() != candidate.len() {
        return false;
    }
    expected
        .iter()
        .zip(candidate)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

/// Settings extraction failures for the legacy remote provider environment.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RemoteTextSettingsError {
    /// Explicit port is outside 1..=65535 or is not an integer.
    #[error("VINPUT_ASR_PORT must be between 1 and 65535, got `{0}`")]
    InvalidPort(String),
    /// Debounce is not a positive integer.
    #[error("VINPUT_ASR_DEBOUNCE_MS must be a positive integer, got `{0}`")]
    InvalidDebounce(String),
    /// Remote input must be protected by an API key.
    #[error("remote ASR provider requires VINPUT_ASR_API_KEY")]
    MissingApiKey,
}

/// Debounce scheduling requested by one protocol transition.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RemoteDebounceAction {
    /// Leave the existing timer unchanged.
    #[default]
    Unchanged,
    /// Schedule or reschedule finalization.
    Schedule,
    /// Cancel a pending finalization.
    Cancel,
}

/// Messages and timer changes emitted by one deterministic protocol transition.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RemoteProtocolEffects {
    /// JSON messages for the browser/input client.
    pub input_messages: Vec<Value>,
    /// JSON messages for the `OpenAI` Realtime-compatible output client.
    pub output_messages: Vec<Value>,
    /// Debounce timer change for the runtime layer.
    pub debounce: RemoteDebounceAction,
}

/// Protocol-level authorization and connection errors.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RemoteTextProtocolError {
    /// API key validation failed.
    #[error("unauthorized")]
    Unauthorized,
    /// The Realtime endpoint is intentionally restricted to loopback peers.
    #[error("realtime endpoint is local-only")]
    RealtimeEndpointLocalOnly,
}

/// Network-independent state machine for browser input and Realtime output.
#[derive(Debug, Default)]
pub struct RemoteTextProtocol {
    input_connected: bool,
    output_connected: bool,
    current_text: String,
    next_event_id: u64,
}

impl RemoteTextProtocol {
    /// Authenticates and claims the single browser/input connection.
    pub fn connect_input(
        &mut self,
        settings: &RemoteTextServiceSettings,
        api_key: &str,
    ) -> RemoteProtocolEffects {
        if !settings.validates_api_key(api_key) {
            return RemoteProtocolEffects {
                input_messages: vec![json!({"type":"error","message":"Unauthorized."})],
                ..RemoteProtocolEffects::default()
            };
        }
        let mut input_messages = vec![json!({"type":"auth_ok"})];
        if self.input_connected {
            input_messages.push(json!({
                "type":"error",
                "message":"Input client already connected."
            }));
            return RemoteProtocolEffects {
                input_messages,
                ..RemoteProtocolEffects::default()
            };
        }
        self.input_connected = true;
        input_messages.push(json!({
            "type":"init",
            "output_status": if self.output_connected { "connected" } else { "disconnected" }
        }));
        RemoteProtocolEffects {
            input_messages,
            ..RemoteProtocolEffects::default()
        }
    }

    /// Claims the single Realtime output connection after HTTP authorization.
    pub fn connect_output(&mut self) -> RemoteProtocolEffects {
        if self.output_connected {
            let event_id = self.new_id("event");
            return RemoteProtocolEffects {
                output_messages: vec![json!({
                    "event_id": event_id,
                    "type":"error",
                    "error":{"message":"Output client already connected."}
                })],
                ..RemoteProtocolEffects::default()
            };
        }
        self.output_connected = true;
        RemoteProtocolEffects {
            input_messages: self
                .input_connected
                .then(|| json!({"type":"output_connected"}))
                .into_iter()
                .collect(),
            ..RemoteProtocolEffects::default()
        }
    }

    /// Releases the browser/input connection and cancels its pending debounce.
    pub fn disconnect_input(&mut self) -> RemoteProtocolEffects {
        self.input_connected = false;
        RemoteProtocolEffects {
            debounce: RemoteDebounceAction::Cancel,
            ..RemoteProtocolEffects::default()
        }
    }

    /// Releases the output connection, clears pending text, and notifies input.
    pub fn disconnect_output(&mut self) -> RemoteProtocolEffects {
        self.output_connected = false;
        self.current_text.clear();
        RemoteProtocolEffects {
            input_messages: self
                .input_connected
                .then(|| json!({"type":"output_disconnected"}))
                .into_iter()
                .collect(),
            debounce: RemoteDebounceAction::Cancel,
            ..RemoteProtocolEffects::default()
        }
    }

    /// Applies one parsed browser/input JSON event.
    pub fn handle_input_event(&mut self, event: &Value) -> RemoteProtocolEffects {
        match event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "text_update" => {
                event
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .clone_into(&mut self.current_text);
                RemoteProtocolEffects {
                    debounce: RemoteDebounceAction::Schedule,
                    ..RemoteProtocolEffects::default()
                }
            }
            "finalize" => {
                let mut effects = self.final_result_effects();
                effects.debounce = RemoteDebounceAction::Cancel;
                effects
            }
            _ => RemoteProtocolEffects::default(),
        }
    }

    /// Applies one parsed `OpenAI` Realtime-compatible JSON event.
    pub fn handle_output_event(&mut self, event: &Value) -> RemoteProtocolEffects {
        match event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "session.update" => {
                let event_id = self.new_id("event");
                RemoteProtocolEffects {
                    output_messages: vec![json!({
                        "event_id": event_id,
                        "type":"session.updated",
                        "session": event.get("session").cloned().unwrap_or_else(|| json!({}))
                    })],
                    ..RemoteProtocolEffects::default()
                }
            }
            "input_audio_buffer.commit" if self.current_text.is_empty() => {
                self.empty_commit_effects()
            }
            "input_audio_buffer.commit" => {
                let mut effects = self.final_result_effects();
                effects.debounce = RemoteDebounceAction::Cancel;
                effects
            }
            _ => RemoteProtocolEffects::default(),
        }
    }

    /// Fires the runtime debounce deadline.
    pub fn fire_debounce(&mut self) -> RemoteProtocolEffects {
        self.final_result_effects()
    }

    /// Current uncommitted browser text, exposed for diagnostics and tests.
    #[must_use]
    pub fn current_text(&self) -> &str {
        &self.current_text
    }

    fn empty_commit_effects(&mut self) -> RemoteProtocolEffects {
        let event_id = self.new_id("event");
        let item_id = self.new_id("item");
        RemoteProtocolEffects {
            output_messages: vec![json!({
                "event_id": event_id,
                "type":"input_audio_buffer.committed",
                "item_id": item_id
            })],
            ..RemoteProtocolEffects::default()
        }
    }

    fn final_result_effects(&mut self) -> RemoteProtocolEffects {
        if !self.output_connected || self.current_text.is_empty() {
            return RemoteProtocolEffects::default();
        }
        let text = std::mem::take(&mut self.current_text);
        let item_id = self.new_id("item");
        let committed_event_id = self.new_id("event");
        let delta_event_id = self.new_id("event");
        let completed_event_id = self.new_id("event");
        RemoteProtocolEffects {
            output_messages: vec![
                json!({
                    "event_id": committed_event_id,
                    "type":"input_audio_buffer.committed",
                    "item_id": item_id
                }),
                json!({
                    "event_id": delta_event_id,
                    "type":"conversation.item.input_audio_transcription.delta",
                    "item_id": item_id,
                    "delta": text
                }),
                json!({
                    "event_id": completed_event_id,
                    "type":"conversation.item.input_audio_transcription.completed",
                    "item_id": item_id,
                    "transcript": text
                }),
            ],
            ..RemoteProtocolEffects::default()
        }
    }

    fn new_id(&mut self, prefix: &str) -> String {
        self.next_event_id += 1;
        format!("{prefix}_{}", self.next_event_id)
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use serde_json::json;
    use vinput_config::VinputConfig;

    use super::{
        REMOTE_TEXT_PROVIDER_ID, RemoteDebounceAction, RemoteTextProtocol, RemoteTextProtocolError,
        RemoteTextSettingsError, remote_text_settings,
    };

    fn config_with_remote_env(env: &serde_json::Value) -> VinputConfig {
        VinputConfig::from_json_str(
            &serde_json::to_string(&json!({
                "version":1,
                "asr":{
                    "active_provider":REMOTE_TEXT_PROVIDER_ID,
                    "providers":[{
                        "id":REMOTE_TEXT_PROVIDER_ID,
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
            .expect("serialize remote config"),
        )
        .expect("parse remote config")
    }

    #[test]
    fn settings_are_disabled_for_other_active_providers() {
        let config = VinputConfig::bundled_default().expect("parse bundled config");
        assert_eq!(remote_text_settings(&config).unwrap(), None);
    }

    #[test]
    fn settings_apply_legacy_defaults_and_redact_debug() {
        let config = config_with_remote_env(&json!({"VINPUT_ASR_API_KEY":"fixture-key"}));
        let settings = remote_text_settings(&config).unwrap().unwrap();
        assert_eq!(settings.port, 8080);
        assert_eq!(settings.debounce_ms, 1500);
        assert!(settings.validates_api_key("fixture-key"));
        assert!(!settings.validates_api_key("wrong"));
        let debug = format!("{settings:?}");
        assert!(debug.contains("api_key_configured: true"));
        assert!(!debug.contains("fixture-key"));
    }

    #[test]
    fn settings_accept_explicit_and_url_derived_ports() {
        let explicit = config_with_remote_env(&json!({
            "VINPUT_ASR_API_KEY":"key",
            "VINPUT_ASR_PORT":"9001",
            "VINPUT_ASR_URL":"ws://127.0.0.1:9002/v1/realtime",
            "VINPUT_ASR_DEBOUNCE_MS":"250"
        }));
        let settings = remote_text_settings(&explicit).unwrap().unwrap();
        assert_eq!(settings.port, 9001);
        assert_eq!(settings.debounce_ms, 250);

        let derived = config_with_remote_env(&json!({
            "VINPUT_ASR_API_KEY":"key",
            "VINPUT_ASR_URL":"ws://[::1]:9010/v1/realtime"
        }));
        assert_eq!(remote_text_settings(&derived).unwrap().unwrap().port, 9010);
    }

    #[test]
    fn settings_reject_invalid_environment() {
        let missing_key = config_with_remote_env(&json!({}));
        assert_eq!(
            remote_text_settings(&missing_key).unwrap_err(),
            RemoteTextSettingsError::MissingApiKey
        );
        let invalid_port = config_with_remote_env(&json!({
            "VINPUT_ASR_API_KEY":"key",
            "VINPUT_ASR_PORT":"70000"
        }));
        assert!(matches!(
            remote_text_settings(&invalid_port),
            Err(RemoteTextSettingsError::InvalidPort(_))
        ));
        let invalid_debounce = config_with_remote_env(&json!({
            "VINPUT_ASR_API_KEY":"key",
            "VINPUT_ASR_DEBOUNCE_MS":"0"
        }));
        assert!(matches!(
            remote_text_settings(&invalid_debounce),
            Err(RemoteTextSettingsError::InvalidDebounce(_))
        ));
    }

    #[test]
    fn realtime_authorization_is_loopback_and_bearer_protected() {
        let config = config_with_remote_env(&json!({"VINPUT_ASR_API_KEY":"key"}));
        let settings = remote_text_settings(&config).unwrap().unwrap();
        assert_eq!(
            settings.authorize_realtime(IpAddr::V4(Ipv4Addr::LOCALHOST), Some("wrong")),
            Err(RemoteTextProtocolError::Unauthorized)
        );
        assert_eq!(
            settings.authorize_realtime(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 3)), Some("key")),
            Err(RemoteTextProtocolError::RealtimeEndpointLocalOnly)
        );
        settings
            .authorize_realtime(IpAddr::V4(Ipv4Addr::LOCALHOST), Some("key"))
            .unwrap();
    }

    #[test]
    fn input_authentication_and_output_connection_match_legacy_messages() {
        let config = config_with_remote_env(&json!({"VINPUT_ASR_API_KEY":"key"}));
        let settings = remote_text_settings(&config).unwrap().unwrap();
        let mut protocol = RemoteTextProtocol::default();

        let unauthorized = protocol.connect_input(&settings, "bad");
        assert_eq!(unauthorized.input_messages[0]["message"], "Unauthorized.");

        let connected = protocol.connect_input(&settings, "key");
        assert_eq!(connected.input_messages[0]["type"], "auth_ok");
        assert_eq!(connected.input_messages[1]["output_status"], "disconnected");
        let output = protocol.connect_output();
        assert_eq!(output.input_messages[0]["type"], "output_connected");

        let duplicate_input = protocol.connect_input(&settings, "key");
        assert_eq!(duplicate_input.input_messages.len(), 2);
        assert_eq!(
            duplicate_input.input_messages[1]["message"],
            "Input client already connected."
        );
        let duplicate_output = protocol.connect_output();
        assert_eq!(duplicate_output.output_messages[0]["type"], "error");
    }

    #[test]
    fn text_update_and_debounce_emit_realtime_transcription_events() {
        let mut protocol = RemoteTextProtocol::default();
        protocol.connect_output();
        let update = protocol.handle_input_event(&json!({
            "type":"text_update",
            "text":"hello remote"
        }));
        assert_eq!(update.debounce, RemoteDebounceAction::Schedule);
        assert_eq!(protocol.current_text(), "hello remote");

        let result = protocol.fire_debounce();
        assert_eq!(result.output_messages.len(), 3);
        assert_eq!(
            result.output_messages[0]["type"],
            "input_audio_buffer.committed"
        );
        assert_eq!(
            result.output_messages[1]["type"],
            "conversation.item.input_audio_transcription.delta"
        );
        assert_eq!(result.output_messages[1]["delta"], "hello remote");
        assert_eq!(
            result.output_messages[2]["type"],
            "conversation.item.input_audio_transcription.completed"
        );
        assert_eq!(result.output_messages[2]["transcript"], "hello remote");
        assert!(protocol.current_text().is_empty());
    }

    #[test]
    fn finalize_and_realtime_commit_cover_nonempty_and_empty_text() {
        let mut protocol = RemoteTextProtocol::default();
        protocol.connect_output();
        protocol.handle_input_event(&json!({"type":"text_update","text":"final"}));
        let finalized = protocol.handle_input_event(&json!({"type":"finalize"}));
        assert_eq!(finalized.debounce, RemoteDebounceAction::Cancel);
        assert_eq!(finalized.output_messages.len(), 3);

        let empty = protocol.handle_output_event(&json!({"type":"input_audio_buffer.commit"}));
        assert_eq!(empty.output_messages.len(), 1);
        assert_eq!(
            empty.output_messages[0]["type"],
            "input_audio_buffer.committed"
        );
    }

    #[test]
    fn session_update_is_echoed_and_output_disconnect_clears_state() {
        let config = config_with_remote_env(&json!({"VINPUT_ASR_API_KEY":"key"}));
        let settings = remote_text_settings(&config).unwrap().unwrap();
        let mut protocol = RemoteTextProtocol::default();
        protocol.connect_input(&settings, "key");
        protocol.connect_output();
        protocol.handle_input_event(&json!({"type":"text_update","text":"pending"}));

        let session = protocol.handle_output_event(&json!({
            "type":"session.update",
            "session":{"input_audio_format":"pcm16"}
        }));
        assert_eq!(session.output_messages[0]["type"], "session.updated");
        assert_eq!(
            session.output_messages[0]["session"]["input_audio_format"],
            "pcm16"
        );

        let disconnected = protocol.disconnect_output();
        assert_eq!(disconnected.debounce, RemoteDebounceAction::Cancel);
        assert_eq!(
            disconnected.input_messages[0]["type"],
            "output_disconnected"
        );
        assert!(protocol.current_text().is_empty());
    }
}

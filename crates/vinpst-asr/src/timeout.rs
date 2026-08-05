//! ASR timeout capability diagnostics.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vinpst_config::{AsrConfig, AsrProviderKind};

/// Whether a configured ASR timeout is enforced by the selected backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AsrTimeoutEnforcement {
    /// The active provider does not configure a timeout.
    NotConfigured,
    /// The backend can terminate its process when the deadline expires.
    Enforced,
    /// The value is retained for compatibility but the runtime cannot cancel native decode.
    Unsupported,
}

/// Non-mutating diagnostic view of active-provider timeout behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AsrTimeoutProbe {
    /// Active provider id from config.
    pub provider_id: String,
    /// Active provider kind, when the provider exists.
    pub provider_kind: Option<AsrProviderKind>,
    /// Configured timeout in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Effective timeout enforcement classification.
    pub enforcement: AsrTimeoutEnforcement,
    /// Stable human-readable explanation.
    pub reason: String,
}

impl AsrTimeoutProbe {
    /// Inspects timeout behavior for the active ASR provider.
    #[must_use]
    pub fn inspect(config: &AsrConfig) -> Self {
        let provider = config
            .providers
            .iter()
            .find(|provider| provider.id == config.active_provider);
        let provider_kind = provider.map(|provider| provider.kind.clone());
        let timeout_ms = provider.and_then(|provider| provider.timeout_ms);
        let (enforcement, reason) = match (provider_kind.as_ref(), timeout_ms) {
            (_, None) => (
                AsrTimeoutEnforcement::NotConfigured,
                "active ASR provider does not configure timeout_ms".to_owned(),
            ),
            (Some(AsrProviderKind::Command), Some(timeout_ms)) => (
                AsrTimeoutEnforcement::Enforced,
                format!(
                    "command ASR helper is terminated when its {timeout_ms} ms deadline expires"
                ),
            ),
            (Some(AsrProviderKind::Local), Some(timeout_ms)) => (
                AsrTimeoutEnforcement::Unsupported,
                format!(
                    "native sherpa decode is synchronous and cannot be safely cancelled; configured {timeout_ms} ms is diagnostic-only"
                ),
            ),
            (Some(AsrProviderKind::Remote), Some(timeout_ms)) => (
                AsrTimeoutEnforcement::Enforced,
                format!(
                    "remote ASR HTTP request is cancelled when its {timeout_ms} ms deadline expires"
                ),
            ),
            (None, Some(_)) => unreachable!("missing provider cannot expose timeout_ms"),
        };
        Self {
            provider_id: config.active_provider.clone(),
            provider_kind,
            timeout_ms,
            enforcement,
            reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{AsrTimeoutEnforcement, AsrTimeoutProbe};
    use vinpst_config::{AsrConfig, AsrProviderConfig, AsrProviderKind};

    fn config(kind: AsrProviderKind, timeout_ms: Option<u64>) -> AsrConfig {
        AsrConfig {
            active_provider: "active".to_owned(),
            providers: vec![AsrProviderConfig {
                id: "active".to_owned(),
                kind,
                timeout_ms,
                model: None,
                hotwords_file: None,
                command: None,
                args: Vec::new(),
                env: HashMap::new(),
                endpoint: None,
            }],
            ..AsrConfig::default()
        }
    }

    #[test]
    fn command_timeout_is_enforced() {
        let probe = AsrTimeoutProbe::inspect(&config(AsrProviderKind::Command, Some(250)));
        assert_eq!(probe.enforcement, AsrTimeoutEnforcement::Enforced);
        assert_eq!(probe.timeout_ms, Some(250));
        assert!(probe.reason.contains("terminated"));
    }

    #[test]
    fn native_timeout_is_explicitly_diagnostic_only() {
        let probe = AsrTimeoutProbe::inspect(&config(AsrProviderKind::Local, Some(250)));
        assert_eq!(probe.enforcement, AsrTimeoutEnforcement::Unsupported);
        assert!(probe.reason.contains("synchronous"));
        assert!(probe.reason.contains("diagnostic-only"));
    }

    #[test]
    fn remote_timeout_is_enforced_by_http_request() {
        let probe = AsrTimeoutProbe::inspect(&config(AsrProviderKind::Remote, Some(250)));
        assert_eq!(probe.enforcement, AsrTimeoutEnforcement::Enforced);
        assert_eq!(probe.timeout_ms, Some(250));
        assert!(probe.reason.contains("HTTP request"));
        assert!(probe.reason.contains("deadline"));
    }

    #[test]
    fn absent_timeout_is_not_reported_as_unsupported() {
        let probe = AsrTimeoutProbe::inspect(&config(AsrProviderKind::Local, None));
        assert_eq!(probe.enforcement, AsrTimeoutEnforcement::NotConfigured);
        assert_eq!(probe.timeout_ms, None);
    }
}

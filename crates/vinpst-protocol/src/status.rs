//! Service status strings shared over D-Bus.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use strum::EnumString;

/// Daemon lifecycle state exposed to the Fcitx5 frontend.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, EnumString,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum ServiceStatus {
    /// No active recording or inference work.
    Idle,
    /// Audio capture is active.
    Recording,
    /// ASR backend is producing text.
    Inferring,
    /// Optional post-processing is applying scene/LLM rules.
    Postprocessing,
    /// The daemon is in an error state.
    Error,
}

impl ServiceStatus {
    /// Status values in legacy protocol order.
    pub const ALL: [Self; 5] = [
        Self::Idle,
        Self::Recording,
        Self::Inferring,
        Self::Postprocessing,
        Self::Error,
    ];

    /// Wire-format status strings in legacy protocol order.
    pub const WIRE_VALUES: [&'static str; 5] =
        ["idle", "recording", "inferring", "postprocessing", "error"];

    /// Returns the wire-format string used by the legacy daemon.
    #[must_use]
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Recording => "recording",
            Self::Inferring => "inferring",
            Self::Postprocessing => "postprocessing",
            Self::Error => "error",
        }
    }

    /// Parses a wire-format status string and rejects unknown values.
    ///
    /// Rust-side consumers use this strict parser so a daemon protocol drift is
    /// not silently interpreted as an idle service.
    pub fn parse_wire(input: &str) -> Result<Self, strum::ParseError> {
        Self::from_str(input)
    }

    /// Parses a wire-format status using the frozen C++ fallback semantics.
    ///
    /// The legacy `StringToStatus` helper maps every unknown string to `idle`.
    /// Compatibility callers that intentionally need that behavior should use
    /// this method instead of weakening [`Self::parse_wire`].
    #[must_use]
    pub fn parse_legacy_wire(input: &str) -> Self {
        Self::parse_wire(input).unwrap_or(Self::Idle)
    }
}

impl fmt::Display for ServiceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire_str())
    }
}

#[cfg(test)]
mod tests {
    use super::ServiceStatus;

    #[test]
    fn status_roundtrips_as_legacy_lowercase_strings() {
        assert_eq!(
            ServiceStatus::WIRE_VALUES,
            ["idle", "recording", "inferring", "postprocessing", "error"]
        );

        for status in ServiceStatus::ALL {
            let json = serde_json::to_string(&status).unwrap();
            assert_eq!(json, format!("\"{}\"", status.as_wire_str()));
            assert_eq!(
                ServiceStatus::parse_wire(status.as_wire_str()).unwrap(),
                status
            );
            assert_eq!(
                ServiceStatus::parse_legacy_wire(status.as_wire_str()),
                status
            );
        }
    }

    #[test]
    fn legacy_status_parser_falls_back_to_idle_for_unknown_values() {
        assert!(ServiceStatus::parse_wire("future-status").is_err());
        assert_eq!(
            ServiceStatus::parse_legacy_wire("future-status"),
            ServiceStatus::Idle
        );
        assert_eq!(ServiceStatus::parse_legacy_wire(""), ServiceStatus::Idle);
    }
}

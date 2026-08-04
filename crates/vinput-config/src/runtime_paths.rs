//! Shared runtime path resolution used by daemon backends and management clients.

use std::{ffi::OsString, path::PathBuf};

/// Environment variable overriding the sherpa model root.
pub const SHERPA_MODEL_ROOT_ENV: &str = "VINPUT_SHERPA_MODEL_ROOT";

/// Returns the effective sherpa model root used by the native ASR runtime.
#[must_use]
pub fn sherpa_model_root() -> PathBuf {
    sherpa_model_root_from(std::env::var_os(SHERPA_MODEL_ROOT_ENV))
}

fn sherpa_model_root_from(value: Option<OsString>) -> PathBuf {
    value.map_or_else(|| PathBuf::from("."), PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sherpa_model_root_defaults_to_current_directory() {
        assert_eq!(sherpa_model_root_from(None), PathBuf::from("."));
    }

    #[test]
    fn sherpa_model_root_preserves_environment_override() {
        assert_eq!(
            sherpa_model_root_from(Some(OsString::from("/srv/vinput-models"))),
            PathBuf::from("/srv/vinput-models")
        );
    }
}

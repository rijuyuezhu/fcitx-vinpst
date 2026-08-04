//! Retry state and validation for hotword files saved before daemon activation failed.

use std::{fmt, path::PathBuf};

use crate::{
    ConfigDocument, ensure_config_document_current,
    hotword_path::resolved_hotword_content_path,
    hotword_persistence::{HotwordContentSnapshot, read_hotword_snapshot},
    reload_asr_backend_and_wait,
};

#[derive(Clone, PartialEq, Eq)]
enum PendingHotwordTarget {
    ConfigOnly {
        configured_path: Option<String>,
    },
    File {
        path: PathBuf,
        baseline: HotwordContentSnapshot,
    },
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct PendingHotwordActivation {
    provider_id: String,
    target: PendingHotwordTarget,
}

impl PendingHotwordActivation {
    pub(super) fn for_config(provider_id: String, configured_path: Option<String>) -> Self {
        Self {
            provider_id,
            target: PendingHotwordTarget::ConfigOnly { configured_path },
        }
    }

    pub(super) fn for_file(
        provider_id: String,
        path: PathBuf,
        baseline: HotwordContentSnapshot,
    ) -> Self {
        Self {
            provider_id,
            target: PendingHotwordTarget::File { path, baseline },
        }
    }

    pub(super) fn matches_provider(&self, provider_id: &str) -> bool {
        self.provider_id == provider_id
    }

    pub(super) fn matches_loaded_file(
        &self,
        provider_id: &str,
        path: &PathBuf,
        baseline: &HotwordContentSnapshot,
    ) -> bool {
        if self.provider_id != provider_id {
            return false;
        }
        match &self.target {
            PendingHotwordTarget::ConfigOnly { .. } => true,
            PendingHotwordTarget::File {
                path: pending_path,
                baseline: pending_baseline,
            } => pending_path == path && pending_baseline == baseline,
        }
    }
}

impl fmt::Debug for PendingHotwordActivation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PendingHotwordActivation")
            .field("provider_id", &self.provider_id)
            .field(
                "target",
                &match self.target {
                    PendingHotwordTarget::ConfigOnly { .. } => "config",
                    PendingHotwordTarget::File { .. } => "file",
                },
            )
            .finish()
    }
}

pub(super) fn retry_hotword_activation(
    document: &ConfigDocument,
    pending: &PendingHotwordActivation,
) -> Result<String, String> {
    validate_pending_activation(document, pending)?;
    let summary = reload_asr_backend_and_wait(&pending.provider_id)?;
    validate_pending_activation(document, pending)?;
    Ok(format!("Hotword activation retried; {summary}."))
}

pub(super) fn validate_pending_activation(
    document: &ConfigDocument,
    pending: &PendingHotwordActivation,
) -> Result<(), String> {
    ensure_config_document_current(document)?;
    if document.config.asr.active_provider != pending.provider_id {
        return Err(
            "The active ASR provider changed after the hotword file was saved; save or load the current provider before retrying activation."
                .to_owned(),
        );
    }
    match &pending.target {
        PendingHotwordTarget::ConfigOnly { configured_path } => {
            let current = configured_hotword_value(&document.config, &pending.provider_id)?;
            if &current != configured_path {
                return Err(
                    "The configured hotword path changed after activation became pending; save the current provider before retrying activation."
                        .to_owned(),
                );
            }
        }
        PendingHotwordTarget::File { path, baseline } => {
            let current_path =
                resolved_hotword_content_path(&document.config, &pending.provider_id)?;
            if current_path.as_deref() != Some(path) {
                return Err(
                    "The configured hotword target changed after the file was saved; reload configuration and content before retrying activation."
                        .to_owned(),
                );
            }
            let current = read_hotword_snapshot(path)?;
            if &current != baseline {
                return Err(
                    "The saved hotword file changed before activation could be retried; reload its content before continuing."
                        .to_owned(),
                );
            }
        }
    }
    Ok(())
}

fn configured_hotword_value(
    config: &vinput_config::VinputConfig,
    provider_id: &str,
) -> Result<Option<String>, String> {
    let provider = config
        .asr
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| format!("ASR provider `{provider_id}` is no longer configured."))?;
    Ok(provider
        .hotwords_file
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use vinput_config::VinputConfig;

    use super::*;

    #[test]
    fn retry_validation_requires_current_config_target_and_file_version() {
        let directory = tempfile::tempdir().expect("temp dir");
        let config_path = directory.path().join("config.json");
        let hotword_path = directory.path().join("hotwords.txt");
        fs::write(&hotword_path, "alpha\n").expect("hotword fixture");

        let mut config = VinputConfig::bundled_default().expect("bundled config");
        config.asr.providers[0].hotwords_file = Some(hotword_path.to_string_lossy().into_owned());
        fs::write(
            &config_path,
            serde_json::to_vec_pretty(&config).expect("serialize config"),
        )
        .expect("config fixture");
        let document = ConfigDocument {
            path: config_path.clone(),
            from_disk: true,
            config: config.clone(),
        };
        let pending = PendingHotwordActivation::for_file(
            config.asr.active_provider.clone(),
            hotword_path.clone(),
            read_hotword_snapshot(&hotword_path).expect("baseline"),
        );
        validate_pending_activation(&document, &pending).expect("current retry state");
        let config_only = PendingHotwordActivation::for_config(
            document.config.asr.active_provider.clone(),
            configured_hotword_value(&document.config, &document.config.asr.active_provider)
                .expect("configured target"),
        );
        validate_pending_activation(&document, &config_only)
            .expect("current config-only retry state");

        fs::write(&hotword_path, "external\n").expect("external hotword update");
        assert!(
            validate_pending_activation(&document, &pending)
                .expect_err("reject external file update")
                .contains("changed before activation")
        );
        validate_pending_activation(&document, &config_only)
            .expect("config-only retry does not claim file content");

        fs::write(&hotword_path, "alpha\n").expect("restore text with new version");
        let mut superseding = config;
        superseding.asr.providers[0].hotwords_file = Some(
            directory
                .path()
                .join("other.txt")
                .to_string_lossy()
                .into_owned(),
        );
        fs::write(
            &config_path,
            serde_json::to_vec_pretty(&superseding).expect("serialize superseding config"),
        )
        .expect("superseding config");
        assert!(
            validate_pending_activation(&document, &pending)
                .expect_err("reject superseding config")
                .contains("changed on disk")
        );
        assert!(
            validate_pending_activation(&document, &config_only)
                .expect_err("config-only retry also rejects superseding config")
                .contains("changed on disk")
        );

        let superseding_document = ConfigDocument {
            path: config_path,
            from_disk: true,
            config: superseding,
        };
        assert!(
            validate_pending_activation(&superseding_document, &config_only)
                .expect_err("config-only retry rejects a new current target")
                .contains("configured hotword path changed")
        );
    }
}

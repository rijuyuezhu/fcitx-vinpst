//! Hotword filesystem path resolution shared by GUI lifecycle operations.

use std::path::{Path, PathBuf};

use vinput_config::{AsrProviderKind, VinputConfig};

pub(super) fn resolved_hotword_content_path(
    config: &VinputConfig,
    provider_id: &str,
) -> Result<Option<PathBuf>, String> {
    let provider = config
        .asr
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| format!("ASR provider `{provider_id}` is no longer configured."))?;
    let Some(configured) = provider
        .hotwords_file
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
    else {
        return Ok(None);
    };
    reject_url_like_hotword_path(configured)?;
    let configured = Path::new(configured);
    if configured.is_absolute() {
        return Ok(Some(configured.to_path_buf()));
    }
    match provider.kind {
        AsrProviderKind::Local => resolve_local_hotword_path(provider.model.as_deref(), configured),
        AsrProviderKind::Command => Err(
            "Relative hotword paths for command providers are resolved by the external command; configure an absolute path to edit content in the GUI."
                .to_owned(),
        ),
        AsrProviderKind::Remote => Err(format!(
            "ASR provider `{provider_id}` does not support hotword files."
        )),
    }
}

pub(super) fn reject_url_like_hotword_path(value: &str) -> Result<(), String> {
    if value.contains("://") {
        return Err(
            "The selected provider hotword value is URL-like, not a filesystem path; configure a local file path before editing content in the GUI."
                .to_owned(),
        );
    }
    Ok(())
}

fn resolve_local_hotword_path(
    configured_model: Option<&str>,
    configured_hotwords: &Path,
) -> Result<Option<PathBuf>, String> {
    let model = configured_model
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .ok_or_else(|| {
            "The selected local provider has no model directory for resolving its relative hotword path."
                .to_owned()
        })?;
    if model.contains("://") {
        return Err(
            "The selected local provider model is not a filesystem path, so the GUI cannot resolve its relative hotword path."
                .to_owned(),
        );
    }
    let model = Path::new(model);
    if !model.is_absolute() {
        return Err(
            "The selected local provider uses both a relative model path and a relative hotword path. Their effective target depends on the daemon process environment and working directory; configure an absolute hotword path or an absolute model path before editing content in the GUI."
                .to_owned(),
        );
    }
    if !model.is_dir() {
        return Err(
            "The selected local provider model directory does not exist or is not a directory; install or correct the model before editing a relative hotword path in the GUI."
                .to_owned(),
        );
    }
    Ok(Some(model.join(configured_hotwords)))
}

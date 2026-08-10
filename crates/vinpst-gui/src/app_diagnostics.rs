//! Redacted application diagnostics and headless package-check projection.

use std::{fmt, path::Path};

use serde_json::{Value, json};

use crate::{
    App, DaemonLoadState, GuiLocale, OperationState, Page, ResourceSelection, interaction,
    load_config_document, query_daemon_snapshot,
};

impl fmt::Debug for App {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let config_state = match &self.config {
            Ok(document) if document.from_disk => "disk",
            Ok(_) => "bundled",
            Err(_) => "invalid",
        };
        let draft_dirty = matches!(
            (&self.config, &self.draft),
            (Ok(document), Some(draft)) if draft.is_dirty(&document.config)
        );
        let daemon_state = match self.daemon {
            DaemonLoadState::Loading => "loading",
            DaemonLoadState::Stopped => "stopped",
            DaemonLoadState::Ready(_) => "ready",
            DaemonLoadState::Failed(_) => "failed",
        };
        let operation_state = match self.operation {
            OperationState::Idle => "idle",
            OperationState::Running(_) => "running",
            OperationState::Succeeded(_) => "succeeded",
            OperationState::Failed(_) => "failed",
        };
        let selected_resource = self
            .selected_resource
            .as_ref()
            .map(|selection| match selection {
                ResourceSelection::InstalledModel(_) => "installed-model",
                ResourceSelection::AsrProvider(_) => "asr-provider",
                ResourceSelection::LlmProvider(_) => "llm-provider",
                ResourceSelection::LlmAdapter(_) => "llm-adapter",
            });

        formatter
            .debug_struct("App")
            .field("locale", &self.locale)
            .field("page", &self.page)
            .field("filter_len", &self.filter.len())
            .field("model_filter_len", &self.model_filter.len())
            .field("config_state", &config_state)
            .field("draft_dirty", &draft_dirty)
            .field("daemon_state", &daemon_state)
            .field("operation_state", &operation_state)
            .field("model_install_active", &self.model_install.is_active())
            .field(
                "script_install_blocks_operations",
                &self.script_install.blocks_operations(),
            )
            .field(
                "installed_model_count",
                &self.installed_models.as_ref().map(Vec::len).ok(),
            )
            .field("selected_resource", &selected_resource)
            .field(
                "removal_confirmation_open",
                &self.removal_confirmation.is_some(),
            )
            .field("scene_editor_open", &self.scene_editor.is_some())
            .field(
                "asr_provider_editor_open",
                &self.asr_provider_editor.is_some(),
            )
            .field(
                "llm_provider_editor_open",
                &self.llm_provider_editor.is_some(),
            )
            .field(
                "adapter_config_editor_open",
                &self.adapter_config_editor.is_some(),
            )
            .field(
                "hotword_operation_active",
                &self.active_hotword_operation_id.is_some(),
            )
            .finish_non_exhaustive()
    }
}

/// Builds a redacted machine-readable snapshot for package and CI checks.
pub fn headless_snapshot(path: Option<&Path>, probe_daemon: bool) -> Result<Value, String> {
    let document = load_config_document(path)?;
    let daemon = if probe_daemon {
        match query_daemon_snapshot() {
            Ok(snapshot) => json!({
                "ok": true,
                "status": snapshot.status,
                "runtime": snapshot.runtime,
            }),
            Err(error) => json!({
                "ok": false,
                "error": error,
            }),
        }
    } else {
        json!({
            "ok": null,
            "skipped": true,
        })
    };
    Ok(json!({
        "ok": true,
        "application": "vinpst-gui",
        "ui_locale": GuiLocale::detect().code(),
        "config": {
            "path": document.path,
            "from_disk": document.from_disk,
            "summary": document.config.summary(),
            "capture_device": document.config.global.capture_device,
            "default_language": document.config.global.default_language,
            "llm_provider_count": document.config.llm.providers.len(),
            "adapter_count": document.config.llm.adapters.len(),
        },
        "daemon": daemon,
        "interaction": interaction::capability_snapshot(),
        "pages": Page::ALL.map(Page::machine_label),
    }))
}

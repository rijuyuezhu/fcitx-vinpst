//! Typed text-adapter configuration forms for the LLM page.

use std::{collections::HashMap, fmt};

use iced::{
    Element, Length, Task,
    widget::{button, column, row, text, text_input},
};
use vinput_config::{LlmAdapterConfig, VinputConfig};

use crate::{
    App, ConfigSaveOutcome, Message, OperationState, SecretInput, load_config_document,
    save_updated_config_with_daemon,
};

/// One editable text-adapter field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterConfigEditorField {
    /// Executable path or command name.
    Command,
    /// JSON array of command arguments.
    Args,
    /// JSON object of environment variables.
    Environment,
    /// Optional working directory.
    WorkingDirectory,
}

/// One text-adapter configuration interaction.
#[derive(Clone)]
pub enum AdapterConfigMessage {
    /// Open one configured adapter for editing.
    BeginEdit(String),
    /// Update one editor field with a redacted value wrapper.
    EditorChanged {
        /// Typed field being edited.
        field: AdapterConfigEditorField,
        /// User-entered value excluded from generic Debug output.
        value: SecretInput,
    },
    /// Restore the loaded adapter values.
    ResetEdit,
    /// Close the editor without saving.
    CancelEdit,
    /// Validate and persist the adapter form.
    Save,
    /// Result of one asynchronous adapter config mutation.
    MutationFinished(Result<AdapterConfigMutationOutcome, String>),
}

impl fmt::Debug for AdapterConfigMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BeginEdit(id) => formatter.debug_tuple("BeginEdit").field(id).finish(),
            Self::EditorChanged { field, .. } => formatter
                .debug_struct("EditorChanged")
                .field("field", field)
                .field("value", &"<redacted>")
                .finish(),
            Self::ResetEdit => formatter.write_str("ResetEdit"),
            Self::CancelEdit => formatter.write_str("CancelEdit"),
            Self::Save => formatter.write_str("Save"),
            Self::MutationFinished(Ok(outcome)) => formatter
                .debug_struct("MutationFinished")
                .field("adapter_id", &outcome.adapter_id)
                .field("status", &"saved")
                .finish(),
            Self::MutationFinished(Err(_)) => formatter
                .debug_struct("MutationFinished")
                .field("status", &"failed")
                .finish(),
        }
    }
}

/// Result of one persisted text-adapter configuration mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterConfigMutationOutcome {
    /// Shared atomic config-save receipt and daemon reload summary.
    pub save: ConfigSaveOutcome,
    /// Stable adapter id that was updated.
    pub adapter_id: String,
}

#[derive(Clone, PartialEq, Eq)]
struct AdapterConfigEditorFields {
    command: SecretInput,
    args: SecretInput,
    environment: SecretInput,
    working_directory: SecretInput,
}

impl fmt::Debug for AdapterConfigEditorFields {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdapterConfigEditorFields")
            .field("command", &"<redacted command>")
            .field("args", &"<redacted arguments>")
            .field("environment", &"<redacted environment>")
            .field("working_directory", &"<redacted working directory>")
            .finish()
    }
}

/// Active text-adapter editor state.
#[derive(Clone, PartialEq)]
pub(super) struct AdapterConfigEditorState {
    original: LlmAdapterConfig,
    baseline: AdapterConfigEditorFields,
    fields: AdapterConfigEditorFields,
}

impl fmt::Debug for AdapterConfigEditorState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdapterConfigEditorState")
            .field("adapter_id", &self.original.id)
            .field("baseline", &self.baseline)
            .field("fields", &self.fields)
            .finish()
    }
}

impl AdapterConfigEditorState {
    fn edit(adapter: &LlmAdapterConfig) -> Self {
        let fields = AdapterConfigEditorFields {
            command: SecretInput::new(adapter.command.clone()),
            args: SecretInput::new(
                serde_json::to_string_pretty(&adapter.args).unwrap_or_else(|_| "[]".to_owned()),
            ),
            environment: SecretInput::new(
                serde_json::to_string_pretty(&adapter.env).unwrap_or_else(|_| "{}".to_owned()),
            ),
            working_directory: SecretInput::new(adapter.working_dir.clone().unwrap_or_default()),
        };
        Self {
            original: adapter.clone(),
            baseline: fields.clone(),
            fields,
        }
    }

    fn update(&mut self, field: AdapterConfigEditorField, value: SecretInput) {
        match field {
            AdapterConfigEditorField::Command => self.fields.command = value,
            AdapterConfigEditorField::Args => self.fields.args = value,
            AdapterConfigEditorField::Environment => self.fields.environment = value,
            AdapterConfigEditorField::WorkingDirectory => self.fields.working_directory = value,
        }
    }

    fn reset(&mut self) {
        self.fields = self.baseline.clone();
    }

    fn is_dirty(&self) -> bool {
        self.fields != self.baseline
    }

    fn adapter(&self) -> Result<LlmAdapterConfig, String> {
        let command = self.fields.command.as_str().trim();
        if command.is_empty() {
            return Err("Text adapter command cannot be empty.".to_owned());
        }
        let mut adapter = self.original.clone();
        command.clone_into(&mut adapter.command);
        adapter.args = parse_string_array(self.fields.args.as_str())?;
        adapter.env = parse_string_map(self.fields.environment.as_str())?;
        adapter.working_dir = optional_trimmed(self.fields.working_directory.as_str());
        Ok(adapter)
    }
}

impl App {
    pub(super) fn intercept_adapter_config_message(
        &mut self,
        message: &Message,
    ) -> Option<Task<Message>> {
        if let Message::AdapterConfig(message) = message {
            if self.is_busy() && !matches!(message, AdapterConfigMessage::MutationFinished(_)) {
                return Some(Task::none());
            }
            return Some(self.handle_adapter_config_message(message.clone()));
        }
        let Some(editor) = &self.adapter_config_editor else {
            return None;
        };
        let blocks_open_editor = matches!(
            message,
            Message::ReloadConfig
                | Message::SaveConfig
                | Message::InstallModel
                | Message::RetryModelInstall
                | Message::RemoveInstalledModel(_)
                | Message::Scene(_)
                | Message::AsrProvider(_)
                | Message::LlmProvider(_)
                | Message::Hotword(_)
                | Message::AdapterRuntime(_)
                | Message::InstallProvider
                | Message::InstallAdapter
                | Message::ConfirmScriptInstall
                | Message::RetryScriptInstall
                | Message::RetryScriptConfigUpdate
                | Message::EditProviderScript(_)
                | Message::RemoveProvider(_)
                | Message::RemoveAdapter(_)
        ) || matches!(message, Message::SelectPage(page) if *page != crate::Page::Llm && editor.is_dirty());
        if blocks_open_editor {
            self.operation = OperationState::Failed(
                "Save or cancel the open text-adapter form before continuing.".to_owned(),
            );
            return Some(Task::none());
        }
        None
    }

    fn handle_adapter_config_message(&mut self, message: AdapterConfigMessage) -> Task<Message> {
        match message {
            AdapterConfigMessage::BeginEdit(adapter_id) => {
                self.begin_edit_adapter_config(&adapter_id);
                Task::none()
            }
            AdapterConfigMessage::EditorChanged { field, value } => {
                if let Some(editor) = &mut self.adapter_config_editor {
                    editor.update(field, value);
                }
                Task::none()
            }
            AdapterConfigMessage::ResetEdit => {
                if let Some(editor) = &mut self.adapter_config_editor {
                    editor.reset();
                }
                self.operation = OperationState::Idle;
                Task::none()
            }
            AdapterConfigMessage::CancelEdit => {
                self.adapter_config_editor = None;
                self.operation = OperationState::Idle;
                Task::none()
            }
            AdapterConfigMessage::Save => self.begin_adapter_config_save(),
            AdapterConfigMessage::MutationFinished(result) => {
                self.finish_adapter_config_mutation(result)
            }
        }
    }

    fn begin_edit_adapter_config(&mut self, adapter_id: &str) {
        if self.adapter_config_editor.is_some() {
            return;
        }
        if let Err(error) = self.ensure_adapter_config_editor_allowed() {
            self.operation = OperationState::Failed(error);
            return;
        }
        if !self.guard_hotword_changes("editing a text adapter") {
            return;
        }
        let Some(adapter) = self
            .config
            .as_ref()
            .ok()
            .and_then(|document| {
                document
                    .config
                    .llm
                    .adapters
                    .iter()
                    .find(|adapter| adapter.id == adapter_id)
            })
            .cloned()
        else {
            self.operation = OperationState::Failed(format!(
                "Text adapter `{adapter_id}` is no longer configured."
            ));
            return;
        };
        self.adapter_config_editor = Some(AdapterConfigEditorState::edit(&adapter));
        self.operation = OperationState::Idle;
    }

    fn begin_adapter_config_save(&mut self) -> Task<Message> {
        let Some(editor) = self.adapter_config_editor.clone() else {
            return Task::none();
        };
        if !editor.is_dirty() {
            return Task::none();
        }
        if let Err(error) = self.ensure_adapter_config_editor_allowed() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if !self.guard_hotword_changes("saving a text adapter") {
            return Task::none();
        }
        let Ok(document) = &self.config else {
            self.operation = OperationState::Failed("No valid config is loaded.".to_owned());
            return Task::none();
        };
        let updated = match edit_adapter_config(&document.config, &editor) {
            Ok(updated) => updated,
            Err(error) => {
                self.operation = OperationState::Failed(error);
                return Task::none();
            }
        };
        self.operation = OperationState::Running("Saving text adapter…");
        let document = document.clone();
        let adapter_id = editor.original.id;
        Task::perform(
            async move {
                save_updated_config_with_daemon(&document, &updated)
                    .map(|save| AdapterConfigMutationOutcome { save, adapter_id })
            },
            |result| Message::AdapterConfig(AdapterConfigMessage::MutationFinished(result)),
        )
    }

    fn finish_adapter_config_mutation(
        &mut self,
        result: Result<AdapterConfigMutationOutcome, String>,
    ) -> Task<Message> {
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                self.operation = OperationState::Failed(error);
                return Task::none();
            }
        };
        let backup = outcome.save.backup_path.as_ref().map_or_else(
            || "no previous file".to_owned(),
            |path| format!("backup {}", path.display()),
        );
        self.replace_config(load_config_document(Some(&outcome.save.path)));
        self.operation = OperationState::Succeeded(format!(
            "Updated text adapter `{}`. Saved {} ({backup}); {}",
            outcome.adapter_id,
            outcome.save.path.display(),
            outcome.save.daemon_reload
        ));
        self.begin_daemon_refresh(false)
    }

    fn ensure_adapter_config_editor_allowed(&self) -> Result<(), String> {
        self.ensure_no_unsaved_config_draft()?;
        self.ensure_no_open_scene_editor()?;
        self.ensure_no_open_asr_provider_editor()?;
        self.ensure_no_open_llm_provider_editor()?;
        Ok(())
    }

    pub(super) fn adapter_config_editor_view(&self, busy: bool) -> Option<Element<'_, Message>> {
        self.adapter_config_editor
            .as_ref()
            .map(|editor| adapter_config_editor_view(editor, busy))
    }
}

fn edit_adapter_config(
    config: &VinputConfig,
    editor: &AdapterConfigEditorState,
) -> Result<VinputConfig, String> {
    let adapter = editor.adapter()?;
    let mut updated = config.clone();
    let Some(index) = updated
        .llm
        .adapters
        .iter()
        .position(|candidate| candidate.id == editor.original.id)
    else {
        return Err(format!(
            "Text adapter `{}` is no longer configured.",
            editor.original.id
        ));
    };
    if updated.llm.adapters[index] != editor.original {
        return Err(format!(
            "Text adapter `{}` changed after the form was opened; reopen it before saving.",
            editor.original.id
        ));
    }
    updated.llm.adapters[index] = adapter;
    updated
        .validate()
        .map_err(|error| format!("Validate updated text-adapter config: {error}"))?;
    Ok(updated)
}

fn adapter_config_editor_view(
    editor: &AdapterConfigEditorState,
    busy: bool,
) -> Element<'_, Message> {
    let dirty = editor.is_dirty();
    column![
        text("Edit text adapter").size(22),
        text(format!("Adapter id: {} (immutable)", editor.original.id)),
        labeled_input(
            "Command",
            "/path/to/adapter",
            editor.fields.command.as_str(),
            AdapterConfigEditorField::Command,
            false,
        ),
        labeled_input(
            "Arguments",
            "JSON string array",
            editor.fields.args.as_str(),
            AdapterConfigEditorField::Args,
            true,
        ),
        labeled_input(
            "Environment",
            "JSON string object",
            editor.fields.environment.as_str(),
            AdapterConfigEditorField::Environment,
            true,
        ),
        labeled_input(
            "Working directory",
            "optional absolute or configured path",
            editor.fields.working_directory.as_str(),
            AdapterConfigEditorField::WorkingDirectory,
            false,
        ),
        row![
            button("Update adapter").on_press_maybe(
                (dirty && !busy).then_some(Message::AdapterConfig(AdapterConfigMessage::Save)),
            ),
            button("Reset form").on_press_maybe(
                (dirty && !busy).then_some(Message::AdapterConfig(AdapterConfigMessage::ResetEdit)),
            ),
            button("Cancel").on_press_maybe(
                (!busy).then_some(Message::AdapterConfig(AdapterConfigMessage::CancelEdit)),
            ),
            text(if dirty {
                "Unsaved adapter changes"
            } else {
                "Adapter form is unchanged"
            }),
        ]
        .spacing(10),
    ]
    .spacing(10)
    .into()
}

fn labeled_input<'a>(
    label: &'static str,
    placeholder: &'static str,
    value: &'a str,
    field: AdapterConfigEditorField,
    secure: bool,
) -> Element<'a, Message> {
    row![
        text(label).width(160),
        text_input(placeholder, value)
            .secure(secure)
            .on_input(move |value| {
                Message::AdapterConfig(AdapterConfigMessage::EditorChanged {
                    field,
                    value: SecretInput::new(value),
                })
            })
            .width(Length::Fill),
    ]
    .spacing(10)
    .into()
}

fn parse_string_array(value: &str) -> Result<Vec<String>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str::<Vec<String>>(value)
        .map_err(|error| format!("Parse adapter arguments as a JSON string array: {error}"))
}

fn parse_string_map(value: &str) -> Result<HashMap<String, String>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(HashMap::new());
    }
    serde_json::from_str::<HashMap<String, String>>(value)
        .map_err(|error| format!("Parse adapter environment as a JSON string object: {error}"))
}

fn optional_trimmed(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn adapter() -> LlmAdapterConfig {
        LlmAdapterConfig {
            id: "adapter-a".to_owned(),
            command: "/usr/bin/adapter".to_owned(),
            args: vec!["--json".to_owned()],
            env: HashMap::from([("TOKEN".to_owned(), "secret".to_owned())]),
            working_dir: Some("/srv/adapter".to_owned()),
            extra: HashMap::from([("future".to_owned(), json!({"enabled": true}))]),
        }
    }

    #[test]
    fn editor_preserves_identity_and_extra_while_updating_typed_fields() {
        let original = adapter();
        let mut editor = AdapterConfigEditorState::edit(&original);
        editor.update(
            AdapterConfigEditorField::Command,
            SecretInput::new(" /opt/adapter ".to_owned()),
        );
        editor.update(
            AdapterConfigEditorField::Args,
            SecretInput::new("[\"--stream\",\"--lang=en\"]".to_owned()),
        );
        editor.update(
            AdapterConfigEditorField::Environment,
            SecretInput::new("{\"API_KEY\":\"value\"}".to_owned()),
        );
        editor.update(
            AdapterConfigEditorField::WorkingDirectory,
            SecretInput::new(" /opt/state ".to_owned()),
        );

        let updated = editor.adapter().expect("adapter should validate");
        assert_eq!(updated.id, original.id);
        assert_eq!(updated.extra, original.extra);
        assert_eq!(updated.command, "/opt/adapter");
        assert_eq!(updated.args, ["--stream", "--lang=en"]);
        assert_eq!(
            updated.env.get("API_KEY").map(String::as_str),
            Some("value")
        );
        assert_eq!(updated.working_dir.as_deref(), Some("/opt/state"));
    }

    #[test]
    fn editor_rejects_empty_command_and_invalid_json_collections() {
        let mut editor = AdapterConfigEditorState::edit(&adapter());
        editor.update(
            AdapterConfigEditorField::Command,
            SecretInput::new("  ".to_owned()),
        );
        assert!(editor.adapter().is_err());
        assert!(parse_string_array("{\"not\":\"array\"}").is_err());
        assert!(parse_string_map("[\"not-object\"]").is_err());
    }

    #[test]
    fn editor_debug_and_messages_redact_process_configuration() {
        let editor = AdapterConfigEditorState::edit(&adapter());
        let debug = format!("{editor:?}");
        assert!(!debug.contains("/usr/bin/adapter"));
        assert!(!debug.contains("--json"));
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("/srv/adapter"));

        let message = AdapterConfigMessage::EditorChanged {
            field: AdapterConfigEditorField::Environment,
            value: SecretInput::new("message-secret".to_owned()),
        };
        assert!(!format!("{message:?}").contains("message-secret"));
    }

    #[test]
    fn edit_rejects_stale_adapter_without_pinning_error_prose() {
        let original = adapter();
        let editor = AdapterConfigEditorState::edit(&original);
        let mut config = VinputConfig::bundled_default().expect("bundled config");
        config.llm.adapters = vec![original.clone()];
        let updated = edit_adapter_config(&config, &editor).expect("current adapter is valid");
        assert_eq!(
            updated.llm.adapters.as_slice(),
            std::slice::from_ref(&original)
        );

        config.llm.adapters[0].command = "/external/change".to_owned();
        assert!(edit_adapter_config(&config, &editor).is_err());
    }

    #[test]
    fn dirty_adapter_form_blocks_runtime_control_and_cross_page_navigation() {
        let (mut app, _) = App::boot();
        let original = adapter();
        app.config
            .as_mut()
            .expect("bundled config should load")
            .config
            .llm
            .adapters = vec![original.clone()];
        app.page = crate::Page::Llm;
        app.adapter_config_editor = Some(AdapterConfigEditorState::edit(&original));
        app.adapter_config_editor
            .as_mut()
            .expect("editor should remain open")
            .update(
                AdapterConfigEditorField::Command,
                SecretInput::new("/opt/changed-adapter".to_owned()),
            );

        let _ = app.update(Message::AdapterRuntime(
            crate::AdapterRuntimeMessage::Start(original.id.clone()),
        ));
        assert!(app.adapter_config_editor.is_some());
        assert!(matches!(app.operation, OperationState::Failed(_)));

        let _ = app.update(Message::SelectPage(crate::Page::Control));
        assert_eq!(app.page, crate::Page::Llm);
        assert!(app.adapter_config_editor.is_some());
    }
}

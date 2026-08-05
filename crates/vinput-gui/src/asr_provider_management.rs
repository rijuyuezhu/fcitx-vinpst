//! Typed ASR provider editor state, validation, persistence, and rendering.

use std::{collections::HashMap, fmt};

use iced::{
    Element, Length, Task,
    widget::{button, column, row, text, text_input},
};
use vinput_config::{AsrProviderConfig, AsrProviderKind, VinputConfig, redact_url_for_diagnostics};

use crate::{
    App, ConfigDocument, ConfigSaveOutcome, Message, OperationState, SecretInput,
    load_config_document, save_updated_config_with_daemon,
};

/// One editable field in the ASR provider form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsrProviderEditorField {
    /// Optional provider timeout in milliseconds.
    TimeoutMs,
    /// Optional provider model identifier.
    Model,
    /// Command executable for command providers.
    Command,
    /// JSON array of command arguments.
    Args,
    /// JSON object of command environment values.
    Environment,
    /// Endpoint for remote providers.
    Endpoint,
}

/// One ASR provider lifecycle interaction handled by the Resources page.
#[derive(Debug, Clone)]
pub enum AsrProviderMessage {
    /// Open one configured provider for editing.
    BeginEdit(String),
    /// Update one field without exposing entered values through `Debug`.
    EditorChanged {
        /// Typed field being edited.
        field: AsrProviderEditorField,
        /// Redacted user-entered value.
        value: SecretInput,
    },
    /// Restore the form to its initially loaded values.
    ResetEdit,
    /// Close the provider form without saving.
    CancelEdit,
    /// Validate and persist the provider form.
    Save,
    /// Result of one asynchronous provider mutation.
    MutationFinished(Result<AsrProviderMutationOutcome, String>),
}

/// Result of one persisted ASR provider mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrProviderMutationOutcome {
    /// Shared atomic config-save receipt and daemon reload summary.
    pub save: ConfigSaveOutcome,
    /// Stable provider id that was updated.
    pub provider_id: String,
}

#[derive(Clone, PartialEq, Eq)]
struct AsrProviderEditorFields {
    timeout_ms: String,
    model: String,
    command: SecretInput,
    args: SecretInput,
    environment: SecretInput,
    endpoint: SecretInput,
}

impl fmt::Debug for AsrProviderEditorFields {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AsrProviderEditorFields")
            .field("timeout_ms", &self.timeout_ms)
            .field("model", &self.model)
            .field("command", &"<redacted command>")
            .field("args", &"<redacted arguments>")
            .field("environment", &"<redacted environment>")
            .field(
                "endpoint",
                &redact_url_for_diagnostics(self.endpoint.as_str()),
            )
            .finish()
    }
}

/// Active ASR provider editor state.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct AsrProviderEditorState {
    original: AsrProviderConfig,
    baseline: AsrProviderEditorFields,
    fields: AsrProviderEditorFields,
    endpoint_secure: bool,
}

impl fmt::Debug for AsrProviderEditorState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AsrProviderEditorState")
            .field("provider_id", &self.original.id)
            .field("provider_kind", &self.original.kind)
            .field("baseline", &self.baseline)
            .field("fields", &self.fields)
            .field("endpoint_secure", &self.endpoint_secure)
            .finish()
    }
}

impl AsrProviderEditorState {
    fn edit(provider: &AsrProviderConfig) -> Self {
        let fields = AsrProviderEditorFields {
            timeout_ms: provider
                .timeout_ms
                .map_or_else(String::new, |value| value.to_string()),
            model: provider.model.clone().unwrap_or_default(),
            command: SecretInput::new(provider.command.clone().unwrap_or_default()),
            args: SecretInput::new(
                serde_json::to_string_pretty(&provider.args).unwrap_or_else(|_| "[]".to_owned()),
            ),
            environment: SecretInput::new(
                serde_json::to_string_pretty(&provider.env).unwrap_or_else(|_| "{}".to_owned()),
            ),
            endpoint: SecretInput::new(provider.endpoint.clone().unwrap_or_default()),
        };
        Self {
            original: provider.clone(),
            endpoint_secure: endpoint_input_is_secure(fields.endpoint.as_str()),
            baseline: fields.clone(),
            fields,
        }
    }

    fn update(&mut self, field: AsrProviderEditorField, value: SecretInput) {
        let value = value.into_inner();
        match field {
            AsrProviderEditorField::TimeoutMs => self.fields.timeout_ms = value,
            AsrProviderEditorField::Model => self.fields.model = value,
            AsrProviderEditorField::Command => self.fields.command = SecretInput::new(value),
            AsrProviderEditorField::Args => self.fields.args = SecretInput::new(value),
            AsrProviderEditorField::Environment => {
                self.fields.environment = SecretInput::new(value);
            }
            AsrProviderEditorField::Endpoint => {
                self.endpoint_secure |= endpoint_input_is_secure(&value);
                self.fields.endpoint = SecretInput::new(value);
            }
        }
    }

    fn reset(&mut self) {
        self.fields = self.baseline.clone();
        self.endpoint_secure = endpoint_input_is_secure(self.baseline.endpoint.as_str());
    }

    fn is_dirty(&self) -> bool {
        self.fields != self.baseline
    }

    fn provider(&self) -> Result<AsrProviderConfig, String> {
        let mut provider = self.original.clone();
        provider.timeout_ms = parse_optional_timeout(&self.fields.timeout_ms)?;
        provider.model = optional_trimmed(&self.fields.model);

        match provider.kind {
            AsrProviderKind::Local => {}
            AsrProviderKind::Command => {
                let command = self.fields.command.as_str().trim();
                if command.is_empty() {
                    return Err("Command ASR provider command cannot be empty.".to_owned());
                }
                provider.command = Some(command.to_owned());
                provider.args = parse_string_array(self.fields.args.as_str(), "arguments")?;
                provider.env = parse_string_map(self.fields.environment.as_str())?;
            }
            AsrProviderKind::Remote => {
                let endpoint = self.fields.endpoint.as_str().trim();
                if endpoint.is_empty() {
                    return Err("Remote ASR provider endpoint cannot be empty.".to_owned());
                }
                provider.endpoint = Some(endpoint.to_owned());
            }
        }
        Ok(provider)
    }
}

impl App {
    pub(super) fn intercept_asr_provider_message(
        &mut self,
        message: &Message,
    ) -> Option<Task<Message>> {
        if let Message::AsrProvider(message) = message {
            if self.is_busy() && !matches!(message, AsrProviderMessage::MutationFinished(_)) {
                return Some(Task::none());
            }
            return Some(self.handle_asr_provider_message(message.clone()));
        }
        let Some(editor) = &self.asr_provider_editor else {
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
                | Message::LlmProvider(_)
                | Message::Hotword(_)
                | Message::InstallProvider
                | Message::InstallAdapter
                | Message::ConfirmScriptInstall
                | Message::RetryScriptInstall
                | Message::RetryScriptConfigUpdate
                | Message::EditProviderScript(_)
                | Message::RemoveProvider(_)
                | Message::RemoveAdapter(_)
        ) || matches!(message, Message::SelectPage(page) if *page != crate::Page::Resources && editor.is_dirty());
        if blocks_open_editor {
            self.operation = OperationState::Failed(
                "Save or cancel the open ASR provider form before continuing.".to_owned(),
            );
            return Some(Task::none());
        }
        None
    }

    pub(super) fn handle_asr_provider_message(
        &mut self,
        message: AsrProviderMessage,
    ) -> Task<Message> {
        match message {
            AsrProviderMessage::BeginEdit(id) => self.begin_edit_asr_provider(&id),
            AsrProviderMessage::EditorChanged { field, value } => {
                if let Some(editor) = &mut self.asr_provider_editor {
                    editor.update(field, value);
                }
            }
            AsrProviderMessage::ResetEdit => {
                if !self.is_busy() {
                    if let Some(editor) = &mut self.asr_provider_editor {
                        editor.reset();
                    }
                    self.operation = OperationState::Idle;
                }
            }
            AsrProviderMessage::CancelEdit => {
                if !self.is_busy() {
                    self.asr_provider_editor = None;
                    self.operation = OperationState::Idle;
                }
            }
            AsrProviderMessage::Save => return self.begin_asr_provider_save(),
            AsrProviderMessage::MutationFinished(result) => {
                return self.finish_asr_provider_mutation(result);
            }
        }
        Task::none()
    }

    fn begin_edit_asr_provider(&mut self, provider_id: &str) {
        if self.is_busy() || self.asr_provider_editor.is_some() {
            return;
        }
        if let Err(error) = self.ensure_asr_provider_editor_allowed() {
            self.operation = OperationState::Failed(error);
            return;
        }
        if !self.guard_hotword_changes("editing an ASR provider") {
            return;
        }
        let Some(provider) = self
            .config
            .as_ref()
            .ok()
            .and_then(|document| {
                document
                    .config
                    .asr
                    .providers
                    .iter()
                    .find(|provider| provider.id == provider_id)
            })
            .cloned()
        else {
            self.operation = OperationState::Failed(format!(
                "ASR provider `{provider_id}` is no longer configured."
            ));
            return;
        };
        self.asr_provider_editor = Some(AsrProviderEditorState::edit(&provider));
        self.operation = OperationState::Idle;
    }

    fn begin_asr_provider_save(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let Some(editor) = self.asr_provider_editor.clone() else {
            return Task::none();
        };
        if !editor.is_dirty() {
            return Task::none();
        }
        if let Err(error) = self.ensure_asr_provider_editor_allowed() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if !self.guard_hotword_changes("saving an ASR provider") {
            return Task::none();
        }
        let Ok(document) = &self.config else {
            self.operation = OperationState::Failed("No valid config is loaded.".to_owned());
            return Task::none();
        };
        let updated = match edit_asr_provider(&document.config, &editor) {
            Ok(updated) => updated,
            Err(error) => {
                self.operation = OperationState::Failed(error);
                return Task::none();
            }
        };
        self.begin_asr_provider_mutation(document.clone(), updated, editor.original.id)
    }

    fn ensure_asr_provider_editor_allowed(&self) -> Result<(), String> {
        self.ensure_no_unsaved_config_draft()?;
        self.ensure_no_open_scene_editor()?;
        self.ensure_no_open_llm_provider_editor()?;
        Ok(())
    }

    fn begin_asr_provider_mutation(
        &mut self,
        document: ConfigDocument,
        updated: VinputConfig,
        provider_id: String,
    ) -> Task<Message> {
        self.operation = OperationState::Running("Saving ASR provider…");
        Task::perform(
            async move {
                save_updated_config_with_daemon(&document, &updated)
                    .map(|save| AsrProviderMutationOutcome { save, provider_id })
            },
            |result| Message::AsrProvider(AsrProviderMessage::MutationFinished(result)),
        )
    }

    fn finish_asr_provider_mutation(
        &mut self,
        result: Result<AsrProviderMutationOutcome, String>,
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
            "Updated ASR provider `{}`. Saved {} ({backup}); {}",
            outcome.provider_id,
            outcome.save.path.display(),
            outcome.save.daemon_reload
        ));
        self.begin_daemon_refresh(false)
    }

    pub(super) fn asr_provider_editor_view(&self, busy: bool) -> Option<Element<'_, Message>> {
        self.asr_provider_editor
            .as_ref()
            .map(|editor| provider_editor_view(editor, busy))
    }
}

fn edit_asr_provider(
    config: &VinputConfig,
    editor: &AsrProviderEditorState,
) -> Result<VinputConfig, String> {
    let provider = editor.provider()?;
    let mut updated = config.clone();
    let Some(index) = updated
        .asr
        .providers
        .iter()
        .position(|candidate| candidate.id == editor.original.id)
    else {
        return Err(format!(
            "ASR provider `{}` is no longer configured.",
            editor.original.id
        ));
    };
    if updated.asr.providers[index] != editor.original {
        return Err(format!(
            "ASR provider `{}` changed after the form was opened; reopen it before saving.",
            editor.original.id
        ));
    }
    updated.asr.providers[index] = provider;
    updated
        .validate()
        .map_err(|error| format!("Validate updated ASR provider config: {error}"))?;
    Ok(updated)
}

fn provider_editor_view(editor: &AsrProviderEditorState, busy: bool) -> Element<'_, Message> {
    let dirty = editor.is_dirty();
    let kind = kind_label(&editor.original.kind);
    let mut body = column![
        text("Edit ASR provider").size(22),
        text(format!(
            "Provider id: {} (immutable) · type: {kind} (immutable)",
            editor.original.id
        )),
        labeled_input(
            "Timeout (ms)",
            "blank uses backend default",
            &editor.fields.timeout_ms,
            AsrProviderEditorField::TimeoutMs,
            false,
        ),
        labeled_input(
            "Model",
            "optional model id",
            &editor.fields.model,
            AsrProviderEditorField::Model,
            false,
        ),
    ]
    .spacing(10);

    match editor.original.kind {
        AsrProviderKind::Local => {
            body = body.push(text(
                "Hotword path and content remain managed on the Hotwords page.",
            ));
        }
        AsrProviderKind::Command => {
            body = body
                .push(labeled_input(
                    "Command",
                    "/path/to/provider",
                    editor.fields.command.as_str(),
                    AsrProviderEditorField::Command,
                    false,
                ))
                .push(labeled_input(
                    "Arguments",
                    "JSON string array",
                    editor.fields.args.as_str(),
                    AsrProviderEditorField::Args,
                    true,
                ))
                .push(labeled_input(
                    "Environment",
                    "JSON string object",
                    editor.fields.environment.as_str(),
                    AsrProviderEditorField::Environment,
                    true,
                ));
        }
        AsrProviderKind::Remote => {
            body = body.push(labeled_input(
                "Endpoint",
                "https://provider.example/v1/audio/transcriptions",
                editor.fields.endpoint.as_str(),
                AsrProviderEditorField::Endpoint,
                editor.endpoint_secure,
            ));
        }
    }

    body.push(
        row![
            button("Update provider").on_press_maybe(
                (dirty && !busy).then_some(Message::AsrProvider(AsrProviderMessage::Save)),
            ),
            button("Reset form").on_press_maybe(
                (dirty && !busy).then_some(Message::AsrProvider(AsrProviderMessage::ResetEdit)),
            ),
            button("Cancel").on_press_maybe(
                (!busy).then_some(Message::AsrProvider(AsrProviderMessage::CancelEdit)),
            ),
            text(if dirty {
                "Unsaved provider changes"
            } else {
                "Provider form is unchanged"
            }),
        ]
        .spacing(10),
    )
    .into()
}

fn labeled_input<'a>(
    label: &'static str,
    placeholder: &'static str,
    value: &'a str,
    field: AsrProviderEditorField,
    secure: bool,
) -> Element<'a, Message> {
    row![
        text(label).width(160),
        text_input(placeholder, value)
            .secure(secure)
            .on_input(move |value| {
                Message::AsrProvider(AsrProviderMessage::EditorChanged {
                    field,
                    value: SecretInput::new(value),
                })
            })
            .width(Length::Fill),
    ]
    .spacing(10)
    .into()
}

fn parse_optional_timeout(value: &str) -> Result<Option<u64>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let timeout = value.parse::<u64>().map_err(|_| {
        "ASR provider timeout must be a positive integer in milliseconds.".to_owned()
    })?;
    if timeout == 0 {
        return Err("ASR provider timeout must be greater than zero.".to_owned());
    }
    Ok(Some(timeout))
}

fn parse_string_array(value: &str, label: &str) -> Result<Vec<String>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str::<Vec<String>>(value)
        .map_err(|error| format!("Parse command {label} as a JSON string array: {error}"))
}

fn parse_string_map(value: &str) -> Result<HashMap<String, String>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(HashMap::new());
    }
    serde_json::from_str::<HashMap<String, String>>(value)
        .map_err(|error| format!("Parse command environment as a JSON string object: {error}"))
}

fn optional_trimmed(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn endpoint_input_is_secure(value: &str) -> bool {
    if let Ok(url) = url::Url::parse(value) {
        return !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some_and(|query| !query.is_empty())
            || url.fragment().is_some_and(|fragment| !fragment.is_empty());
    }
    value.contains('@') || value.contains('?') || value.contains('#')
}

const fn kind_label(kind: &AsrProviderKind) -> &'static str {
    match kind {
        AsrProviderKind::Local => "local",
        AsrProviderKind::Remote => "remote",
        AsrProviderKind::Command => "command",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn command_provider() -> AsrProviderConfig {
        AsrProviderConfig {
            id: "command-provider".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(4_000),
            model: Some("model-a".to_owned()),
            hotwords_file: Some("/tmp/hotwords.txt".to_owned()),
            command: Some("/usr/bin/provider".to_owned()),
            args: vec!["--json".to_owned()],
            env: HashMap::from([("TOKEN".to_owned(), "secret".to_owned())]),
            endpoint: None,
        }
    }

    #[test]
    fn command_editor_preserves_identity_and_hotword_while_updating_typed_fields() {
        let original = command_provider();
        let mut editor = AsrProviderEditorState::edit(&original);
        editor.update(
            AsrProviderEditorField::TimeoutMs,
            SecretInput::new("9000".to_owned()),
        );
        editor.update(
            AsrProviderEditorField::Command,
            SecretInput::new(" /opt/provider ".to_owned()),
        );
        editor.update(
            AsrProviderEditorField::Args,
            SecretInput::new("[\"--stream\", \"--lang=en\"]".to_owned()),
        );
        editor.update(
            AsrProviderEditorField::Environment,
            SecretInput::new("{\"API_KEY\":\"value\"}".to_owned()),
        );

        let provider = editor.provider().expect("provider should validate");
        assert_eq!(provider.id, original.id);
        assert_eq!(provider.kind, original.kind);
        assert_eq!(provider.hotwords_file, original.hotwords_file);
        assert_eq!(provider.timeout_ms, Some(9_000));
        assert_eq!(provider.command.as_deref(), Some("/opt/provider"));
        assert_eq!(provider.args, ["--stream", "--lang=en"]);
        assert_eq!(
            provider.env.get("API_KEY").map(String::as_str),
            Some("value")
        );
    }

    #[test]
    fn provider_editor_rejects_invalid_timeout_args_environment_and_required_targets() {
        assert!(parse_optional_timeout("0").is_err());
        assert!(parse_optional_timeout("1.5").is_err());
        assert!(parse_string_array("{\"not\":\"array\"}", "arguments").is_err());
        assert!(parse_string_map("[\"not-object\"]").is_err());

        let mut command = AsrProviderEditorState::edit(&command_provider());
        command.update(
            AsrProviderEditorField::Command,
            SecretInput::new("   ".to_owned()),
        );
        assert!(command.provider().is_err());

        let mut remote = command_provider();
        remote.kind = AsrProviderKind::Remote;
        remote.command = None;
        remote.args.clear();
        remote.env.clear();
        remote.endpoint = Some("https://example.invalid/asr".to_owned());
        let mut remote = AsrProviderEditorState::edit(&remote);
        remote.update(
            AsrProviderEditorField::Endpoint,
            SecretInput::new(String::new()),
        );
        assert!(remote.provider().is_err());
    }

    #[test]
    fn provider_editor_debug_redacts_command_arguments_environment_and_endpoint_secrets() {
        let mut provider = command_provider();
        provider.command = Some("/secret/path/provider".to_owned());
        provider.args = vec!["--token=argument-secret".to_owned()];
        provider
            .env
            .insert("KEY".to_owned(), "environment-secret".to_owned());
        provider.endpoint =
            Some("https://user:pass@example.invalid/asr?token=query-secret".to_owned());
        let debug = format!("{:?}", AsrProviderEditorState::edit(&provider));
        assert!(!debug.contains("/secret/path/provider"));
        assert!(!debug.contains("argument-secret"));
        assert!(!debug.contains("environment-secret"));
        assert!(!debug.contains("query-secret"));
        assert!(!debug.contains("pass"));
    }

    #[test]
    fn edit_rejects_stale_provider_and_validates_complete_config() {
        let provider = command_provider();
        let editor = AsrProviderEditorState::edit(&provider);
        let mut config = VinputConfig::bundled_default().expect("bundled config should validate");
        config.asr.providers = vec![provider.clone()];
        config.asr.active_provider = provider.id.clone();

        let updated = edit_asr_provider(&config, &editor).expect("unchanged provider is valid");
        assert_eq!(
            updated.asr.providers.as_slice(),
            std::slice::from_ref(&provider)
        );

        let mut stale = config;
        stale.asr.providers[0].timeout_ms = Some(8_000);
        let error = edit_asr_provider(&stale, &editor).expect_err("stale form must fail");
        assert!(error.contains("changed after the form was opened"));
    }

    #[test]
    fn dirty_provider_form_blocks_navigation_and_resource_mutations() {
        let (mut app, _) = App::boot();
        let provider = app
            .config
            .as_ref()
            .expect("bundled config should load")
            .config
            .asr
            .providers
            .first()
            .expect("bundled config should include a provider")
            .clone();
        app.page = crate::Page::Resources;
        app.asr_provider_editor = Some(AsrProviderEditorState::edit(&provider));
        app.asr_provider_editor
            .as_mut()
            .expect("editor should remain open")
            .update(
                AsrProviderEditorField::TimeoutMs,
                SecretInput::new("12345".to_owned()),
            );

        let _ = app.update(Message::SelectPage(crate::Page::Llm));
        assert_eq!(app.page, crate::Page::Resources);
        assert!(app.asr_provider_editor.is_some());

        let _ = app.update(Message::InstallModel);
        assert!(app.asr_provider_editor.is_some());
        assert!(matches!(app.operation, OperationState::Failed(_)));
    }
}

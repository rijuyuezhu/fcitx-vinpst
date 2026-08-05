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
    script_management::managed_provider_script_path,
};

/// One editable field in the ASR provider form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsrProviderEditorField {
    /// Stable id for a new custom provider.
    Id,
    /// Optional provider timeout in milliseconds.
    TimeoutMs,
    /// Optional provider model identifier.
    Model,
    /// Command executable for command providers.
    Command,
    /// JSON array of command arguments.
    Args,
    /// Endpoint for remote providers.
    Endpoint,
}

/// One ASR provider lifecycle interaction handled by the Resources page.
#[derive(Debug, Clone)]
pub enum AsrProviderMessage {
    /// Open an empty custom provider form.
    BeginAdd,
    /// Open one configured provider for editing.
    BeginEdit(String),
    /// Select the provider kind while creating a custom entry.
    KindChanged(AsrProviderKind),
    /// Update one field without exposing entered values through `Debug`.
    EditorChanged {
        /// Typed field being edited.
        field: AsrProviderEditorField,
        /// Redacted user-entered value.
        value: SecretInput,
    },
    /// Update one visible environment-variable key.
    EnvironmentKeyChanged {
        /// Stable row index in the current form.
        index: usize,
        /// Visible environment-variable key.
        key: String,
    },
    /// Update one redacted environment-variable value.
    EnvironmentValueChanged {
        /// Stable row index in the current form.
        index: usize,
        /// Secret environment-variable value.
        value: SecretInput,
    },
    /// Append one empty environment-variable row.
    AddEnvironment,
    /// Remove one environment-variable row.
    RemoveEnvironment(usize),
    /// Restore the form to its initially loaded values.
    ResetEdit,
    /// Close the provider form without saving.
    CancelEdit,
    /// Validate and persist the provider form.
    Save,
    /// Remove one inactive user-defined provider from configuration only.
    Remove(String),
    /// Result of one asynchronous provider mutation.
    MutationFinished(Result<AsrProviderMutationOutcome, String>),
    /// Result of one asynchronous user-defined provider removal.
    RemovalFinished(Result<AsrProviderRemovalOutcome, String>),
}

/// Result of one persisted ASR provider mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrProviderMutationOutcome {
    /// Shared atomic config-save receipt and daemon reload summary.
    pub save: ConfigSaveOutcome,
    /// Stable provider id that was updated.
    pub provider_id: String,
    /// Whether the mutation created a new custom provider.
    pub created: bool,
}

/// Result of removing one user-defined ASR provider from configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrProviderRemovalOutcome {
    /// Shared atomic config-save receipt and daemon reload summary.
    pub save: ConfigSaveOutcome,
    /// Stable provider id removed from configuration.
    pub provider_id: String,
}

#[derive(Clone, PartialEq, Eq)]
struct AsrProviderEnvironmentEntry {
    key: String,
    value: SecretInput,
}

impl fmt::Debug for AsrProviderEnvironmentEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AsrProviderEnvironmentEntry")
            .field("key", &self.key)
            .field("value", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
struct AsrProviderEditorFields {
    id: String,
    timeout_ms: String,
    model: String,
    command: SecretInput,
    args: SecretInput,
    environment: Vec<AsrProviderEnvironmentEntry>,
    endpoint: SecretInput,
}

impl fmt::Debug for AsrProviderEditorFields {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AsrProviderEditorFields")
            .field("id", &self.id)
            .field("timeout_ms", &self.timeout_ms)
            .field("model", &self.model)
            .field("command", &"<redacted command>")
            .field("args", &"<redacted arguments>")
            .field("environment", &self.environment)
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
    original: Option<AsrProviderConfig>,
    kind: AsrProviderKind,
    baseline: AsrProviderEditorFields,
    fields: AsrProviderEditorFields,
    endpoint_secure: bool,
}

impl fmt::Debug for AsrProviderEditorState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AsrProviderEditorState")
            .field("provider_id", &self.fields.id)
            .field("provider_kind", &self.kind)
            .field(
                "mode",
                &if self.original.is_some() {
                    "edit"
                } else {
                    "add"
                },
            )
            .field("baseline", &self.baseline)
            .field("fields", &self.fields)
            .field("endpoint_secure", &self.endpoint_secure)
            .finish()
    }
}

impl AsrProviderEditorState {
    fn add() -> Self {
        let fields = AsrProviderEditorFields {
            id: String::new(),
            timeout_ms: String::new(),
            model: String::new(),
            command: SecretInput::new(String::new()),
            args: SecretInput::new("[]".to_owned()),
            environment: Vec::new(),
            endpoint: SecretInput::new(String::new()),
        };
        Self {
            original: None,
            kind: AsrProviderKind::Command,
            endpoint_secure: false,
            baseline: fields.clone(),
            fields,
        }
    }

    fn edit(provider: &AsrProviderConfig) -> Self {
        let fields = AsrProviderEditorFields {
            id: provider.id.clone(),
            timeout_ms: provider
                .timeout_ms
                .map_or_else(String::new, |value| value.to_string()),
            model: provider.model.clone().unwrap_or_default(),
            command: SecretInput::new(provider.command.clone().unwrap_or_default()),
            args: SecretInput::new(
                serde_json::to_string_pretty(&provider.args).unwrap_or_else(|_| "[]".to_owned()),
            ),
            environment: environment_entries(&provider.env),
            endpoint: SecretInput::new(provider.endpoint.clone().unwrap_or_default()),
        };
        Self {
            original: Some(provider.clone()),
            kind: provider.kind.clone(),
            endpoint_secure: endpoint_input_is_secure(fields.endpoint.as_str()),
            baseline: fields.clone(),
            fields,
        }
    }

    fn update(&mut self, field: AsrProviderEditorField, value: SecretInput) {
        let value = value.into_inner();
        match field {
            AsrProviderEditorField::Id if self.original.is_none() => self.fields.id = value,
            AsrProviderEditorField::Id => {}
            AsrProviderEditorField::TimeoutMs => self.fields.timeout_ms = value,
            AsrProviderEditorField::Model => self.fields.model = value,
            AsrProviderEditorField::Command => self.fields.command = SecretInput::new(value),
            AsrProviderEditorField::Args => self.fields.args = SecretInput::new(value),
            AsrProviderEditorField::Endpoint => {
                self.endpoint_secure |= endpoint_input_is_secure(&value);
                self.fields.endpoint = SecretInput::new(value);
            }
        }
    }

    fn update_environment_key(&mut self, index: usize, key: String) {
        if let Some(entry) = self.fields.environment.get_mut(index) {
            entry.key = key;
        }
    }

    fn update_environment_value(&mut self, index: usize, value: SecretInput) {
        if let Some(entry) = self.fields.environment.get_mut(index) {
            entry.value = value;
        }
    }

    fn add_environment(&mut self) {
        self.fields.environment.push(AsrProviderEnvironmentEntry {
            key: String::new(),
            value: SecretInput::new(String::new()),
        });
    }

    fn remove_environment(&mut self, index: usize) {
        if index < self.fields.environment.len() {
            self.fields.environment.remove(index);
        }
    }

    fn set_kind(&mut self, kind: AsrProviderKind) {
        if self.original.is_none() {
            self.kind = kind;
        }
    }

    fn reset(&mut self) {
        self.fields = self.baseline.clone();
        if let Some(original) = &self.original {
            self.kind = original.kind.clone();
        } else {
            self.kind = AsrProviderKind::Command;
        }
        self.endpoint_secure = endpoint_input_is_secure(self.baseline.endpoint.as_str());
    }

    fn is_dirty(&self) -> bool {
        let baseline_kind = self
            .original
            .as_ref()
            .map_or(AsrProviderKind::Command, |provider| provider.kind.clone());
        self.fields != self.baseline || self.kind != baseline_kind
    }

    fn provider(&self) -> Result<AsrProviderConfig, String> {
        let id = self
            .original
            .as_ref()
            .map_or_else(|| self.fields.id.trim(), |provider| provider.id.as_str());
        if id.is_empty() {
            return Err("ASR provider id cannot be empty.".to_owned());
        }
        let mut provider = self.original.clone().unwrap_or_else(|| AsrProviderConfig {
            id: id.to_owned(),
            kind: self.kind.clone(),
            timeout_ms: None,
            model: None,
            hotwords_file: None,
            command: None,
            args: Vec::new(),
            env: HashMap::new(),
            endpoint: None,
        });
        id.clone_into(&mut provider.id);
        provider.kind = self.kind.clone();
        provider.timeout_ms = parse_optional_timeout(&self.fields.timeout_ms)?;
        provider.model = optional_trimmed(&self.fields.model);

        match provider.kind {
            AsrProviderKind::Local => {
                provider.command = None;
                provider.args.clear();
                provider.env.clear();
                provider.endpoint = None;
            }
            AsrProviderKind::Command => {
                let command = self.fields.command.as_str().trim();
                if command.is_empty() {
                    return Err("Command ASR provider command cannot be empty.".to_owned());
                }
                provider.command = Some(command.to_owned());
                provider.args = parse_string_array(self.fields.args.as_str(), "arguments")?;
                provider.env = environment_map(&self.fields.environment)?;
                provider.endpoint = None;
            }
            AsrProviderKind::Remote => {
                let endpoint = self.fields.endpoint.as_str().trim();
                if endpoint.is_empty() {
                    return Err("Remote ASR provider endpoint cannot be empty.".to_owned());
                }
                provider.endpoint = Some(endpoint.to_owned());
                provider.command = None;
                provider.args.clear();
                provider.env.clear();
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
            if self.is_busy()
                && !matches!(
                    message,
                    AsrProviderMessage::MutationFinished(_)
                        | AsrProviderMessage::RemovalFinished(_)
                )
            {
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
            AsrProviderMessage::BeginAdd => self.begin_add_asr_provider(),
            AsrProviderMessage::BeginEdit(id) => self.begin_edit_asr_provider(&id),
            AsrProviderMessage::KindChanged(kind) => {
                if let Some(editor) = &mut self.asr_provider_editor {
                    editor.set_kind(kind);
                }
            }
            AsrProviderMessage::EditorChanged { field, value } => {
                if let Some(editor) = &mut self.asr_provider_editor {
                    editor.update(field, value);
                }
            }
            AsrProviderMessage::EnvironmentKeyChanged { index, key } => {
                if let Some(editor) = &mut self.asr_provider_editor {
                    editor.update_environment_key(index, key);
                }
            }
            AsrProviderMessage::EnvironmentValueChanged { index, value } => {
                if let Some(editor) = &mut self.asr_provider_editor {
                    editor.update_environment_value(index, value);
                }
            }
            AsrProviderMessage::AddEnvironment => {
                if let Some(editor) = &mut self.asr_provider_editor {
                    editor.add_environment();
                }
            }
            AsrProviderMessage::RemoveEnvironment(index) => {
                if let Some(editor) = &mut self.asr_provider_editor {
                    editor.remove_environment(index);
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
            AsrProviderMessage::Remove(provider_id) => {
                return self.begin_custom_asr_provider_removal(&provider_id);
            }
            AsrProviderMessage::MutationFinished(result) => {
                return self.finish_asr_provider_mutation(result);
            }
            AsrProviderMessage::RemovalFinished(result) => {
                return self.finish_custom_asr_provider_removal(result);
            }
        }
        Task::none()
    }

    fn begin_add_asr_provider(&mut self) {
        if self.is_busy() || self.asr_provider_editor.is_some() {
            return;
        }
        if let Err(error) = self.ensure_asr_provider_editor_allowed() {
            self.operation = OperationState::Failed(error);
            return;
        }
        if !self.guard_hotword_changes("adding an ASR provider") {
            return;
        }
        self.asr_provider_editor = Some(AsrProviderEditorState::add());
        self.operation = OperationState::Idle;
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
        let updated = match upsert_asr_provider(&document.config, &editor) {
            Ok(updated) => updated,
            Err(error) => {
                self.operation = OperationState::Failed(error);
                return Task::none();
            }
        };
        let provider_id = editor.fields.id.trim().to_owned();
        let created = editor.original.is_none();
        self.begin_asr_provider_mutation(document.clone(), updated, provider_id, created)
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
        created: bool,
    ) -> Task<Message> {
        self.operation = OperationState::Running("Saving ASR provider…");
        Task::perform(
            async move {
                save_updated_config_with_daemon(&document, &updated).map(|save| {
                    AsrProviderMutationOutcome {
                        save,
                        provider_id,
                        created,
                    }
                })
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
            "{} ASR provider `{}`. Saved {} ({backup}); {}",
            if outcome.created { "Added" } else { "Updated" },
            outcome.provider_id,
            outcome.save.path.display(),
            outcome.save.daemon_reload
        ));
        self.begin_daemon_refresh(false)
    }

    fn begin_custom_asr_provider_removal(&mut self, provider_id: &str) -> Task<Message> {
        if self.asr_provider_editor.is_some() {
            self.operation = OperationState::Failed(
                "Save or cancel the open ASR provider form before removing a provider.".to_owned(),
            );
            return Task::none();
        }
        if let Err(error) = self.ensure_asr_provider_editor_allowed() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if !self.guard_hotword_changes("removing an ASR provider") {
            return Task::none();
        }
        let Ok(document) = &self.config else {
            self.operation = OperationState::Failed("No valid config is loaded.".to_owned());
            return Task::none();
        };
        let updated = match remove_custom_asr_provider_config(&document.config, provider_id) {
            Ok(updated) => updated,
            Err(error) => {
                self.operation = OperationState::Failed(error);
                return Task::none();
            }
        };
        self.operation = OperationState::Running("Removing ASR provider…");
        let document = document.clone();
        let provider_id = provider_id.to_owned();
        Task::perform(
            async move {
                save_updated_config_with_daemon(&document, &updated)
                    .map(|save| AsrProviderRemovalOutcome { save, provider_id })
            },
            |result| Message::AsrProvider(AsrProviderMessage::RemovalFinished(result)),
        )
    }

    fn finish_custom_asr_provider_removal(
        &mut self,
        result: Result<AsrProviderRemovalOutcome, String>,
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
            "Removed custom ASR provider `{}`. Saved {} ({backup}); {}",
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

fn upsert_asr_provider(
    config: &VinputConfig,
    editor: &AsrProviderEditorState,
) -> Result<VinputConfig, String> {
    let provider = editor.provider()?;
    let mut updated = config.clone();
    let Some(original) = &editor.original else {
        if updated
            .asr
            .providers
            .iter()
            .any(|candidate| candidate.id == provider.id)
        {
            return Err(format!(
                "ASR provider `{}` is already configured.",
                provider.id
            ));
        }
        updated.asr.providers.push(provider);
        updated
            .validate()
            .map_err(|error| format!("Validate new ASR provider config: {error}"))?;
        return Ok(updated);
    };
    let Some(index) = updated
        .asr
        .providers
        .iter()
        .position(|candidate| candidate.id == original.id)
    else {
        return Err(format!(
            "ASR provider `{}` is no longer configured.",
            original.id
        ));
    };
    if updated.asr.providers[index] != *original {
        return Err(format!(
            "ASR provider `{}` changed after the form was opened; reopen it before saving.",
            original.id
        ));
    }
    updated.asr.providers[index] = provider;
    updated
        .validate()
        .map_err(|error| format!("Validate updated ASR provider config: {error}"))?;
    Ok(updated)
}

fn remove_custom_asr_provider_config(
    config: &VinputConfig,
    provider_id: &str,
) -> Result<VinputConfig, String> {
    remove_custom_asr_provider_config_with(config, provider_id, |provider| {
        managed_provider_script_path(provider).is_some()
    })
}

fn remove_custom_asr_provider_config_with(
    config: &VinputConfig,
    provider_id: &str,
    is_managed: impl FnOnce(&AsrProviderConfig) -> bool,
) -> Result<VinputConfig, String> {
    if config.asr.active_provider == provider_id {
        return Err(format!(
            "Active ASR provider `{provider_id}` cannot be removed; select another provider first."
        ));
    }
    let mut updated = config.clone();
    let index = updated
        .asr
        .providers
        .iter()
        .position(|provider| provider.id == provider_id)
        .ok_or_else(|| format!("ASR provider `{provider_id}` is not configured."))?;
    if is_managed(&updated.asr.providers[index]) {
        return Err(format!(
            "ASR provider `{provider_id}` is registry-managed and must use managed removal."
        ));
    }
    updated.asr.providers.remove(index);
    updated
        .validate()
        .map_err(|error| format!("Validate configuration after removing {provider_id}: {error}"))?;
    Ok(updated)
}

fn provider_editor_view(editor: &AsrProviderEditorState, busy: bool) -> Element<'_, Message> {
    let dirty = editor.is_dirty();
    let adding = editor.original.is_none();
    column![
        text(if adding {
            "Add custom ASR provider"
        } else {
            "Edit ASR provider"
        })
        .size(22),
        provider_identity_view(editor, busy),
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
        provider_kind_fields(editor, busy),
        row![
            button(if adding {
                "Add provider"
            } else {
                "Update provider"
            })
            .on_press_maybe(
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
    ]
    .spacing(10)
    .into()
}

fn provider_identity_view(editor: &AsrProviderEditorState, busy: bool) -> Element<'_, Message> {
    if editor.original.is_some() {
        return text(format!(
            "Provider id: {} (immutable) · type: {} (immutable)",
            editor.fields.id,
            kind_label(&editor.kind)
        ))
        .into();
    }
    column![
        labeled_input(
            "Provider id",
            "custom-provider",
            &editor.fields.id,
            AsrProviderEditorField::Id,
            false,
        ),
        row![
            text("Provider type").width(160),
            kind_button("Local", AsrProviderKind::Local, &editor.kind, busy),
            kind_button("Command", AsrProviderKind::Command, &editor.kind, busy),
            kind_button("Remote", AsrProviderKind::Remote, &editor.kind, busy),
        ]
        .spacing(10),
    ]
    .spacing(10)
    .into()
}

fn provider_kind_fields(editor: &AsrProviderEditorState, busy: bool) -> Element<'_, Message> {
    match editor.kind {
        AsrProviderKind::Local => {
            text("Hotword path and content remain managed on the Hotwords page.").into()
        }
        AsrProviderKind::Command => column![
            labeled_input(
                "Command",
                "/path/to/provider",
                editor.fields.command.as_str(),
                AsrProviderEditorField::Command,
                false,
            ),
            labeled_input(
                "Arguments",
                "JSON string array",
                editor.fields.args.as_str(),
                AsrProviderEditorField::Args,
                true,
            ),
            environment_editor_view(&editor.fields.environment, busy),
        ]
        .spacing(10)
        .into(),
        AsrProviderKind::Remote => labeled_input(
            "Endpoint",
            "https://provider.example/v1/audio/transcriptions",
            editor.fields.endpoint.as_str(),
            AsrProviderEditorField::Endpoint,
            editor.endpoint_secure,
        ),
    }
}

fn environment_editor_view(
    entries: &[AsrProviderEnvironmentEntry],
    busy: bool,
) -> Element<'_, Message> {
    let mut body = column![
        row![
            text("Environment").size(18).width(Length::Fill),
            button("Add variable").on_press_maybe(
                (!busy).then_some(Message::AsrProvider(AsrProviderMessage::AddEnvironment)),
            ),
        ]
        .spacing(10)
    ]
    .spacing(8);
    if entries.is_empty() {
        body = body.push(text("No environment variables configured."));
    }
    for (index, entry) in entries.iter().enumerate() {
        body = body.push(
            row![
                text_input("Variable name", &entry.key)
                    .on_input(move |key| Message::AsrProvider(
                        AsrProviderMessage::EnvironmentKeyChanged { index, key }
                    ))
                    .width(Length::FillPortion(2)),
                text_input("Value", entry.value.as_str())
                    .secure(true)
                    .on_input(move |value| Message::AsrProvider(
                        AsrProviderMessage::EnvironmentValueChanged {
                            index,
                            value: SecretInput::new(value),
                        }
                    ))
                    .width(Length::FillPortion(3)),
                button("Remove").on_press_maybe((!busy).then_some(Message::AsrProvider(
                    AsrProviderMessage::RemoveEnvironment(index),
                ))),
            ]
            .spacing(10),
        );
    }
    body.into()
}

fn kind_button<'a>(
    label: &'static str,
    kind: AsrProviderKind,
    selected: &AsrProviderKind,
    busy: bool,
) -> iced::widget::Button<'a, Message> {
    button(text(if &kind == selected {
        format!("{label} (selected)")
    } else {
        label.to_owned()
    }))
    .on_press_maybe(
        (!busy && &kind != selected)
            .then_some(Message::AsrProvider(AsrProviderMessage::KindChanged(kind))),
    )
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

fn environment_entries(environment: &HashMap<String, String>) -> Vec<AsrProviderEnvironmentEntry> {
    let mut entries = environment
        .iter()
        .map(|(key, value)| AsrProviderEnvironmentEntry {
            key: key.clone(),
            value: SecretInput::new(value.clone()),
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.key.cmp(&right.key));
    entries
}

fn environment_map(
    entries: &[AsrProviderEnvironmentEntry],
) -> Result<HashMap<String, String>, String> {
    let mut environment = HashMap::with_capacity(entries.len());
    for entry in entries {
        if entry.key.trim().is_empty() {
            return Err("Command environment variable names cannot be empty.".to_owned());
        }
        if environment
            .insert(entry.key.clone(), entry.value.as_str().to_owned())
            .is_some()
        {
            return Err(format!(
                "Command environment variable `{}` is duplicated.",
                entry.key
            ));
        }
    }
    Ok(environment)
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
            env: HashMap::from([
                ("OTHER".to_owned(), "keep".to_owned()),
                ("TOKEN".to_owned(), "secret".to_owned()),
            ]),
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
        let token_index = editor
            .fields
            .environment
            .iter()
            .position(|entry| entry.key == "TOKEN")
            .expect("TOKEN row");
        editor.update_environment_key(token_index, "API_KEY".to_owned());
        editor.update_environment_value(token_index, SecretInput::new("value".to_owned()));

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
        assert_eq!(provider.env.get("OTHER").map(String::as_str), Some("keep"));
    }

    #[test]
    fn add_builds_kind_specific_providers_and_rejects_duplicates() {
        let mut config = VinputConfig::bundled_default().expect("bundled config should validate");

        let mut command = AsrProviderEditorState::add();
        assert!(!command.is_dirty());
        command.update(
            AsrProviderEditorField::Id,
            SecretInput::new(" custom-command ".to_owned()),
        );
        command.update(
            AsrProviderEditorField::Command,
            SecretInput::new(" /opt/custom-provider ".to_owned()),
        );
        command.update(
            AsrProviderEditorField::Args,
            SecretInput::new(r#"["--json"]"#.to_owned()),
        );
        command.add_environment();
        command.update_environment_key(0, "TOKEN".to_owned());
        command.update_environment_value(0, SecretInput::new("secret".to_owned()));
        let command_provider = command
            .provider()
            .expect("command provider should validate");
        assert_eq!(command_provider.id, "custom-command");
        assert_eq!(command_provider.kind, AsrProviderKind::Command);
        assert_eq!(
            command_provider.command.as_deref(),
            Some("/opt/custom-provider")
        );
        assert_eq!(command_provider.args, ["--json"]);
        assert_eq!(
            command_provider.env.get("TOKEN").map(String::as_str),
            Some("secret")
        );
        assert!(command_provider.endpoint.is_none());

        config =
            upsert_asr_provider(&config, &command).expect("new command provider should persist");
        assert!(upsert_asr_provider(&config, &command).is_err());

        let mut local = AsrProviderEditorState::add();
        local.set_kind(AsrProviderKind::Local);
        local.update(
            AsrProviderEditorField::Id,
            SecretInput::new("local-provider".to_owned()),
        );
        local.update(
            AsrProviderEditorField::Model,
            SecretInput::new(" /models/asr ".to_owned()),
        );
        local.update(
            AsrProviderEditorField::Command,
            SecretInput::new("ignored-command".to_owned()),
        );
        local.update(
            AsrProviderEditorField::Endpoint,
            SecretInput::new("https://ignored.invalid".to_owned()),
        );
        let local_provider = local.provider().expect("local provider should validate");
        assert_eq!(local_provider.kind, AsrProviderKind::Local);
        assert_eq!(local_provider.model.as_deref(), Some("/models/asr"));
        assert!(local_provider.command.is_none());
        assert!(local_provider.args.is_empty());
        assert!(local_provider.env.is_empty());
        assert!(local_provider.endpoint.is_none());

        let mut remote = AsrProviderEditorState::add();
        remote.set_kind(AsrProviderKind::Remote);
        remote.update(
            AsrProviderEditorField::Id,
            SecretInput::new("remote-provider".to_owned()),
        );
        remote.update(
            AsrProviderEditorField::Endpoint,
            SecretInput::new(" https://example.invalid/asr ".to_owned()),
        );
        remote.update(
            AsrProviderEditorField::Command,
            SecretInput::new("ignored-command".to_owned()),
        );
        remote.update(
            AsrProviderEditorField::Args,
            SecretInput::new(r#"["ignored"]"#.to_owned()),
        );
        let remote_provider = remote.provider().expect("remote provider should validate");
        assert_eq!(remote_provider.kind, AsrProviderKind::Remote);
        assert_eq!(
            remote_provider.endpoint.as_deref(),
            Some("https://example.invalid/asr")
        );
        assert!(remote_provider.command.is_none());
        assert!(remote_provider.args.is_empty());
        assert!(remote_provider.env.is_empty());
    }

    #[test]
    fn custom_provider_removal_is_config_only_and_rejects_active_or_managed_entries() {
        let provider = command_provider();
        let mut config = VinputConfig::bundled_default().expect("bundled config");
        config.asr.providers.push(provider.clone());

        let updated = remove_custom_asr_provider_config_with(&config, &provider.id, |_| false)
            .expect("inactive custom provider should be removed");
        assert!(
            updated
                .asr
                .providers
                .iter()
                .all(|candidate| candidate.id != provider.id)
        );

        assert!(remove_custom_asr_provider_config_with(&config, &provider.id, |_| true).is_err());
        config.asr.active_provider = provider.id.clone();
        assert!(remove_custom_asr_provider_config_with(&config, &provider.id, |_| false).is_err());
        assert!(remove_custom_asr_provider_config_with(&config, "missing", |_| false).is_err());
    }

    #[test]
    fn edit_mode_ignores_identity_and_kind_messages() {
        let original = command_provider();
        let mut editor = AsrProviderEditorState::edit(&original);
        editor.update(
            AsrProviderEditorField::Id,
            SecretInput::new("forged-id".to_owned()),
        );
        editor.set_kind(AsrProviderKind::Remote);

        let provider = editor
            .provider()
            .expect("edited provider should remain valid");
        assert_eq!(provider.id, original.id);
        assert_eq!(provider.kind, original.kind);
    }

    #[test]
    fn provider_editor_rejects_invalid_timeout_args_environment_and_required_targets() {
        assert!(parse_optional_timeout("0").is_err());
        assert!(parse_optional_timeout("1.5").is_err());
        assert!(parse_string_array("{\"not\":\"array\"}", "arguments").is_err());
        assert!(
            environment_map(&[AsrProviderEnvironmentEntry {
                key: "   ".to_owned(),
                value: SecretInput::new("value".to_owned()),
            }])
            .is_err()
        );
        assert!(
            environment_map(&[
                AsrProviderEnvironmentEntry {
                    key: "DUPLICATE".to_owned(),
                    value: SecretInput::new("one".to_owned()),
                },
                AsrProviderEnvironmentEntry {
                    key: "DUPLICATE".to_owned(),
                    value: SecretInput::new("two".to_owned()),
                },
            ])
            .is_err()
        );

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

        let message = AsrProviderMessage::EnvironmentValueChanged {
            index: 0,
            value: SecretInput::new("message-secret".to_owned()),
        };
        assert!(!format!("{message:?}").contains("message-secret"));
    }

    #[test]
    fn edit_rejects_stale_provider_and_validates_complete_config() {
        let provider = command_provider();
        let editor = AsrProviderEditorState::edit(&provider);
        let mut config = VinputConfig::bundled_default().expect("bundled config should validate");
        config.asr.providers = vec![provider.clone()];
        config.asr.active_provider = provider.id.clone();

        let updated = upsert_asr_provider(&config, &editor).expect("unchanged provider is valid");
        assert_eq!(
            updated.asr.providers.as_slice(),
            std::slice::from_ref(&provider)
        );

        let mut stale = config;
        stale.asr.providers[0].timeout_ms = Some(8_000);
        assert!(upsert_asr_provider(&stale, &editor).is_err());
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

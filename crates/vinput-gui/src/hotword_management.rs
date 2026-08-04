//! Hotword provider selection, path lifecycle, content editing, and persistence.

use std::{
    fmt,
    path::{Path, PathBuf},
};

#[cfg(test)]
use std::fs;

use iced::{
    Element, Length, Task,
    widget::{button, column, pick_list, row, scrollable, text, text_editor, text_input},
};
use vinput_config::{AsrProviderConfig, AsrProviderKind, VinputConfig};

use crate::{
    App, ConfigDocument, ConfigSaveOutcome, DAEMON_RELOAD_REQUESTED, Message, OperationState,
    SecretInput, ensure_config_document_current,
    hotword_activation_retry::{PendingHotwordActivation, retry_hotword_activation},
    hotword_persistence::{
        HotwordContentSnapshot, ensure_hotword_path_update_current, read_hotword_snapshot,
        save_hotword_content_with_daemon, save_hotword_path_with_daemon,
    },
    load_config_document, wait_for_requested_asr_backend,
};

/// One ASR provider that supports hotword files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotwordProviderSelection {
    id: String,
    kind: AsrProviderKind,
}

impl HotwordProviderSelection {
    fn new(provider: &AsrProviderConfig) -> Self {
        Self {
            id: provider.id.clone(),
            kind: provider.kind.clone(),
        }
    }

    fn id(&self) -> &str {
        &self.id
    }
}

impl fmt::Display for HotwordProviderSelection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.kind {
            AsrProviderKind::Local => "local",
            AsrProviderKind::Command => "command",
            AsrProviderKind::Remote => "remote",
        };
        write!(formatter, "Provider: {} · {kind}", self.id)
    }
}

/// Typed hotword lifecycle interaction.
#[derive(Clone)]
pub enum HotwordMessage {
    /// Select one hotword-capable ASR provider.
    ProviderSelected(HotwordProviderSelection),
    /// Update the proposed hotword file path.
    PathChanged(SecretInput),
    /// Persist the proposed path for the selected provider.
    SetPath,
    /// Clear the configured path for the selected provider.
    ClearPath,
    /// Load the configured file into the editor.
    LoadContent,
    /// Result of one asynchronous content load.
    ContentLoaded {
        /// Operation generation used to reject stale completions.
        operation_id: u64,
        /// Loaded content or a path-free error.
        result: Result<LoadedHotwordContent, String>,
    },
    /// Apply one multiline editor action without exposing its payload in diagnostics.
    ContentAction(text_editor::Action),
    /// Atomically save the loaded content.
    SaveContent,
    /// Restore the configured path and last loaded content.
    ResetChanges,
    /// Retry applying an already saved hotword file to the active daemon backend.
    RetryActivation,
    /// Result of one asynchronous activation retry.
    ActivationRetried {
        /// Operation generation used to reject stale completions.
        operation_id: u64,
        /// Secret-free retry summary or error.
        result: Result<String, String>,
    },
    /// Result of one asynchronous content save.
    ContentSaved {
        /// Operation generation used to reject stale completions.
        operation_id: u64,
        /// Secret-free file-save and daemon-activation outcome or error.
        result: Result<HotwordContentSaveOutcome, String>,
    },
    /// Result of one persisted path mutation.
    MutationFinished(Result<HotwordMutationOutcome, String>),
}

impl fmt::Debug for HotwordMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProviderSelected(selection) => formatter
                .debug_tuple("ProviderSelected")
                .field(selection)
                .finish(),
            Self::PathChanged(_) => formatter.write_str("PathChanged(<redacted>)"),
            Self::SetPath => formatter.write_str("SetPath"),
            Self::ClearPath => formatter.write_str("ClearPath"),
            Self::LoadContent => formatter.write_str("LoadContent"),
            Self::ContentLoaded {
                operation_id,
                result,
            } => formatter
                .debug_struct("ContentLoaded")
                .field("operation_id", operation_id)
                .field("result", &result.as_ref().map(|_| "<redacted content>"))
                .finish(),
            Self::ContentAction(_) => formatter.write_str("ContentAction(<redacted>)"),
            Self::SaveContent => formatter.write_str("SaveContent"),
            Self::ResetChanges => formatter.write_str("ResetChanges"),
            Self::RetryActivation => formatter.write_str("RetryActivation"),
            Self::ActivationRetried {
                operation_id,
                result,
            } => formatter
                .debug_struct("ActivationRetried")
                .field("operation_id", operation_id)
                .field("result", &if result.is_ok() { "applied" } else { "failed" })
                .finish(),
            Self::ContentSaved {
                operation_id,
                result,
            } => formatter
                .debug_struct("ContentSaved")
                .field("operation_id", operation_id)
                .field("result", result)
                .finish(),
            Self::MutationFinished(result) => formatter
                .debug_tuple("MutationFinished")
                .field(&result.as_ref().map(|_| "<redacted outcome>"))
                .finish(),
        }
    }
}

/// Result of one persisted hotword path mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotwordMutationOutcome {
    save: ConfigSaveOutcome,
    summary: String,
    activation_error: Option<String>,
}

/// Result of publishing one hotword file and applying it to the active daemon backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotwordContentSaveOutcome {
    /// File-save and activation summary without hotword content or paths.
    pub summary: String,
    /// Safe activation failure, when the file was saved but the active backend did not apply it.
    pub activation_error: Option<String>,
    /// Final disk snapshot, redacted by its custom `Debug` implementation.
    pub(crate) baseline: Option<HotwordContentSnapshot>,
    /// Whether the saved file can be applied again without rewriting it.
    pub(crate) retry_activation: bool,
}

#[derive(Clone)]
pub struct LoadedHotwordContent {
    provider_id: String,
    path: PathBuf,
    snapshot: HotwordContentSnapshot,
}

impl fmt::Debug for LoadedHotwordContent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoadedHotwordContent")
            .field("provider_id", &self.provider_id)
            .field("path", &"<redacted path>")
            .field("snapshot", &self.snapshot)
            .finish()
    }
}

pub(super) struct HotwordEditorState {
    selected_provider: Option<String>,
    configured_path: Option<PathBuf>,
    content_path: Option<PathBuf>,
    content_path_error: Option<String>,
    path_input: String,
    content: text_editor::Content,
    loaded_path: Option<PathBuf>,
    baseline: Option<HotwordContentSnapshot>,
    pending_activation: Option<PendingHotwordActivation>,
}

impl fmt::Debug for HotwordEditorState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HotwordEditorState")
            .field("selected_provider", &self.selected_provider)
            .field("configured_path", &self.configured_path.is_some())
            .field("content_path", &self.content_path.is_some())
            .field("content_path_error", &self.content_path_error.is_some())
            .field("path_dirty", &self.path_is_dirty())
            .field("content_loaded", &self.baseline.is_some())
            .field("content_dirty", &self.content_is_dirty())
            .field("pending_activation", &self.pending_activation.is_some())
            .finish_non_exhaustive()
    }
}

impl HotwordEditorState {
    pub(super) fn empty() -> Self {
        Self {
            selected_provider: None,
            configured_path: None,
            content_path: None,
            content_path_error: None,
            path_input: String::new(),
            content: text_editor::Content::new(),
            loaded_path: None,
            baseline: None,
            pending_activation: None,
        }
    }

    pub(super) fn from_config(config: &VinputConfig, preferred_provider: Option<&str>) -> Self {
        let providers = hotword_provider_options(config);
        let selected_provider = preferred_provider
            .filter(|preferred| providers.iter().any(|provider| provider.id() == *preferred))
            .or_else(|| {
                providers
                    .iter()
                    .any(|provider| provider.id() == config.asr.active_provider)
                    .then_some(config.asr.active_provider.as_str())
            })
            .or_else(|| providers.first().map(HotwordProviderSelection::id))
            .map(str::to_owned);
        let configured_path = selected_provider
            .as_deref()
            .and_then(|provider_id| configured_hotword_path(config, provider_id));
        let (content_path, content_path_error) =
            selected_provider
                .as_deref()
                .map_or(
                    (None, None),
                    |provider_id| match resolved_hotword_content_path(config, provider_id) {
                        Ok(path) => (path, None),
                        Err(error) => (None, Some(error)),
                    },
                );
        Self {
            selected_provider,
            path_input: configured_path
                .as_deref()
                .map_or_else(String::new, |path| path.to_string_lossy().into_owned()),
            configured_path,
            content_path,
            content_path_error,
            content: text_editor::Content::new(),
            loaded_path: None,
            baseline: None,
            pending_activation: None,
        }
    }
    pub(super) fn from_document(
        document: &Result<ConfigDocument, String>,
        preferred_provider: Option<&str>,
    ) -> Self {
        document.as_ref().map_or_else(
            |_| Self::empty(),
            |document| Self::from_config(&document.config, preferred_provider),
        )
    }

    pub(super) fn selected_provider_id(&self) -> Option<&str> {
        self.selected_provider.as_deref()
    }

    pub(super) fn has_unsaved_changes(&self) -> bool {
        self.path_is_dirty() || self.content_is_dirty()
    }

    fn select_provider(&mut self, config: &VinputConfig, provider_id: &str) {
        self.selected_provider = Some(provider_id.to_owned());
        self.configured_path = configured_hotword_path(config, provider_id);
        (self.content_path, self.content_path_error) =
            match resolved_hotword_content_path(config, provider_id) {
                Ok(path) => (path, None),
                Err(error) => (None, Some(error)),
            };
        self.path_input = self
            .configured_path
            .as_deref()
            .map_or_else(String::new, |path| path.to_string_lossy().into_owned());
        self.clear_loaded_content();
    }

    fn clear_loaded_content(&mut self) {
        self.content = text_editor::Content::new();
        self.loaded_path = None;
        self.baseline = None;
        self.pending_activation = None;
    }

    fn reset_changes(&mut self) {
        self.path_input = self
            .configured_path
            .as_deref()
            .map_or_else(String::new, |path| path.to_string_lossy().into_owned());
        if let Some(baseline) = &self.baseline {
            self.content = text_editor::Content::with_text(&baseline.content);
        }
    }

    fn apply_loaded(&mut self, loaded: LoadedHotwordContent) {
        self.content = text_editor::Content::with_text(&loaded.snapshot.content);
        self.loaded_path = Some(loaded.path);
        self.baseline = Some(loaded.snapshot);
        self.pending_activation = None;
    }

    fn path_is_dirty(&self) -> bool {
        let proposed = self.path_input.trim();
        match &self.configured_path {
            Some(configured) => Path::new(proposed) != configured,
            None => !proposed.is_empty(),
        }
    }

    fn content_is_dirty(&self) -> bool {
        self.baseline
            .as_ref()
            .is_some_and(|baseline| baseline.content != self.content.text())
    }

    fn content_matches_target(&self) -> bool {
        self.loaded_path.is_some() && self.loaded_path == self.content_path
    }
}

impl App {
    pub(super) fn guard_hotword_changes(&mut self, blocked_action: &str) -> bool {
        if self.hotword_editor.has_unsaved_changes() {
            self.operation = OperationState::Failed(format!(
                "Save or reset hotword changes before {blocked_action}."
            ));
            false
        } else {
            true
        }
    }

    pub(super) fn refresh_hotword_editor(&mut self, config: &Result<ConfigDocument, String>) {
        let preferred = self
            .hotword_editor
            .selected_provider_id()
            .map(str::to_owned);
        self.hotword_editor = HotwordEditorState::from_document(config, preferred.as_deref());
        self.active_hotword_operation_id = None;
    }

    pub(super) fn handle_hotword_message(&mut self, message: HotwordMessage) -> Task<Message> {
        match message {
            HotwordMessage::ProviderSelected(selection) => {
                self.select_hotword_provider(&selection);
            }
            HotwordMessage::PathChanged(value) => {
                self.hotword_editor.path_input = value.into_inner();
                self.hotword_editor.pending_activation = None;
            }
            HotwordMessage::SetPath => return self.begin_hotword_path_set(),
            HotwordMessage::ClearPath => return self.begin_hotword_path_clear(),
            HotwordMessage::LoadContent => return self.begin_hotword_content_load(),
            HotwordMessage::ContentLoaded {
                operation_id,
                result,
            } => self.finish_hotword_content_load(operation_id, result),
            HotwordMessage::ContentAction(action) => {
                if self.hotword_editor.baseline.is_some() {
                    self.hotword_editor.content.perform(action);
                    self.hotword_editor.pending_activation = None;
                }
            }
            HotwordMessage::SaveContent => return self.begin_hotword_content_save(),
            HotwordMessage::RetryActivation => return self.begin_hotword_activation_retry(),
            HotwordMessage::ActivationRetried {
                operation_id,
                result,
            } => return self.finish_hotword_activation_retry(operation_id, result),
            HotwordMessage::ResetChanges => {
                self.hotword_editor.reset_changes();
                self.operation = OperationState::Idle;
            }
            HotwordMessage::ContentSaved {
                operation_id,
                result,
            } => return self.finish_hotword_content_save(operation_id, result),
            HotwordMessage::MutationFinished(result) => {
                return self.finish_hotword_mutation(result);
            }
        }
        Task::none()
    }

    fn select_hotword_provider(&mut self, selection: &HotwordProviderSelection) {
        if self.hotword_editor.has_unsaved_changes() {
            self.operation = OperationState::Failed(
                "Save or reset hotword changes before selecting another provider.".to_owned(),
            );
            return;
        }
        let Ok(document) = &self.config else {
            self.operation = OperationState::Failed("No valid config is loaded.".to_owned());
            return;
        };
        if !hotword_provider_options(&document.config)
            .iter()
            .any(|provider| provider == selection)
        {
            self.operation =
                OperationState::Failed("The selected hotword provider is unavailable.".to_owned());
            return;
        }
        self.hotword_editor
            .select_provider(&document.config, selection.id());
        self.operation = OperationState::Idle;
    }

    fn begin_hotword_path_set(&mut self) -> Task<Message> {
        if self.hotword_editor.content_is_dirty() {
            self.operation = OperationState::Failed(
                "Save the edited hotword content before changing its configured path.".to_owned(),
            );
            return Task::none();
        }
        let path = self.hotword_editor.path_input.trim();
        if path.is_empty() {
            self.operation =
                OperationState::Failed("Hotword file path cannot be empty.".to_owned());
            return Task::none();
        }
        let path = path.to_owned();
        self.begin_hotword_path_mutation(Some(&path), "Setting hotword path…")
    }

    fn begin_hotword_path_clear(&mut self) -> Task<Message> {
        if self.hotword_editor.content_is_dirty() {
            self.operation = OperationState::Failed(
                "Save the edited hotword content before clearing its configured path.".to_owned(),
            );
            return Task::none();
        }
        self.begin_hotword_path_mutation(None, "Clearing hotword path…")
    }

    fn begin_hotword_path_mutation(
        &mut self,
        path: Option<&str>,
        progress: &'static str,
    ) -> Task<Message> {
        if let Err(error) = self.ensure_no_unsaved_config_draft() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        let Some(provider_id) = self.hotword_editor.selected_provider.clone() else {
            self.operation =
                OperationState::Failed("No hotword-capable provider is selected.".to_owned());
            return Task::none();
        };
        let Ok(document) = &self.config else {
            self.operation = OperationState::Failed("No valid config is loaded.".to_owned());
            return Task::none();
        };
        let updated = match update_hotword_path(&document.config, &provider_id, path) {
            Ok(updated) => updated,
            Err(error) => {
                self.operation = OperationState::Failed(error);
                return Task::none();
            }
        };
        let prerequisite_path = if path.is_some() {
            match local_hotword_prerequisite_path(&updated, &provider_id) {
                Ok(path) => path,
                Err(error) => {
                    self.operation = OperationState::Failed(error);
                    return Task::none();
                }
            }
        } else {
            None
        };
        let summary = if path.is_some() {
            format!("Updated hotword path for provider `{provider_id}`.")
        } else {
            format!("Cleared hotword path for provider `{provider_id}`.")
        };
        let document = document.clone();
        self.operation = OperationState::Running(progress);
        Task::perform(
            async move {
                let should_confirm = updated.asr.active_provider == provider_id;
                let mut save = save_hotword_path_with_daemon(
                    &document,
                    &updated,
                    prerequisite_path.as_deref(),
                )?;
                let activation_error =
                    if should_confirm && save.daemon_reload == DAEMON_RELOAD_REQUESTED {
                        match wait_for_requested_asr_backend(&provider_id) {
                            Ok(summary) => {
                                match ensure_hotword_path_update_current(&save.path, &updated) {
                                    Ok(()) => {
                                        save.daemon_reload = summary;
                                        None
                                    }
                                    Err(error) => Some(error),
                                }
                            }
                            Err(error) => Some(error),
                        }
                    } else {
                        None
                    };
                Ok(HotwordMutationOutcome {
                    save,
                    summary,
                    activation_error,
                })
            },
            |result| Message::Hotword(HotwordMessage::MutationFinished(result)),
        )
    }

    fn finish_hotword_mutation(
        &mut self,
        result: Result<HotwordMutationOutcome, String>,
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
        let summary = format!(
            "{} Saved {} ({backup}); {}",
            outcome.summary,
            outcome.save.path.display(),
            outcome.save.daemon_reload
        );
        self.operation = match outcome.activation_error {
            Some(error) => OperationState::Failed(format!("{summary}. {error}")),
            None => OperationState::Succeeded(summary),
        };
        self.begin_daemon_refresh(false)
    }

    fn begin_hotword_content_load(&mut self) -> Task<Message> {
        if self.hotword_editor.content_is_dirty() {
            self.operation = OperationState::Failed(
                "Save the edited hotword content before loading it again.".to_owned(),
            );
            return Task::none();
        }
        if self.hotword_editor.path_is_dirty() {
            self.operation = OperationState::Failed(
                "Set or reset the hotword path before loading content.".to_owned(),
            );
            return Task::none();
        }
        let Some(provider_id) = self.hotword_editor.selected_provider.clone() else {
            self.operation =
                OperationState::Failed("No hotword-capable provider is selected.".to_owned());
            return Task::none();
        };
        let Some(path) = self.hotword_editor.content_path.clone() else {
            self.operation = OperationState::Failed(
                self.hotword_editor
                    .content_path_error
                    .clone()
                    .unwrap_or_else(|| {
                        "Set a hotword file path before loading content.".to_owned()
                    }),
            );
            return Task::none();
        };
        let operation_id = self.next_hotword_operation_id;
        self.next_hotword_operation_id = self.next_hotword_operation_id.saturating_add(1);
        self.active_hotword_operation_id = Some(operation_id);
        self.operation = OperationState::Running("Loading hotword content…");
        Task::perform(
            async move {
                read_hotword_snapshot(&path).map(|snapshot| LoadedHotwordContent {
                    provider_id,
                    path,
                    snapshot,
                })
            },
            move |result| {
                Message::Hotword(HotwordMessage::ContentLoaded {
                    operation_id,
                    result,
                })
            },
        )
    }

    fn finish_hotword_content_load(
        &mut self,
        operation_id: u64,
        result: Result<LoadedHotwordContent, String>,
    ) {
        if self.active_hotword_operation_id != Some(operation_id) {
            return;
        }
        self.active_hotword_operation_id = None;
        let loaded = match result {
            Ok(loaded) => loaded,
            Err(error) => {
                self.operation = OperationState::Failed(error);
                return;
            }
        };
        if self.hotword_editor.selected_provider.as_deref() != Some(&loaded.provider_id)
            || self.hotword_editor.content_path.as_ref() != Some(&loaded.path)
        {
            self.operation = OperationState::Failed(
                "Discarded stale hotword content loaded for a previous selection.".to_owned(),
            );
            return;
        }
        let existed = loaded.snapshot.existed;
        self.hotword_editor.apply_loaded(loaded);
        self.operation = OperationState::Succeeded(if existed {
            "Loaded configured hotword content.".to_owned()
        } else {
            "Configured hotword file does not exist yet; loaded an empty editor.".to_owned()
        });
    }

    fn begin_hotword_content_save(&mut self) -> Task<Message> {
        if self.hotword_editor.path_is_dirty() {
            self.operation = OperationState::Failed(
                "Set or reset the hotword path before saving content.".to_owned(),
            );
            return Task::none();
        }
        if !self.hotword_editor.content_matches_target() {
            self.operation = OperationState::Failed(
                "Load the configured hotword file before saving content.".to_owned(),
            );
            return Task::none();
        }
        let Some(path) = self.hotword_editor.content_path.clone() else {
            self.operation = OperationState::Failed(
                self.hotword_editor
                    .content_path_error
                    .clone()
                    .unwrap_or_else(|| "Set a hotword file path before saving content.".to_owned()),
            );
            return Task::none();
        };
        let Some(expected) = self.hotword_editor.baseline.clone() else {
            self.operation = OperationState::Failed(
                "Load the configured hotword file before saving content.".to_owned(),
            );
            return Task::none();
        };
        let Some(provider_id) = self.hotword_editor.selected_provider.clone() else {
            self.operation =
                OperationState::Failed("No hotword-capable provider is selected.".to_owned());
            return Task::none();
        };
        let Ok(document) = &self.config else {
            self.operation = OperationState::Failed("No valid config is loaded.".to_owned());
            return Task::none();
        };
        let document = document.clone();
        let content = self.hotword_editor.content.text();
        let operation_id = self.next_hotword_operation_id;
        self.next_hotword_operation_id = self.next_hotword_operation_id.saturating_add(1);
        self.active_hotword_operation_id = Some(operation_id);
        self.operation = OperationState::Running("Saving hotword content…");
        Task::perform(
            async move {
                save_hotword_content_for_document(
                    &document,
                    &provider_id,
                    &path,
                    &expected,
                    &content,
                )
            },
            move |result| {
                Message::Hotword(HotwordMessage::ContentSaved {
                    operation_id,
                    result,
                })
            },
        )
    }

    fn finish_hotword_content_save(
        &mut self,
        operation_id: u64,
        result: Result<HotwordContentSaveOutcome, String>,
    ) -> Task<Message> {
        if self.active_hotword_operation_id != Some(operation_id) {
            return Task::none();
        }
        self.active_hotword_operation_id = None;
        match result {
            Ok(outcome) => {
                let HotwordContentSaveOutcome {
                    summary,
                    activation_error,
                    baseline,
                    retry_activation,
                } = outcome;
                if let Some(baseline) = baseline {
                    self.hotword_editor.baseline = Some(baseline.clone());
                    self.hotword_editor.pending_activation = retry_activation.then(|| {
                        PendingHotwordActivation::new(
                            self.hotword_editor
                                .selected_provider
                                .clone()
                                .expect("content save requires a selected provider"),
                            self.hotword_editor
                                .content_path
                                .clone()
                                .expect("content save requires a resolved path"),
                            baseline,
                        )
                    });
                } else {
                    self.hotword_editor.pending_activation = None;
                }
                self.operation = match activation_error {
                    Some(error) => OperationState::Failed(format!("{summary} {error}")),
                    None => OperationState::Succeeded(summary),
                };
                self.begin_daemon_refresh(false)
            }
            Err(error) => {
                self.operation = OperationState::Failed(error);
                Task::none()
            }
        }
    }

    fn begin_hotword_activation_retry(&mut self) -> Task<Message> {
        let Some(pending) = self.hotword_editor.pending_activation.clone() else {
            self.operation =
                OperationState::Failed("No saved hotword activation is pending retry.".to_owned());
            return Task::none();
        };
        let Ok(document) = &self.config else {
            self.operation = OperationState::Failed("No valid config is loaded.".to_owned());
            return Task::none();
        };
        let document = document.clone();
        let operation_id = self.next_hotword_operation_id;
        self.next_hotword_operation_id = self.next_hotword_operation_id.saturating_add(1);
        self.active_hotword_operation_id = Some(operation_id);
        self.operation = OperationState::Running("Retrying hotword activation…");
        Task::perform(
            async move { retry_hotword_activation(&document, &pending) },
            move |result| {
                Message::Hotword(HotwordMessage::ActivationRetried {
                    operation_id,
                    result,
                })
            },
        )
    }

    fn finish_hotword_activation_retry(
        &mut self,
        operation_id: u64,
        result: Result<String, String>,
    ) -> Task<Message> {
        if self.active_hotword_operation_id != Some(operation_id) {
            return Task::none();
        }
        self.active_hotword_operation_id = None;
        self.operation = match result {
            Ok(summary) => {
                self.hotword_editor.pending_activation = None;
                OperationState::Succeeded(summary)
            }
            Err(error) => OperationState::Failed(error),
        };
        self.begin_daemon_refresh(false)
    }

    pub(super) fn hotwords_page(&self) -> Element<'_, Message> {
        let busy = self.is_busy();
        let mut body = column![text("Hotwords").size(30)].spacing(12);
        if let Some(notice) = self.operation_notice() {
            body = body.push(notice);
        }
        let Ok(document) = &self.config else {
            return scrollable(body.push(text("No valid configuration is loaded."))).into();
        };
        let provider_options = hotword_provider_options(&document.config);
        if provider_options.is_empty() {
            return scrollable(body.push(text(
                "No local or command ASR provider supports hotword files.",
            )))
            .into();
        }
        body = body.push(self.hotword_provider_picker(&provider_options, busy));
        body = body.push(self.hotword_path_controls(busy));
        body = body.push(self.hotword_content_controls(busy));
        body = body.push(self.hotword_content_editor(busy));
        scrollable(body).into()
    }

    fn hotword_provider_picker<'a>(
        &'a self,
        provider_options: &[HotwordProviderSelection],
        busy: bool,
    ) -> Element<'a, Message> {
        let selected = self
            .hotword_editor
            .selected_provider
            .as_deref()
            .and_then(|id| {
                provider_options
                    .iter()
                    .find(|provider| provider.id() == id)
                    .cloned()
            });
        let provider_picker: Element<'_, Message> = if busy {
            text(
                selected
                    .as_ref()
                    .map_or_else(|| "No provider selected".to_owned(), ToString::to_string),
            )
            .width(Length::Fill)
            .into()
        } else {
            pick_list(provider_options.to_vec(), selected, |selection| {
                Message::Hotword(HotwordMessage::ProviderSelected(selection))
            })
            .width(Length::Fill)
            .into()
        };
        row![text("ASR provider").width(160), provider_picker]
            .spacing(10)
            .align_y(iced::Alignment::Center)
            .into()
    }

    fn hotword_path_controls(&self, busy: bool) -> Element<'_, Message> {
        let path_dirty = self.hotword_editor.path_is_dirty();
        let content_dirty = self.hotword_editor.content_is_dirty();
        row![
            text("Hotword file").width(160),
            text_input(
                "Path to a UTF-8 hotword file",
                &self.hotword_editor.path_input
            )
            .on_input_maybe((!busy).then_some(|value| {
                Message::Hotword(HotwordMessage::PathChanged(SecretInput::new(value)))
            }))
            .width(Length::Fill),
            button("Set path").on_press_maybe(
                (!busy
                    && path_dirty
                    && !content_dirty
                    && !self.hotword_editor.path_input.trim().is_empty())
                .then_some(Message::Hotword(HotwordMessage::SetPath)),
            ),
            button("Clear path").on_press_maybe(
                (!busy && self.hotword_editor.configured_path.is_some() && !content_dirty)
                    .then_some(Message::Hotword(HotwordMessage::ClearPath)),
            ),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center)
        .into()
    }

    fn hotword_content_controls(&self, busy: bool) -> Element<'_, Message> {
        let path_dirty = self.hotword_editor.path_is_dirty();
        let content_dirty = self.hotword_editor.content_is_dirty();
        row![
            button("Load content").on_press_maybe(
                (!busy
                    && self.hotword_editor.content_path.is_some()
                    && !path_dirty
                    && !content_dirty)
                    .then_some(Message::Hotword(HotwordMessage::LoadContent)),
            ),
            button("Save content").on_press_maybe(
                (!busy
                    && !path_dirty
                    && content_dirty
                    && self.hotword_editor.content_matches_target())
                .then_some(Message::Hotword(HotwordMessage::SaveContent)),
            ),
            button("Reset changes").on_press_maybe(
                (!busy && self.hotword_editor.has_unsaved_changes())
                    .then_some(Message::Hotword(HotwordMessage::ResetChanges)),
            ),
            button("Retry activation").on_press_maybe(
                (!busy
                    && !path_dirty
                    && !content_dirty
                    && self.hotword_editor.pending_activation.is_some())
                .then_some(Message::Hotword(HotwordMessage::RetryActivation)),
            ),
            text(self.hotword_content_status(content_dirty)),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center)
        .into()
    }

    fn hotword_content_editor(&self, busy: bool) -> Element<'_, Message> {
        if busy || self.hotword_editor.baseline.is_none() {
            text_editor::<Message, iced::Theme, iced::Renderer>(&self.hotword_editor.content)
                .placeholder("One hotword entry per line")
                .height(Length::Fixed(320.0))
                .into()
        } else {
            text_editor::<Message, iced::Theme, iced::Renderer>(&self.hotword_editor.content)
                .placeholder("One hotword entry per line")
                .height(Length::Fixed(320.0))
                .on_action(|action| Message::Hotword(HotwordMessage::ContentAction(action)))
                .into()
        }
    }

    fn hotword_content_status(&self, content_dirty: bool) -> &str {
        if let Some(error) = &self.hotword_editor.content_path_error {
            error
        } else if self.hotword_editor.pending_activation.is_some() {
            "Hotword content is saved; daemon activation can be retried"
        } else if self.hotword_editor.baseline.is_some() {
            if content_dirty {
                "Unsaved hotword content"
            } else {
                "Hotword content is unchanged"
            }
        } else {
            "Load the configured file to edit its contents"
        }
    }
}

fn hotword_provider_options(config: &VinputConfig) -> Vec<HotwordProviderSelection> {
    config
        .asr
        .providers
        .iter()
        .filter(|provider| hotword_kind_supported(&provider.kind))
        .map(HotwordProviderSelection::new)
        .collect()
}

fn configured_hotword_path(config: &VinputConfig, provider_id: &str) -> Option<PathBuf> {
    config
        .asr
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .and_then(|provider| provider.hotwords_file.as_deref())
        .map(PathBuf::from)
}

fn local_hotword_prerequisite_path(
    config: &VinputConfig,
    provider_id: &str,
) -> Result<Option<PathBuf>, String> {
    let provider = config
        .asr
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| format!("ASR provider `{provider_id}` is no longer configured."))?;
    if provider.kind == AsrProviderKind::Local {
        resolved_hotword_content_path(config, provider_id)
    } else {
        Ok(None)
    }
}

fn save_hotword_content_for_document(
    document: &ConfigDocument,
    provider_id: &str,
    expected_path: &Path,
    expected: &HotwordContentSnapshot,
    content: &str,
) -> Result<HotwordContentSaveOutcome, String> {
    let active_provider_id =
        (document.config.asr.active_provider == provider_id).then_some(provider_id);
    save_hotword_content_with_daemon(expected_path, expected, content, active_provider_id, || {
        ensure_config_document_current(document)?;
        let current_path = resolved_hotword_content_path(&document.config, provider_id)?;
        if current_path.as_deref() != Some(expected_path) {
            return Err(
                "The configured hotword target changed after loading; reload configuration and content before saving."
                    .to_owned(),
            );
        }
        Ok(())
    })
}

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
    let configured = Path::new(configured);
    if configured.is_absolute() {
        return Ok(Some(configured.to_path_buf()));
    }
    match provider.kind {
        AsrProviderKind::Local => {
            let model = provider
                .model
                .as_deref()
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
            Ok(Some(model.join(configured)))
        }
        AsrProviderKind::Command => Err(
            "Relative hotword paths for command providers are resolved by the external command; configure an absolute path to edit content in the GUI."
                .to_owned(),
        ),
        AsrProviderKind::Remote => Err(format!(
            "ASR provider `{provider_id}` does not support hotword files."
        )),
    }
}

fn update_hotword_path(
    config: &VinputConfig,
    provider_id: &str,
    path: Option<&str>,
) -> Result<VinputConfig, String> {
    let mut updated = config.clone();
    let provider = updated
        .asr
        .providers
        .iter_mut()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| format!("ASR provider `{provider_id}` is no longer configured."))?;
    if !hotword_kind_supported(&provider.kind) {
        return Err(format!(
            "ASR provider `{provider_id}` does not support hotword files."
        ));
    }
    provider.hotwords_file = path
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_owned);
    if path.is_some() && provider.hotwords_file.is_none() {
        return Err("Hotword file path cannot be empty.".to_owned());
    }
    updated
        .validate()
        .map_err(|error| format!("Validate hotword configuration: {error}"))?;
    Ok(updated)
}

const fn hotword_kind_supported(kind: &AsrProviderKind) -> bool {
    matches!(kind, AsrProviderKind::Local | AsrProviderKind::Command)
}

#[cfg(test)]
mod tests;

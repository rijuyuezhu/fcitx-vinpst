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
use vinput_config::{AsrProviderConfig, AsrProviderKind, VinputConfig, sherpa_model_root};

use crate::{
    App, ConfigDocument, ConfigSaveOutcome, Message, OperationState, SecretInput,
    ensure_config_document_current,
    hotword_persistence::{
        HotwordContentSnapshot, read_hotword_snapshot, save_hotword_content_with_daemon,
        save_hotword_path_with_daemon,
    },
    load_config_document,
};

#[cfg(test)]
use crate::hotword_persistence::save_hotword_content_with_reload;

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
    /// Result of one asynchronous content save.
    ContentSaved {
        /// Operation generation used to reject stale completions.
        operation_id: u64,
        /// Secret-free save summary or error.
        result: Result<String, String>,
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
                }
            }
            HotwordMessage::SaveContent => return self.begin_hotword_content_save(),
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
                save_hotword_path_with_daemon(&document, &updated, prerequisite_path.as_deref())
                    .map(|save| HotwordMutationOutcome { save, summary })
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
        self.operation = OperationState::Succeeded(format!(
            "{} Saved {} ({backup}); {}",
            outcome.summary,
            outcome.save.path.display(),
            outcome.save.daemon_reload
        ));
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
        result: Result<String, String>,
    ) -> Task<Message> {
        if self.active_hotword_operation_id != Some(operation_id) {
            return Task::none();
        }
        self.active_hotword_operation_id = None;
        match result {
            Ok(summary) => {
                self.hotword_editor.baseline = Some(HotwordContentSnapshot {
                    existed: true,
                    content: self.hotword_editor.content.text(),
                });
                self.operation = OperationState::Succeeded(summary);
                self.begin_daemon_refresh(false)
            }
            Err(error) => {
                self.operation = OperationState::Failed(error);
                Task::none()
            }
        }
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
) -> Result<String, String> {
    save_hotword_content_with_daemon(expected_path, expected, content, || {
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

fn resolved_hotword_content_path(
    config: &VinputConfig,
    provider_id: &str,
) -> Result<Option<PathBuf>, String> {
    resolve_hotword_content_path(config, provider_id, || Ok(sherpa_model_root()))
}

#[cfg(test)]
fn resolved_hotword_content_path_with_model_root(
    config: &VinputConfig,
    provider_id: &str,
    model_root: &Path,
) -> Result<Option<PathBuf>, String> {
    resolve_hotword_content_path(config, provider_id, || Ok(model_root.to_path_buf()))
}

fn resolve_hotword_content_path(
    config: &VinputConfig,
    provider_id: &str,
    model_root: impl FnOnce() -> Result<PathBuf, String>,
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
            let model_dir = if model.is_absolute() {
                model.to_path_buf()
            } else {
                model_root()?.join(model)
            };
            Ok(Some(model_dir.join(configured)))
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
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn provider(id: &str, kind: AsrProviderKind) -> AsrProviderConfig {
        let endpoint =
            (kind == AsrProviderKind::Remote).then(|| "https://example.invalid/asr".to_owned());
        let command = (kind == AsrProviderKind::Command).then(|| "/bin/true".to_owned());
        AsrProviderConfig {
            id: id.to_owned(),
            kind,
            timeout_ms: None,
            model: None,
            hotwords_file: None,
            command,
            args: Vec::new(),
            env: HashMap::new(),
            endpoint,
        }
    }

    #[test]
    fn provider_options_include_only_hotword_capable_backends() {
        let mut config = VinputConfig::bundled_default().expect("bundled config");
        config.asr.providers = vec![
            provider("local", AsrProviderKind::Local),
            provider("remote", AsrProviderKind::Remote),
            provider("command", AsrProviderKind::Command),
        ];
        config.asr.active_provider = "remote".to_owned();

        let options = hotword_provider_options(&config);
        assert_eq!(
            options
                .iter()
                .map(HotwordProviderSelection::id)
                .collect::<Vec<_>>(),
            vec!["local", "command"]
        );
        assert_eq!(
            HotwordEditorState::from_config(&config, None)
                .selected_provider
                .as_deref(),
            Some("local")
        );
    }

    #[test]
    fn path_mutation_sets_clears_and_rejects_remote_providers() {
        let mut config = VinputConfig::bundled_default().expect("bundled config");
        config.asr.providers = vec![
            provider("local", AsrProviderKind::Local),
            provider("remote", AsrProviderKind::Remote),
        ];
        config.asr.active_provider = "local".to_owned();

        let updated =
            update_hotword_path(&config, "local", Some("  words.txt  ")).expect("set hotword path");
        assert_eq!(
            updated.asr.providers[0].hotwords_file.as_deref(),
            Some("words.txt")
        );
        let cleared = update_hotword_path(&updated, "local", None).expect("clear hotword path");
        assert_eq!(cleared.asr.providers[0].hotwords_file, None);
        assert!(update_hotword_path(&config, "remote", Some("words.txt")).is_err());
    }

    #[test]
    fn content_path_matches_runtime_resolution_rules() {
        let model_root = Path::new("/managed-models");
        let mut config = VinputConfig::bundled_default().expect("bundled config");
        let mut local = provider("local", AsrProviderKind::Local);
        local.model = Some("paraformer".to_owned());
        local.hotwords_file = Some("hotwords.txt".to_owned());
        let mut command = provider("command", AsrProviderKind::Command);
        command.hotwords_file = Some("relative-command-hotwords.txt".to_owned());
        config.asr.providers = vec![local, command];
        config.asr.active_provider = "local".to_owned();

        assert_eq!(
            resolved_hotword_content_path_with_model_root(&config, "local", model_root)
                .expect("resolve local hotwords"),
            Some(model_root.join("paraformer/hotwords.txt"))
        );
        let command_error =
            resolved_hotword_content_path_with_model_root(&config, "command", model_root)
                .expect_err("relative command path is external");
        assert!(command_error.contains("external command"));

        config.asr.providers[1].hotwords_file = Some("/tmp/command-hotwords.txt".to_owned());
        assert_eq!(
            resolved_hotword_content_path_with_model_root(&config, "command", model_root)
                .expect("resolve absolute command hotwords"),
            Some(PathBuf::from("/tmp/command-hotwords.txt"))
        );
    }

    #[test]
    fn content_save_is_atomic_conflict_aware_and_rolls_back_reload_failures() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("hotwords.txt");
        fs::write(&path, "alpha\n").expect("write fixture");
        let baseline = read_hotword_snapshot(&path).expect("read baseline");

        let summary = save_hotword_content_with_reload(&path, &baseline, "beta\n", || {
            Ok("fixture reload".to_owned())
        })
        .expect("save content");
        assert!(summary.contains("fixture reload"));
        assert_eq!(fs::read_to_string(&path).expect("saved content"), "beta\n");

        let loaded = read_hotword_snapshot(&path).expect("read saved content");
        fs::write(&path, "external\n").expect("external update");
        let conflict = save_hotword_content_with_reload(&path, &loaded, "gamma\n", || {
            Ok("unreachable reload".to_owned())
        })
        .expect_err("reject external update");
        assert!(conflict.contains("changed outside"));
        assert_eq!(
            fs::read_to_string(&path).expect("external content"),
            "external\n"
        );

        let external = read_hotword_snapshot(&path).expect("read external content");
        let reload_error = save_hotword_content_with_reload(&path, &external, "delta\n", || {
            Err("fixture reload failure".to_owned())
        })
        .expect_err("rollback reload failure");
        assert!(reload_error.contains("previous content restored"));
        assert_eq!(
            fs::read_to_string(&path).expect("restored content"),
            "external\n"
        );
    }

    #[test]
    fn content_save_rejects_external_config_target_changes_before_write() {
        let directory = tempfile::tempdir().expect("temp dir");
        let config_path = directory.path().join("config.json");
        let old_path = directory.path().join("old-hotwords.txt");
        let new_path = directory.path().join("new-hotwords.txt");
        fs::write(&old_path, "alpha\n").expect("old hotwords fixture");

        let mut config = VinputConfig::bundled_default().expect("bundled config");
        let mut local = provider("local", AsrProviderKind::Local);
        local.model = Some(
            directory
                .path()
                .join("model")
                .to_string_lossy()
                .into_owned(),
        );
        local.hotwords_file = Some(old_path.to_string_lossy().into_owned());
        config.asr.providers = vec![local];
        config.asr.active_provider = "local".to_owned();
        fs::write(
            &config_path,
            serde_json::to_vec_pretty(&config).expect("serialize config"),
        )
        .expect("write config");
        let document = ConfigDocument {
            path: config_path.clone(),
            from_disk: true,
            config: config.clone(),
        };
        let baseline = read_hotword_snapshot(&old_path).expect("read hotwords");

        config.asr.providers[0].hotwords_file = Some(new_path.to_string_lossy().into_owned());
        fs::write(
            &config_path,
            serde_json::to_vec_pretty(&config).expect("serialize external config"),
        )
        .expect("write external config");

        let error = save_hotword_content_for_document(
            &document,
            "local",
            &old_path,
            &baseline,
            "should-not-write\n",
        )
        .expect_err("reject external config change");
        assert!(error.contains("changed on disk"));
        assert_eq!(
            fs::read_to_string(&old_path).expect("old content"),
            "alpha\n"
        );
        assert!(!new_path.exists());
    }

    #[test]
    fn hotword_messages_redact_paths_and_loaded_content() {
        let path_message = HotwordMessage::PathChanged(SecretInput::new(
            "/home/user/private/hotwords.txt".to_owned(),
        ));
        assert!(!format!("{path_message:?}").contains("/home/user"));

        let loaded = LoadedHotwordContent {
            provider_id: "local".to_owned(),
            path: PathBuf::from("/home/user/private/hotwords.txt"),
            snapshot: HotwordContentSnapshot {
                existed: true,
                content: "private phrase".to_owned(),
            },
        };
        let message = HotwordMessage::ContentLoaded {
            operation_id: 7,
            result: Ok(loaded),
        };
        let debug = format!("{message:?}");
        assert!(!debug.contains("private phrase"));
        assert!(!debug.contains("/home/user"));
    }
}

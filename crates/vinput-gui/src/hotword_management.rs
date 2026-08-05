//! Hotword provider selection, path lifecycle, content editing, and persistence.

mod file_picker;
mod view;

use std::{
    fmt,
    path::{Path, PathBuf},
};

#[cfg(test)]
use std::fs;

use iced::{Task, widget::text_editor};
use vinput_config::{AsrProviderConfig, AsrProviderKind, VinputConfig};

use crate::{
    App, ConfigDocument, ConfigSaveOutcome, DAEMON_RELOAD_REQUESTED, GuiLocale, GuiText, Message,
    OperationState, SecretInput, ensure_config_document_current,
    hotword_activation_retry::{
        PendingHotwordActivation, retry_hotword_activation, validate_pending_activation,
    },
    hotword_path::{reject_url_like_hotword_path, resolved_hotword_content_path},
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
    /// Open a desktop file chooser for one existing hotword file.
    BrowsePath,
    /// Complete one asynchronous file chooser interaction.
    PathPicked(Result<Option<SecretInput>, String>),
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
            Self::BrowsePath => formatter.write_str("BrowsePath"),
            Self::PathPicked(result) => formatter
                .debug_tuple("PathPicked")
                .field(&match result {
                    Ok(Some(_)) => "selected",
                    Ok(None) => "cancelled",
                    Err(_) => "failed",
                })
                .finish(),
            Self::SetPath => formatter.write_str("SetPath"),
            Self::ClearPath => formatter.write_str("ClearPath"),
            Self::LoadContent => formatter.write_str("LoadContent"),
            Self::ContentLoaded {
                operation_id,
                result,
            } => formatter
                .debug_struct("ContentLoaded")
                .field("operation_id", operation_id)
                .field("result", &if result.is_ok() { "loaded" } else { "failed" })
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
                .field("result", &if result.is_ok() { "saved" } else { "failed" })
                .finish(),
            Self::MutationFinished(result) => formatter
                .debug_tuple("MutationFinished")
                .field(&if result.is_ok() { "saved" } else { "failed" })
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
    pending_activation: Option<PendingHotwordActivation>,
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
        let clear_pending = self.pending_activation.as_ref().is_some_and(|pending| {
            pending.matches_provider(&loaded.provider_id)
                && !pending.matches_loaded_file(&loaded.provider_id, &loaded.path, &loaded.snapshot)
        });
        self.content = text_editor::Content::with_text(&loaded.snapshot.content);
        self.loaded_path = Some(loaded.path);
        self.baseline = Some(loaded.snapshot);
        if clear_pending {
            self.pending_activation = None;
        }
    }

    fn apply_saved_content_baseline(
        &mut self,
        provider_id: &str,
        baseline: Option<HotwordContentSnapshot>,
        retry_activation: bool,
    ) {
        let clear_pending = self
            .pending_activation
            .as_ref()
            .is_some_and(|pending| pending.matches_provider(provider_id));
        if let Some(baseline) = baseline {
            self.loaded_path.clone_from(&self.content_path);
            self.baseline = Some(baseline.clone());
            if retry_activation {
                self.pending_activation = Some(PendingHotwordActivation::for_file(
                    provider_id.to_owned(),
                    self.content_path
                        .clone()
                        .expect("content save requires a resolved path"),
                    baseline,
                ));
            } else if clear_pending {
                self.pending_activation = None;
            }
        } else {
            self.loaded_path = None;
            self.baseline = None;
            if clear_pending {
                self.pending_activation = None;
            }
        }
    }

    fn pending_activation_for_selected_provider(&self) -> bool {
        self.selected_provider
            .as_deref()
            .zip(self.pending_activation.as_ref())
            .is_some_and(|(provider_id, pending)| pending.matches_provider(provider_id))
    }

    fn path_is_dirty(&self) -> bool {
        normalized_hotword_path(&self.path_input) != self.configured_path
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
    pub(super) fn guard_hotword_changes(&mut self, _blocked_action: &str) -> bool {
        if self.hotword_editor.has_unsaved_changes() {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::HotwordChangesBlocked).to_owned());
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
        let pending = self.hotword_editor.pending_activation.clone();
        let mut refreshed = HotwordEditorState::from_document(config, preferred.as_deref());
        if let (Ok(document), Some(pending)) = (config.as_ref(), pending)
            && validate_pending_activation(document, &pending).is_ok()
        {
            refreshed.pending_activation = Some(pending);
        }
        self.hotword_editor = refreshed;
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
            HotwordMessage::BrowsePath => return self.begin_hotword_file_browse(),
            HotwordMessage::PathPicked(result) => self.finish_hotword_file_browse(result),
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
                self.locale
                    .text(GuiText::SaveOrResetHotwordBeforeProvider)
                    .to_owned(),
            );
            return;
        }
        let Ok(document) = &self.config else {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::NoValidConfigLoaded).to_owned());
            return;
        };
        if !hotword_provider_options(&document.config)
            .iter()
            .any(|provider| provider == selection)
        {
            self.operation = OperationState::Failed(
                self.locale
                    .text(GuiText::SelectedHotwordProviderUnavailable)
                    .to_owned(),
            );
            return;
        }
        self.hotword_editor
            .select_provider(&document.config, selection.id());
        self.operation = OperationState::Idle;
    }

    fn begin_hotword_path_set(&mut self) -> Task<Message> {
        if self.hotword_editor.content_is_dirty() {
            self.operation = OperationState::Failed(
                self.locale
                    .text(GuiText::SaveHotwordBeforePathChange)
                    .to_owned(),
            );
            return Task::none();
        }
        let path = self.hotword_editor.path_input.trim();
        if path.is_empty() {
            self.operation = OperationState::Failed(
                self.locale
                    .text(GuiText::HotwordPathCannotBeEmpty)
                    .to_owned(),
            );
            return Task::none();
        }
        let path = path.to_owned();
        self.begin_hotword_path_mutation(Some(&path), GuiText::SettingHotwordPath)
    }

    fn begin_hotword_path_clear(&mut self) -> Task<Message> {
        if self.hotword_editor.content_is_dirty() {
            self.operation = OperationState::Failed(
                self.locale
                    .text(GuiText::SaveHotwordBeforePathClear)
                    .to_owned(),
            );
            return Task::none();
        }
        self.begin_hotword_path_mutation(None, GuiText::ClearingHotwordPath)
    }

    fn begin_hotword_path_mutation(
        &mut self,
        path: Option<&str>,
        progress: GuiText,
    ) -> Task<Message> {
        if let Err(error) = self.ensure_no_unsaved_config_draft() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        let Some(provider_id) = self.hotword_editor.selected_provider.clone() else {
            self.operation = OperationState::Failed(
                self.locale
                    .text(GuiText::NoHotwordProviderSelected)
                    .to_owned(),
            );
            return Task::none();
        };
        let Ok(document) = &self.config else {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::NoValidConfigLoaded).to_owned());
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
        let pending_config_path = updated
            .asr
            .providers
            .iter()
            .find(|provider| provider.id == provider_id)
            .and_then(|provider| provider.hotwords_file.clone());
        let summary = self
            .locale
            .hotword_path_changed(&provider_id, path.is_some());
        let document = document.clone();
        let locale = self.locale;
        self.operation = OperationState::Running(locale.text(progress));
        crate::blocking_task::perform(
            "vinput-gui-hotword-path-mutation",
            move || {
                apply_hotword_path_mutation(
                    &document,
                    &updated,
                    prerequisite_path.as_deref(),
                    &provider_id,
                    pending_config_path,
                    summary,
                    locale,
                )
            },
            |result| {
                Message::Hotword(HotwordMessage::MutationFinished(
                    result.unwrap_or_else(|failure| Err(failure.to_string())),
                ))
            },
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
        let path = outcome.save.path.display().to_string();
        let backup = outcome
            .save
            .backup_path
            .as_ref()
            .map(|path| path.display().to_string());
        let mutated_provider = self.hotword_editor.selected_provider.clone();
        let pending_activation = outcome.pending_activation.clone().or_else(|| {
            self.hotword_editor
                .pending_activation
                .clone()
                .filter(|pending| {
                    mutated_provider
                        .as_deref()
                        .is_some_and(|provider_id| !pending.matches_provider(provider_id))
                })
        });
        self.replace_config(load_config_document(Some(&outcome.save.path)));
        self.hotword_editor.pending_activation = pending_activation;
        let summary = self.locale.save_receipt(
            &outcome.summary,
            &path,
            backup.as_deref(),
            &outcome.save.daemon_reload,
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
                self.locale
                    .text(GuiText::SaveHotwordBeforeReload)
                    .to_owned(),
            );
            return Task::none();
        }
        if self.hotword_editor.path_is_dirty() {
            self.operation = OperationState::Failed(
                self.locale
                    .text(GuiText::SetOrResetPathBeforeLoad)
                    .to_owned(),
            );
            return Task::none();
        }
        let Some(provider_id) = self.hotword_editor.selected_provider.clone() else {
            self.operation = OperationState::Failed(
                self.locale
                    .text(GuiText::NoHotwordProviderSelected)
                    .to_owned(),
            );
            return Task::none();
        };
        let Some(path) = self.hotword_editor.content_path.clone() else {
            self.operation = OperationState::Failed(
                self.hotword_editor
                    .content_path_error
                    .clone()
                    .unwrap_or_else(|| self.locale.text(GuiText::SetPathBeforeLoad).to_owned()),
            );
            return Task::none();
        };
        let operation_id = self.next_hotword_operation_id;
        self.next_hotword_operation_id = self.next_hotword_operation_id.saturating_add(1);
        self.active_hotword_operation_id = Some(operation_id);
        self.operation = OperationState::Running(self.locale.text(GuiText::LoadingHotwordContent));
        crate::blocking_task::perform(
            "vinput-gui-hotword-content-load",
            move || {
                read_hotword_snapshot(&path).map(|snapshot| LoadedHotwordContent {
                    provider_id,
                    path,
                    snapshot,
                })
            },
            move |result| {
                Message::Hotword(HotwordMessage::ContentLoaded {
                    operation_id,
                    result: result.unwrap_or_else(|failure| Err(failure.to_string())),
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
                self.locale
                    .text(GuiText::DiscardedStaleHotwordContent)
                    .to_owned(),
            );
            return;
        }
        let existed = loaded.snapshot.existed;
        self.hotword_editor.apply_loaded(loaded);
        self.operation = OperationState::Succeeded(if existed {
            self.locale.text(GuiText::LoadedHotwordContent).to_owned()
        } else {
            self.locale
                .text(GuiText::MissingHotwordFileEmptyEditor)
                .to_owned()
        });
    }

    fn begin_hotword_content_save(&mut self) -> Task<Message> {
        if self.hotword_editor.path_is_dirty() {
            self.operation = OperationState::Failed(
                self.locale
                    .text(GuiText::SetOrResetPathBeforeSave)
                    .to_owned(),
            );
            return Task::none();
        }
        if !self.hotword_editor.content_matches_target() {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::LoadHotwordBeforeSave).to_owned());
            return Task::none();
        }
        let Some(path) = self.hotword_editor.content_path.clone() else {
            self.operation = OperationState::Failed(
                self.hotword_editor
                    .content_path_error
                    .clone()
                    .unwrap_or_else(|| self.locale.text(GuiText::SetPathBeforeSave).to_owned()),
            );
            return Task::none();
        };
        let Some(expected) = self.hotword_editor.baseline.clone() else {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::LoadHotwordBeforeSave).to_owned());
            return Task::none();
        };
        let Some(provider_id) = self.hotword_editor.selected_provider.clone() else {
            self.operation = OperationState::Failed(
                self.locale
                    .text(GuiText::NoHotwordProviderSelected)
                    .to_owned(),
            );
            return Task::none();
        };
        let Ok(document) = &self.config else {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::NoValidConfigLoaded).to_owned());
            return Task::none();
        };
        let document = document.clone();
        let content = self.hotword_editor.content.text();
        let operation_id = self.next_hotword_operation_id;
        self.next_hotword_operation_id = self.next_hotword_operation_id.saturating_add(1);
        self.active_hotword_operation_id = Some(operation_id);
        self.operation = OperationState::Running(self.locale.text(GuiText::SavingHotwordContent));
        crate::blocking_task::perform(
            "vinput-gui-hotword-content-save",
            move || {
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
                    result: result.unwrap_or_else(|failure| Err(failure.to_string())),
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
                let selected_provider = self
                    .hotword_editor
                    .selected_provider
                    .clone()
                    .expect("content save requires a selected provider");
                self.hotword_editor.apply_saved_content_baseline(
                    &selected_provider,
                    baseline,
                    retry_activation,
                );
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
            self.operation = OperationState::Failed(
                self.locale
                    .text(GuiText::NoPendingHotwordActivation)
                    .to_owned(),
            );
            return Task::none();
        };
        if !self
            .hotword_editor
            .pending_activation_for_selected_provider()
        {
            self.operation = OperationState::Failed(
                self.locale
                    .text(GuiText::SelectPendingHotwordProvider)
                    .to_owned(),
            );
            return Task::none();
        }
        let Ok(document) = &self.config else {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::NoValidConfigLoaded).to_owned());
            return Task::none();
        };
        let document = document.clone();
        let operation_id = self.next_hotword_operation_id;
        self.next_hotword_operation_id = self.next_hotword_operation_id.saturating_add(1);
        self.active_hotword_operation_id = Some(operation_id);
        self.operation =
            OperationState::Running(self.locale.text(GuiText::RetryingHotwordActivation));
        crate::blocking_task::perform(
            "vinput-gui-hotword-activation-retry",
            move || retry_hotword_activation(&document, &pending),
            move |result| {
                Message::Hotword(HotwordMessage::ActivationRetried {
                    operation_id,
                    result: result.unwrap_or_else(|failure| Err(failure.to_string())),
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

fn apply_hotword_path_mutation(
    document: &ConfigDocument,
    updated: &VinputConfig,
    prerequisite_path: Option<&Path>,
    provider_id: &str,
    pending_config_path: Option<String>,
    summary: String,
    locale: GuiLocale,
) -> Result<HotwordMutationOutcome, String> {
    let should_confirm = updated.asr.active_provider == provider_id;
    let mut save = save_hotword_path_with_daemon(document, updated, prerequisite_path)?;
    let (activation_error, pending_activation) = if should_confirm {
        if save.daemon_reload == DAEMON_RELOAD_REQUESTED {
            match wait_for_requested_asr_backend(provider_id) {
                Ok(reload_summary) => match ensure_hotword_path_update_current(&save.path, updated)
                {
                    Ok(()) => {
                        save.daemon_reload = reload_summary;
                        (None, None)
                    }
                    Err(error) => (Some(error), None),
                },
                Err(error) => (
                    Some(error),
                    Some(PendingHotwordActivation::for_config(
                        provider_id.to_owned(),
                        pending_config_path,
                    )),
                ),
            }
        } else {
            (
                Some(locale.text(GuiText::HotwordActivationNotApplied).to_owned()),
                Some(PendingHotwordActivation::for_config(
                    provider_id.to_owned(),
                    pending_config_path,
                )),
            )
        }
    } else {
        (None, None)
    };
    Ok(HotwordMutationOutcome {
        save,
        summary,
        activation_error,
        pending_activation,
    })
}

fn configured_hotword_path(config: &VinputConfig, provider_id: &str) -> Option<PathBuf> {
    config
        .asr
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .and_then(|provider| provider.hotwords_file.as_deref())
        .and_then(normalized_hotword_path)
}

fn normalized_hotword_path(value: &str) -> Option<PathBuf> {
    let value = value.trim();
    (!value.is_empty()).then(|| PathBuf::from(value))
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
    if let Some(path) = provider.hotwords_file.as_deref() {
        reject_url_like_hotword_path(path)?;
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

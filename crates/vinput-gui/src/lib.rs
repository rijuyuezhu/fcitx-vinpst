//! Rust management GUI state, data loading, and D-Bus integration.

use std::{
    env,
    path::{Path, PathBuf},
    time::Duration,
};

use iced::{
    Element, Length, Subscription, Task, Theme,
    widget::{button, column, container, row, scrollable, text},
};
use serde_json::{Value, json};
#[cfg(test)]
use vinput_config::AsrProviderKind;
use vinput_config::{VinputConfig, config_backup_path, write_config_file};
use vinput_protocol::{TextAdapterState, dbus};
use vinput_registry::{InstalledModelInfo, LiveScriptKind};

mod adapter_config_management;
mod adapter_runtime;
mod asr_provider_management;
mod asr_reload_confirmation;
mod config_editor;
mod daemon_client;
mod daemon_control;
mod daemon_owner_monitor;
mod desktop_actions;
mod form_guards;
mod hotword_activation_retry;
mod hotword_management;
mod hotword_path;
mod hotword_persistence;
mod i18n;
mod llm_provider_management;
mod message;
mod model_install;
mod model_management;
mod page;
mod provider_script_edit;
mod resource_details;
mod resource_pages;
mod scene_management;
mod script_install;
mod script_management;
mod script_recovery;
mod script_removal;
mod script_transaction;
mod startup_notifications;

use adapter_config_management::AdapterConfigEditorState;
pub use adapter_config_management::{
    AdapterConfigEditorField, AdapterConfigMessage, AdapterConfigMutationOutcome,
};
pub use adapter_runtime::{
    AdapterRuntimeAction, AdapterRuntimeConfirmation, AdapterRuntimeError,
    AdapterRuntimeErrorCategory, AdapterRuntimeMessage, AdapterRuntimeOutcome,
};
use asr_provider_management::AsrProviderEditorState;
pub use asr_provider_management::{
    AsrProviderEditorField, AsrProviderMessage, AsrProviderMutationOutcome,
};
pub(crate) use asr_reload_confirmation::{
    reload_asr_backend, reload_asr_backend_and_wait, wait_for_requested_asr_backend,
};
pub use daemon_client::query_daemon_snapshot;
pub(crate) use daemon_client::{daemon_proxy, query_daemon_snapshot_if_owned};
pub use daemon_control::{
    DaemonControlAction, DaemonControlConfirmation, DaemonControlFailure, DaemonControlMessage,
    DaemonControlObservation, DaemonControlOutcome,
};
pub use daemon_owner_monitor::DaemonOwnerEvent;
use daemon_owner_monitor::DaemonOwnerMonitorState;
pub use desktop_actions::{DesktopActionMessage, DesktopOpenFailure, DesktopOpenOutcome};
use hotword_management::HotwordEditorState;
pub use hotword_management::{HotwordMessage, HotwordMutationOutcome, HotwordProviderSelection};
pub use i18n::GuiLocale;
pub(crate) use i18n::{DaemonActionName, GuiText};
use llm_provider_management::LlmProviderEditorState;
pub use llm_provider_management::{
    LlmProviderEditorField, LlmProviderMessage, LlmProviderMutationOutcome, LlmProviderTestOutcome,
};
pub use message::{ConfigDraftMessage, Message};
pub use model_install::ModelInstallOutcome;
use model_install::ModelInstallState;
pub use model_management::default_model_root;
use model_management::{load_installed_models, model_is_active, remove_installed_model};
pub use page::Page;
use resource_details::ResourceSelection;
use scene_management::SceneEditorState;
pub use scene_management::{
    SceneEditorField, SceneMessage, SceneMutationOutcome, SceneProviderSelection,
};
use script_install::ScriptInstallState;
pub use script_install::{ScriptInstallOutcome, ScriptPreparationResult, SecretInput};
use startup_notifications::StartupNotificationState;
pub use startup_notifications::{
    StartupNotification, StartupNotificationLoadOutcome, StartupNotificationMessage,
};

pub(crate) const DAEMON_RELOAD_REQUESTED: &str = "daemon config reload requested";

/// A validated config document loaded for the GUI.
#[derive(Debug, Clone)]
pub struct ConfigDocument {
    /// Requested or discovered config path.
    pub path: PathBuf,
    /// Whether the config came from disk instead of the bundled fallback.
    pub from_disk: bool,
    /// Validated typed config.
    pub config: VinputConfig,
}

/// Redacted daemon state shown in the GUI.
#[derive(Debug, Clone, PartialEq)]
pub struct DaemonSnapshot {
    /// Legacy daemon status wire value.
    pub status: String,
    /// Runtime diagnostic JSON returned by the daemon.
    pub runtime: Value,
    /// Typed text-adapter runtime state returned by the daemon.
    pub text_adapters: TextAdapterState,
}

#[derive(Debug, Clone, PartialEq)]
struct ConfigDraft {
    default_language: String,
    capture_device: String,
    duck_output_while_recording: bool,
    duck_output_volume: f32,
    vad_enabled: bool,
    vad_threshold: f32,
    active_provider: String,
    active_scene: String,
}

impl ConfigDraft {
    fn from_config(config: &VinputConfig) -> Self {
        Self {
            default_language: config.global.default_language.clone(),
            capture_device: config.global.capture_device.clone(),
            duck_output_while_recording: config.global.duck_output_while_recording,
            duck_output_volume: config.global.duck_output_volume,
            vad_enabled: config.asr.vad.enabled,
            vad_threshold: config.asr.vad.threshold,
            active_provider: config.asr.active_provider.clone(),
            active_scene: config.scenes.active_scene.clone(),
        }
    }

    fn is_dirty(&self, config: &VinputConfig) -> bool {
        self != &Self::from_config(config)
    }

    fn apply_to(&self, config: &mut VinputConfig) {
        config
            .global
            .default_language
            .clone_from(&self.default_language);
        config
            .global
            .capture_device
            .clone_from(&self.capture_device);
        config.global.duck_output_while_recording = self.duck_output_while_recording;
        config.global.duck_output_volume = self.duck_output_volume;
        config.asr.vad.enabled = self.vad_enabled;
        config.asr.vad.threshold = self.vad_threshold;
        config.asr.active_provider.clone_from(&self.active_provider);
        config.scenes.active_scene.clone_from(&self.active_scene);
    }
}

/// Result of a successful GUI config save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigSaveOutcome {
    /// Final user config path.
    pub path: PathBuf,
    /// Adjacent backup written before replacement, when the config already existed.
    pub backup_path: Option<PathBuf>,
    /// Daemon reload outcome, without config contents or credentials.
    pub daemon_reload: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum OperationState {
    Idle,
    Running(&'static str),
    Succeeded(String),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
enum DaemonLoadState {
    Loading,
    Ready(DaemonSnapshot),
    Failed(String),
}

/// GUI state.
#[derive(Debug)]
pub struct App {
    locale: GuiLocale,
    page: Page,
    filter: String,
    config: Result<ConfigDocument, String>,
    draft: Option<ConfigDraft>,
    daemon: DaemonLoadState,
    daemon_owner_generation: u64,
    active_daemon_refresh_id: Option<u64>,
    next_daemon_refresh_id: u64,
    daemon_owner_monitor: DaemonOwnerMonitorState,
    active_daemon_control_id: Option<u64>,
    next_daemon_control_id: u64,
    operation: OperationState,
    startup_notification: StartupNotificationState,
    model_selector: String,
    model_install: ModelInstallState,
    next_model_install_id: u64,
    provider_selector: String,
    adapter_selector: String,
    script_install: ScriptInstallState,
    next_script_install_id: u64,
    installed_models: Result<Vec<InstalledModelInfo>, String>,
    selected_resource: Option<ResourceSelection>,
    scene_editor: Option<SceneEditorState>,
    asr_provider_editor: Option<AsrProviderEditorState>,
    llm_provider_editor: Option<LlmProviderEditorState>,
    adapter_config_editor: Option<AdapterConfigEditorState>,
    llm_provider_test_text: SecretInput,
    hotword_editor: HotwordEditorState,
    active_hotword_operation_id: Option<u64>,
    next_hotword_operation_id: u64,
}

impl App {
    /// Creates the initial GUI state and starts a daemon refresh.
    pub fn boot() -> (Self, Task<Message>) {
        let config = load_config_document(None);
        let draft = config
            .as_ref()
            .ok()
            .map(|document| ConfigDraft::from_config(&document.config));
        let hotword_editor = HotwordEditorState::from_document(&config, None);
        let mut app = Self {
            locale: GuiLocale::detect(),
            page: Page::Control,
            filter: String::new(),
            config,
            draft,
            daemon: DaemonLoadState::Loading,
            daemon_owner_generation: 1,
            active_daemon_refresh_id: None,
            next_daemon_refresh_id: 1,
            daemon_owner_monitor: DaemonOwnerMonitorState::Connecting,
            active_daemon_control_id: None,
            next_daemon_control_id: 1,
            operation: OperationState::Idle,
            startup_notification: StartupNotificationState::Loading,
            model_selector: String::new(),
            model_install: ModelInstallState::default(),
            next_model_install_id: 1,
            provider_selector: String::new(),
            adapter_selector: String::new(),
            script_install: ScriptInstallState::default(),
            next_script_install_id: 1,
            installed_models: load_installed_models(),
            selected_resource: None,
            scene_editor: None,
            asr_provider_editor: None,
            llm_provider_editor: None,
            adapter_config_editor: None,
            llm_provider_test_text: SecretInput::new("Connectivity test".to_owned()),
            hotword_editor,
            active_hotword_operation_id: None,
            next_hotword_operation_id: 1,
        };
        let daemon_task = app.begin_daemon_refresh(true);
        let notification_task = app.begin_startup_notification_load();
        (app, Task::batch([daemon_task, notification_task]))
    }

    /// Applies a GUI message.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        if let Some(task) = self.intercept_startup_notification_message(&message) {
            return task;
        }
        if let Some(task) = self.intercept_desktop_action_message(&message) {
            return task;
        }
        if let Some(task) = self.intercept_daemon_control_message(&message) {
            return task;
        }
        if let Some(task) = self.intercept_adapter_config_message(&message) {
            return task;
        }
        if let Some(task) = self.intercept_adapter_runtime_message(&message) {
            return task;
        }
        if let Some(task) = self.intercept_asr_provider_message(&message) {
            return task;
        }
        if self.is_busy() && message.blocked_while_busy() {
            return Task::none();
        }
        self.update_unblocked(message)
    }
    fn update_unblocked(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SelectPage(page) => self.select_page(page),
            Message::FilterChanged(filter) => self.filter = filter,
            Message::RefreshDaemon => return self.begin_daemon_refresh(true),
            Message::DaemonLoaded {
                operation_id,
                result,
            } => self.finish_daemon_refresh(operation_id, result),
            Message::DaemonFallbackPollTick => return self.begin_daemon_fallback_poll(),
            Message::DaemonFallbackPolled {
                operation_id,
                result,
            } => self.finish_daemon_fallback_poll(operation_id, result),
            Message::DaemonOwnerEvent(event) => return self.handle_daemon_owner_event(event),
            Message::ReloadConfig => self.reload_config(),
            Message::ConfigDraft(message) => self.update_config_draft(message),
            Message::Scene(message) => return self.handle_scene_message(message),
            Message::LlmProvider(message) => return self.handle_llm_provider_message(message),
            Message::Hotword(message) => return self.handle_hotword_message(message),
            Message::ResetConfigDraft => self.reset_config_draft(),
            Message::SaveConfig => return self.begin_config_save(),
            Message::ConfigSaved(result) => return self.finish_config_save(result),
            Message::StartRecording => return self.begin_recording(true),
            Message::StopRecording => return self.begin_recording(false),
            Message::RecordingActionFinished { start, result } => {
                return self.finish_recording(start, result);
            }
            Message::ModelSelectorChanged(value) => self.model_selector = value,
            Message::InstallModel => return self.begin_model_install(),
            Message::CancelModelInstall => self.model_install.cancel(),
            Message::RetryModelInstall => return self.retry_model_install(),
            Message::ModelInstallProgressTick => self.model_install.refresh_progress(),
            Message::RemoveInstalledModel(path) => return self.begin_model_remove(path),
            Message::ModelInstalled {
                operation_id,
                outcome,
            } => return self.finish_model_install(operation_id, outcome),
            Message::ModelRemoved(result) => return self.finish_model_remove(result),
            Message::SelectInstalledModelDetail(path) => self.select_installed_model_detail(path),
            Message::SelectAsrProviderDetail(id) => self.select_asr_provider_detail(id),
            Message::SelectLlmProviderDetail(id) => self.select_llm_provider_detail(id),
            Message::SelectLlmAdapterDetail(id) => self.select_llm_adapter_detail(id),
            Message::ClearResourceDetail => self.clear_resource_detail(),
            Message::ProviderSelectorChanged(value) => self.provider_selector = value,
            Message::AdapterSelectorChanged(value) => self.adapter_selector = value,
            Message::InstallProvider => return self.begin_provider_install(),
            Message::InstallAdapter => return self.begin_adapter_install(),
            Message::ScriptPrepared {
                operation_id,
                outcome,
            } => return self.finish_script_preparation(operation_id, outcome.into_inner()),
            Message::ScriptEnvironmentChanged { name, value } => {
                self.update_script_environment(&name, value);
            }
            Message::ConfirmScriptInstall => return self.confirm_script_install(),
            Message::CancelScriptInstall => self.script_install.cancel(),
            Message::RetryScriptInstall => return self.retry_script_install(),
            Message::RetryScriptConfigUpdate => return self.retry_script_config_update(),
            Message::DismissScriptRecovery => self.dismiss_script_recovery(),
            Message::ScriptInstallProgressTick => self.script_install.refresh_progress(),
            Message::ScriptInstalled {
                operation_id,
                outcome,
            } => return self.finish_script_install(operation_id, outcome),
            Message::EditProviderScript(id) => return self.begin_provider_script_edit(&id),
            Message::ProviderScriptEdited(result) => self.finish_provider_script_edit(result),
            Message::RemoveProvider(id) => {
                return self.begin_script_remove(LiveScriptKind::AsrProvider, id);
            }
            Message::RemoveAdapter(id) => {
                return self.begin_script_remove(LiveScriptKind::LlmAdapter, id);
            }
            Message::ScriptRemoved(result) => return self.finish_script_remove(result),
            Message::StartupNotification(_)
            | Message::DesktopAction(_)
            | Message::DaemonControl(_)
            | Message::AdapterRuntime(_)
            | Message::AdapterConfig(_)
            | Message::AsrProvider(_) => unreachable!("domain messages are intercepted"),
        }
        Task::none()
    }

    /// Subscribes to owner changes and uses low-frequency polling only as a fallback.
    pub fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = self.daemon_reconciliation_subscriptions();
        if self.model_install.is_active() {
            subscriptions.push(
                iced::time::every(Duration::from_millis(100))
                    .map(|_| Message::ModelInstallProgressTick),
            );
        }
        if self.script_install.has_worker() {
            subscriptions.push(
                iced::time::every(Duration::from_millis(100))
                    .map(|_| Message::ScriptInstallProgressTick),
            );
        }
        Subscription::batch(subscriptions)
    }

    pub(crate) fn ensure_no_unsaved_config_draft(&self) -> Result<(), String> {
        ensure_resource_mutation_draft_clean(&self.config, self.draft.as_ref())
    }

    pub(crate) fn ensure_no_open_scene_editor(&self) -> Result<(), String> {
        if self.scene_editor.is_some() {
            return Err(
                "Save or cancel the open Scene form before modifying providers or adapters."
                    .to_owned(),
            );
        }
        Ok(())
    }

    fn reload_config(&mut self) {
        if !self.guard_hotword_changes("reloading configuration") {
            return;
        }
        let path = self
            .config
            .as_ref()
            .ok()
            .map(|document| document.path.clone());
        self.replace_config(load_config_document(path.as_deref()));
        self.installed_models = load_installed_models();
        self.operation = OperationState::Idle;
    }

    fn reset_config_draft(&mut self) {
        self.draft = self
            .config
            .as_ref()
            .ok()
            .map(|document| ConfigDraft::from_config(&document.config));
        self.operation = OperationState::Idle;
    }

    fn begin_config_save(&mut self) -> Task<Message> {
        let (Ok(document), Some(draft)) = (&self.config, &self.draft) else {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::NoValidConfigLoaded).to_owned());
            return Task::none();
        };
        self.operation = OperationState::Running(self.locale.text(GuiText::SavingConfiguration));
        let document = document.clone();
        let draft = draft.clone();
        Task::perform(
            async move { save_config_with_daemon(&document, &draft) },
            Message::ConfigSaved,
        )
    }

    fn finish_config_save(&mut self, result: Result<ConfigSaveOutcome, String>) -> Task<Message> {
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                self.operation = OperationState::Failed(error);
                return Task::none();
            }
        };
        let path = outcome.path.display().to_string();
        let backup = outcome
            .backup_path
            .as_ref()
            .map(|path| path.display().to_string());
        self.replace_config(load_config_document(Some(&outcome.path)));
        self.operation = OperationState::Succeeded(self.locale.config_save_receipt(
            &path,
            backup.as_deref(),
            &outcome.daemon_reload,
        ));
        self.begin_daemon_refresh(false)
    }

    fn begin_recording(&mut self, start: bool) -> Task<Message> {
        let scene = self
            .draft
            .as_ref()
            .map_or_else(String::new, |draft| draft.active_scene.clone());
        self.operation = OperationState::Running(self.locale.text(if start {
            GuiText::StartingRecording
        } else {
            GuiText::StoppingRecording
        }));
        Task::perform(
            async move { run_recording_action(start, &scene) },
            move |result| Message::RecordingActionFinished { start, result },
        )
    }

    fn finish_recording(&mut self, start: bool, result: Result<(), String>) -> Task<Message> {
        self.operation = match result {
            Ok(()) => OperationState::Succeeded(
                self.locale
                    .text(if start {
                        GuiText::RecordingStarted
                    } else {
                        GuiText::RecordingStopped
                    })
                    .to_owned(),
            ),
            Err(error) => OperationState::Failed(error),
        };
        self.begin_daemon_refresh(false)
    }

    fn begin_model_install(&mut self) -> Task<Message> {
        let selector = self.model_selector.trim().to_owned();
        self.begin_model_install_for(selector)
    }

    fn retry_model_install(&mut self) -> Task<Message> {
        let Some(selector) = self.model_install.retry_selector() else {
            return Task::none();
        };
        self.model_selector.clone_from(&selector);
        self.begin_model_install_for(selector)
    }

    fn begin_model_install_for(&mut self, selector: String) -> Task<Message> {
        if selector.is_empty() {
            self.operation = OperationState::Failed(
                self.locale
                    .text(GuiText::EnterRegistryModelSelector)
                    .to_owned(),
            );
            return Task::none();
        }
        let Ok(document) = &self.config else {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::NoValidConfigLoaded).to_owned());
            return Task::none();
        };
        if self.model_install.is_active() || matches!(self.operation, OperationState::Running(_)) {
            return Task::none();
        }
        let operation_id = self.next_model_install_id;
        self.next_model_install_id = self.next_model_install_id.wrapping_add(1).max(1);
        let (state, task) =
            ModelInstallState::start(document.config.clone(), selector, operation_id, self.locale);
        self.operation = OperationState::Idle;
        self.model_install = state;
        task
    }

    fn begin_model_remove(&mut self, target_path: PathBuf) -> Task<Message> {
        let Ok(document) = &self.config else {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::NoValidConfigLoaded).to_owned());
            return Task::none();
        };
        self.operation = OperationState::Running(self.locale.text(GuiText::RemovingModel));
        let config = document.config.clone();
        let locale = self.locale;
        Task::perform(
            async move { remove_installed_model(&config, &target_path, locale) },
            Message::ModelRemoved,
        )
    }

    fn finish_model_install(
        &mut self,
        operation_id: u64,
        outcome: ModelInstallOutcome,
    ) -> Task<Message> {
        if !self.model_install.finish(operation_id, outcome) {
            return Task::none();
        }
        self.installed_models = load_installed_models();
        self.begin_daemon_refresh(false)
    }

    fn finish_model_remove(&mut self, result: Result<String, String>) -> Task<Message> {
        self.installed_models = load_installed_models();
        self.operation = match result {
            Ok(summary) => OperationState::Succeeded(summary),
            Err(error) => OperationState::Failed(error),
        };
        self.begin_daemon_refresh(false)
    }

    fn replace_config(&mut self, config: Result<ConfigDocument, String>) {
        self.draft = config
            .as_ref()
            .ok()
            .map(|document| ConfigDraft::from_config(&document.config));
        self.refresh_hotword_editor(&config);
        self.config = config;
        self.scene_editor = None;
        self.asr_provider_editor = None;
        self.llm_provider_editor = None;
        self.adapter_config_editor = None;
    }

    /// Renders the GUI.
    #[must_use]
    pub fn view(&self) -> Element<'_, Message> {
        let busy = self.is_busy();
        let navigation = self.navigation_view(busy);

        let page_content = match self.page {
            Page::Control => self.control_page(),
            Page::Resources => self.resources_page(),
            Page::Llm => self.llm_page(),
            Page::Hotwords => self.hotwords_page(),
        };
        let content = match self.startup_notification_view() {
            Some(notification) => column![notification, page_content].spacing(12),
            None => column![page_content],
        };

        container(
            row![
                container(navigation).width(190).padding(18),
                container(content).width(Length::Fill).padding(24)
            ]
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn control_page(&self) -> Element<'_, Message> {
        let busy = self.is_busy();
        let mut body = column![
            text(self.locale.text(GuiText::Control)).size(30),
            text(self.locale.text(GuiText::DaemonService)).size(22),
            self.daemon_control_actions(busy),
            self.daemon_status_view(),
            text(self.locale.text(GuiText::Recording)).size(22),
            self.recording_actions(busy),
        ]
        .spacing(14);
        if let Some(notice) = self.operation_notice() {
            body = body.push(notice);
        }
        body = body.push(self.config_editor(busy));
        scrollable(body).into()
    }

    fn recording_actions(&self, busy: bool) -> Element<'_, Message> {
        let daemon_status = match &self.daemon {
            DaemonLoadState::Ready(snapshot) => Some(snapshot.status.as_str()),
            DaemonLoadState::Loading | DaemonLoadState::Failed(_) => None,
        };
        let can_start = !busy && daemon_status == Some("idle");
        let can_stop = !busy && daemon_status == Some("recording");
        row![
            button(self.locale.text(GuiText::ReloadConfig))
                .on_press_maybe((!busy).then_some(Message::ReloadConfig)),
            button(self.locale.text(GuiText::StartRecording))
                .on_press_maybe(can_start.then_some(Message::StartRecording)),
            button(self.locale.text(GuiText::StopRecording))
                .on_press_maybe(can_stop.then_some(Message::StopRecording)),
        ]
        .spacing(10)
        .into()
    }

    fn daemon_status_view(&self) -> Element<'_, Message> {
        let daemon = match &self.daemon {
            DaemonLoadState::Loading => text(self.locale.text(GuiText::DaemonLoading)),
            DaemonLoadState::Ready(snapshot) => text(self.locale.daemon_status(&snapshot.status)),
            DaemonLoadState::Failed(error) => text(self.locale.daemon_unavailable(error)),
        };
        let monitor = match &self.daemon_owner_monitor {
            DaemonOwnerMonitorState::Connecting => {
                self.locale.text(GuiText::OwnerMonitorConnecting).to_owned()
            }
            DaemonOwnerMonitorState::Ready => {
                self.locale.text(GuiText::OwnerMonitorReady).to_owned()
            }
            DaemonOwnerMonitorState::Failed(error) => self.locale.owner_monitor_degraded(error),
        };
        column![daemon, text(monitor)].spacing(5).into()
    }

    fn is_busy(&self) -> bool {
        matches!(self.operation, OperationState::Running(_))
            || self.model_install.is_active()
            || self.script_install.blocks_operations()
    }

    fn operation_notice(&self) -> Option<Element<'_, Message>> {
        match &self.operation {
            OperationState::Idle => self
                .model_install
                .view(self.locale)
                .or_else(|| self.script_install.view(self.locale)),
            OperationState::Running(message) => Some(text(*message).into()),
            OperationState::Succeeded(message) => {
                Some(text(self.locale.operation_success(message)).into())
            }
            OperationState::Failed(message) => {
                Some(text(self.locale.operation_error(message)).into())
            }
        }
    }
}

/// Returns the default user config path.
pub fn default_config_path() -> Result<PathBuf, String> {
    let config_home = match env::var_os("XDG_CONFIG_HOME") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => {
            let home = env::var_os("HOME").ok_or_else(|| {
                "HOME or XDG_CONFIG_HOME is required to locate the user config".to_owned()
            })?;
            PathBuf::from(home).join(".config")
        }
    };
    Ok(config_home.join("fcitx-vinput").join("config.json"))
}

/// Loads and validates a config document, falling back to the bundled default if absent.
pub fn load_config_document(path: Option<&Path>) -> Result<ConfigDocument, String> {
    let path = match path {
        Some(path) => path.to_path_buf(),
        None => default_config_path()?,
    };
    let (config, from_disk) = if path.exists() {
        (
            VinputConfig::from_json_file(&path).map_err(|error| error.to_string())?,
            true,
        )
    } else {
        (
            VinputConfig::bundled_default().map_err(|error| error.to_string())?,
            false,
        )
    };
    config.validate().map_err(|error| error.to_string())?;
    Ok(ConfigDocument {
        path,
        from_disk,
        config,
    })
}

fn daemon_state_from_poll(result: Result<Option<DaemonSnapshot>, String>) -> DaemonLoadState {
    match result {
        Ok(Some(snapshot)) => DaemonLoadState::Ready(snapshot),
        Ok(None) => DaemonLoadState::Failed(
            "Daemon is not running; waiting for its D-Bus owner.".to_owned(),
        ),
        Err(error) => DaemonLoadState::Failed(error),
    }
}

fn ensure_config_save_allowed(snapshot: &DaemonSnapshot) -> Result<(), String> {
    let active_session = snapshot.runtime["active_session"]
        .as_bool()
        .unwrap_or(false);
    if snapshot.status != "idle" || active_session {
        return Err(format!(
            "Configuration cannot be saved while the daemon is `{}` or has an active session.",
            snapshot.status
        ));
    }
    Ok(())
}

fn ensure_resource_mutation_draft_clean(
    config: &Result<ConfigDocument, String>,
    draft: Option<&ConfigDraft>,
) -> Result<(), String> {
    let (Ok(document), Some(draft)) = (config, draft) else {
        return Ok(());
    };
    if draft.is_dirty(&document.config) {
        return Err(
            "Save or reset the Control page changes before modifying resources.".to_owned(),
        );
    }
    Ok(())
}

#[cfg(test)]
fn persist_config_draft(
    document: &ConfigDocument,
    draft: &ConfigDraft,
) -> Result<ConfigSaveOutcome, String> {
    let mut updated = document.config.clone();
    draft.apply_to(&mut updated);
    persist_updated_config(document, &updated)
}

pub(crate) fn ensure_config_mutation_allowed(document: &ConfigDocument) -> Result<(), String> {
    if let Ok(snapshot) = query_daemon_snapshot() {
        ensure_config_save_allowed(&snapshot)?;
    }
    ensure_config_document_current(document)
}

pub(crate) fn ensure_config_document_current(document: &ConfigDocument) -> Result<(), String> {
    if document.from_disk {
        if !document.path.exists() {
            return Err(format!(
                "Config {} disappeared; reload before saving.",
                document.path.display()
            ));
        }
        let current = VinputConfig::from_json_file(&document.path).map_err(|error| {
            format!(
                "Reload current config {} before saving: {error}",
                document.path.display()
            )
        })?;
        current.validate().map_err(|error| {
            format!(
                "Validate current config {} before saving: {error}",
                document.path.display()
            )
        })?;
        if current != document.config {
            return Err(format!(
                "Config {} changed on disk; reload instead of overwriting external changes.",
                document.path.display()
            ));
        }
    } else if document.path.exists() {
        return Err(format!(
            "Config {} was created after the GUI loaded; reload before saving.",
            document.path.display()
        ));
    }
    Ok(())
}

pub(crate) fn persist_updated_config(
    document: &ConfigDocument,
    updated: &VinputConfig,
) -> Result<ConfigSaveOutcome, String> {
    ensure_config_document_current(document)?;
    updated
        .validate()
        .map_err(|error| format!("Validate edited configuration: {error}"))?;
    let backup_path = document
        .from_disk
        .then(|| config_backup_path(&document.path));
    let receipt = write_config_file(updated, &document.path, backup_path.as_deref())
        .map_err(|error| format!("Save configuration: {error}"))?;
    Ok(ConfigSaveOutcome {
        path: receipt.path,
        backup_path: receipt.backup_path,
        daemon_reload: "daemon reload not attempted".to_owned(),
    })
}

pub(crate) fn save_updated_config_with_daemon(
    document: &ConfigDocument,
    updated: &VinputConfig,
) -> Result<ConfigSaveOutcome, String> {
    let daemon = query_daemon_snapshot();
    if let Ok(snapshot) = &daemon {
        ensure_config_save_allowed(snapshot)?;
    }
    let mut outcome = persist_updated_config(document, updated)?;
    outcome.daemon_reload = match daemon {
        Ok(_) => match reload_asr_backend() {
            Ok(()) => DAEMON_RELOAD_REQUESTED.to_owned(),
            Err(error) => {
                let rollback = restore_config_document(document);
                return Err(match rollback {
                    Ok(()) => {
                        format!("Daemon config reload failed: {error}; previous config restored.")
                    }
                    Err(rollback_error) => format!(
                        "Daemon config reload failed: {error}; restoring previous config also failed: {rollback_error}"
                    ),
                });
            }
        },
        Err(error) => format!("config saved; daemon reload skipped: {error}"),
    };
    Ok(outcome)
}

fn restore_config_document(document: &ConfigDocument) -> Result<(), String> {
    if document.from_disk {
        write_config_file(&document.config, &document.path, None)
            .map(|_| ())
            .map_err(|error| format!("Restore config {}: {error}", document.path.display()))
    } else {
        match std::fs::remove_file(&document.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!(
                "Remove newly created config {}: {error}",
                document.path.display()
            )),
        }
    }
}

fn save_config_with_daemon(
    document: &ConfigDocument,
    draft: &ConfigDraft,
) -> Result<ConfigSaveOutcome, String> {
    let mut updated = document.config.clone();
    draft.apply_to(&mut updated);
    save_updated_config_with_daemon(document, &updated)
}

fn run_recording_action(start: bool, scene: &str) -> Result<(), String> {
    let connection = zbus::blocking::Connection::session().map_err(|error| error.to_string())?;
    let proxy = daemon_proxy(&connection)?;
    if start {
        proxy
            .call::<_, _, ()>(dbus::method::START_RECORDING, &())
            .map_err(|error| error.to_string())?;
        Ok(())
    } else {
        let _: String = proxy
            .call(dbus::method::STOP_RECORDING, &scene)
            .map_err(|error| error.to_string())?;
        Ok(())
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
        "application": "vinput-gui",
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
        "pages": Page::ALL.map(Page::machine_label),
    }))
}

#[cfg(test)]
fn filtered_asr_rows(config: &VinputConfig, filter: &str) -> Vec<String> {
    let filter = filter.to_ascii_lowercase();
    config
        .asr
        .providers
        .iter()
        .filter_map(|provider| {
            let kind = match provider.kind {
                AsrProviderKind::Local => "local",
                AsrProviderKind::Remote => "remote",
                AsrProviderKind::Command => "command",
            };
            let model = provider.model.as_deref().unwrap_or("unselected model");
            let row = format!("{} · {kind} · {model}", provider.id);
            row.to_ascii_lowercase().contains(&filter).then_some(row)
        })
        .collect()
}

#[cfg(test)]
fn filtered_scene_rows(config: &VinputConfig, filter: &str) -> Vec<String> {
    let filter = filter.to_ascii_lowercase();
    config
        .scenes
        .definitions
        .iter()
        .filter_map(|scene| {
            let marker = if scene.id == config.scenes.active_scene {
                "active"
            } else {
                "available"
            };
            let row = format!("{} · {} · {marker}", scene.id, scene.label);
            row.to_ascii_lowercase().contains(&filter).then_some(row)
        })
        .collect()
}

#[cfg(test)]
fn llm_adapter_rows(config: &VinputConfig) -> Vec<String> {
    config
        .llm
        .adapters
        .iter()
        .map(|adapter| format!("{} · command adapter", adapter.id))
        .collect()
}

/// Runs the native GUI application.
pub fn run() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .title(GuiLocale::detect().text(GuiText::ApplicationTitle))
        .subscription(App::subscription)
        .theme(Theme::TokyoNight)
        .window_size((960.0, 640.0))
        .run()
}

#[cfg(test)]
mod tests;

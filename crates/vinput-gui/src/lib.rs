//! Rust management GUI state, data loading, and D-Bus integration.

use std::{
    env,
    path::{Path, PathBuf},
    time::Duration,
};

use iced::{
    Element, Length, Task, Theme,
    widget::{
        button, checkbox, column, container, pick_list, row, scrollable, slider, text, text_input,
    },
};
use serde_json::{Value, json};
use vinput_config::{
    AsrProviderKind, VinputConfig, config_backup_path, redact_url_for_diagnostics,
    write_config_file,
};
use vinput_protocol::dbus;
use vinput_registry::{
    InstalledModelInfo, LiveModelInstallRequest, LiveModelRegistry, ManagedModelRemoveRequest,
    RegistryTextSource, ReqwestRegistryAssetSource, ReqwestRegistryTextSource, install_live_model,
    managed_model_dir_name, remove_managed_model, scan_installed_models,
};

/// Product display name.
pub const APPLICATION_TITLE: &str = "Vinput Configuration";

/// Main GUI pages matching the legacy management surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// Daemon and audio controls.
    Control,
    /// ASR providers and scenes.
    Resources,
    /// LLM providers and adapters.
    Llm,
    /// Hotword file configuration.
    Hotwords,
}

impl Page {
    const ALL: [Self; 4] = [Self::Control, Self::Resources, Self::Llm, Self::Hotwords];

    fn label(self) -> &'static str {
        match self {
            Self::Control => "Control",
            Self::Resources => "Resources",
            Self::Llm => "LLM",
            Self::Hotwords => "Hotwords",
        }
    }
}

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
#[derive(Debug, Clone)]
pub struct App {
    page: Page,
    filter: String,
    config: Result<ConfigDocument, String>,
    draft: Option<ConfigDraft>,
    daemon: DaemonLoadState,
    operation: OperationState,
    model_selector: String,
    installed_models: Result<Vec<InstalledModelInfo>, String>,
}

/// GUI messages.
#[derive(Debug, Clone)]
pub enum Message {
    /// Select a main page.
    SelectPage(Page),
    /// Update the current resource filter.
    FilterChanged(String),
    /// Refresh daemon state over D-Bus.
    RefreshDaemon,
    /// Result of an asynchronous daemon refresh.
    DaemonLoaded(Result<DaemonSnapshot, String>),
    /// Reload config from disk.
    ReloadConfig,
    /// Update the default recognition language draft.
    DefaultLanguageChanged(String),
    /// Update the capture target draft.
    CaptureDeviceChanged(String),
    /// Toggle output ducking in the draft.
    DuckOutputChanged(bool),
    /// Update the output ducking volume in the draft.
    DuckVolumeChanged(f32),
    /// Toggle VAD in the draft.
    VadEnabledChanged(bool),
    /// Update the VAD threshold in the draft.
    VadThresholdChanged(f32),
    /// Select the active ASR provider in the draft.
    ActiveProviderChanged(String),
    /// Select the active scene in the draft.
    ActiveSceneChanged(String),
    /// Restore editable fields from the loaded config.
    ResetConfigDraft,
    /// Validate, back up, and atomically save the config draft.
    SaveConfig,
    /// Result of an asynchronous config save.
    ConfigSaved(Result<ConfigSaveOutcome, String>),
    /// Start normal recording over D-Bus.
    StartRecording,
    /// Stop recording over D-Bus.
    StopRecording,
    /// Result of an asynchronous recording action.
    RecordingActionFinished(Result<String, String>),
    /// Update the live registry model id or short id to install.
    ModelSelectorChanged(String),
    /// Install or update the selected live registry model.
    InstallModel,
    /// Result of a live registry model installation.
    ModelInstalled(Result<String, String>),
    /// Remove one inactive installed model directory.
    RemoveInstalledModel(PathBuf),
    /// Result of an installed model removal.
    ModelRemoved(Result<String, String>),
}

impl App {
    /// Creates the initial GUI state and starts a daemon refresh.
    pub fn boot() -> (Self, Task<Message>) {
        let config = load_config_document(None);
        let draft = config
            .as_ref()
            .ok()
            .map(|document| ConfigDraft::from_config(&document.config));
        (
            Self {
                page: Page::Control,
                filter: String::new(),
                config,
                draft,
                daemon: DaemonLoadState::Loading,
                operation: OperationState::Idle,
                model_selector: String::new(),
                installed_models: load_installed_models(),
            },
            daemon_refresh_task(),
        )
    }

    /// Applies a GUI message.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SelectPage(page) => self.page = page,
            Message::FilterChanged(filter) => self.filter = filter,
            Message::RefreshDaemon => {
                self.daemon = DaemonLoadState::Loading;
                return daemon_refresh_task();
            }
            Message::DaemonLoaded(result) => {
                self.daemon = match result {
                    Ok(snapshot) => DaemonLoadState::Ready(snapshot),
                    Err(error) => DaemonLoadState::Failed(error),
                };
            }
            Message::ReloadConfig => self.reload_config(),
            Message::DefaultLanguageChanged(value) => self.update_draft(|draft| {
                draft.default_language = value;
            }),
            Message::CaptureDeviceChanged(value) => self.update_draft(|draft| {
                draft.capture_device = value;
            }),
            Message::DuckOutputChanged(value) => self.update_draft(|draft| {
                draft.duck_output_while_recording = value;
            }),
            Message::DuckVolumeChanged(value) => self.update_draft(|draft| {
                draft.duck_output_volume = value;
            }),
            Message::VadEnabledChanged(value) => self.update_draft(|draft| {
                draft.vad_enabled = value;
            }),
            Message::VadThresholdChanged(value) => self.update_draft(|draft| {
                draft.vad_threshold = value;
            }),
            Message::ActiveProviderChanged(value) => self.update_draft(|draft| {
                draft.active_provider = value;
            }),
            Message::ActiveSceneChanged(value) => self.update_draft(|draft| {
                draft.active_scene = value;
            }),
            Message::ResetConfigDraft => self.reset_config_draft(),
            Message::SaveConfig => return self.begin_config_save(),
            Message::ConfigSaved(result) => return self.finish_config_save(result),
            Message::StartRecording => return self.begin_recording(true),
            Message::StopRecording => return self.begin_recording(false),
            Message::RecordingActionFinished(result) => return self.finish_recording(result),
            Message::ModelSelectorChanged(value) => self.model_selector = value,
            Message::InstallModel => return self.begin_model_install(),
            Message::RemoveInstalledModel(path) => return self.begin_model_remove(path),
            Message::ModelInstalled(result) | Message::ModelRemoved(result) => {
                return self.finish_model_operation(result);
            }
        }
        Task::none()
    }

    fn update_draft(&mut self, update: impl FnOnce(&mut ConfigDraft)) {
        if let Some(draft) = &mut self.draft {
            update(draft);
        }
    }

    fn reload_config(&mut self) {
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
            self.operation = OperationState::Failed("No valid config is loaded.".to_owned());
            return Task::none();
        };
        self.operation = OperationState::Running("Saving configuration…");
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
        let backup = outcome.backup_path.as_ref().map_or_else(
            || "no previous file".to_owned(),
            |path| format!("backup {}", path.display()),
        );
        self.replace_config(load_config_document(Some(&outcome.path)));
        self.operation = OperationState::Succeeded(format!(
            "Saved {} ({backup}); {}",
            outcome.path.display(),
            outcome.daemon_reload
        ));
        daemon_refresh_task()
    }

    fn begin_recording(&mut self, start: bool) -> Task<Message> {
        let scene = self
            .draft
            .as_ref()
            .map_or_else(String::new, |draft| draft.active_scene.clone());
        self.operation = OperationState::Running(if start {
            "Starting recording…"
        } else {
            "Stopping recording…"
        });
        Task::perform(
            async move { run_recording_action(start, &scene) },
            Message::RecordingActionFinished,
        )
    }

    fn finish_recording(&mut self, result: Result<String, String>) -> Task<Message> {
        self.operation = match result {
            Ok(summary) => OperationState::Succeeded(summary),
            Err(error) => OperationState::Failed(error),
        };
        daemon_refresh_task()
    }

    fn begin_model_install(&mut self) -> Task<Message> {
        let selector = self.model_selector.trim().to_owned();
        if selector.is_empty() {
            self.operation = OperationState::Failed(
                "Enter a registry model id or short id before installing.".to_owned(),
            );
            return Task::none();
        }
        let Ok(document) = &self.config else {
            self.operation = OperationState::Failed("No valid config is loaded.".to_owned());
            return Task::none();
        };
        self.operation = OperationState::Running("Installing model…");
        let config = document.config.clone();
        Task::perform(
            async move { install_registry_model(&config, &selector) },
            Message::ModelInstalled,
        )
    }

    fn begin_model_remove(&mut self, target_path: PathBuf) -> Task<Message> {
        let Ok(document) = &self.config else {
            self.operation = OperationState::Failed("No valid config is loaded.".to_owned());
            return Task::none();
        };
        self.operation = OperationState::Running("Removing model…");
        let config = document.config.clone();
        Task::perform(
            async move { remove_installed_model(&config, &target_path) },
            Message::ModelRemoved,
        )
    }

    fn finish_model_operation(&mut self, result: Result<String, String>) -> Task<Message> {
        self.installed_models = load_installed_models();
        self.operation = match result {
            Ok(summary) => OperationState::Succeeded(summary),
            Err(error) => OperationState::Failed(error),
        };
        daemon_refresh_task()
    }

    fn replace_config(&mut self, config: Result<ConfigDocument, String>) {
        self.draft = config
            .as_ref()
            .ok()
            .map(|document| ConfigDraft::from_config(&document.config));
        self.config = config;
    }

    /// Renders the GUI.
    #[must_use]
    pub fn view(&self) -> Element<'_, Message> {
        let navigation = Page::ALL.into_iter().fold(
            column![text(APPLICATION_TITLE).size(24)].spacing(10),
            |navigation, page| {
                navigation.push(
                    button(text(page.label()))
                        .width(Length::Fill)
                        .on_press(Message::SelectPage(page)),
                )
            },
        );

        let content = match self.page {
            Page::Control => self.control_page(),
            Page::Resources => self.resources_page(),
            Page::Llm => self.llm_page(),
            Page::Hotwords => self.hotwords_page(),
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
        let busy = matches!(self.operation, OperationState::Running(_));
        let mut body = column![
            text("Control").size(30),
            self.control_actions(busy),
            self.daemon_status_view(),
        ]
        .spacing(14);
        if let Some(notice) = self.operation_notice() {
            body = body.push(notice);
        }
        body = body.push(self.config_editor(busy));
        scrollable(body).into()
    }

    fn control_actions(&self, busy: bool) -> Element<'_, Message> {
        let daemon_status = match &self.daemon {
            DaemonLoadState::Ready(snapshot) => Some(snapshot.status.as_str()),
            DaemonLoadState::Loading | DaemonLoadState::Failed(_) => None,
        };
        let can_start = !busy && daemon_status == Some("idle");
        let can_stop = !busy && daemon_status == Some("recording");
        row![
            button("Refresh daemon").on_press(Message::RefreshDaemon),
            button("Reload config").on_press(Message::ReloadConfig),
            button("Start recording").on_press_maybe(can_start.then_some(Message::StartRecording)),
            button("Stop recording").on_press_maybe(can_stop.then_some(Message::StopRecording)),
        ]
        .spacing(10)
        .into()
    }

    fn daemon_status_view(&self) -> Element<'_, Message> {
        match &self.daemon {
            DaemonLoadState::Loading => text("Daemon: loading…"),
            DaemonLoadState::Ready(snapshot) => text(format!("Daemon: {}", snapshot.status)),
            DaemonLoadState::Failed(error) => text(format!("Daemon unavailable: {error}")),
        }
        .into()
    }

    fn operation_notice(&self) -> Option<Element<'_, Message>> {
        match &self.operation {
            OperationState::Idle => None,
            OperationState::Running(message) => Some(text(*message).into()),
            OperationState::Succeeded(message) => Some(text(format!("Success: {message}")).into()),
            OperationState::Failed(message) => Some(text(format!("Error: {message}")).into()),
        }
    }

    fn config_editor(&self, busy: bool) -> Element<'_, Message> {
        match (&self.config, &self.draft) {
            (Ok(document), Some(draft)) => Self::loaded_config_editor(document, draft, busy),
            (Err(error), _) => text(format!("Config error: {error}")).into(),
            (Ok(_), None) => text("Config draft is unavailable.").into(),
        }
    }

    fn loaded_config_editor<'a>(
        document: &'a ConfigDocument,
        draft: &'a ConfigDraft,
        busy: bool,
    ) -> Element<'a, Message> {
        column![
            text(format!("Config: {}", document.path.display())),
            text(format!(
                "Source: {}",
                if document.from_disk {
                    "user file"
                } else {
                    "bundled default; Save creates the user file"
                }
            )),
            text("General").size(22),
            Self::general_config_editor(document, draft),
            text("Audio and VAD").size(22),
            Self::audio_vad_editor(draft),
            Self::config_save_controls(document, draft, busy),
        ]
        .spacing(12)
        .into()
    }

    fn general_config_editor<'a>(
        document: &'a ConfigDocument,
        draft: &'a ConfigDraft,
    ) -> Element<'a, Message> {
        let provider_options = document
            .config
            .asr
            .providers
            .iter()
            .map(|provider| provider.id.clone())
            .collect::<Vec<_>>();
        let scene_options = document
            .config
            .scenes
            .definitions
            .iter()
            .map(|scene| scene.id.clone())
            .collect::<Vec<_>>();
        column![
            row![
                text("Default language").width(180),
                text_input("for example en-US or zh-CN", &draft.default_language)
                    .on_input(Message::DefaultLanguageChanged)
                    .width(Length::Fill),
            ]
            .spacing(12),
            row![
                text("Capture device").width(180),
                text_input("PipeWire target", &draft.capture_device)
                    .on_input(Message::CaptureDeviceChanged)
                    .width(Length::Fill),
            ]
            .spacing(12),
            row![
                text("Active ASR provider").width(180),
                pick_list(
                    provider_options,
                    Some(draft.active_provider.clone()),
                    Message::ActiveProviderChanged,
                )
                .width(Length::Fill),
            ]
            .spacing(12),
            row![
                text("Active scene").width(180),
                pick_list(
                    scene_options,
                    Some(draft.active_scene.clone()),
                    Message::ActiveSceneChanged,
                )
                .width(Length::Fill),
            ]
            .spacing(12),
        ]
        .spacing(12)
        .into()
    }

    fn audio_vad_editor(draft: &ConfigDraft) -> Element<'_, Message> {
        column![
            checkbox(draft.duck_output_while_recording)
                .label("Duck output while recording")
                .on_toggle(Message::DuckOutputChanged),
            row![
                text(format!(
                    "Duck volume: {:.0}%",
                    draft.duck_output_volume * 100.0
                ))
                .width(180),
                slider(
                    0.0_f32..=1.0_f32,
                    draft.duck_output_volume,
                    Message::DuckVolumeChanged,
                )
                .step(0.05_f32)
                .width(Length::Fill),
            ]
            .spacing(12),
            checkbox(draft.vad_enabled)
                .label("Enable voice activity detection")
                .on_toggle(Message::VadEnabledChanged),
            row![
                text(format!("VAD threshold: {:.2}", draft.vad_threshold)).width(180),
                slider(
                    0.05_f32..=0.95_f32,
                    draft.vad_threshold,
                    Message::VadThresholdChanged,
                )
                .step(0.05_f32)
                .width(Length::Fill),
            ]
            .spacing(12),
        ]
        .spacing(12)
        .into()
    }

    fn config_save_controls<'a>(
        document: &'a ConfigDocument,
        draft: &'a ConfigDraft,
        busy: bool,
    ) -> Element<'a, Message> {
        let dirty = *draft != ConfigDraft::from_config(&document.config);
        row![
            button("Save configuration")
                .on_press_maybe((dirty && !busy).then_some(Message::SaveConfig)),
            button("Reset changes")
                .on_press_maybe((dirty && !busy).then_some(Message::ResetConfigDraft)),
            text(if dirty {
                "Unsaved changes"
            } else {
                "Configuration is up to date"
            }),
        ]
        .spacing(10)
        .into()
    }

    fn resources_page(&self) -> Element<'_, Message> {
        let busy = matches!(self.operation, OperationState::Running(_));
        let mut body = column![
            text("Resources").size(30),
            text_input("Filter providers and scenes", &self.filter)
                .on_input(Message::FilterChanged),
            text("Managed ASR models").size(22),
            row![
                text_input("Registry model id or short id", &self.model_selector)
                    .on_input(Message::ModelSelectorChanged)
                    .width(Length::Fill),
                button("Install or update").on_press_maybe(
                    (!busy && !self.model_selector.trim().is_empty())
                        .then_some(Message::InstallModel),
                ),
            ]
            .spacing(10),
        ]
        .spacing(12);

        if let Some(notice) = self.operation_notice() {
            body = body.push(notice);
        }

        match &self.installed_models {
            Ok(models) if models.is_empty() => {
                body = body.push(text("No managed ASR models installed."));
            }
            Ok(models) => {
                for model in models {
                    let active = self
                        .config
                        .as_ref()
                        .is_ok_and(|document| model_is_active(&document.config, &model.model_dir));
                    let title = model
                        .display_title(&[])
                        .unwrap_or_else(|| model.stable_model_id());
                    let directory = model
                        .model_dir
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("managed-model");
                    let marker = if active { "active" } else { "inactive" };
                    body = body.push(
                        row![
                            text(format!(
                                "{title} · {directory} · {} files · {marker}",
                                model.file_count
                            ))
                            .width(Length::Fill),
                            button("Remove").on_press_maybe((!busy && !active).then_some(
                                Message::RemoveInstalledModel(model.model_dir.clone(),)
                            ),),
                        ]
                        .spacing(10),
                    );
                }
            }
            Err(error) => {
                body = body.push(text(format!("Installed model scan failed: {error}")));
            }
        }

        match &self.config {
            Ok(document) => {
                body = body.push(text("ASR providers").size(22));
                for provider in filtered_asr_rows(&document.config, &self.filter) {
                    body = body.push(text(provider));
                }
                body = body.push(text("Scenes").size(22));
                for scene in filtered_scene_rows(&document.config, &self.filter) {
                    body = body.push(text(scene));
                }
            }
            Err(error) => body = body.push(text(format!("Config error: {error}"))),
        }

        scrollable(body).into()
    }

    fn llm_page(&self) -> Element<'_, Message> {
        let mut body = column![text("LLM").size(30)].spacing(12);
        match &self.config {
            Ok(document) => {
                body = body.push(text("Providers").size(22));
                for provider in &document.config.llm.providers {
                    let endpoint = if provider.base_url.is_empty() {
                        "adapter/local".to_owned()
                    } else {
                        redact_url_for_diagnostics(&provider.base_url)
                    };
                    body = body.push(text(format!(
                        "{} · {} · {}",
                        provider.id,
                        provider.model.as_deref().unwrap_or("default model"),
                        endpoint
                    )));
                }
                if document.config.llm.providers.is_empty() {
                    body = body.push(text("No LLM providers configured."));
                }

                body = body.push(text("Adapters").size(22));
                for adapter in llm_adapter_rows(&document.config) {
                    body = body.push(text(adapter));
                }
                if document.config.llm.adapters.is_empty() {
                    body = body.push(text("No text adapters configured."));
                }
            }
            Err(error) => body = body.push(text(format!("Config error: {error}"))),
        }
        scrollable(body).into()
    }

    fn hotwords_page(&self) -> Element<'_, Message> {
        let mut body = column![text("Hotwords").size(30)].spacing(12);
        match &self.config {
            Ok(document) => {
                let mut count = 0;
                for provider in &document.config.asr.providers {
                    if let Some(path) = provider.hotwords_file.as_deref() {
                        count += 1;
                        body = body.push(text(format!("{} · {path}", provider.id)));
                    }
                }
                if count == 0 {
                    body = body.push(text("No hotword files configured."));
                }
            }
            Err(error) => body = body.push(text(format!("Config error: {error}"))),
        }
        scrollable(body).into()
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

/// Returns the managed ASR model root used by CLI and GUI workflows.
pub fn default_model_root() -> Result<PathBuf, String> {
    Ok(user_data_home()?.join("fcitx-vinput").join("models"))
}

fn default_model_staging_root() -> Result<PathBuf, String> {
    Ok(user_cache_home()?
        .join("fcitx-vinput")
        .join("model-install"))
}

fn user_data_home() -> Result<PathBuf, String> {
    match env::var_os("XDG_DATA_HOME") {
        Some(value) if !value.is_empty() => Ok(PathBuf::from(value)),
        _ => Ok(user_home()?.join(".local/share")),
    }
}

fn user_cache_home() -> Result<PathBuf, String> {
    match env::var_os("XDG_CACHE_HOME") {
        Some(value) if !value.is_empty() => Ok(PathBuf::from(value)),
        _ => Ok(user_home()?.join(".cache")),
    }
}

fn user_home() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is required to locate managed model storage".to_owned())
}

fn load_installed_models() -> Result<Vec<InstalledModelInfo>, String> {
    let root = default_model_root()?;
    scan_installed_models(&root).map_err(|error| error.to_string())
}

fn install_registry_model(config: &VinputConfig, selector: &str) -> Result<String, String> {
    let registry = fetch_live_model_registry(config)?;
    let model = registry
        .model_by_id_or_short_id(selector)
        .ok_or_else(|| format!("Unknown registry model id or short id `{selector}`."))?;
    let model_name = managed_model_dir_name(model);
    let model_dir = default_model_root()?.join(&model_name);
    let staging_dir = default_model_staging_root()?.join(&model_name);
    let source = ReqwestRegistryAssetSource::with_timeout(Duration::from_secs(300));
    let installed = install_live_model(
        &source,
        &LiveModelInstallRequest {
            model,
            model_dir,
            staging_dir,
            display: Some(model.installed_display_metadata(&config.global.default_language, None)),
        },
    )
    .map_err(|error| format!("Model installation failed: {error}"))?;
    let checksum = if installed.checksum_verified() {
        "checksum verified"
    } else {
        "registry provided no checksum"
    };
    Ok(format!(
        "Installed {} into managed model `{model_name}` ({checksum}).",
        model.resolved_title(None)
    ))
}

fn fetch_live_model_registry(config: &VinputConfig) -> Result<LiveModelRegistry, String> {
    let source = ReqwestRegistryTextSource::with_timeout(Duration::from_secs(30));
    fetch_live_model_registry_from(config, &source)
}

fn fetch_live_model_registry_from(
    config: &VinputConfig,
    source: &impl RegistryTextSource,
) -> Result<LiveModelRegistry, String> {
    let urls = config
        .registry
        .base_urls
        .iter()
        .map(|base| format!("{}/registry/models.json", base.trim_end_matches('/')))
        .collect::<Vec<_>>();
    if urls.is_empty() {
        return Err("No registry mirrors are configured.".to_owned());
    }
    let mut failure_count = 0;
    for url in &urls {
        match source.fetch_registry_text(url) {
            Ok(text) => {
                return LiveModelRegistry::from_json_str(&text)
                    .map_err(|error| format!("Registry model catalog is invalid: {error}"));
            }
            Err(_) => failure_count += 1,
        }
    }
    Err(format!(
        "All {failure_count} configured registry mirrors failed."
    ))
}

fn remove_installed_model(config: &VinputConfig, target_path: &Path) -> Result<String, String> {
    let model_root = default_model_root()?;
    let active_model_paths = config
        .asr
        .providers
        .iter()
        .filter(|provider| provider.kind == AsrProviderKind::Local)
        .filter_map(|provider| provider.model.as_deref())
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    remove_managed_model(&ManagedModelRemoveRequest {
        model_root: &model_root,
        target_path,
        active_model_paths: &active_model_paths,
    })
    .map_err(|error| error.to_string())?;
    let directory = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("managed model");
    Ok(format!("Removed inactive managed model `{directory}`."))
}

fn model_is_active(config: &VinputConfig, model_dir: &Path) -> bool {
    config.asr.providers.iter().any(|provider| {
        provider.kind == AsrProviderKind::Local
            && provider
                .model
                .as_deref()
                .is_some_and(|model| Path::new(model) == model_dir)
    })
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

/// Queries daemon status and runtime diagnostics using the shared D-Bus contract.
pub fn query_daemon_snapshot() -> Result<DaemonSnapshot, String> {
    let connection = zbus::blocking::Connection::session().map_err(|error| error.to_string())?;
    let proxy = daemon_proxy(&connection)?;
    let status = proxy
        .call::<_, _, String>(dbus::method::GET_STATUS, &())
        .map_err(|error| error.to_string())?;
    let runtime_json = proxy
        .call::<_, _, String>(dbus::method::GET_RUNTIME_STATUS, &())
        .map_err(|error| error.to_string())?;
    let runtime = serde_json::from_str(&runtime_json).map_err(|error| error.to_string())?;
    Ok(DaemonSnapshot { status, runtime })
}

fn daemon_proxy(
    connection: &zbus::blocking::Connection,
) -> Result<zbus::blocking::Proxy<'_>, String> {
    zbus::blocking::Proxy::new(
        connection,
        dbus::SERVICE_BUS_NAME,
        dbus::SERVICE_OBJECT_PATH,
        dbus::SERVICE_INTERFACE,
    )
    .map_err(|error| error.to_string())
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

fn persist_config_draft(
    document: &ConfigDocument,
    draft: &ConfigDraft,
) -> Result<ConfigSaveOutcome, String> {
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

    let mut updated = document.config.clone();
    draft.apply_to(&mut updated);
    updated
        .validate()
        .map_err(|error| format!("Validate edited configuration: {error}"))?;
    let backup_path = document
        .from_disk
        .then(|| config_backup_path(&document.path));
    let receipt = write_config_file(&updated, &document.path, backup_path.as_deref())
        .map_err(|error| format!("Save configuration: {error}"))?;
    Ok(ConfigSaveOutcome {
        path: receipt.path,
        backup_path: receipt.backup_path,
        daemon_reload: "daemon reload not attempted".to_owned(),
    })
}

fn save_config_with_daemon(
    document: &ConfigDocument,
    draft: &ConfigDraft,
) -> Result<ConfigSaveOutcome, String> {
    let daemon = query_daemon_snapshot();
    if let Ok(snapshot) = &daemon {
        ensure_config_save_allowed(snapshot)?;
    }

    let mut outcome = persist_config_draft(document, draft)?;
    outcome.daemon_reload = match daemon {
        Ok(_) => match reload_asr_backend() {
            Ok(()) => "daemon ASR reload requested".to_owned(),
            Err(error) => format!("config saved; daemon reload failed: {error}"),
        },
        Err(error) => format!("config saved; daemon reload skipped: {error}"),
    };
    Ok(outcome)
}

fn reload_asr_backend() -> Result<(), String> {
    let connection = zbus::blocking::Connection::session().map_err(|error| error.to_string())?;
    let proxy = daemon_proxy(&connection)?;
    proxy
        .call::<_, _, ()>(dbus::method::RELOAD_ASR_BACKEND, &())
        .map_err(|error| error.to_string())
}

fn run_recording_action(start: bool, scene: &str) -> Result<String, String> {
    let connection = zbus::blocking::Connection::session().map_err(|error| error.to_string())?;
    let proxy = daemon_proxy(&connection)?;
    if start {
        proxy
            .call::<_, _, ()>(dbus::method::START_RECORDING, &())
            .map_err(|error| error.to_string())?;
        Ok("Recording started.".to_owned())
    } else {
        let _: String = proxy
            .call(dbus::method::STOP_RECORDING, &scene)
            .map_err(|error| error.to_string())?;
        Ok("Recording stopped; the recognition result was delivered to the frontend.".to_owned())
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
        "pages": Page::ALL.map(Page::label),
    }))
}

fn daemon_refresh_task() -> Task<Message> {
    Task::perform(async { query_daemon_snapshot() }, Message::DaemonLoaded)
}

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
        .title(APPLICATION_TITLE)
        .theme(Theme::TokyoNight)
        .window_size((960.0, 640.0))
        .run()
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs};

    use super::*;

    #[derive(Default)]
    struct StubRegistryTextSource {
        responses: HashMap<String, Result<String, String>>,
    }

    impl RegistryTextSource for StubRegistryTextSource {
        fn fetch_registry_text(&self, url: &str) -> Result<String, String> {
            self.responses
                .get(url)
                .cloned()
                .unwrap_or_else(|| Err("missing fixture".to_owned()))
        }
    }

    #[test]
    fn bundled_snapshot_is_redacted_and_has_legacy_pages() {
        let snapshot = headless_snapshot(Some(Path::new("/missing/config.json")), false)
            .expect("build offline GUI snapshot");
        assert_eq!(snapshot["application"], "vinput-gui");
        assert_eq!(
            snapshot["pages"],
            json!(["Control", "Resources", "LLM", "Hotwords"])
        );
        assert_eq!(snapshot["daemon"]["skipped"], true);
        assert!(!snapshot.to_string().contains("api_key"));
    }

    #[test]
    fn resource_filter_matches_provider_and_scene_rows() {
        let config = VinputConfig::bundled_default().expect("bundled config");
        assert!(
            filtered_asr_rows(&config, "sherpa")
                .iter()
                .any(|row| row.contains("sherpa-onnx"))
        );
        assert!(
            filtered_scene_rows(&config, "raw")
                .iter()
                .any(|row| row.contains("__raw__"))
        );
    }

    #[test]
    fn registry_model_fetch_uses_mirror_fallback_without_leaking_urls() {
        let mut config = VinputConfig::bundled_default().expect("bundled config");
        let first = "https://user:super-secret@first.invalid".to_owned();
        let second = "https://second.invalid".to_owned();
        config.registry.base_urls = vec![first.clone(), second.clone()];
        let model_json = json!({
            "version": 1,
            "items": [{
                "id": "model.test.fixture",
                "short_id": "fixture",
                "urls": ["https://assets.invalid/fixture.tar.zst"]
            }]
        })
        .to_string();
        let source = StubRegistryTextSource {
            responses: HashMap::from([
                (
                    format!("{first}/registry/models.json"),
                    Err("connection failed".to_owned()),
                ),
                (format!("{second}/registry/models.json"), Ok(model_json)),
            ]),
        };

        let registry = fetch_live_model_registry_from(&config, &source).expect("mirror fallback");
        assert!(registry.model_by_id_or_short_id("fixture").is_some());

        let failed = StubRegistryTextSource::default();
        let error =
            fetch_live_model_registry_from(&config, &failed).expect_err("all mirrors should fail");
        assert_eq!(error, "All 2 configured registry mirrors failed.");
        assert!(!error.contains("super-secret"));
        assert!(!error.contains("first.invalid"));
    }

    #[test]
    fn active_model_detection_matches_only_local_provider_paths() {
        let mut config = VinputConfig::bundled_default().expect("bundled config");
        let model_dir = PathBuf::from("/managed/models/active");
        let provider = config
            .asr
            .providers
            .iter_mut()
            .find(|provider| provider.kind == AsrProviderKind::Local)
            .expect("local provider");
        provider.model = Some(model_dir.to_string_lossy().into_owned());
        assert!(model_is_active(&config, &model_dir));
        assert!(!model_is_active(
            &config,
            Path::new("/managed/models/inactive")
        ));
    }

    #[test]
    fn adapter_rows_never_expose_commands_or_environment() {
        let mut config = VinputConfig::bundled_default().expect("bundled config");
        config.llm.adapters.push(vinput_config::LlmAdapterConfig {
            id: "safe-adapter".to_owned(),
            command: "helper --token super-secret".to_owned(),
            args: vec!["--api-key".to_owned(), "another-secret".to_owned()],
            env: HashMap::from([("TOKEN".to_owned(), "env-secret".to_owned())]),
            working_dir: None,
            extra: HashMap::new(),
        });

        let rows = llm_adapter_rows(&config).join("\n");
        assert_eq!(rows, "safe-adapter · command adapter");
        assert!(!rows.contains("secret"));
        assert!(!rows.contains("token"));
    }

    #[test]
    fn disk_config_is_validated_before_display() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = directory.path().join("config.json");
        let mut config = VinputConfig::bundled_default().expect("bundled config");
        config.global.default_language = "zh-CN".to_owned();
        fs::write(
            &path,
            serde_json::to_vec_pretty(&config).expect("serialize config"),
        )
        .expect("write config");

        let loaded = load_config_document(Some(&path)).expect("load config");
        assert!(loaded.from_disk);
        assert_eq!(loaded.config.global.default_language, "zh-CN");
    }

    #[test]
    fn config_draft_applies_every_editable_field() {
        let mut config = VinputConfig::bundled_default().expect("bundled config");
        let provider = config
            .asr
            .providers
            .last()
            .expect("bundled provider")
            .id
            .clone();
        let scene = config
            .scenes
            .definitions
            .last()
            .expect("bundled scene")
            .id
            .clone();
        let mut draft = ConfigDraft::from_config(&config);
        draft.default_language = "zh-CN".to_owned();
        draft.capture_device = "test-source".to_owned();
        draft.duck_output_while_recording = true;
        draft.duck_output_volume = 0.4;
        draft.vad_enabled = false;
        draft.vad_threshold = 0.65;
        draft.active_provider.clone_from(&provider);
        draft.active_scene.clone_from(&scene);

        draft.apply_to(&mut config);

        config.validate().expect("validate edited config");
        assert_eq!(config.global.default_language, "zh-CN");
        assert_eq!(config.global.capture_device, "test-source");
        assert!(config.global.duck_output_while_recording);
        assert!((config.global.duck_output_volume - 0.4).abs() < f32::EPSILON);
        assert!(!config.asr.vad.enabled);
        assert!((config.asr.vad.threshold - 0.65).abs() < f32::EPSILON);
        assert_eq!(config.asr.active_provider, provider);
        assert_eq!(config.scenes.active_scene, scene);
    }

    #[test]
    fn config_draft_creates_missing_user_file_without_backup() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = directory.path().join("nested/config.json");
        let config = VinputConfig::bundled_default().expect("bundled config");
        let document = ConfigDocument {
            path: path.clone(),
            from_disk: false,
            config,
        };
        let mut draft = ConfigDraft::from_config(&document.config);
        draft.default_language = "zh-CN".to_owned();

        let outcome = persist_config_draft(&document, &draft).expect("create user config");

        assert_eq!(outcome.path, path);
        assert_eq!(outcome.backup_path, None);
        assert_eq!(
            VinputConfig::from_json_file(&outcome.path)
                .expect("load created config")
                .global
                .default_language,
            "zh-CN"
        );
    }

    #[test]
    fn config_draft_replaces_existing_file_with_backup() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = directory.path().join("config.json");
        let config = VinputConfig::bundled_default().expect("bundled config");
        write_config_file(&config, &path, None).expect("write original config");
        let document = load_config_document(Some(&path)).expect("load original config");
        let mut draft = ConfigDraft::from_config(&document.config);
        draft.capture_device = "replacement-source".to_owned();

        let outcome = persist_config_draft(&document, &draft).expect("replace user config");

        let backup_path = config_backup_path(&path);
        assert_eq!(outcome.backup_path.as_deref(), Some(backup_path.as_path()));
        assert_eq!(
            VinputConfig::from_json_file(&path)
                .expect("load replaced config")
                .global
                .capture_device,
            "replacement-source"
        );
        assert_eq!(
            VinputConfig::from_json_file(&backup_path)
                .expect("load backup config")
                .global
                .capture_device,
            config.global.capture_device
        );
    }

    #[test]
    fn config_draft_rejects_external_changes_without_overwrite() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = directory.path().join("config.json");
        let config = VinputConfig::bundled_default().expect("bundled config");
        write_config_file(&config, &path, None).expect("write original config");
        let document = load_config_document(Some(&path)).expect("load original config");
        let mut external = config.clone();
        external.global.capture_device = "external-source".to_owned();
        write_config_file(&external, &path, None).expect("write external update");
        let mut draft = ConfigDraft::from_config(&document.config);
        draft.capture_device = "gui-source".to_owned();

        let error = persist_config_draft(&document, &draft)
            .expect_err("external update must block GUI save");

        assert!(error.contains("changed on disk"));
        assert_eq!(
            VinputConfig::from_json_file(&path)
                .expect("load preserved external config")
                .global
                .capture_device,
            "external-source"
        );
        assert!(!config_backup_path(&path).exists());
    }

    #[test]
    fn config_save_guard_requires_idle_daemon_without_active_session() {
        let idle = DaemonSnapshot {
            status: "idle".to_owned(),
            runtime: json!({"active_session": false}),
        };
        assert!(ensure_config_save_allowed(&idle).is_ok());

        let recording = DaemonSnapshot {
            status: "recording".to_owned(),
            runtime: json!({"active_session": true}),
        };
        assert!(ensure_config_save_allowed(&recording).is_err());

        let inconsistent = DaemonSnapshot {
            status: "idle".to_owned(),
            runtime: json!({"active_session": true}),
        };
        assert!(ensure_config_save_allowed(&inconsistent).is_err());
    }
}

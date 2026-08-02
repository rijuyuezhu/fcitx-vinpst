//! Rust management GUI state, data loading, and D-Bus integration.

use std::{
    env,
    path::{Path, PathBuf},
};

use iced::{
    Element, Length, Task, Theme,
    widget::{button, column, container, row, scrollable, text, text_input},
};
use serde_json::{Value, json};
use vinput_config::{AsrProviderKind, VinputConfig, redact_url_for_diagnostics};
use vinput_protocol::dbus;

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
    daemon: DaemonLoadState,
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
}

impl App {
    /// Creates the initial GUI state and starts a daemon refresh.
    pub fn boot() -> (Self, Task<Message>) {
        let config = load_config_document(None);
        (
            Self {
                page: Page::Control,
                filter: String::new(),
                config,
                daemon: DaemonLoadState::Loading,
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
            Message::ReloadConfig => {
                self.config = load_config_document(None);
            }
        }
        Task::none()
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
        let mut body = column![
            text("Control").size(30),
            row![
                button("Refresh daemon").on_press(Message::RefreshDaemon),
                button("Reload config").on_press(Message::ReloadConfig),
            ]
            .spacing(10),
        ]
        .spacing(14);

        body = body.push(match &self.daemon {
            DaemonLoadState::Loading => text("Daemon: loading…"),
            DaemonLoadState::Ready(snapshot) => text(format!("Daemon: {}", snapshot.status)),
            DaemonLoadState::Failed(error) => text(format!("Daemon unavailable: {error}")),
        });

        match &self.config {
            Ok(document) => {
                let config = &document.config;
                body = body
                    .push(text(format!("Config: {}", document.path.display())))
                    .push(text(format!(
                        "Source: {}",
                        if document.from_disk {
                            "user file"
                        } else {
                            "bundled default"
                        }
                    )))
                    .push(text(format!(
                        "Active scene: {}",
                        config.scenes.active_scene
                    )))
                    .push(text(format!(
                        "Active ASR provider: {}",
                        config.asr.active_provider
                    )))
                    .push(text(format!(
                        "Capture device: {}",
                        config.global.capture_device
                    )))
                    .push(text(format!(
                        "Language: {}",
                        config.global.default_language
                    )))
                    .push(text(format!(
                        "VAD: {} (threshold {:.2})",
                        enabled_label(config.asr.vad.enabled),
                        config.asr.vad.threshold
                    )))
                    .push(text(format!(
                        "Output ducking: {} ({:.0}%)",
                        enabled_label(config.global.duck_output_while_recording),
                        config.global.duck_output_volume * 100.0
                    )));
            }
            Err(error) => body = body.push(text(format!("Config error: {error}"))),
        }

        scrollable(body).into()
    }

    fn resources_page(&self) -> Element<'_, Message> {
        let mut body = column![
            text("Resources").size(30),
            text_input("Filter providers and scenes", &self.filter)
                .on_input(Message::FilterChanged),
        ]
        .spacing(12);

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
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        dbus::SERVICE_BUS_NAME,
        dbus::SERVICE_OBJECT_PATH,
        dbus::SERVICE_INTERFACE,
    )
    .map_err(|error| error.to_string())?;
    let status = proxy
        .call::<_, _, String>(dbus::method::GET_STATUS, &())
        .map_err(|error| error.to_string())?;
    let runtime_json = proxy
        .call::<_, _, String>(dbus::method::GET_RUNTIME_STATUS, &())
        .map_err(|error| error.to_string())?;
    let runtime = serde_json::from_str(&runtime_json).map_err(|error| error.to_string())?;
    Ok(DaemonSnapshot { status, runtime })
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

fn enabled_label(value: bool) -> &'static str {
    if value { "enabled" } else { "disabled" }
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
}

//! Desktop file-opening actions shared by the management GUI.

use std::{
    ffi::OsString,
    fmt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
};

use iced::{Element, Task, widget::button};
use vinput_daemon_control::{FLATPAK_INFO_PATH_ENV, FLATPAK_SPAWN_ENV};

use crate::{App, Message, OperationState};

const DESKTOP_OPENER_ENV: &str = "VINPUT_DESKTOP_OPENER";
const DEFAULT_FLATPAK_INFO_PATH: &str = "/.flatpak-info";

/// One global desktop integration interaction.
#[derive(Debug, Clone, Copy)]
pub enum DesktopActionMessage {
    /// Open the loaded configuration file in the desktop's associated application.
    OpenConfig,
    /// Complete one asynchronous desktop-open request.
    ConfigOpened(Result<DesktopOpenOutcome, DesktopOpenFailure>),
}

/// Successful handoff to the configured desktop opener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopOpenOutcome {
    host_wrapped: bool,
}

/// Fixed path-free failure category for desktop opening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopOpenFailure {
    /// The configured opener process could not be started.
    LaunchFailed,
    /// The child-reaper thread could not be created after the opener started.
    ReaperFailed,
}

impl DesktopOpenFailure {
    fn message(self) -> &'static str {
        match self {
            Self::LaunchFailed => {
                "Cannot open the config file: the desktop opener could not be started."
            }
            Self::ReaperFailed => {
                "Cannot open the config file: the desktop opener could not be supervised safely."
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct DesktopOpenCommand {
    program: OsString,
    args: Vec<OsString>,
    host_wrapped: bool,
}

impl fmt::Debug for DesktopOpenCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DesktopOpenCommand")
            .field("program", &self.program)
            .field("argument_count", &self.args.len())
            .field("host_wrapped", &self.host_wrapped)
            .field("target", &"<redacted path>")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DesktopOpenEnvironment {
    opener_program: OsString,
    flatpak_info_path: PathBuf,
    flatpak_spawn_program: OsString,
}

impl DesktopOpenEnvironment {
    fn from_process() -> Self {
        let opener_program = std::env::var_os(DESKTOP_OPENER_ENV)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| OsString::from("xdg-open"));
        let flatpak_info_path = std::env::var_os(FLATPAK_INFO_PATH_ENV)
            .filter(|value| !value.is_empty())
            .map_or_else(|| PathBuf::from(DEFAULT_FLATPAK_INFO_PATH), PathBuf::from);
        let flatpak_spawn_program = std::env::var_os(FLATPAK_SPAWN_ENV)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| OsString::from("flatpak-spawn"));
        Self {
            opener_program,
            flatpak_info_path,
            flatpak_spawn_program,
        }
    }

    #[cfg(test)]
    fn new(
        opener_program: impl Into<OsString>,
        flatpak_info_path: impl Into<PathBuf>,
        flatpak_spawn_program: impl Into<OsString>,
    ) -> Self {
        Self {
            opener_program: opener_program.into(),
            flatpak_info_path: flatpak_info_path.into(),
            flatpak_spawn_program: flatpak_spawn_program.into(),
        }
    }

    fn is_flatpak(&self) -> bool {
        self.flatpak_info_path.exists()
    }
}

impl App {
    pub(super) fn desktop_action_button(&self, busy: bool) -> Element<'_, Message> {
        button("Open config")
            .on_press_maybe(
                (!busy && self.config.is_ok())
                    .then_some(Message::DesktopAction(DesktopActionMessage::OpenConfig)),
            )
            .into()
    }

    pub(super) fn intercept_desktop_action_message(
        &mut self,
        message: &Message,
    ) -> Option<Task<Message>> {
        let Message::DesktopAction(message) = message else {
            return None;
        };
        if self.is_busy() && !matches!(message, DesktopActionMessage::ConfigOpened(_)) {
            return Some(Task::none());
        }
        Some(self.handle_desktop_action_message(*message))
    }

    fn handle_desktop_action_message(&mut self, message: DesktopActionMessage) -> Task<Message> {
        match message {
            DesktopActionMessage::OpenConfig => self.begin_open_config(),
            DesktopActionMessage::ConfigOpened(result) => {
                self.operation = match result {
                    Ok(outcome) => OperationState::Succeeded(if outcome.host_wrapped {
                        "Passed the config file to the host desktop opener.".to_owned()
                    } else {
                        "Passed the config file to the desktop opener.".to_owned()
                    }),
                    Err(error) => OperationState::Failed(error.message().to_owned()),
                };
                Task::none()
            }
        }
    }

    fn begin_open_config(&mut self) -> Task<Message> {
        let Ok(document) = &self.config else {
            self.operation = OperationState::Failed("No valid config is loaded.".to_owned());
            return Task::none();
        };
        let path = document.path.clone();
        self.operation = OperationState::Running("Opening config file…");
        Task::perform(
            async move { spawn_desktop_open(&path, &DesktopOpenEnvironment::from_process()) },
            |result| Message::DesktopAction(DesktopActionMessage::ConfigOpened(result)),
        )
    }
}

fn desktop_open_command(path: &Path, environment: &DesktopOpenEnvironment) -> DesktopOpenCommand {
    if environment.is_flatpak() {
        return DesktopOpenCommand {
            program: environment.flatpak_spawn_program.clone(),
            args: vec![
                OsString::from("--host"),
                environment.opener_program.clone(),
                path.as_os_str().to_owned(),
            ],
            host_wrapped: true,
        };
    }
    DesktopOpenCommand {
        program: environment.opener_program.clone(),
        args: vec![path.as_os_str().to_owned()],
        host_wrapped: false,
    }
}

fn spawn_desktop_open(
    path: &Path,
    environment: &DesktopOpenEnvironment,
) -> Result<DesktopOpenOutcome, DesktopOpenFailure> {
    let command = desktop_open_command(path, environment);
    let mut child = Command::new(&command.program)
        .args(&command.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| DesktopOpenFailure::LaunchFailed)?;
    let host_wrapped = command.host_wrapped;
    let (sender, receiver) = mpsc::sync_channel::<Child>(1);
    if thread::Builder::new()
        .name("vinput-desktop-opener-reaper".to_owned())
        .spawn(move || {
            if let Ok(mut child) = receiver.recv() {
                let _ = child.wait();
            }
        })
        .is_err()
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err(DesktopOpenFailure::ReaperFailed);
    }
    if let Err(error) = sender.send(child) {
        let mut child = error.0;
        let _ = child.kill();
        let _ = child.wait();
        return Err(DesktopOpenFailure::ReaperFailed);
    }
    Ok(DesktopOpenOutcome { host_wrapped })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_and_flatpak_commands_are_direct_argv_and_redact_paths() {
        let directory = tempfile::tempdir().expect("desktop fixture");
        let path = directory.path().join("config with spaces.json");
        let native = desktop_open_command(
            &path,
            &DesktopOpenEnvironment::new("/usr/bin/xdg-open", "/missing", "flatpak-spawn"),
        );
        assert_eq!(native.program, std::ffi::OsStr::new("/usr/bin/xdg-open"));
        assert_eq!(native.args, [path.as_os_str()]);
        assert!(!native.host_wrapped);
        assert!(!format!("{native:?}").contains("config with spaces"));

        let flatpak_info = directory.path().join("flatpak-info");
        std::fs::write(&flatpak_info, "fixture").expect("flatpak marker");
        let wrapped = desktop_open_command(
            &path,
            &DesktopOpenEnvironment::new(
                "/custom/xdg-open",
                &flatpak_info,
                "/custom/flatpak-spawn",
            ),
        );
        assert_eq!(
            wrapped.program,
            std::ffi::OsStr::new("/custom/flatpak-spawn")
        );
        assert_eq!(
            wrapped.args,
            [
                std::ffi::OsStr::new("--host"),
                std::ffi::OsStr::new("/custom/xdg-open"),
                path.as_os_str(),
            ]
        );
        assert!(wrapped.host_wrapped);
    }

    #[test]
    fn opener_spawn_accepts_a_successful_direct_launcher() {
        let environment = DesktopOpenEnvironment::new("/bin/true", "/missing", "flatpak-spawn");
        let outcome = spawn_desktop_open(Path::new("/tmp/config.json"), &environment)
            .expect("direct opener should start");
        assert!(!outcome.host_wrapped);
    }

    #[test]
    fn opener_spawn_reports_only_fixed_categories() {
        let environment = DesktopOpenEnvironment::new(
            "/definitely/missing/vinput-opener",
            "/missing",
            "flatpak-spawn",
        );
        let error = spawn_desktop_open(Path::new("/secret/config.json"), &environment)
            .expect_err("missing opener should fail");
        assert_eq!(error, DesktopOpenFailure::LaunchFailed);
        assert!(!format!("{error:?}").contains("secret"));
    }
}

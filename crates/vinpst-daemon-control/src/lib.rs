//! Shared per-user daemon service-control command construction and execution.

use std::{fmt, path::PathBuf, process::Command};

/// Installed systemd user-service unit name.
pub const DAEMON_SERVICE_NAME: &str = "vinpst-daemon.service";
/// Environment override for the `systemctl` executable.
pub const SYSTEMCTL_ENV: &str = "VINPST_DAEMON_SYSTEMCTL";
/// Environment override for the Flatpak host-command wrapper.
pub const FLATPAK_SPAWN_ENV: &str = "VINPST_FLATPAK_SPAWN";
/// Environment override for Flatpak detection.
pub const FLATPAK_INFO_PATH_ENV: &str = "VINPST_FLATPAK_INFO_PATH";

const DEFAULT_FLATPAK_INFO_PATH: &str = "/.flatpak-info";

/// One supported systemd user-service operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserServiceAction {
    /// Stop the daemon service.
    Stop,
    /// Restart the daemon service.
    Restart,
    /// Disable and stop the daemon service.
    DisableNow,
    /// Reload the user systemd manager configuration.
    DaemonReload,
    /// Read the daemon unit's main process id.
    MainPid,
}

impl UserServiceAction {
    /// Stable action name used by CLI and GUI diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::DisableNow => "disable-now",
            Self::DaemonReload => "daemon-reload",
            Self::MainPid => "main-pid",
        }
    }

    /// Whether executing this action mutates user-service state.
    #[must_use]
    pub const fn mutates_user_service(self) -> bool {
        matches!(
            self,
            Self::Stop | Self::Restart | Self::DisableNow | Self::DaemonReload
        )
    }

    fn args(self) -> Vec<String> {
        let values: &[&str] = match self {
            Self::Stop => &["--user", "stop", DAEMON_SERVICE_NAME],
            Self::Restart => &["--user", "restart", DAEMON_SERVICE_NAME],
            Self::DisableNow => &["--user", "disable", "--now", DAEMON_SERVICE_NAME],
            Self::DaemonReload => &["--user", "daemon-reload"],
            Self::MainPid => &[
                "--user",
                "show",
                "--property",
                "MainPID",
                "--value",
                DAEMON_SERVICE_NAME,
            ],
        };
        values.iter().map(|value| (*value).to_owned()).collect()
    }
}

/// Process environment inputs used to construct one service-control command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonControlEnvironment {
    systemctl_program: String,
    flatpak_info_path: PathBuf,
    flatpak_spawn_program: String,
}

impl DaemonControlEnvironment {
    /// Reads command overrides and Flatpak detection inputs from the process environment.
    pub fn from_process() -> Self {
        let systemctl_program =
            std::env::var(SYSTEMCTL_ENV).unwrap_or_else(|_| "systemctl".to_owned());
        let flatpak_info_path = std::env::var_os(FLATPAK_INFO_PATH_ENV)
            .filter(|value| !value.is_empty())
            .map_or_else(|| PathBuf::from(DEFAULT_FLATPAK_INFO_PATH), PathBuf::from);
        let flatpak_spawn_program =
            std::env::var(FLATPAK_SPAWN_ENV).unwrap_or_else(|_| "flatpak-spawn".to_owned());
        Self {
            systemctl_program,
            flatpak_info_path,
            flatpak_spawn_program,
        }
    }

    /// Creates explicit command inputs, primarily for deterministic integration tests.
    pub fn new(
        systemctl_program: impl Into<String>,
        flatpak_info_path: impl Into<PathBuf>,
        flatpak_spawn_program: impl Into<String>,
    ) -> Self {
        Self {
            systemctl_program: systemctl_program.into(),
            flatpak_info_path: flatpak_info_path.into(),
            flatpak_spawn_program: flatpak_spawn_program.into(),
        }
    }

    /// Whether the configured Flatpak information path exists.
    #[must_use]
    pub fn is_flatpak(&self) -> bool {
        self.flatpak_info_path.exists()
    }
}

/// One direct-argv service-control command.
#[derive(Clone, PartialEq, Eq)]
pub struct UserServiceCommand {
    /// Executable invoked by the current process.
    pub program: String,
    /// Direct argument vector excluding `program`.
    pub args: Vec<String>,
}

impl fmt::Debug for UserServiceCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserServiceCommand")
            .field("program", &self.program)
            .field("argument_count", &self.args.len())
            .field("host_wrapped", &self.is_host_wrapped())
            .finish()
    }
}

impl UserServiceCommand {
    /// Complete argv including the executable.
    #[must_use]
    pub fn argv(&self) -> Vec<String> {
        std::iter::once(self.program.clone())
            .chain(self.args.iter().cloned())
            .collect()
    }

    /// Shell-free display form used only for diagnostics and dry-run output.
    #[must_use]
    pub fn display(&self) -> String {
        self.argv().join(" ")
    }

    /// Whether this command uses `flatpak-spawn --host`.
    #[must_use]
    pub fn is_host_wrapped(&self) -> bool {
        self.args.first().map(String::as_str) == Some("--host") && self.args.len() >= 2
    }

    /// Actual target executable after an optional host wrapper.
    #[must_use]
    pub fn target_program(&self) -> &str {
        if self.is_host_wrapped() {
            &self.args[1]
        } else {
            &self.program
        }
    }

    /// Optional host-wrapper executable.
    #[must_use]
    pub fn host_wrapper_program(&self) -> Option<&str> {
        self.is_host_wrapped().then_some(self.program.as_str())
    }
}

/// Builds one systemd user-service command from the current process environment.
#[must_use]
pub fn user_service_command(action: UserServiceAction) -> UserServiceCommand {
    user_service_command_with(action, &DaemonControlEnvironment::from_process())
}

/// Builds one systemd user-service command from explicit environment inputs.
#[must_use]
pub fn user_service_command_with(
    action: UserServiceAction,
    environment: &DaemonControlEnvironment,
) -> UserServiceCommand {
    let target_program = environment.systemctl_program.clone();
    let target_args = action.args();
    if !environment.is_flatpak() {
        return UserServiceCommand {
            program: target_program,
            args: target_args,
        };
    }
    let mut args = Vec::with_capacity(target_args.len() + 2);
    args.push("--host".to_owned());
    args.push(target_program);
    args.extend(target_args);
    UserServiceCommand {
        program: environment.flatpak_spawn_program.clone(),
        args,
    }
}

/// Result of executing one direct-argv service-control command.
#[derive(Clone, PartialEq, Eq)]
pub struct UserServiceCommandOutcome {
    /// Whether the child exited successfully.
    pub ok: bool,
    /// Numeric child exit status when available.
    pub exit_status: Option<i32>,
    /// Captured standard output.
    pub stdout: String,
    /// Captured standard error.
    pub stderr: String,
    /// Spawn failure when the child could not be started.
    pub error: Option<String>,
}

impl fmt::Debug for UserServiceCommandOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserServiceCommandOutcome")
            .field("ok", &self.ok)
            .field("exit_status", &self.exit_status)
            .field("stdout_bytes", &self.stdout.len())
            .field("stderr_bytes", &self.stderr.len())
            .field("spawn_failed", &self.error.is_some())
            .finish()
    }
}

/// Executes one service-control command without a shell.
#[must_use]
pub fn run_user_service_command(command: &UserServiceCommand) -> UserServiceCommandOutcome {
    match Command::new(&command.program).args(&command.args).output() {
        Ok(output) => UserServiceCommandOutcome {
            ok: output.status.success(),
            exit_status: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            error: None,
        },
        Err(error) => UserServiceCommandOutcome {
            ok: false,
            exit_status: None,
            stdout: String::new(),
            stderr: String::new(),
            error: Some(error.to_string()),
        },
    }
}

/// Executes one service-control command without a shell while inheriting stdio.
///
/// This is intended for long-running interactive commands such as
/// `journalctl --follow`, where buffering child output until exit would make the
/// command unusable. The returned outcome therefore contains no captured
/// stdout/stderr.
#[must_use]
pub fn run_user_service_command_streaming(
    command: &UserServiceCommand,
) -> UserServiceCommandOutcome {
    match Command::new(&command.program).args(&command.args).status() {
        Ok(status) => UserServiceCommandOutcome {
            ok: status.success(),
            exit_status: status.code(),
            stdout: String::new(),
            stderr: String::new(),
            error: None,
        },
        Err(error) => UserServiceCommandOutcome {
            ok: false,
            exit_status: None,
            stdout: String::new(),
            stderr: String::new(),
            error: Some(error.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_commands_match_the_user_service_contract() {
        let environment = DaemonControlEnvironment::new(
            "/custom/systemctl",
            "/definitely/not/a/flatpak/info/file",
            "/custom/flatpak-spawn",
        );
        let stop = user_service_command_with(UserServiceAction::Stop, &environment);
        assert_eq!(
            stop.argv(),
            ["/custom/systemctl", "--user", "stop", DAEMON_SERVICE_NAME,]
        );
        assert!(!stop.is_host_wrapped());

        let restart = user_service_command_with(UserServiceAction::Restart, &environment);
        assert_eq!(
            restart.argv(),
            [
                "/custom/systemctl",
                "--user",
                "restart",
                DAEMON_SERVICE_NAME,
            ]
        );
    }

    #[test]
    fn flatpak_commands_preserve_host_and_tool_overrides() {
        let directory = tempfile::tempdir().expect("temp dir");
        let info = directory.path().join("flatpak-info");
        std::fs::write(&info, "fixture").expect("write Flatpak marker");
        let environment =
            DaemonControlEnvironment::new("/custom/systemctl", &info, "/custom/flatpak-spawn");
        let command = user_service_command_with(UserServiceAction::Restart, &environment);
        assert_eq!(command.program, "/custom/flatpak-spawn");
        assert_eq!(
            command.args,
            [
                "--host",
                "/custom/systemctl",
                "--user",
                "restart",
                DAEMON_SERVICE_NAME,
            ]
        );
        assert!(command.is_host_wrapped());
        assert_eq!(command.target_program(), "/custom/systemctl");
        assert_eq!(
            command.host_wrapper_program(),
            Some("/custom/flatpak-spawn")
        );
    }

    #[test]
    fn command_execution_captures_output_without_debug_exposure() {
        let command = UserServiceCommand {
            program: "/bin/echo".to_owned(),
            args: vec!["output-secret".to_owned()],
        };
        let outcome = run_user_service_command(&command);
        assert!(outcome.ok);
        assert!(outcome.stdout.contains("output-secret"));
        assert!(!format!("{outcome:?}").contains("output-secret"));
    }
}

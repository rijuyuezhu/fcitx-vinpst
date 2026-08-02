use std::path::PathBuf;

const DEFAULT_FLATPAK_INFO_PATH: &str = "/.flatpak-info";
pub(crate) const FLATPAK_INFO_PATH_ENV: &str = "VINPUT_FLATPAK_INFO_PATH";
pub(crate) const FLATPAK_SPAWN_ENV: &str = "VINPUT_FLATPAK_SPAWN";

pub(crate) fn is_flatpak() -> bool {
    flatpak_info_path().0.exists()
}

pub(crate) fn wrap_host_command(
    target_program: String,
    target_args: Vec<String>,
) -> (String, Vec<String>) {
    if !is_flatpak() {
        return (target_program, target_args);
    }

    wrap_host_command_for(target_program, target_args, flatpak_spawn_program())
}

pub(crate) fn daemon_log_args(service_name: &str) -> Vec<String> {
    daemon_log_args_for(service_name, is_flatpak())
}

pub(crate) fn sandbox_json(host_command: bool) -> serde_json::Value {
    let (info_path, path_overridden) = flatpak_info_path();
    serde_json::json!({
        "kind": host_command.then_some("flatpak"),
        "detected": host_command,
        "info_path": info_path,
        "info_path_env_override": FLATPAK_INFO_PATH_ENV,
        "info_path_overridden": path_overridden,
        "host_command": host_command,
    })
}

pub(crate) fn host_wrapper_json(program: &str) -> serde_json::Value {
    serde_json::json!({
        "program": program,
        "env_override": FLATPAK_SPAWN_ENV,
        "overridden": std::env::var_os(FLATPAK_SPAWN_ENV).is_some(),
    })
}

fn flatpak_info_path() -> (PathBuf, bool) {
    match std::env::var_os(FLATPAK_INFO_PATH_ENV) {
        Some(value) if !value.is_empty() => (PathBuf::from(value), true),
        _ => (PathBuf::from(DEFAULT_FLATPAK_INFO_PATH), false),
    }
}

fn flatpak_spawn_program() -> String {
    std::env::var(FLATPAK_SPAWN_ENV).unwrap_or_else(|_| "flatpak-spawn".to_owned())
}

fn wrap_host_command_for(
    target_program: String,
    target_args: Vec<String>,
    wrapper: String,
) -> (String, Vec<String>) {
    let mut args = Vec::with_capacity(target_args.len() + 2);
    args.push("--host".to_owned());
    args.push(target_program);
    args.extend(target_args);
    (wrapper, args)
}

fn daemon_log_args_for(service_name: &str, sandboxed: bool) -> Vec<String> {
    let values = if sandboxed {
        vec!["--user", "-t", "flatpak", "--grep", "vinput"]
    } else {
        vec!["--user", "-u", service_name]
    };
    values.into_iter().map(str::to_owned).collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{daemon_log_args_for, wrap_host_command_for};

    #[test]
    fn host_command_prefix_matches_legacy_flatpak_contract() {
        let (program, args) = wrap_host_command_for(
            "systemctl".to_owned(),
            ["--user", "restart", "vinput-daemon.service"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            "flatpak-spawn".to_owned(),
        );

        assert_eq!(program, "flatpak-spawn");
        assert_eq!(
            args,
            [
                "--host",
                "systemctl",
                "--user",
                "restart",
                "vinput-daemon.service"
            ]
        );
    }

    #[test]
    fn daemon_log_filter_matches_flatpak_and_host_contracts() {
        assert_eq!(
            daemon_log_args_for("vinput-daemon.service", true),
            ["--user", "-t", "flatpak", "--grep", "vinput"]
        );
        assert_eq!(
            daemon_log_args_for("vinput-daemon.service", false),
            ["--user", "-u", "vinput-daemon.service"]
        );
    }

    #[test]
    fn default_flatpak_info_path_is_absolute() {
        assert!(Path::new(super::DEFAULT_FLATPAK_INFO_PATH).is_absolute());
    }
}

use std::{fs, path::PathBuf};

use anyhow::Context;

const DEFAULT_FLATPAK_INFO_PATH: &str = "/.flatpak-info";
const DEFAULT_FLATPAK_APP_ID: &str = "org.fcitx.Fcitx5";
const DEFAULT_FLATPAK_ADDON_ROOT: &str = "/app/addons/Vinpst";
const REQUIRED_FILESYSTEM_PERMISSIONS: [&str; 3] =
    ["xdg-run/pipewire-0", "xdg-config/systemd", "xdg-cache"];
pub(crate) const FLATPAK_INFO_PATH_ENV: &str = "VINPST_FLATPAK_INFO_PATH";
pub(crate) const FLATPAK_SPAWN_ENV: &str = "VINPST_FLATPAK_SPAWN";
pub(crate) const FLATPAK_APP_ID_ENV: &str = "VINPST_FLATPAK_APP_ID";
pub(crate) const FLATPAK_ADDON_ROOT_ENV: &str = "VINPST_FLATPAK_ADDON_ROOT";

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

pub(crate) fn permission_report_json() -> serde_json::Value {
    let (info_path, overridden) = flatpak_info_path();
    if !info_path.exists() {
        return serde_json::json!({
            "detected": false,
            "info_path": info_path,
            "info_path_overridden": overridden,
            "missing_permissions": [],
            "remediation_commands": [],
            "read_error": null,
        });
    }
    match fs::read_to_string(&info_path) {
        Ok(contents) => {
            let missing = missing_permissions_from_info(&contents);
            let remediation = permission_remediation_commands(&missing, &flatpak_app_id());
            serde_json::json!({
                "detected": true,
                "info_path": info_path,
                "info_path_overridden": overridden,
                "missing_permissions": missing,
                "remediation_commands": remediation,
                "read_error": null,
            })
        }
        Err(error) => {
            let missing = required_permission_labels();
            let remediation = permission_remediation_commands(&missing, &flatpak_app_id());
            serde_json::json!({
                "detected": true,
                "info_path": info_path,
                "info_path_overridden": overridden,
                "missing_permissions": missing,
                "remediation_commands": remediation,
                "read_error": error.to_string(),
            })
        }
    }
}

pub(crate) fn default_service_template_path() -> PathBuf {
    flatpak_addon_root().join("share/systemd/user/vinpst-daemon.service")
}

pub(crate) fn render_flatpak_service(template: &str) -> anyhow::Result<String> {
    let app_id = flatpak_app_id();
    let daemon = flatpak_addon_root().join("bin/vinpst-daemon");
    render_flatpak_service_for(template, &app_id, &daemon.to_string_lossy())
}

fn flatpak_app_id() -> String {
    std::env::var(FLATPAK_APP_ID_ENV).unwrap_or_else(|_| DEFAULT_FLATPAK_APP_ID.to_owned())
}

fn flatpak_addon_root() -> PathBuf {
    std::env::var_os(FLATPAK_ADDON_ROOT_ENV)
        .filter(|value| !value.is_empty())
        .map_or_else(|| PathBuf::from(DEFAULT_FLATPAK_ADDON_ROOT), PathBuf::from)
}

fn missing_permissions_from_info(contents: &str) -> Vec<String> {
    let filesystems = context_values(contents, "filesystems");
    REQUIRED_FILESYSTEM_PERMISSIONS
        .into_iter()
        .filter(|required| {
            !filesystems
                .iter()
                .any(|entry| filesystem_permission_satisfies(entry, required))
        })
        .map(|value| format!("filesystem:{value}"))
        .collect()
}

fn filesystem_permission_satisfies(entry: &str, required: &str) -> bool {
    if entry == required {
        return true;
    }
    let Some((path, mode)) = entry.rsplit_once(':') else {
        return false;
    };
    path == required && matches!(mode, "rw" | "create")
}

fn required_permission_labels() -> Vec<String> {
    REQUIRED_FILESYSTEM_PERMISSIONS
        .into_iter()
        .map(|value| format!("filesystem:{value}"))
        .collect()
}

fn permission_remediation_commands(missing: &[String], app_id: &str) -> Vec<String> {
    let mut commands = missing
        .iter()
        .filter_map(|permission| match permission.as_str() {
            "filesystem:xdg-run/pipewire-0" => Some(format!(
                "flatpak override --user --filesystem=xdg-run/pipewire-0 {app_id}"
            )),
            "filesystem:xdg-config/systemd" => Some(format!(
                "flatpak override --user --filesystem=xdg-config/systemd:create {app_id}"
            )),
            "filesystem:xdg-cache" => Some(format!(
                "flatpak override --user --filesystem=xdg-cache {app_id}"
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    if !commands.is_empty() {
        commands.push(format!("flatpak kill {app_id}"));
    }
    commands
}

fn context_values(contents: &str, key: &str) -> Vec<String> {
    let mut in_context = false;
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_context = line == "[Context]";
            continue;
        }
        if !in_context {
            continue;
        }
        if let Some(value) = line.strip_prefix(&format!("{key}=")) {
            return value
                .split(';')
                .map(str::trim)
                .filter(|value| !value.is_empty() && !value.starts_with('!'))
                .map(str::to_owned)
                .collect();
        }
    }
    Vec::new()
}

fn render_flatpak_service_for(
    template: &str,
    app_id: &str,
    daemon_program: &str,
) -> anyhow::Result<String> {
    let mut output = Vec::new();
    let mut found_start = false;
    let mut found_stop = false;
    for line in template.lines() {
        if let Some(command) = line.strip_prefix("ExecStart=") {
            let args = command
                .split_once(char::is_whitespace)
                .map_or("", |(_, args)| args.trim());
            let suffix = if args.is_empty() {
                String::new()
            } else {
                format!(" {args}")
            };
            output.push(format!(
                "ExecStart=flatpak run --command={daemon_program} {app_id}{suffix}"
            ));
            found_start = true;
        } else if line.starts_with("ExecStop=") {
            output.push("ExecStop=pkill -INT vinpst-daemon".to_owned());
            found_stop = true;
        } else {
            output.push(line.to_owned());
        }
    }
    anyhow::ensure!(found_start, "service template has no ExecStart entry");
    if !found_stop {
        let position = output
            .iter()
            .position(|line| line.starts_with("ExecStart="))
            .context("locate rewritten ExecStart entry")?
            + 1;
        output.insert(position, "ExecStop=pkill -INT vinpst-daemon".to_owned());
    }
    Ok(format!("{}\n", output.join("\n")))
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
        vec!["--user", "-t", "flatpak", "--grep", "vinpst"]
    } else {
        vec!["--user", "-u", service_name]
    };
    values.into_iter().map(str::to_owned).collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        daemon_log_args_for, missing_permissions_from_info, permission_remediation_commands,
        render_flatpak_service_for, wrap_host_command_for,
    };

    #[test]
    fn host_command_prefix_matches_legacy_flatpak_contract() {
        let (program, args) = wrap_host_command_for(
            "systemctl".to_owned(),
            ["--user", "restart", "vinpst-daemon.service"]
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
                "vinpst-daemon.service"
            ]
        );
    }

    #[test]
    fn daemon_log_filter_matches_flatpak_and_host_contracts() {
        assert_eq!(
            daemon_log_args_for("vinpst-daemon.service", true),
            ["--user", "-t", "flatpak", "--grep", "vinpst"]
        );
        assert_eq!(
            daemon_log_args_for("vinpst-daemon.service", false),
            ["--user", "-u", "vinpst-daemon.service"]
        );
    }

    #[test]
    fn permission_parser_matches_flatpak_override_metadata() {
        let complete = "[Context]\nsockets=wayland;\nfilesystems=xdg-cache;xdg-run/pipewire-0;xdg-config/systemd:create;\n";
        assert!(missing_permissions_from_info(complete).is_empty());

        let partial =
            "[Context]\nsockets=wayland;pipewire;\nfilesystems=xdg-cache;xdg-config/systemd:ro;\n";
        assert_eq!(
            missing_permissions_from_info(partial),
            [
                "filesystem:xdg-run/pipewire-0",
                "filesystem:xdg-config/systemd"
            ]
        );
    }

    #[test]
    fn permission_remediation_matches_documented_flatpak_overrides() {
        let commands = permission_remediation_commands(
            &[
                "filesystem:xdg-run/pipewire-0".to_owned(),
                "filesystem:xdg-config/systemd".to_owned(),
                "filesystem:xdg-cache".to_owned(),
            ],
            "org.fcitx.Fcitx5",
        );
        assert_eq!(
            commands,
            [
                "flatpak override --user --filesystem=xdg-run/pipewire-0 org.fcitx.Fcitx5",
                "flatpak override --user --filesystem=xdg-config/systemd:create org.fcitx.Fcitx5",
                "flatpak override --user --filesystem=xdg-cache org.fcitx.Fcitx5",
                "flatpak kill org.fcitx.Fcitx5",
            ]
        );
    }

    #[test]
    fn service_rewrite_preserves_args_and_adds_flatpak_stop() {
        let template = "[Service]\nExecStart=/usr/bin/vinpst-daemon --dbus --configured-backends\n";
        let rendered = render_flatpak_service_for(
            template,
            "org.fcitx.Fcitx5",
            "/app/addons/Vinpst/bin/vinpst-daemon",
        )
        .expect("rewrite Flatpak service");
        assert!(rendered.contains(
            "ExecStart=flatpak run --command=/app/addons/Vinpst/bin/vinpst-daemon org.fcitx.Fcitx5 --dbus --configured-backends"
        ));
        assert!(rendered.contains("ExecStop=pkill -INT vinpst-daemon"));
    }

    #[test]
    fn default_flatpak_info_path_is_absolute() {
        assert!(Path::new(super::DEFAULT_FLATPAK_INFO_PATH).is_absolute());
    }
}

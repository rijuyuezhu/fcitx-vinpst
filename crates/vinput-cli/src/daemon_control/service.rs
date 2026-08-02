use super::status::daemon_status_via_dbus;
use super::{ProcessCommand, daemon_owner_probe_plan_json, dbus, optional_json_str};

use crate::sandbox;

pub(super) fn print_daemon_user_service_plan(
    action: &str,
    log_lines: Option<u16>,
    dry_run: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    let command = daemon_user_service_command(action, log_lines)?;
    if dry_run {
        let output = daemon_user_service_dry_run_json(action, &command);
        if json_output {
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            print_daemon_user_service_dry_run_text(action, &command);
        }
        return Ok(());
    }

    let output = run_daemon_user_service_command(action, &command);
    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_daemon_user_service_result_text(&output);
    }
    Ok(())
}

pub(super) struct UserServiceCommand {
    pub(super) program: String,
    pub(super) args: Vec<String>,
}

impl UserServiceCommand {
    pub(super) fn argv(&self) -> Vec<String> {
        std::iter::once(self.program.clone())
            .chain(self.args.iter().cloned())
            .collect()
    }

    pub(super) fn display(&self) -> String {
        self.argv().join(" ")
    }

    pub(super) fn is_host_wrapped(&self) -> bool {
        self.args.first().map(String::as_str) == Some("--host") && self.args.len() >= 2
    }

    pub(super) fn target_program(&self) -> &str {
        if self.is_host_wrapped() {
            &self.args[1]
        } else {
            &self.program
        }
    }

    pub(super) fn host_wrapper_program(&self) -> Option<&str> {
        self.is_host_wrapped().then_some(self.program.as_str())
    }
}

pub(super) fn daemon_user_service_command(
    action: &str,
    log_lines: Option<u16>,
) -> anyhow::Result<UserServiceCommand> {
    const SERVICE_NAME: &str = "vinput-daemon.service";
    if log_lines == Some(0) {
        anyhow::bail!("daemon log --lines must be greater than 0");
    }

    let systemctl =
        || std::env::var("VINPUT_DAEMON_SYSTEMCTL").unwrap_or_else(|_| "systemctl".to_owned());
    let owned = |values: &[&str]| values.iter().map(|value| (*value).to_owned()).collect();
    let (target_program, target_args) = match action {
        "stop" => (systemctl(), owned(&["--user", "stop", SERVICE_NAME])),
        "restart" => (systemctl(), owned(&["--user", "restart", SERVICE_NAME])),
        "disable-now" => (
            systemctl(),
            owned(&["--user", "disable", "--now", SERVICE_NAME]),
        ),
        "daemon-reload" => (systemctl(), owned(&["--user", "daemon-reload"])),
        "main-pid" => (
            systemctl(),
            owned(&[
                "--user",
                "show",
                "--property",
                "MainPID",
                "--value",
                SERVICE_NAME,
            ]),
        ),
        "log" => {
            let mut args = sandbox::daemon_log_args(SERVICE_NAME);
            if let Some(lines) = log_lines {
                args.extend(["-n".to_owned(), lines.to_string()]);
            }
            (
                std::env::var("VINPUT_DAEMON_JOURNALCTL")
                    .unwrap_or_else(|_| "journalctl".to_owned()),
                args,
            )
        }
        _ => anyhow::bail!("unsupported daemon user service action `{action}`"),
    };
    let (program, args) = sandbox::wrap_host_command(target_program, target_args);
    Ok(UserServiceCommand { program, args })
}

pub(super) fn daemon_user_service_dry_run_json(
    action: &str,
    command: &UserServiceCommand,
) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": true,
        "action": action,
        "will_mutate_user_service": false,
        "strategy": "systemd-user-service",
        "tool": daemon_user_service_tool_json(action, command),
        "sandbox": sandbox::sandbox_json(command.is_host_wrapped()),
        "host_wrapper": daemon_user_service_host_wrapper_json(command),
        "command": command.display(),
        "command_argv": command.argv(),
        "owner_probe": daemon_owner_probe_plan_json(),
        "fallback": daemon_user_service_fallback(),
        "fallback_steps": daemon_user_service_fallback_steps(),
        "next_steps": daemon_user_service_next_steps(action),
    })
}

fn daemon_user_service_tool_json(action: &str, command: &UserServiceCommand) -> serde_json::Value {
    let (name, env_override) = daemon_user_service_tool(action);
    serde_json::json!({
        "name": name,
        "program": command.target_program(),
        "env_override": env_override,
        "overridden": std::env::var_os(env_override).is_some(),
    })
}

fn daemon_user_service_host_wrapper_json(
    command: &UserServiceCommand,
) -> Option<serde_json::Value> {
    command
        .host_wrapper_program()
        .map(sandbox::host_wrapper_json)
}

fn print_daemon_user_service_tool_text(action: &str, command: &UserServiceCommand) {
    let (name, env_override) = daemon_user_service_tool(action);
    println!("tool: {name}");
    println!("tool_program: {}", command.target_program());
    println!("tool_env_override: {env_override}");
    println!(
        "tool_overridden: {}",
        std::env::var_os(env_override).is_some()
    );
}
fn print_daemon_user_service_sandbox_text(command: &UserServiceCommand) {
    println!(
        "sandbox: {}",
        if command.is_host_wrapped() {
            "flatpak"
        } else {
            "none"
        }
    );
    println!("host_command: {}", command.is_host_wrapped());
    if let Some(program) = command.host_wrapper_program() {
        println!("host_wrapper_program: {program}");
        println!("host_wrapper_env_override: {}", sandbox::FLATPAK_SPAWN_ENV);
        println!(
            "host_wrapper_overridden: {}",
            std::env::var_os(sandbox::FLATPAK_SPAWN_ENV).is_some()
        );
    }
}

fn daemon_user_service_tool(action: &str) -> (&'static str, &'static str) {
    match action {
        "log" => ("journalctl", "VINPUT_DAEMON_JOURNALCTL"),
        _ => ("systemctl", "VINPUT_DAEMON_SYSTEMCTL"),
    }
}

fn print_daemon_user_service_dry_run_text(action: &str, command: &UserServiceCommand) {
    println!("dry_run: true");
    println!("action: {action}");
    println!("will_mutate_user_service: false");
    println!("strategy: systemd-user-service");
    print_daemon_user_service_tool_text(action, command);
    print_daemon_user_service_sandbox_text(command);
    println!("command: {}", command.display());
    println!("owner_probe: GetNameOwner, GetConnectionUnixProcessID, procfs exe/cmdline");
    println!("fallback: {}", daemon_user_service_fallback());
    println!("fallback_step: {}", daemon_user_service_fallback_steps()[0]);
    println!("next_step: {}", daemon_user_service_next_steps(action)[0]);
}

pub(super) fn run_daemon_user_service_command(
    action: &str,
    command: &UserServiceCommand,
) -> serde_json::Value {
    let will_mutate_user_service = matches!(action, "stop" | "restart" | "daemon-reload");
    match ProcessCommand::new(&command.program)
        .args(&command.args)
        .output()
    {
        Ok(output) => {
            let exit_status = output.status.code();
            serde_json::json!({
                "ok": output.status.success(),
                "dry_run": false,
                "action": action,
                "will_mutate_user_service": will_mutate_user_service,
                "strategy": "systemd-user-service",
                "tool": daemon_user_service_tool_json(action, command),
                "sandbox": sandbox::sandbox_json(command.is_host_wrapped()),
                "host_wrapper": daemon_user_service_host_wrapper_json(command),
                "command": command.display(),
                "command_argv": command.argv(),
                "owner_probe": daemon_owner_probe_plan_json(),
                "exit_status": exit_status,
                "stdout": String::from_utf8_lossy(&output.stdout),
                "stderr": String::from_utf8_lossy(&output.stderr),
                "fallback": daemon_user_service_fallback(),
                "fallback_steps": daemon_user_service_fallback_steps(),
                "next_steps": daemon_user_service_next_steps(action),
            })
        }
        Err(error) => serde_json::json!({
            "ok": false,
            "dry_run": false,
            "action": action,
            "will_mutate_user_service": will_mutate_user_service,
            "strategy": "systemd-user-service",
            "tool": daemon_user_service_tool_json(action, command),
            "sandbox": sandbox::sandbox_json(command.is_host_wrapped()),
            "host_wrapper": daemon_user_service_host_wrapper_json(command),
            "command": command.display(),
            "command_argv": command.argv(),
            "owner_probe": daemon_owner_probe_plan_json(),
            "exit_status": null,
            "stdout": "",
            "stderr": "",
            "error": error.to_string(),
            "fallback": daemon_user_service_fallback(),
            "fallback_steps": daemon_user_service_fallback_steps(),
            "next_steps": daemon_user_service_next_steps(action),
        }),
    }
}

fn print_daemon_user_service_result_text(output: &serde_json::Value) {
    println!("dry_run: false");
    println!("action: {}", optional_json_str(&output["action"]));
    println!(
        "will_mutate_user_service: {}",
        output["will_mutate_user_service"]
            .as_bool()
            .unwrap_or(false)
    );
    println!("strategy: systemd-user-service");
    if output["tool"].is_object() {
        println!("tool: {}", optional_json_str(&output["tool"]["name"]));
        println!(
            "tool_program: {}",
            optional_json_str(&output["tool"]["program"])
        );
        println!(
            "tool_env_override: {}",
            optional_json_str(&output["tool"]["env_override"])
        );
        println!("tool_overridden: {}", output["tool"]["overridden"]);
    }
    println!("sandbox: {}", optional_json_str(&output["sandbox"]["kind"]));
    println!(
        "host_command: {}",
        output["sandbox"]["host_command"].as_bool().unwrap_or(false)
    );
    if output["host_wrapper"].is_object() {
        println!(
            "host_wrapper_program: {}",
            optional_json_str(&output["host_wrapper"]["program"])
        );
        println!(
            "host_wrapper_env_override: {}",
            optional_json_str(&output["host_wrapper"]["env_override"])
        );
        println!(
            "host_wrapper_overridden: {}",
            output["host_wrapper"]["overridden"]
        );
    }
    println!("command: {}", optional_json_str(&output["command"]));
    println!("ok: {}", output["ok"].as_bool().unwrap_or(false));
    match output["exit_status"].as_i64() {
        Some(status) => println!("exit_status: {status}"),
        None => println!("exit_status: -"),
    }
    if let Some(stdout) = output["stdout"].as_str().filter(|value| !value.is_empty()) {
        print!("stdout: {stdout}");
        if !stdout.ends_with('\n') {
            println!();
        }
    }
    if let Some(stderr) = output["stderr"].as_str().filter(|value| !value.is_empty()) {
        print!("stderr: {stderr}");
        if !stderr.ends_with('\n') {
            println!();
        }
    }
    if let Some(error) = output["error"].as_str().filter(|value| !value.is_empty()) {
        println!("error: {error}");
    }
    if output["ok"].as_bool() != Some(true) {
        println!("fallback: {}", daemon_user_service_fallback());
        if let Some(fallback_step) = output["fallback_steps"]
            .as_array()
            .and_then(|steps| steps.first())
            .and_then(serde_json::Value::as_str)
        {
            println!("fallback_step: {fallback_step}");
        }
    }
    if let Some(next_step) = output["next_steps"]
        .as_array()
        .and_then(|steps| steps.first())
        .and_then(serde_json::Value::as_str)
    {
        println!("next_step: {next_step}");
    }
}

fn daemon_user_service_next_steps(action: &str) -> Vec<&'static str> {
    match action {
        "log" => vec![
            "adjust --lines to inspect more or fewer journal entries",
            "run vinput daemon status to inspect live D-Bus/runtime state",
        ],
        "restart" => vec![
            "run vinput daemon status to verify the restarted daemon",
            "run vinput daemon log --lines 100 if restart failed",
        ],
        _ => vec![
            "run vinput daemon status to verify daemon availability",
            "run vinput daemon log --lines 100 if service control failed",
        ],
    }
}

fn daemon_user_service_fallback_steps() -> Vec<&'static str> {
    vec![
        "run vinput activation-service --user-status to inspect the per-user D-Bus activation service",
        "run vinput daemon start --dry-run --json to inspect activation strategy",
        "run vinput daemon status --dry-run --json to inspect daemon owner/procfs probes",
    ]
}

const fn daemon_user_service_fallback() -> &'static str {
    "inspect the per-user D-Bus activation service and daemon process manually"
}

pub(super) fn print_daemon_start(dry_run: bool, json_output: bool) -> anyhow::Result<()> {
    if dry_run {
        let output = serde_json::json!({
            "ok": true,
            "dry_run": true,
            "action": "start",
            "will_call_dbus": false,
            "activation": {
                "strategy": "dbus-service-activation",
                "trigger_method": dbus::method::GET_STATUS,
            },
            "dbus": {
                "service": dbus::SERVICE_BUS_NAME,
                "object_path": dbus::SERVICE_OBJECT_PATH,
                "interface": dbus::SERVICE_INTERFACE,
                "method": dbus::method::GET_STATUS,
            },
            "owner_probe": daemon_owner_probe_plan_json(),
            "next_steps": daemon_start_next_steps(),
        });
        if json_output {
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!("dry_run: true");
            println!("action: start");
            println!("will_call_dbus: false");
            println!("strategy: dbus-service-activation");
            println!("method: {}", dbus::method::GET_STATUS);
            println!("service: {}", dbus::SERVICE_BUS_NAME);
            println!("object_path: {}", dbus::SERVICE_OBJECT_PATH);
            println!("interface: {}", dbus::SERVICE_INTERFACE);
            println!("owner_probe: GetNameOwner, GetConnectionUnixProcessID, procfs exe/cmdline");
            println!("next_step: {}", daemon_start_next_steps()[0]);
        }
        return Ok(());
    }

    let status = daemon_status_via_dbus()?;
    let output = serde_json::json!({
        "ok": true,
        "dry_run": false,
        "action": "start",
        "will_call_dbus": true,
        "called": true,
        "activation": {
            "strategy": "dbus-service-activation",
            "trigger_method": dbus::method::GET_STATUS,
        },
        "daemon": status,
        "next_steps": daemon_start_next_steps(),
    });
    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("dry_run: false");
        println!("action: start");
        println!("will_call_dbus: true");
        println!("called: true");
        println!("status: {}", optional_json_str(&output["daemon"]["status"]));
    }
    Ok(())
}

fn daemon_start_next_steps() -> Vec<&'static str> {
    vec![
        "run vinput daemon status to inspect live D-Bus/runtime state",
        "run vinput daemon log --lines 100 if activation failed",
    ]
}

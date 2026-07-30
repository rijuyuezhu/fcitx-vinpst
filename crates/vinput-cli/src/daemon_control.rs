use std::{
    fs,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
};

use anyhow::Context;
use vinput_protocol::dbus;

use crate::DaemonCommand;

pub(crate) fn handle_daemon_command(command: &DaemonCommand) -> anyhow::Result<()> {
    match command {
        DaemonCommand::Start { dry_run, json } => print_daemon_start(*dry_run, *json),
        DaemonCommand::Status { dry_run, json } => print_daemon_status(*dry_run, *json),
        DaemonCommand::ReloadAsr { dry_run, json } => print_daemon_reload_asr_plan(*dry_run, *json),
        DaemonCommand::Stop { dry_run, json } => {
            print_daemon_user_service_plan("stop", None, *dry_run, *json)
        }
        DaemonCommand::Restart { dry_run, json } => {
            print_daemon_user_service_plan("restart", None, *dry_run, *json)
        }
        DaemonCommand::Log {
            lines,
            dry_run,
            json,
        } => print_daemon_user_service_plan("log", *lines, *dry_run, *json),
    }
}

fn print_daemon_user_service_plan(
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

struct UserServiceCommand {
    program: String,
    args: Vec<String>,
}

impl UserServiceCommand {
    fn argv(&self) -> Vec<String> {
        std::iter::once(self.program.clone())
            .chain(self.args.iter().cloned())
            .collect()
    }

    fn display(&self) -> String {
        self.argv().join(" ")
    }
}

fn daemon_user_service_command(
    action: &str,
    log_lines: Option<u16>,
) -> anyhow::Result<UserServiceCommand> {
    const SERVICE_NAME: &str = "vinput-daemon.service";
    if log_lines == Some(0) {
        anyhow::bail!("daemon log --lines must be greater than 0");
    }
    match action {
        "stop" => Ok(UserServiceCommand {
            program: std::env::var("VINPUT_DAEMON_SYSTEMCTL")
                .unwrap_or_else(|_| "systemctl".to_owned()),
            args: ["--user", "stop", SERVICE_NAME]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        }),
        "restart" => Ok(UserServiceCommand {
            program: std::env::var("VINPUT_DAEMON_SYSTEMCTL")
                .unwrap_or_else(|_| "systemctl".to_owned()),
            args: ["--user", "restart", SERVICE_NAME]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        }),
        "log" => {
            let mut args = ["--user", "-u", SERVICE_NAME]
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if let Some(lines) = log_lines {
                args.extend(["-n".to_owned(), lines.to_string()]);
            }
            Ok(UserServiceCommand {
                program: std::env::var("VINPUT_DAEMON_JOURNALCTL")
                    .unwrap_or_else(|_| "journalctl".to_owned()),
                args,
            })
        }
        _ => anyhow::bail!("unsupported daemon user service action `{action}`"),
    }
}

fn daemon_user_service_dry_run_json(
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
        "program": command.program,
        "env_override": env_override,
        "overridden": std::env::var_os(env_override).is_some(),
    })
}

fn print_daemon_user_service_tool_text(action: &str, command: &UserServiceCommand) {
    let (name, env_override) = daemon_user_service_tool(action);
    println!("tool: {name}");
    println!("tool_program: {}", command.program);
    println!("tool_env_override: {env_override}");
    println!(
        "tool_overridden: {}",
        std::env::var_os(env_override).is_some()
    );
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
    println!("command: {}", command.display());
    println!("owner_probe: GetNameOwner, GetConnectionUnixProcessID, procfs exe/cmdline");
    println!("fallback: {}", daemon_user_service_fallback());
    println!("fallback_step: {}", daemon_user_service_fallback_steps()[0]);
    println!("next_step: {}", daemon_user_service_next_steps(action)[0]);
}

fn run_daemon_user_service_command(
    action: &str,
    command: &UserServiceCommand,
) -> serde_json::Value {
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
                "will_mutate_user_service": action != "log",
                "strategy": "systemd-user-service",
                "tool": daemon_user_service_tool_json(action, command),
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
            "will_mutate_user_service": action != "log",
            "strategy": "systemd-user-service",
            "tool": daemon_user_service_tool_json(action, command),
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

fn print_daemon_start(dry_run: bool, json_output: bool) -> anyhow::Result<()> {
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

type DaemonAsrBackendStateTuple = (
    String,
    String,
    String,
    String,
    String,
    bool,
    bool,
    Vec<String>,
);

fn print_daemon_status(dry_run: bool, json_output: bool) -> anyhow::Result<()> {
    if dry_run {
        let output = daemon_status_dry_run_json();
        if json_output {
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            print_daemon_status_dry_run_text();
        }
        return Ok(());
    }

    let snapshot = daemon_status_via_dbus()?;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&snapshot)?);
    } else {
        print_daemon_status_text(&snapshot);
    }
    Ok(())
}

fn daemon_status_dry_run_json() -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": true,
        "will_call_dbus": false,
        "dbus": daemon_status_dbus_plan_json(),
        "reports": [
            "service_status",
            "bus_owner",
            "daemon_handoff",
            "asr_backend",
            "runtime_status",
            "text_adapters"
        ],
        "owner_probe": daemon_owner_probe_plan_json(),
        "next_steps": [
            "run vinput daemon status without --dry-run to query live daemon diagnostics",
            "run vinput adapter status to inspect text adapter PID/running state",
            "run vinput doctor to inspect local setup and activation readiness"
        ],
    })
}

fn daemon_status_dbus_plan_json() -> serde_json::Value {
    serde_json::json!({
        "service": dbus::SERVICE_BUS_NAME,
        "object_path": dbus::SERVICE_OBJECT_PATH,
        "interface": dbus::SERVICE_INTERFACE,
        "methods": [
            dbus::method::GET_STATUS,
            dbus::method::GET_ASR_BACKEND_STATE,
            dbus::method::GET_RUNTIME_STATUS,
        ],
    })
}

fn print_daemon_status_dry_run_text() {
    println!("dry_run: true");
    println!("will_call_dbus: false");
    println!("service: {}", dbus::SERVICE_BUS_NAME);
    println!("object_path: {}", dbus::SERVICE_OBJECT_PATH);
    println!("interface: {}", dbus::SERVICE_INTERFACE);
    println!(
        "methods: {}, {}, {}",
        dbus::method::GET_STATUS,
        dbus::method::GET_ASR_BACKEND_STATE,
        dbus::method::GET_RUNTIME_STATUS
    );
    println!(
        "reports: service_status, bus_owner, daemon_handoff, asr_backend, runtime_status, text_adapters"
    );
    println!("owner_probe: GetNameOwner, GetConnectionUnixProcessID, procfs exe/cmdline");
    println!("next_step: run vinput daemon status without --dry-run");
}

pub(crate) fn daemon_owner_probe_plan_json() -> serde_json::Value {
    serde_json::json!({
        "service": "org.freedesktop.DBus",
        "object_path": "/org/freedesktop/DBus",
        "interface": "org.freedesktop.DBus",
        "target_name": dbus::SERVICE_BUS_NAME,
        "methods": [
            "GetNameOwner",
            "GetConnectionUnixProcessID"
        ],
        "process_fields": [
            "unix_process_id",
            "exe",
            "cmdline"
        ],
        "stale_owner_hints": [
            "runtime-status-unavailable",
            "unexpected owner executable",
            "activation service points to an old daemon path",
            "owner executable inode was deleted during package replacement"
        ]
    })
}

fn daemon_status_via_dbus() -> anyhow::Result<serde_json::Value> {
    let connection = zbus::blocking::Connection::session().context("connect to session bus")?;
    let proxy = daemon_service_proxy(&connection)?;
    let status: String = proxy
        .call(dbus::method::GET_STATUS, &())
        .context("call GetStatus on daemon D-Bus service")?;
    // Creating a proxy does not necessarily activate a D-Bus service. Collect owner
    // diagnostics immediately after the first successful method call so an
    // activation-backed daemon is visible on the initial `daemon status` query.
    let owner = daemon_owner_diagnostics(&connection);
    let handoff = daemon_handoff_diagnostics(&owner);
    let asr: DaemonAsrBackendStateTuple = proxy
        .call(dbus::method::GET_ASR_BACKEND_STATE, &())
        .context("call GetAsrBackendState on daemon D-Bus service")?;
    let runtime_status_json: String = proxy
        .call(dbus::method::GET_RUNTIME_STATUS, &())
        .context("call GetRuntimeStatus on daemon D-Bus service")?;
    let runtime_status = serde_json::from_str::<serde_json::Value>(&runtime_status_json)
        .context("parse daemon runtime status JSON")?;
    Ok(serde_json::json!({
        "ok": true,
        "dry_run": false,
        "will_call_dbus": true,
        "dbus": daemon_status_dbus_plan_json(),
        "status": status,
        "asr_backend": {
            "target_provider_id": asr.0,
            "target_model_id": asr.1,
            "effective_provider_id": asr.2,
            "effective_model_id": asr.3,
            "last_error": asr.4,
            "reload_in_progress": asr.5,
            "has_effective_backend": asr.6,
            "remote_endpoints": asr.7,
        },
        "runtime_status": runtime_status,
        "owner": owner,
        "handoff": handoff,
    }))
}

fn daemon_owner_diagnostics(connection: &zbus::blocking::Connection) -> serde_json::Value {
    let mut output = serde_json::json!({
        "service": dbus::SERVICE_BUS_NAME,
        "unique_name": null,
        "unix_process_id": null,
        "process": null,
        "ok": false,
    });
    let bus_proxy = match zbus::blocking::Proxy::new(
        connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    ) {
        Ok(proxy) => proxy,
        Err(error) => {
            output["error"] = serde_json::json!(error.to_string());
            return output;
        }
    };
    let owner = match bus_proxy.call::<_, _, String>("GetNameOwner", &(dbus::SERVICE_BUS_NAME)) {
        Ok(owner) => owner,
        Err(error) => {
            output["error"] = serde_json::json!(error.to_string());
            return output;
        }
    };
    output["unique_name"] = serde_json::json!(owner);
    let Some(owner) = output["unique_name"].as_str() else {
        return output;
    };
    match bus_proxy.call::<_, _, u32>("GetConnectionUnixProcessID", &(owner)) {
        Ok(pid) => {
            output["unix_process_id"] = serde_json::json!(pid);
            output["process"] = daemon_owner_process_json(pid);
            output["ok"] = serde_json::json!(true);
        }
        Err(error) => {
            output["error"] = serde_json::json!(error.to_string());
        }
    }
    output
}

fn daemon_owner_process_json(pid: u32) -> serde_json::Value {
    let proc_root = PathBuf::from("/proc").join(pid.to_string());
    let exe = fs::read_link(proc_root.join("exe"))
        .ok()
        .map(|path| path.display().to_string());
    let cmdline = fs::read(proc_root.join("cmdline"))
        .ok()
        .map(|bytes| {
            bytes
                .split(|byte| *byte == 0)
                .filter(|part| !part.is_empty())
                .map(|part| String::from_utf8_lossy(part).into_owned())
                .collect::<Vec<_>>()
        })
        .filter(|parts| !parts.is_empty());
    serde_json::json!({
        "exe": exe,
        "cmdline": cmdline.unwrap_or_default(),
    })
}

const DELETED_EXECUTABLE_SUFFIX: &str = " (deleted)";

fn daemon_handoff_diagnostics(owner: &serde_json::Value) -> serde_json::Value {
    let expected = expected_sibling_daemon_path().filter(|path| path.exists());
    daemon_handoff_diagnostics_for_paths(owner["process"]["exe"].as_str(), expected.as_deref())
}

fn expected_sibling_daemon_path() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(|parent| parent.join("vinput-daemon"))
}

fn daemon_handoff_diagnostics_for_paths(
    owner_executable: Option<&str>,
    expected_executable: Option<&Path>,
) -> serde_json::Value {
    let owner_executable_deleted =
        owner_executable.is_some_and(|path| path.ends_with(DELETED_EXECUTABLE_SUFFIX));
    let normalized_owner_executable =
        owner_executable.map(|path| path.strip_suffix(DELETED_EXECUTABLE_SUFFIX).unwrap_or(path));
    let path_matches = normalized_owner_executable
        .zip(expected_executable)
        .map(|(owner, expected)| executable_paths_match(Path::new(owner), expected));
    let reason = if owner_executable_deleted {
        Some("owner-executable-deleted")
    } else if path_matches == Some(false) {
        Some("owner-executable-path-mismatch")
    } else {
        None
    };
    let restart_recommended = reason.is_some();
    serde_json::json!({
        "expected_executable": expected_executable,
        "owner_executable": owner_executable,
        "normalized_owner_executable": normalized_owner_executable,
        "owner_executable_deleted": owner_executable_deleted,
        "path_matches": path_matches,
        "restart_recommended": restart_recommended,
        "reason": reason,
        "automatic_restart_performed": false,
        "next_step": restart_recommended.then_some("run vinput daemon restart"),
    })
}

fn executable_paths_match(owner: &Path, expected: &Path) -> bool {
    match (fs::canonicalize(owner), fs::canonicalize(expected)) {
        (Ok(owner), Ok(expected)) => owner == expected,
        _ => owner == expected,
    }
}

fn print_daemon_status_text(snapshot: &serde_json::Value) {
    println!("status: {}", optional_json_str(&snapshot["status"]));
    if !snapshot["owner"].is_null() {
        println!(
            "owner_unique_name: {}",
            optional_json_str(&snapshot["owner"]["unique_name"])
        );
        match snapshot["owner"]["unix_process_id"].as_u64() {
            Some(pid) => println!("owner_pid: {pid}"),
            None => println!("owner_pid: -"),
        }
        println!(
            "owner_exe: {}",
            optional_json_str(&snapshot["owner"]["process"]["exe"])
        );
        println!(
            "owner_cmdline: {}",
            json_string_array_summary(&snapshot["owner"]["process"]["cmdline"])
        );
    }
    print_daemon_handoff_text(&snapshot["handoff"]);
    println!(
        "target_provider_id: {}",
        optional_json_str(&snapshot["asr_backend"]["target_provider_id"])
    );
    println!(
        "target_model_id: {}",
        optional_json_str(&snapshot["asr_backend"]["target_model_id"])
    );
    println!(
        "effective_provider_id: {}",
        optional_json_str(&snapshot["asr_backend"]["effective_provider_id"])
    );
    println!(
        "effective_model_id: {}",
        optional_json_str(&snapshot["asr_backend"]["effective_model_id"])
    );
    println!(
        "last_error: {}",
        optional_json_str(&snapshot["asr_backend"]["last_error"])
    );
    println!(
        "reload_in_progress: {}",
        snapshot["asr_backend"]["reload_in_progress"]
            .as_bool()
            .unwrap_or(false)
    );
    println!(
        "has_effective_backend: {}",
        snapshot["asr_backend"]["has_effective_backend"]
            .as_bool()
            .unwrap_or(false)
    );
    println!(
        "remote_endpoints: {}",
        json_string_array_summary(&snapshot["asr_backend"]["remote_endpoints"])
    );
    println!(
        "runtime_status: {}",
        optional_json_str(&snapshot["runtime_status"]["status"])
    );
    println!(
        "runtime_uptime_ms: {}",
        snapshot["runtime_status"]["uptime_ms"]
            .as_u64()
            .unwrap_or(0)
    );
    println!(
        "active_session: {}",
        snapshot["runtime_status"]["active_session"]
            .as_bool()
            .unwrap_or(false)
    );
    println!(
        "text_adapter_count: {}",
        snapshot["runtime_status"]["text_adapters"]["adapter_count"]
            .as_u64()
            .unwrap_or(0)
    );
}

fn print_daemon_handoff_text(handoff: &serde_json::Value) {
    println!(
        "handoff_expected_exe: {}",
        optional_json_str(&handoff["expected_executable"])
    );
    println!(
        "handoff_owner_exe_deleted: {}",
        handoff["owner_executable_deleted"]
            .as_bool()
            .unwrap_or(false)
    );
    match handoff["path_matches"].as_bool() {
        Some(matches) => println!("handoff_path_matches: {matches}"),
        None => println!("handoff_path_matches: -"),
    }
    println!(
        "handoff_restart_recommended: {}",
        handoff["restart_recommended"].as_bool().unwrap_or(false)
    );
    println!("handoff_reason: {}", optional_json_str(&handoff["reason"]));
    println!(
        "handoff_next_step: {}",
        optional_json_str(&handoff["next_step"])
    );
}

pub(crate) fn optional_json_str(value: &serde_json::Value) -> &str {
    value.as_str().unwrap_or("-")
}

fn empty_as_dash(value: &str) -> &str {
    if value.is_empty() { "-" } else { value }
}

fn json_string_array_summary(value: &serde_json::Value) -> String {
    let values = value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if values.is_empty() {
        "-".to_owned()
    } else {
        values.join(", ")
    }
}

pub(crate) fn daemon_service_proxy(
    connection: &zbus::blocking::Connection,
) -> anyhow::Result<zbus::blocking::Proxy<'_>> {
    zbus::blocking::Proxy::new(
        connection,
        dbus::SERVICE_BUS_NAME,
        dbus::SERVICE_OBJECT_PATH,
        dbus::SERVICE_INTERFACE,
    )
    .context("create daemon D-Bus proxy")
}

fn print_daemon_reload_asr_plan(dry_run: bool, json_output: bool) -> anyhow::Result<()> {
    let asr_state = if dry_run {
        None
    } else {
        Some(request_asr_reload_via_dbus()?)
    };
    let output = daemon_reload_asr_output(dry_run, asr_state.as_ref());
    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("dry_run: {dry_run}");
        println!("will_call_dbus: {}", !dry_run);
        println!("called: {}", !dry_run);
        println!("service: {}", dbus::SERVICE_BUS_NAME);
        println!("object_path: {}", dbus::SERVICE_OBJECT_PATH);
        println!("interface: {}", dbus::SERVICE_INTERFACE);
        println!("method: {}", dbus::method::RELOAD_ASR_BACKEND);
        if let Some(asr) = asr_state {
            println!("reload_in_progress: {}", asr.5);
            println!("target_provider_id: {}", empty_as_dash(&asr.0));
            println!("target_model_id: {}", empty_as_dash(&asr.1));
            println!("effective_provider_id: {}", empty_as_dash(&asr.2));
            println!("effective_model_id: {}", empty_as_dash(&asr.3));
            println!("last_error: {}", empty_as_dash(&asr.4));
        }
        println!("owner_probe: GetNameOwner, GetConnectionUnixProcessID, procfs exe/cmdline");
    }
    Ok(())
}

fn daemon_reload_asr_output(
    dry_run: bool,
    asr_state: Option<&DaemonAsrBackendStateTuple>,
) -> serde_json::Value {
    let asr_backend = asr_state.map(|asr| {
        serde_json::json!({
            "target_provider_id": asr.0,
            "target_model_id": asr.1,
            "effective_provider_id": asr.2,
            "effective_model_id": asr.3,
            "last_error": asr.4,
            "reload_in_progress": asr.5,
            "has_effective_backend": asr.6,
            "remote_endpoints": asr.7,
        })
    });
    serde_json::json!({
        "ok": true,
        "dry_run": dry_run,
        "will_call_dbus": !dry_run,
        "called": !dry_run,
        "asr_backend": asr_backend,
        "dbus": {
            "service": dbus::SERVICE_BUS_NAME,
            "object_path": dbus::SERVICE_OBJECT_PATH,
            "interface": dbus::SERVICE_INTERFACE,
            "method": dbus::method::RELOAD_ASR_BACKEND,
        },
        "owner_probe": daemon_owner_probe_plan_json(),
        "next_steps": [
            "run vinput daemon status to verify the selected ASR backend",
            "use vinput protocol to inspect the stable method contract"
        ],
    })
}

pub(crate) fn reload_asr_backend_via_dbus() -> anyhow::Result<()> {
    request_asr_reload_via_dbus().map(|_| ())
}

fn request_asr_reload_via_dbus() -> anyhow::Result<DaemonAsrBackendStateTuple> {
    let connection = zbus::blocking::Connection::session().context("connect to session bus")?;
    let proxy = daemon_service_proxy(&connection)?;
    let _: () = proxy
        .call(dbus::method::RELOAD_ASR_BACKEND, &())
        .context("call ReloadAsrBackend on daemon D-Bus service")?;
    proxy
        .call(dbus::method::GET_ASR_BACKEND_STATE, &())
        .context("call GetAsrBackendState after ReloadAsrBackend")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handoff_diagnostics_accept_matching_executable() {
        let directory = tempfile::tempdir().expect("create handoff fixture directory");
        let expected = directory.path().join("vinput-daemon");
        fs::write(&expected, b"fixture").expect("write expected daemon fixture");
        let output = daemon_handoff_diagnostics_for_paths(
            Some(expected.to_str().expect("UTF-8 fixture path")),
            Some(&expected),
        );

        assert_eq!(output["path_matches"], true);
        assert_eq!(output["owner_executable_deleted"], false);
        assert_eq!(output["restart_recommended"], false);
        assert!(output["reason"].is_null());
        assert!(output["next_step"].is_null());
    }

    #[test]
    fn handoff_diagnostics_detect_deleted_owner_inode() {
        let directory = tempfile::tempdir().expect("create handoff fixture directory");
        let expected = directory.path().join("vinput-daemon");
        fs::write(&expected, b"fixture").expect("write expected daemon fixture");
        let owner = format!("{}{}", expected.display(), DELETED_EXECUTABLE_SUFFIX);
        let output = daemon_handoff_diagnostics_for_paths(Some(&owner), Some(&expected));

        assert_eq!(output["path_matches"], true);
        assert_eq!(output["owner_executable_deleted"], true);
        assert_eq!(output["restart_recommended"], true);
        assert_eq!(output["reason"], "owner-executable-deleted");
        assert_eq!(output["next_step"], "run vinput daemon restart");
        assert_eq!(output["automatic_restart_performed"], false);
    }

    #[test]
    fn handoff_diagnostics_detect_executable_path_mismatch() {
        let directory = tempfile::tempdir().expect("create handoff fixture directory");
        let expected = directory.path().join("expected/vinput-daemon");
        let owner = directory.path().join("old/vinput-daemon");
        fs::create_dir_all(expected.parent().expect("expected parent"))
            .expect("create expected parent");
        fs::create_dir_all(owner.parent().expect("owner parent")).expect("create owner parent");
        fs::write(&expected, b"expected").expect("write expected daemon fixture");
        fs::write(&owner, b"owner").expect("write owner daemon fixture");
        let output = daemon_handoff_diagnostics_for_paths(
            Some(owner.to_str().expect("UTF-8 fixture path")),
            Some(&expected),
        );

        assert_eq!(output["path_matches"], false);
        assert_eq!(output["owner_executable_deleted"], false);
        assert_eq!(output["restart_recommended"], true);
        assert_eq!(output["reason"], "owner-executable-path-mismatch");
        assert_eq!(output["next_step"], "run vinput daemon restart");
    }
}

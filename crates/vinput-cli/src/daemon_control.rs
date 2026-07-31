use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
    thread,
    time::Duration,
};

use anyhow::Context;
use vinput_protocol::dbus;

use crate::DaemonCommand;

pub(crate) fn handle_daemon_command(command: &DaemonCommand) -> anyhow::Result<()> {
    match command {
        DaemonCommand::Start { dry_run, json } => print_daemon_start(*dry_run, *json),
        DaemonCommand::Status { dry_run, json } => print_daemon_status(*dry_run, *json),
        DaemonCommand::Handoff { dry_run, json } => print_daemon_handoff(*dry_run, *json),
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

const HANDOFF_VERIFY_ATTEMPTS: u32 = 100;
const HANDOFF_VERIFY_INTERVAL: Duration = Duration::from_millis(50);

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
        "daemon-reload" => Ok(UserServiceCommand {
            program: std::env::var("VINPUT_DAEMON_SYSTEMCTL")
                .unwrap_or_else(|_| "systemctl".to_owned()),
            args: ["--user", "daemon-reload"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        }),
        "main-pid" => Ok(UserServiceCommand {
            program: std::env::var("VINPUT_DAEMON_SYSTEMCTL")
                .unwrap_or_else(|_| "systemctl".to_owned()),
            args: [
                "--user",
                "show",
                "--property",
                "MainPID",
                "--value",
                SERVICE_NAME,
            ]
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

fn print_daemon_handoff(dry_run: bool, json_output: bool) -> anyhow::Result<()> {
    let command = daemon_user_service_command("restart", None)?;
    let output = if dry_run {
        daemon_handoff_dry_run_json(&command)
    } else {
        run_daemon_handoff(&command)?
    };
    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_daemon_handoff_result_text(&output);
    }
    Ok(())
}

fn daemon_handoff_dry_run_json(command: &UserServiceCommand) -> serde_json::Value {
    let main_pid_probe = daemon_user_service_command("main-pid", None)
        .expect("internal systemd MainPID command must be valid");
    let service_reload = daemon_user_service_command("daemon-reload", None)
        .expect("internal systemd daemon-reload command must be valid");
    let kill_program = std::env::var("VINPUT_DAEMON_KILL").unwrap_or_else(|_| "kill".to_owned());
    serde_json::json!({
        "ok": true,
        "dry_run": true,
        "action": "handoff",
        "will_call_dbus": false,
        "will_mutate_user_service": false,
        "will_signal_owner": false,
        "strategy": "systemd-reload-or-guarded-direct-owner-termination",
        "expected_executable": expected_sibling_daemon_path().filter(|path| path.exists()),
        "restart_condition": [
            "owner-executable-deleted",
            "owner-executable-path-mismatch"
        ],
        "systemd_control": {
            "owner_probe": daemon_user_service_dry_run_json("main-pid", &main_pid_probe),
            "reload": daemon_user_service_dry_run_json("daemon-reload", &service_reload),
            "restart": daemon_user_service_dry_run_json("restart", command),
        },
        "direct_control": {
            "program": kill_program,
            "signal": "TERM",
            "guards": [
                "owner-is-idle",
                "no-active-recording-session",
                "same-user-id",
                "vinput-daemon-identity",
                "not-systemd-managed"
            ],
            "reactivation": "reload D-Bus activation config and verify the new owner",
        },
        "service_control": daemon_user_service_dry_run_json("restart", command),
        "verification": {
            "required_after_restart": true,
            "attempts": HANDOFF_VERIFY_ATTEMPTS,
            "interval_ms": HANDOFF_VERIFY_INTERVAL.as_millis(),
            "requires_current_owner": true,
        },
        "next_steps": [
            "run vinput daemon handoff without --dry-run to inspect and conditionally restart",
            "run vinput daemon status to inspect the current owner without restarting"
        ],
    })
}

#[derive(Clone, Copy)]
enum HandoffMutationTarget {
    UserService,
    DirectOwner,
}

#[derive(Clone, Copy)]
enum HandoffOutcome {
    NotAttempted,
    FailedBeforeHandoff,
    Failed,
    Performed,
}

struct HandoffControl {
    strategy: &'static str,
    mutation_target: HandoffMutationTarget,
    outcome: HandoffOutcome,
    failure_status: &'static str,
    systemd_probe: serde_json::Value,
    direct_guard: serde_json::Value,
    direct_revalidation: serde_json::Value,
    dbus_reload: serde_json::Value,
    service_reload: serde_json::Value,
    service_control: serde_json::Value,
    direct_signal: serde_json::Value,
}

impl HandoffControl {
    const fn will_mutate_user_service(&self) -> bool {
        matches!(self.mutation_target, HandoffMutationTarget::UserService)
    }

    const fn will_signal_owner(&self) -> bool {
        matches!(self.mutation_target, HandoffMutationTarget::DirectOwner) && self.attempted()
    }

    const fn attempted(&self) -> bool {
        matches!(
            self.outcome,
            HandoffOutcome::Failed | HandoffOutcome::Performed
        )
    }

    const fn performed(&self) -> bool {
        matches!(self.outcome, HandoffOutcome::Performed)
    }

    const fn ok(&self) -> bool {
        self.performed()
    }
}

fn run_daemon_handoff(command: &UserServiceCommand) -> anyhow::Result<serde_json::Value> {
    let before = daemon_status_via_dbus()?;
    if !daemon_snapshot_requires_handoff(&before) {
        return Ok(daemon_handoff_not_needed_json(&before));
    }

    let control = execute_daemon_handoff(&before, command)?;
    if !control.ok() {
        return Ok(daemon_handoff_failure_json(&before, &control));
    }

    let verification = verify_daemon_handoff();
    let after = verification["snapshot"].clone();
    let verified = verification["ok"].as_bool() == Some(true);
    Ok(daemon_handoff_success_json(
        &before,
        &control,
        &verification,
        &after,
        verified,
    ))
}

fn daemon_handoff_not_needed_json(before: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": false,
        "action": "handoff",
        "will_call_dbus": true,
        "will_mutate_user_service": false,
        "will_signal_owner": false,
        "handoff_strategy": "not-needed",
        "restart_required": false,
        "restart_attempted": false,
        "restart_performed": false,
        "before": before,
        "systemd_probe": null,
        "direct_guard": null,
        "direct_revalidation": null,
        "dbus_reload": null,
        "service_reload": null,
        "service_control": null,
        "direct_signal": null,
        "verification": {
            "ok": true,
            "attempts": 0,
            "status": "not-needed",
            "last_error": null,
        },
        "after": before,
        "next_steps": [
            "no handoff was needed because the running daemon owner is current",
            "run vinput daemon status to inspect live D-Bus/runtime state"
        ],
    })
}

fn daemon_handoff_failure_json(
    before: &serde_json::Value,
    control: &HandoffControl,
) -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "dry_run": false,
        "action": "handoff",
        "will_call_dbus": true,
        "will_mutate_user_service": control.will_mutate_user_service(),
        "will_signal_owner": control.will_signal_owner(),
        "handoff_strategy": control.strategy,
        "restart_required": true,
        "restart_attempted": control.attempted(),
        "restart_performed": control.performed(),
        "before": before,
        "systemd_probe": control.systemd_probe,
        "direct_guard": control.direct_guard,
        "direct_revalidation": control.direct_revalidation,
        "dbus_reload": control.dbus_reload,
        "service_reload": control.service_reload,
        "service_control": control.service_control,
        "direct_signal": control.direct_signal,
        "verification": {
            "ok": false,
            "attempts": 0,
            "status": control.failure_status,
            "last_error": null,
        },
        "after": null,
        "next_steps": [
            "inspect the handoff guard/control result and run vinput daemon log --lines 100",
            "run vinput activation-service --user-status"
        ],
    })
}

fn daemon_handoff_success_json(
    before: &serde_json::Value,
    control: &HandoffControl,
    verification: &serde_json::Value,
    after: &serde_json::Value,
    verified: bool,
) -> serde_json::Value {
    serde_json::json!({
        "ok": verified,
        "dry_run": false,
        "action": "handoff",
        "will_call_dbus": true,
        "will_mutate_user_service": control.will_mutate_user_service(),
        "will_signal_owner": control.will_signal_owner(),
        "handoff_strategy": control.strategy,
        "restart_required": true,
        "restart_attempted": control.attempted(),
        "restart_performed": control.performed(),
        "before": before,
        "systemd_probe": control.systemd_probe,
        "direct_guard": control.direct_guard,
        "direct_revalidation": control.direct_revalidation,
        "dbus_reload": control.dbus_reload,
        "service_reload": control.service_reload,
        "service_control": control.service_control,
        "direct_signal": control.direct_signal,
        "verification": verification,
        "after": after,
        "next_steps": if verified {
            vec![
                "the restarted daemon owner matches the current installation",
                "run vinput daemon status to inspect live D-Bus/runtime state"
            ]
        } else {
            vec![
                "run vinput daemon status and vinput daemon log --lines 100",
                "inspect the user activation service for an old daemon path"
            ]
        },
    })
}

fn execute_daemon_handoff(
    before: &serde_json::Value,
    restart_command: &UserServiceCommand,
) -> anyhow::Result<HandoffControl> {
    let owner_pid = daemon_snapshot_owner_pid(before);
    let systemd_probe = daemon_systemd_owner_probe(owner_pid)?;
    if systemd_probe["owner_matches_main_pid"].as_bool() == Some(true) {
        execute_systemd_daemon_handoff(systemd_probe, restart_command)
    } else {
        execute_direct_daemon_handoff(before, owner_pid, systemd_probe)
    }
}

fn execute_systemd_daemon_handoff(
    systemd_probe: serde_json::Value,
    restart_command: &UserServiceCommand,
) -> anyhow::Result<HandoffControl> {
    let reload_command = daemon_user_service_command("daemon-reload", None)?;
    let service_reload = run_daemon_user_service_command("daemon-reload", &reload_command);
    if service_reload["ok"].as_bool() != Some(true) {
        return Ok(HandoffControl {
            strategy: "systemd-daemon-reload-and-restart",
            mutation_target: HandoffMutationTarget::UserService,
            outcome: HandoffOutcome::FailedBeforeHandoff,
            failure_status: "daemon-reload-failed",
            systemd_probe,
            direct_guard: serde_json::Value::Null,
            direct_revalidation: serde_json::Value::Null,
            dbus_reload: serde_json::Value::Null,
            service_reload,
            service_control: serde_json::Value::Null,
            direct_signal: serde_json::Value::Null,
        });
    }
    let service_control = run_daemon_user_service_command("restart", restart_command);
    let ok = service_control["ok"].as_bool() == Some(true);
    Ok(HandoffControl {
        strategy: "systemd-daemon-reload-and-restart",
        mutation_target: HandoffMutationTarget::UserService,
        outcome: if ok {
            HandoffOutcome::Performed
        } else {
            HandoffOutcome::Failed
        },
        failure_status: "restart-failed",
        systemd_probe,
        direct_guard: serde_json::Value::Null,
        direct_revalidation: serde_json::Value::Null,
        dbus_reload: serde_json::Value::Null,
        service_reload,
        service_control,
        direct_signal: serde_json::Value::Null,
    })
}

fn execute_direct_daemon_handoff(
    before: &serde_json::Value,
    owner_pid: Option<u32>,
    systemd_probe: serde_json::Value,
) -> anyhow::Result<HandoffControl> {
    let direct_guard = direct_owner_handoff_guard(before, &systemd_probe);
    if direct_guard["approved"].as_bool() != Some(true) {
        return Ok(HandoffControl {
            strategy: "direct-owner-terminate-and-reactivate",
            mutation_target: HandoffMutationTarget::DirectOwner,
            outcome: HandoffOutcome::NotAttempted,
            failure_status: "direct-owner-guard-rejected",
            systemd_probe,
            direct_guard,
            direct_revalidation: serde_json::Value::Null,
            dbus_reload: serde_json::Value::Null,
            service_reload: serde_json::Value::Null,
            service_control: serde_json::Value::Null,
            direct_signal: serde_json::Value::Null,
        });
    }

    let dbus_reload = reload_dbus_activation_config();
    if dbus_reload["ok"].as_bool() != Some(true) {
        return Ok(HandoffControl {
            strategy: "direct-owner-terminate-and-reactivate",
            mutation_target: HandoffMutationTarget::DirectOwner,
            outcome: HandoffOutcome::FailedBeforeHandoff,
            failure_status: "dbus-reload-failed",
            systemd_probe,
            direct_guard,
            direct_revalidation: serde_json::Value::Null,
            dbus_reload,
            service_reload: serde_json::Value::Null,
            service_control: serde_json::Value::Null,
            direct_signal: serde_json::Value::Null,
        });
    }

    let direct_revalidation = revalidate_direct_owner_identity(before);
    if direct_revalidation["ok"].as_bool() != Some(true) {
        return Ok(HandoffControl {
            strategy: "direct-owner-terminate-and-reactivate",
            mutation_target: HandoffMutationTarget::DirectOwner,
            outcome: HandoffOutcome::FailedBeforeHandoff,
            failure_status: "direct-owner-identity-changed",
            systemd_probe,
            direct_guard,
            direct_revalidation,
            dbus_reload,
            service_reload: serde_json::Value::Null,
            service_control: serde_json::Value::Null,
            direct_signal: serde_json::Value::Null,
        });
    }

    let owner_pid = owner_pid.context("missing direct daemon owner PID")?;
    let direct_signal = signal_direct_daemon_owner(owner_pid);
    let ok = direct_signal["ok"].as_bool() == Some(true);
    Ok(HandoffControl {
        strategy: "direct-owner-terminate-and-reactivate",
        mutation_target: HandoffMutationTarget::DirectOwner,
        outcome: if ok {
            HandoffOutcome::Performed
        } else {
            HandoffOutcome::Failed
        },
        failure_status: "direct-owner-signal-failed",
        systemd_probe,
        direct_guard,
        direct_revalidation,
        dbus_reload,
        service_reload: serde_json::Value::Null,
        service_control: serde_json::Value::Null,
        direct_signal,
    })
}

fn daemon_snapshot_owner_pid(snapshot: &serde_json::Value) -> Option<u32> {
    snapshot["owner"]["unix_process_id"]
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())
}

fn daemon_systemd_owner_probe(owner_pid: Option<u32>) -> anyhow::Result<serde_json::Value> {
    let command = daemon_user_service_command("main-pid", None)?;
    let command_result = run_daemon_user_service_command("main-pid", &command);
    let main_pid = command_result["stdout"]
        .as_str()
        .map(str::trim)
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|pid| *pid != 0);
    Ok(serde_json::json!({
        "ok": command_result["ok"],
        "owner_pid": owner_pid,
        "main_pid": main_pid,
        "owner_matches_main_pid": owner_pid.zip(main_pid).is_some_and(|(owner, main)| owner == main),
        "command_result": command_result,
    }))
}

fn direct_owner_handoff_guard(
    snapshot: &serde_json::Value,
    systemd_probe: &serde_json::Value,
) -> serde_json::Value {
    let owner_pid = daemon_snapshot_owner_pid(snapshot);
    let process_uid = snapshot["owner"]["process"]["uid"].as_u64();
    let current_uid = fs::metadata("/proc/self")
        .ok()
        .map(|metadata| u64::from(metadata.uid()));
    let executable = snapshot["owner"]["process"]["exe"]
        .as_str()
        .map(|path| path.strip_suffix(DELETED_EXECUTABLE_SUFFIX).unwrap_or(path));
    let executable_name_matches = executable
        .and_then(|path| Path::new(path).file_name())
        .is_some_and(|name| name == "vinput-daemon");
    let command_name_matches = snapshot["owner"]["process"]["cmdline"]
        .as_array()
        .and_then(|values| values.first())
        .and_then(serde_json::Value::as_str)
        .and_then(|path| Path::new(path).file_name())
        .is_some_and(|name| name == "vinput-daemon");
    let systemd_unit_detected = snapshot["owner"]["process"]["cgroup"]
        .as_str()
        .is_some_and(|cgroup| cgroup.contains("vinput-daemon.service"));
    let start_time_present = snapshot["owner"]["process"]["start_time_ticks"]
        .as_u64()
        .is_some();
    let status_idle = snapshot["status"].as_str() == Some("idle");
    let active_session = snapshot["runtime_status"]["active_session"].as_bool();
    let same_uid = process_uid
        .zip(current_uid)
        .is_some_and(|(owner, current)| owner == current);
    let safe_pid = owner_pid.is_some_and(|pid| pid > 1 && pid != std::process::id());
    let systemd_probe_matches = systemd_probe["owner_matches_main_pid"].as_bool() == Some(true);
    let approved = status_idle
        && active_session == Some(false)
        && same_uid
        && safe_pid
        && start_time_present
        && (executable_name_matches || command_name_matches)
        && !systemd_unit_detected
        && !systemd_probe_matches;
    serde_json::json!({
        "approved": approved,
        "owner_pid": owner_pid,
        "owner_uid": process_uid,
        "current_uid": current_uid,
        "same_uid": same_uid,
        "safe_pid": safe_pid,
        "start_time_present": start_time_present,
        "status_idle": status_idle,
        "active_session": active_session,
        "executable_name_matches": executable_name_matches,
        "command_name_matches": command_name_matches,
        "systemd_unit_detected": systemd_unit_detected,
        "systemd_probe_matches": systemd_probe_matches,
    })
}

fn revalidate_direct_owner_identity(snapshot: &serde_json::Value) -> serde_json::Value {
    let Some(pid) = daemon_snapshot_owner_pid(snapshot) else {
        return serde_json::json!({"ok": false, "reason": "missing-owner-pid"});
    };
    let expected = &snapshot["owner"]["process"];
    let current = daemon_owner_process_json(pid);
    let uid_matches = expected["uid"]
        .as_u64()
        .zip(current["uid"].as_u64())
        .is_some_and(|(expected_uid, current_uid)| expected_uid == current_uid);
    let start_time_matches = expected["start_time_ticks"]
        .as_u64()
        .zip(current["start_time_ticks"].as_u64())
        .is_some_and(|(expected_ticks, current_ticks)| expected_ticks == current_ticks);
    let executable_matches = expected["exe"]
        .as_str()
        .map(normalize_deleted_executable_path)
        .zip(
            current["exe"]
                .as_str()
                .map(normalize_deleted_executable_path),
        )
        .is_some_and(|(expected_path, current_path)| expected_path == current_path);
    serde_json::json!({
        "ok": uid_matches && start_time_matches && executable_matches,
        "pid": pid,
        "uid_matches": uid_matches,
        "start_time_matches": start_time_matches,
        "executable_matches": executable_matches,
        "expected": expected,
        "current": current,
    })
}

fn normalize_deleted_executable_path(path: &str) -> &str {
    path.strip_suffix(DELETED_EXECUTABLE_SUFFIX).unwrap_or(path)
}

fn reload_dbus_activation_config() -> serde_json::Value {
    let result = (|| -> anyhow::Result<()> {
        let connection = zbus::blocking::Connection::session().context("connect to session bus")?;
        let proxy = zbus::blocking::Proxy::new(
            &connection,
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
        )
        .context("create D-Bus daemon proxy")?;
        let _: () = proxy
            .call("ReloadConfig", &())
            .context("reload D-Bus activation config")?;
        Ok(())
    })();
    match result {
        Ok(()) => serde_json::json!({"ok": true}),
        Err(error) => serde_json::json!({"ok": false, "error": error.to_string()}),
    }
}

fn signal_direct_daemon_owner(pid: u32) -> serde_json::Value {
    let command = UserServiceCommand {
        program: std::env::var("VINPUT_DAEMON_KILL").unwrap_or_else(|_| "kill".to_owned()),
        args: vec!["-TERM".to_owned(), pid.to_string()],
    };
    match ProcessCommand::new(&command.program)
        .args(&command.args)
        .output()
    {
        Ok(output) => serde_json::json!({
            "ok": output.status.success(),
            "pid": pid,
            "signal": "TERM",
            "command": command.display(),
            "command_argv": command.argv(),
            "exit_status": output.status.code(),
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
        }),
        Err(error) => serde_json::json!({
            "ok": false,
            "pid": pid,
            "signal": "TERM",
            "command": command.display(),
            "command_argv": command.argv(),
            "exit_status": null,
            "stdout": "",
            "stderr": "",
            "error": error.to_string(),
        }),
    }
}

fn daemon_snapshot_requires_handoff(snapshot: &serde_json::Value) -> bool {
    snapshot["handoff"]["restart_recommended"].as_bool() == Some(true)
}

fn verify_daemon_handoff() -> serde_json::Value {
    let mut last_snapshot = None;
    let mut last_error = None;
    for attempt in 1..=HANDOFF_VERIFY_ATTEMPTS {
        match daemon_status_via_dbus() {
            Ok(snapshot) if daemon_snapshot_is_current(&snapshot) => {
                return serde_json::json!({
                    "ok": true,
                    "attempts": attempt,
                    "status": "current-owner",
                    "last_error": null,
                    "snapshot": snapshot,
                });
            }
            Ok(snapshot) => {
                last_snapshot = Some(snapshot);
                last_error = None;
            }
            Err(error) => {
                last_error = Some(error.to_string());
            }
        }
        if attempt < HANDOFF_VERIFY_ATTEMPTS {
            thread::sleep(HANDOFF_VERIFY_INTERVAL);
        }
    }
    serde_json::json!({
        "ok": false,
        "attempts": HANDOFF_VERIFY_ATTEMPTS,
        "status": "stale-or-unavailable",
        "last_error": last_error,
        "snapshot": last_snapshot,
    })
}

fn daemon_snapshot_is_current(snapshot: &serde_json::Value) -> bool {
    snapshot["handoff"]["path_matches"].as_bool() == Some(true)
        && snapshot["handoff"]["owner_executable_deleted"].as_bool() == Some(false)
        && !daemon_snapshot_requires_handoff(snapshot)
}

fn print_daemon_handoff_result_text(output: &serde_json::Value) {
    println!("dry_run: {}", output["dry_run"].as_bool().unwrap_or(false));
    println!("action: handoff");
    println!("ok: {}", output["ok"].as_bool().unwrap_or(false));
    println!(
        "will_mutate_user_service: {}",
        output["will_mutate_user_service"]
            .as_bool()
            .unwrap_or(false)
    );
    println!(
        "will_signal_owner: {}",
        output["will_signal_owner"].as_bool().unwrap_or(false)
    );
    if output["dry_run"].as_bool() == Some(true) {
        println!("strategy: {}", optional_json_str(&output["strategy"]));
        println!(
            "command: {}",
            optional_json_str(&output["service_control"]["command"])
        );
        println!("next_step: {}", first_json_string(&output["next_steps"]));
        return;
    }
    for field in ["restart_required", "restart_attempted", "restart_performed"] {
        println!("{field}: {}", output[field].as_bool().unwrap_or(false));
    }
    println!(
        "handoff_strategy: {}",
        optional_json_str(&output["handoff_strategy"])
    );
    println!(
        "before_reason: {}",
        optional_json_str(&output["before"]["handoff"]["reason"])
    );
    println!(
        "verification_status: {}",
        optional_json_str(&output["verification"]["status"])
    );
    println!(
        "verification_attempts: {}",
        output["verification"]["attempts"].as_u64().unwrap_or(0)
    );
    println!(
        "after_owner_exe: {}",
        optional_json_str(&output["after"]["owner"]["process"]["exe"])
    );
    println!("next_step: {}", first_json_string(&output["next_steps"]));
}

fn first_json_string(value: &serde_json::Value) -> &str {
    value
        .as_array()
        .and_then(|values| values.first())
        .and_then(serde_json::Value::as_str)
        .unwrap_or("-")
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
    let uid = fs::metadata(&proc_root).ok().map(|metadata| metadata.uid());
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
    let cgroup = fs::read_to_string(proc_root.join("cgroup")).ok();
    let start_time_ticks = proc_start_time_ticks(&proc_root);
    serde_json::json!({
        "exe": exe,
        "cmdline": cmdline.unwrap_or_default(),
        "uid": uid,
        "cgroup": cgroup,
        "start_time_ticks": start_time_ticks,
    })
}

fn proc_start_time_ticks(proc_root: &Path) -> Option<u64> {
    let stat = fs::read_to_string(proc_root.join("stat")).ok()?;
    let (_, fields_after_name) = stat.rsplit_once(") ")?;
    fields_after_name
        .split_whitespace()
        .nth(19)
        .and_then(|value| value.parse().ok())
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
        "next_step": restart_recommended.then_some("run vinput daemon handoff"),
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
        assert_eq!(output["next_step"], "run vinput daemon handoff");
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
        assert_eq!(output["next_step"], "run vinput daemon handoff");
    }

    fn direct_guard_snapshot() -> serde_json::Value {
        let uid = fs::metadata("/proc/self")
            .expect("stat current process")
            .uid();
        serde_json::json!({
            "status": "idle",
            "runtime_status": {"active_session": false},
            "owner": {
                "unix_process_id": std::process::id().saturating_add(100),
                "process": {
                    "exe": "/tmp/old/vinput-daemon",
                    "cmdline": ["/tmp/old/vinput-daemon", "--dbus"],
                    "uid": uid,
                    "cgroup": "0::/user.slice/user-1000.slice/app.slice/dbus.service",
                    "start_time_ticks": 12345,
                }
            }
        })
    }

    #[test]
    fn direct_handoff_guard_accepts_idle_same_user_daemon() {
        let snapshot = direct_guard_snapshot();
        let systemd_probe = serde_json::json!({"owner_matches_main_pid": false});

        let guard = direct_owner_handoff_guard(&snapshot, &systemd_probe);

        assert_eq!(guard["approved"], true);
        assert_eq!(guard["same_uid"], true);
        assert_eq!(guard["status_idle"], true);
        assert_eq!(guard["active_session"], false);
        assert_eq!(guard["systemd_unit_detected"], false);
    }

    #[test]
    fn direct_handoff_guard_rejects_active_or_systemd_owned_daemon() {
        let systemd_probe = serde_json::json!({"owner_matches_main_pid": false});
        let mut active = direct_guard_snapshot();
        active["runtime_status"]["active_session"] = serde_json::json!(true);
        assert_eq!(
            direct_owner_handoff_guard(&active, &systemd_probe)["approved"],
            false
        );

        let mut systemd_owned = direct_guard_snapshot();
        systemd_owned["owner"]["process"]["cgroup"] = serde_json::json!(
            "0::/user.slice/user-1000.slice/user@1000.service/app.slice/vinput-daemon.service"
        );
        assert_eq!(
            direct_owner_handoff_guard(&systemd_owned, &systemd_probe)["approved"],
            false
        );
    }

    #[test]
    fn direct_owner_revalidation_detects_pid_identity_changes() {
        let pid = std::process::id();
        let process = daemon_owner_process_json(pid);
        let mut snapshot = serde_json::json!({
            "owner": {
                "unix_process_id": pid,
                "process": process,
            }
        });

        assert_eq!(revalidate_direct_owner_identity(&snapshot)["ok"], true);
        let start_time = snapshot["owner"]["process"]["start_time_ticks"]
            .as_u64()
            .expect("current process start time");
        snapshot["owner"]["process"]["start_time_ticks"] =
            serde_json::json!(start_time.saturating_add(1));
        let changed = revalidate_direct_owner_identity(&snapshot);
        assert_eq!(changed["ok"], false);
        assert_eq!(changed["start_time_matches"], false);
    }
}

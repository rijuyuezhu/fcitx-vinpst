use super::{
    Context, HANDOFF_VERIFY_ATTEMPTS, HANDOFF_VERIFY_INTERVAL, MetadataExt, Path, ProcessCommand,
    fs, optional_json_str, thread,
};
use super::{
    service::{
        UserServiceCommand, daemon_user_service_command, daemon_user_service_dry_run_json,
        run_daemon_user_service_command,
    },
    status::{
        DELETED_EXECUTABLE_SUFFIX, daemon_owner_process_json, daemon_status_via_dbus,
        expected_sibling_daemon_path,
    },
};

use crate::sandbox;

pub(super) fn print_daemon_handoff(dry_run: bool, json_output: bool) -> anyhow::Result<()> {
    let command = daemon_user_service_command("restart", None, false)?;
    let output = if dry_run {
        daemon_handoff_dry_run_json(&command)
    } else {
        run_daemon_handoff(&command)?
    };
    let ok = output["ok"].as_bool() == Some(true);
    if json_output {
        vinpst_terminal::print_json(&output)?;
    } else {
        print_daemon_handoff_result_text(&output);
    }
    if !dry_run && !ok {
        anyhow::bail!("daemon upgrade handoff did not complete safely");
    }
    Ok(())
}

fn daemon_handoff_dry_run_json(command: &UserServiceCommand) -> serde_json::Value {
    let main_pid_probe = daemon_user_service_command("main-pid", None, false)
        .expect("internal systemd MainPID command must be valid");
    let service_reload = daemon_user_service_command("daemon-reload", None, false)
        .expect("internal systemd daemon-reload command must be valid");
    let direct_signal = direct_daemon_signal_command("<owner-pid>");
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
            "guards": [
                "owner-matches-unit-main-pid",
                "owner-is-idle",
                "no-active-recording-session"
            ],
            "reload": daemon_user_service_dry_run_json("daemon-reload", &service_reload),
            "restart": daemon_user_service_dry_run_json("restart", command),
        },
        "direct_control": {
            "program": direct_signal.target_program(),
            "command": direct_signal.display(),
            "command_argv": direct_signal.argv(),
            "sandbox": sandbox::sandbox_json(direct_signal.is_host_wrapped()),
            "host_wrapper": direct_signal.host_wrapper_program().map(sandbox::host_wrapper_json),
            "signal": "TERM",
            "guards": [
                "owner-is-idle",
                "no-active-recording-session",
                "same-user-id",
                "vinpst-daemon-identity",
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
            "run vinpst daemon handoff without --dry-run to inspect and conditionally restart",
            "run vinpst daemon status to inspect the current owner without restarting"
        ],
    })
}

#[derive(Clone, Copy)]
enum HandoffMutationTarget {
    None,
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
    systemd_guard: serde_json::Value,
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
        "systemd_guard": null,
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
            "run vinpst daemon status to inspect live D-Bus/runtime state"
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
        "systemd_guard": control.systemd_guard,
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
            "inspect the handoff guard/control result and run vinpst daemon log --lines 100",
            "run vinpst activation-service --user-status"
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
        "systemd_guard": control.systemd_guard,
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
                "run vinpst daemon status to inspect live D-Bus/runtime state"
            ]
        } else {
            vec![
                "run vinpst daemon status and vinpst daemon log --lines 100",
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
        let systemd_guard = systemd_owner_handoff_guard(before, &systemd_probe);
        if systemd_guard["approved"].as_bool() != Some(true) {
            return Ok(HandoffControl {
                strategy: "systemd-daemon-reload-and-restart",
                mutation_target: HandoffMutationTarget::None,
                outcome: HandoffOutcome::NotAttempted,
                failure_status: "systemd-owner-session-guard-rejected",
                systemd_probe,
                systemd_guard,
                direct_guard: serde_json::Value::Null,
                direct_revalidation: serde_json::Value::Null,
                dbus_reload: serde_json::Value::Null,
                service_reload: serde_json::Value::Null,
                service_control: serde_json::Value::Null,
                direct_signal: serde_json::Value::Null,
            });
        }
        execute_systemd_daemon_handoff(systemd_probe, systemd_guard, restart_command)
    } else {
        execute_direct_daemon_handoff(before, owner_pid, systemd_probe)
    }
}

fn execute_systemd_daemon_handoff(
    systemd_probe: serde_json::Value,
    systemd_guard: serde_json::Value,
    restart_command: &UserServiceCommand,
) -> anyhow::Result<HandoffControl> {
    let reload_command = daemon_user_service_command("daemon-reload", None, false)?;
    let service_reload = run_daemon_user_service_command("daemon-reload", &reload_command);
    if service_reload["ok"].as_bool() != Some(true) {
        return Ok(HandoffControl {
            strategy: "systemd-daemon-reload-and-restart",
            mutation_target: HandoffMutationTarget::UserService,
            outcome: HandoffOutcome::FailedBeforeHandoff,
            failure_status: "daemon-reload-failed",
            systemd_probe,
            systemd_guard: systemd_guard.clone(),
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
        systemd_guard,
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
            systemd_guard: serde_json::Value::Null,
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
            systemd_guard: serde_json::Value::Null,
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
            systemd_guard: serde_json::Value::Null,
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
        systemd_guard: serde_json::Value::Null,
        direct_guard,
        direct_revalidation,
        dbus_reload,
        service_reload: serde_json::Value::Null,
        service_control: serde_json::Value::Null,
        direct_signal,
    })
}

pub(super) fn daemon_snapshot_owner_pid(snapshot: &serde_json::Value) -> Option<u32> {
    snapshot["owner"]["unix_process_id"]
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())
}

pub(super) fn daemon_systemd_owner_probe(
    owner_pid: Option<u32>,
) -> anyhow::Result<serde_json::Value> {
    let command = daemon_user_service_command("main-pid", None, false)?;
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

fn systemd_owner_handoff_guard(
    snapshot: &serde_json::Value,
    systemd_probe: &serde_json::Value,
) -> serde_json::Value {
    let status_idle = snapshot["status"].as_str() == Some("idle");
    let active_session = snapshot["runtime_status"]["active_session"].as_bool();
    let systemd_probe_matches = systemd_probe["owner_matches_main_pid"].as_bool() == Some(true);
    serde_json::json!({
        "approved": systemd_probe_matches && status_idle && active_session == Some(false),
        "status_idle": status_idle,
        "active_session": active_session,
        "systemd_probe_matches": systemd_probe_matches,
    })
}

pub(super) fn direct_owner_handoff_guard(
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
        .is_some_and(|name| name == "vinpst-daemon");
    let command_name_matches = snapshot["owner"]["process"]["cmdline"]
        .as_array()
        .and_then(|values| values.first())
        .and_then(serde_json::Value::as_str)
        .and_then(|path| Path::new(path).file_name())
        .is_some_and(|name| name == "vinpst-daemon");
    let systemd_unit_detected = snapshot["owner"]["process"]["cgroup"]
        .as_str()
        .is_some_and(|cgroup| cgroup.contains("vinpst-daemon.service"));
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

pub(super) fn revalidate_direct_owner_identity(snapshot: &serde_json::Value) -> serde_json::Value {
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

pub(super) fn reload_dbus_activation_config() -> serde_json::Value {
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

fn direct_daemon_signal_command(pid: &str) -> UserServiceCommand {
    let target_program = std::env::var("VINPST_DAEMON_KILL").unwrap_or_else(|_| "kill".to_owned());
    let target_args = vec!["-TERM".to_owned(), pid.to_owned()];
    let (program, args) = sandbox::wrap_host_command(target_program, target_args);
    UserServiceCommand { program, args }
}

pub(super) fn signal_direct_daemon_owner(pid: u32) -> serde_json::Value {
    let command = direct_daemon_signal_command(&pid.to_string());
    match ProcessCommand::new(&command.program)
        .args(&command.args)
        .output()
    {
        Ok(output) => serde_json::json!({
            "ok": output.status.success(),
            "pid": pid,
            "signal": "TERM",
            "tool_program": command.target_program(),
            "sandbox": sandbox::sandbox_json(command.is_host_wrapped()),
            "host_wrapper": command.host_wrapper_program().map(sandbox::host_wrapper_json),
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
            "tool_program": command.target_program(),
            "sandbox": sandbox::sandbox_json(command.is_host_wrapped()),
            "host_wrapper": command.host_wrapper_program().map(sandbox::host_wrapper_json),
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

pub(super) fn print_daemon_handoff_result_text(output: &serde_json::Value) {
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

pub(super) fn first_json_string(value: &serde_json::Value) -> &str {
    value
        .as_array()
        .and_then(|values| values.first())
        .and_then(serde_json::Value::as_str)
        .unwrap_or("-")
}

#[cfg(test)]
mod tests {
    use super::systemd_owner_handoff_guard;

    fn snapshot(status: &str, active_session: bool) -> serde_json::Value {
        serde_json::json!({
            "status": status,
            "runtime_status": {"active_session": active_session}
        })
    }

    #[test]
    fn systemd_handoff_guard_requires_idle_inactive_matching_owner() {
        let matching_probe = serde_json::json!({"owner_matches_main_pid": true});
        let mismatched_probe = serde_json::json!({"owner_matches_main_pid": false});

        assert_eq!(
            systemd_owner_handoff_guard(&snapshot("idle", false), &matching_probe)["approved"],
            true
        );
        assert_eq!(
            systemd_owner_handoff_guard(&snapshot("recording", true), &matching_probe)["approved"],
            false
        );
        assert_eq!(
            systemd_owner_handoff_guard(&snapshot("idle", true), &matching_probe)["approved"],
            false
        );
        assert_eq!(
            systemd_owner_handoff_guard(&snapshot("idle", false), &mismatched_probe)["approved"],
            false
        );
    }
}

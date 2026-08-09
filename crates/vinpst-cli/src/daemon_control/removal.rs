use super::{
    Context, HANDOFF_VERIFY_ATTEMPTS, HANDOFF_VERIFY_INTERVAL, daemon_name_has_owner,
    optional_json_str, thread,
};
use super::{
    handoff::{
        daemon_snapshot_owner_pid, daemon_systemd_owner_probe, direct_owner_handoff_guard,
        first_json_string, reload_dbus_activation_config, revalidate_direct_owner_identity,
        signal_direct_daemon_owner,
    },
    service::{
        UserServiceCommand, daemon_user_service_command, daemon_user_service_dry_run_json,
        run_daemon_user_service_command,
    },
    status::daemon_status_via_dbus,
};

pub(super) fn print_daemon_prepare_remove(
    dry_run: bool,
    preflight: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    let disable_command = daemon_user_service_command("disable-now", None, false)?;
    let output = if dry_run {
        serde_json::json!({
            "ok": true,
            "dry_run": true,
            "preflight": false,
            "action": "prepare-remove",
            "will_call_dbus": false,
            "will_mutate_user_service": false,
            "will_signal_owner": false,
            "activation_metadata_prerequisite": "remove the D-Bus activation service before invoking this command",
            "service_disable": daemon_user_service_dry_run_json("disable-now", &disable_command),
            "direct_owner_guard": {
                "requires_idle": true,
                "requires_inactive_session": true,
                "requires_same_uid": true,
                "requires_exact_pid_and_start_time": true,
                "requires_vinpst_daemon_identity": true,
            },
            "verification": {
                "method": "org.freedesktop.DBus.NameHasOwner",
                "requires_owner_absent": true,
                "attempts": HANDOFF_VERIFY_ATTEMPTS,
                "interval_ms": HANDOFF_VERIFY_INTERVAL.as_millis(),
            },
            "next_steps": [
                "remove the D-Bus activation service from the active search path",
                "run vinpst daemon prepare-remove --preflight in each live user session",
                "run vinpst daemon prepare-remove only after every preflight succeeds"
            ],
        })
    } else {
        run_daemon_prepare_remove(&disable_command, preflight)?
    };
    let ok = output["ok"].as_bool() == Some(true);

    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("dry_run: {}", output["dry_run"].as_bool().unwrap_or(false));
        println!(
            "preflight: {}",
            output["preflight"].as_bool().unwrap_or(false)
        );
        println!("action: prepare-remove");
        println!("ok: {}", output["ok"].as_bool().unwrap_or(false));
        println!(
            "strategy: {}",
            optional_json_str(&output["removal_strategy"])
        );
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
        println!(
            "verification_status: {}",
            optional_json_str(&output["verification"]["status"])
        );
        println!("next_step: {}", first_json_string(&output["next_steps"]));
    }
    if !dry_run && !ok {
        anyhow::bail!("daemon removal handoff did not complete safely");
    }
    Ok(())
}

fn run_daemon_prepare_remove(
    disable_command: &UserServiceCommand,
    preflight: bool,
) -> anyhow::Result<serde_json::Value> {
    let connection = zbus::blocking::Connection::session().context("connect to session bus")?;
    let owner_present = daemon_name_has_owner(&connection)?;
    let dbus_reload = reload_dbus_activation_config();
    if dbus_reload["ok"].as_bool() != Some(true) {
        return Ok(removal_reload_failure(
            owner_present,
            &dbus_reload,
            preflight,
        ));
    }

    if !owner_present {
        let service_disable = if preflight {
            serde_json::Value::Null
        } else {
            run_daemon_user_service_command("disable-now", disable_command)
        };
        let verification = if preflight || service_disable["ok"].as_bool() == Some(true) {
            verify_daemon_owner_absent()
        } else {
            removal_verification_failure("disable-failed")
        };
        let ok = verification["ok"].as_bool() == Some(true)
            && (preflight || service_disable["ok"].as_bool() == Some(true));
        return Ok(serde_json::json!({
            "ok": ok,
            "dry_run": false,
            "preflight": preflight,
            "action": "prepare-remove",
            "will_call_dbus": true,
            "will_mutate_user_service": !preflight
                && service_disable["ok"].as_bool() == Some(true),
            "will_signal_owner": false,
            "removal_strategy": "no-owner",
            "before": null,
            "systemd_probe": null,
            "session_guard": null,
            "service_disable": service_disable,
            "dbus_reload": dbus_reload,
            "direct_guard": null,
            "direct_revalidation": null,
            "direct_signal": null,
            "verification": verification,
            "next_steps": removal_next_steps(ok, true, preflight),
        }));
    }

    let before = daemon_status_via_dbus()?;
    let owner_pid = daemon_snapshot_owner_pid(&before);
    let systemd_probe = daemon_systemd_owner_probe(owner_pid)?;
    if systemd_probe["owner_matches_main_pid"].as_bool() == Some(true) {
        return Ok(prepare_remove_systemd_owner(
            &before,
            &systemd_probe,
            &dbus_reload,
            disable_command,
            preflight,
        ));
    }
    prepare_remove_direct_owner(
        &before,
        owner_pid,
        &systemd_probe,
        &dbus_reload,
        disable_command,
        preflight,
    )
}

fn prepare_remove_systemd_owner(
    before: &serde_json::Value,
    systemd_probe: &serde_json::Value,
    dbus_reload: &serde_json::Value,
    disable_command: &UserServiceCommand,
    preflight: bool,
) -> serde_json::Value {
    let session_guard = removal_session_guard(before);
    if session_guard["approved"].as_bool() != Some(true) {
        return systemd_removal_output(
            false,
            preflight,
            before,
            systemd_probe,
            dbus_reload,
            &session_guard,
            &serde_json::Value::Null,
            &removal_verification_failure("active-session-guard-rejected"),
        );
    }
    if preflight {
        return systemd_removal_output(
            true,
            true,
            before,
            systemd_probe,
            dbus_reload,
            &session_guard,
            &serde_json::Value::Null,
            &removal_preflight_approved(),
        );
    }

    let service_disable = run_daemon_user_service_command("disable-now", disable_command);
    let verification = if service_disable["ok"].as_bool() == Some(true) {
        verify_daemon_owner_absent()
    } else {
        removal_verification_failure("disable-failed")
    };
    let ok =
        service_disable["ok"].as_bool() == Some(true) && verification["ok"].as_bool() == Some(true);
    systemd_removal_output(
        ok,
        false,
        before,
        systemd_probe,
        dbus_reload,
        &session_guard,
        &service_disable,
        &verification,
    )
}

#[allow(clippy::too_many_arguments)]
fn systemd_removal_output(
    ok: bool,
    preflight: bool,
    before: &serde_json::Value,
    systemd_probe: &serde_json::Value,
    dbus_reload: &serde_json::Value,
    session_guard: &serde_json::Value,
    service_disable: &serde_json::Value,
    verification: &serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "ok": ok,
        "dry_run": false,
        "preflight": preflight,
        "action": "prepare-remove",
        "will_call_dbus": true,
        "will_mutate_user_service": !preflight
            && service_disable["ok"].as_bool() == Some(true),
        "will_signal_owner": false,
        "removal_strategy": "systemd-disable-and-stop",
        "before": before,
        "systemd_probe": systemd_probe,
        "session_guard": session_guard,
        "service_disable": service_disable,
        "dbus_reload": dbus_reload,
        "direct_guard": null,
        "direct_revalidation": null,
        "direct_signal": null,
        "verification": verification,
        "next_steps": removal_next_steps(ok, true, preflight),
    })
}

fn prepare_remove_direct_owner(
    before: &serde_json::Value,
    owner_pid: Option<u32>,
    systemd_probe: &serde_json::Value,
    dbus_reload: &serde_json::Value,
    disable_command: &UserServiceCommand,
    preflight: bool,
) -> anyhow::Result<serde_json::Value> {
    let direct_guard = direct_owner_handoff_guard(before, systemd_probe);
    if direct_guard["approved"].as_bool() != Some(true) {
        return Ok(direct_removal_output(
            false,
            preflight,
            before,
            systemd_probe,
            &serde_json::Value::Null,
            dbus_reload,
            &direct_guard,
            &serde_json::Value::Null,
            &serde_json::Value::Null,
            &removal_verification_failure("direct-owner-guard-rejected"),
        ));
    }
    if preflight {
        return Ok(direct_removal_output(
            true,
            true,
            before,
            systemd_probe,
            &serde_json::Value::Null,
            dbus_reload,
            &direct_guard,
            &serde_json::Value::Null,
            &serde_json::Value::Null,
            &removal_preflight_approved(),
        ));
    }

    let service_disable = if systemd_probe["ok"].as_bool() == Some(true) {
        run_daemon_user_service_command("disable-now", disable_command)
    } else {
        serde_json::Value::Null
    };
    if !service_disable.is_null() && service_disable["ok"].as_bool() != Some(true) {
        return Ok(direct_removal_output(
            false,
            false,
            before,
            systemd_probe,
            &service_disable,
            dbus_reload,
            &direct_guard,
            &serde_json::Value::Null,
            &serde_json::Value::Null,
            &removal_verification_failure("disable-failed"),
        ));
    }
    let direct_revalidation = revalidate_direct_owner_identity(before);
    if direct_revalidation["ok"].as_bool() != Some(true) {
        return Ok(direct_removal_output(
            false,
            false,
            before,
            systemd_probe,
            &service_disable,
            dbus_reload,
            &direct_guard,
            &direct_revalidation,
            &serde_json::Value::Null,
            &removal_verification_failure("direct-owner-identity-changed"),
        ));
    }

    let owner_pid = owner_pid.context("missing direct daemon owner PID")?;
    let direct_signal = signal_direct_daemon_owner(owner_pid);
    let verification = if direct_signal["ok"].as_bool() == Some(true) {
        verify_daemon_owner_absent()
    } else {
        removal_verification_failure("signal-failed")
    };
    let service_disable_ok =
        service_disable.is_null() || service_disable["ok"].as_bool() == Some(true);
    let ok = service_disable_ok
        && direct_signal["ok"].as_bool() == Some(true)
        && verification["ok"].as_bool() == Some(true);
    Ok(direct_removal_output(
        ok,
        false,
        before,
        systemd_probe,
        &service_disable,
        dbus_reload,
        &direct_guard,
        &direct_revalidation,
        &direct_signal,
        &verification,
    ))
}

#[allow(clippy::too_many_arguments)]
fn direct_removal_output(
    ok: bool,
    preflight: bool,
    before: &serde_json::Value,
    systemd_probe: &serde_json::Value,
    service_disable: &serde_json::Value,
    dbus_reload: &serde_json::Value,
    direct_guard: &serde_json::Value,
    direct_revalidation: &serde_json::Value,
    direct_signal: &serde_json::Value,
    verification: &serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "ok": ok,
        "dry_run": false,
        "preflight": preflight,
        "action": "prepare-remove",
        "will_call_dbus": true,
        "will_mutate_user_service": !preflight
            && service_disable["ok"].as_bool() == Some(true),
        "will_signal_owner": !preflight && direct_signal["ok"].as_bool() == Some(true),
        "removal_strategy": "direct-owner-terminate",
        "before": before,
        "systemd_probe": systemd_probe,
        "session_guard": null,
        "service_disable": service_disable,
        "dbus_reload": dbus_reload,
        "direct_guard": direct_guard,
        "direct_revalidation": direct_revalidation,
        "direct_signal": direct_signal,
        "verification": verification,
        "next_steps": removal_next_steps(ok, false, preflight),
    })
}

fn removal_reload_failure(
    owner_present: bool,
    dbus_reload: &serde_json::Value,
    preflight: bool,
) -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "dry_run": false,
        "preflight": preflight,
        "action": "prepare-remove",
        "will_call_dbus": true,
        "will_mutate_user_service": false,
        "will_signal_owner": false,
        "removal_strategy": "activation-reload-failed",
        "owner_present": owner_present,
        "before": null,
        "systemd_probe": null,
        "session_guard": null,
        "service_disable": null,
        "dbus_reload": dbus_reload,
        "direct_guard": null,
        "direct_revalidation": null,
        "direct_signal": null,
        "verification": removal_verification_failure("dbus-reload-failed"),
        "next_steps": [
            "verify that the D-Bus activation service was removed from the active search path",
            "retry vinpst daemon prepare-remove before continuing package removal",
        ],
    })
}
pub(super) fn removal_session_guard(snapshot: &serde_json::Value) -> serde_json::Value {
    let status_idle = snapshot["status"].as_str() == Some("idle");
    let active_session = snapshot["runtime_status"]["active_session"].as_bool();
    serde_json::json!({
        "approved": status_idle && active_session == Some(false),
        "status_idle": status_idle,
        "active_session": active_session,
    })
}

fn removal_preflight_approved() -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "attempts": 1,
        "status": "preflight-approved",
        "last_error": null,
    })
}

fn removal_verification_failure(status: &str) -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "attempts": 0,
        "status": status,
        "last_error": null,
    })
}

fn removal_next_steps(ok: bool, systemd: bool, preflight: bool) -> Vec<&'static str> {
    match (ok, systemd, preflight) {
        (true, _, true) => vec![
            "complete preflight for every live user session before mutating any owner",
            "run vinpst daemon prepare-remove after all preflights succeed",
        ],
        (false, _, true) => vec![
            "leave every daemon owner untouched and abort package removal",
            "resolve the reported session guard before retrying preflight",
        ],
        (true, true, false) => vec![
            "continue package removal; the user service is disabled and stopped",
            "reload Fcitx after package removal",
        ],
        (true, false, false) => vec![
            "continue package removal; the direct daemon owner is gone",
            "reload Fcitx after package removal",
        ],
        (false, true, false) => vec![
            "inspect the user systemd service before continuing removal",
            "run systemctl --user status vinpst-daemon.service",
        ],
        (false, false, false) => vec![
            "inspect the direct-owner guard before continuing package removal",
            "verify that the D-Bus activation service was removed before retrying",
        ],
    }
}

fn verify_daemon_owner_absent() -> serde_json::Value {
    let mut last_error = None;
    for attempt in 1..=HANDOFF_VERIFY_ATTEMPTS {
        let result = zbus::blocking::Connection::session()
            .context("connect to session bus")
            .and_then(|connection| daemon_name_has_owner(&connection));
        match result {
            Ok(false) => {
                return serde_json::json!({
                    "ok": true,
                    "attempts": attempt,
                    "status": "owner-absent",
                    "last_error": null,
                });
            }
            Ok(true) => last_error = None,
            Err(error) => last_error = Some(error.to_string()),
        }
        if attempt < HANDOFF_VERIFY_ATTEMPTS {
            thread::sleep(HANDOFF_VERIFY_INTERVAL);
        }
    }
    serde_json::json!({
        "ok": false,
        "attempts": HANDOFF_VERIFY_ATTEMPTS,
        "status": "owner-still-present",
        "last_error": last_error,
    })
}

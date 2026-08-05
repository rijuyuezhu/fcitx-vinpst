use std::{fs, os::unix::fs::MetadataExt};

use super::{
    handoff::{direct_owner_handoff_guard, revalidate_direct_owner_identity},
    removal::removal_session_guard,
    status::{
        DELETED_EXECUTABLE_SUFFIX, daemon_handoff_diagnostics_for_paths, daemon_owner_process_json,
    },
};

#[test]
fn handoff_diagnostics_accept_matching_executable() {
    let directory = tempfile::tempdir().expect("create handoff fixture directory");
    let expected = directory.path().join("vinpst-daemon");
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
    let expected = directory.path().join("vinpst-daemon");
    fs::write(&expected, b"fixture").expect("write expected daemon fixture");
    let owner = format!("{}{}", expected.display(), DELETED_EXECUTABLE_SUFFIX);
    let output = daemon_handoff_diagnostics_for_paths(Some(&owner), Some(&expected));

    assert_eq!(output["path_matches"], true);
    assert_eq!(output["owner_executable_deleted"], true);
    assert_eq!(output["restart_recommended"], true);
    assert_eq!(output["reason"], "owner-executable-deleted");
    assert_eq!(output["next_step"], "run vinpst daemon handoff");
    assert_eq!(output["automatic_restart_performed"], false);
}

#[test]
fn handoff_diagnostics_detect_executable_path_mismatch() {
    let directory = tempfile::tempdir().expect("create handoff fixture directory");
    let expected = directory.path().join("expected/vinpst-daemon");
    let owner = directory.path().join("old/vinpst-daemon");
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
    assert_eq!(output["next_step"], "run vinpst daemon handoff");
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
                "exe": "/tmp/old/vinpst-daemon",
                "cmdline": ["/tmp/old/vinpst-daemon", "--dbus"],
                "uid": uid,
                "cgroup": "0::/user.slice/user-1000.slice/app.slice/dbus.service",
                "start_time_ticks": 12345,
            }
        }
    })
}

#[test]
fn removal_session_guard_requires_idle_without_active_session() {
    let mut snapshot = direct_guard_snapshot();
    assert_eq!(removal_session_guard(&snapshot)["approved"], true);

    snapshot["status"] = serde_json::json!("recording");
    snapshot["runtime_status"]["active_session"] = serde_json::json!(true);
    let guard = removal_session_guard(&snapshot);
    assert_eq!(guard["approved"], false);
    assert_eq!(guard["status_idle"], false);
    assert_eq!(guard["active_session"], true);
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
        "0::/user.slice/user-1000.slice/user@1000.service/app.slice/vinpst-daemon.service"
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

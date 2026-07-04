//! Integration tests for recording control CLI dry-run paths.

mod common;

use common::{assert_json_success, assert_stdout_success, vinput_command};

fn assert_daemon_owner_probe_plan(value: &serde_json::Value) {
    assert_eq!(value["owner_probe"]["target_name"], "org.fcitx.Vinput");
    let owner_methods = value["owner_probe"]["methods"]
        .as_array()
        .expect("owner probe methods");
    assert!(owner_methods.contains(&serde_json::json!("GetNameOwner")));
    assert!(owner_methods.contains(&serde_json::json!("GetConnectionUnixProcessID")));
    let process_fields = value["owner_probe"]["process_fields"]
        .as_array()
        .expect("owner probe process fields");
    for field in ["unix_process_id", "exe", "cmdline"] {
        assert!(
            process_fields.contains(&serde_json::json!(field)),
            "missing owner probe process field {field}"
        );
    }
}

#[test]
fn recording_status_dry_run_json_reports_get_status_plan() {
    let output = vinput_command()
        .args(["recording", "status", "--dry-run", "--json"])
        .output()
        .expect("run vinput recording status dry-run json");

    let value = assert_json_success(output, "recording status dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["action"], "status");
    assert_eq!(value["will_call_dbus"], false);
    assert_eq!(value["called"], false);
    assert_eq!(value["dbus"]["method"], "GetStatus");
    assert_daemon_owner_probe_plan(&value);
}

#[test]
fn recording_status_dry_run_text_reports_expected_fields() {
    let output = vinput_command()
        .args(["recording", "status", "--dry-run"])
        .output()
        .expect("run vinput recording status dry-run text");

    let stdout = assert_stdout_success(output, "recording status dry-run text");
    assert!(stdout.contains("dry_run: true"));
    assert!(stdout.contains("action: status"));
    assert!(stdout.contains("will_call_dbus: false"));
    assert!(stdout.contains("called: false"));
    assert!(stdout.contains("method: GetStatus"));
    assert!(
        stdout
            .contains("owner_probe: GetNameOwner, GetConnectionUnixProcessID, procfs exe/cmdline")
    );
}

#[test]
fn recording_status_help_lists_dry_run_and_json() {
    let output = vinput_command()
        .args(["recording", "status", "--help"])
        .output()
        .expect("run vinput recording status --help");

    let stdout = assert_stdout_success(output, "recording status help");
    assert!(stdout.contains("--dry-run"));
    assert!(stdout.contains("--json"));
}

#[test]
fn global_json_flag_forces_recording_status_json() {
    let output = vinput_command()
        .args(["-j", "recording", "status", "--dry-run"])
        .output()
        .expect("run vinput -j recording status --dry-run");

    let value = assert_json_success(output, "global json recording status");
    assert_eq!(value["ok"], true);
    assert_eq!(value["action"], "status");
    assert_eq!(value["dbus"]["method"], "GetStatus");
    assert_daemon_owner_probe_plan(&value);
}

#[test]
fn global_json_flag_is_accepted_after_subcommands() {
    let output = vinput_command()
        .args(["recording", "status", "--dry-run", "-j"])
        .output()
        .expect("run vinput recording status --dry-run -j");

    let value = assert_json_success(output, "post-subcommand global json recording status");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["dbus"]["method"], "GetStatus");
}

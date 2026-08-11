#![allow(missing_docs)]

mod common;

use common::vinpst_command;

#[test]
fn no_arguments_prints_help_and_succeeds() {
    let output = vinpst_command()
        .output()
        .expect("run vinpst without arguments");
    assert!(
        output.status.success(),
        "status: {:?}",
        output.status.code()
    );
    assert!(output.stderr.is_empty());
    assert!(!output.stdout.is_empty());
}

#[test]
fn frozen_version_short_flag_succeeds() {
    let output = vinpst_command().arg("-v").output().expect("run vinpst -v");
    assert!(
        output.status.success(),
        "status: {:?}",
        output.status.code()
    );
    assert!(output.stderr.is_empty());
    assert!(!output.stdout.is_empty());
}

#[test]
fn previous_clap_version_short_flag_remains_accepted() {
    let output = vinpst_command().arg("-V").output().expect("run vinpst -V");
    assert!(
        output.status.success(),
        "status: {:?}",
        output.status.code()
    );
    assert!(output.stderr.is_empty());
    assert!(!output.stdout.is_empty());
}

#[test]
fn runtime_errors_follow_json_formatter_shape_for_local_json_flag() {
    let output = vinpst_command()
        .args(["daemon", "log", "--lines", "0", "--json"])
        .output()
        .expect("run failing daemon log with local JSON flag");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error: serde_json::Value =
        serde_json::from_slice(&output.stderr).expect("runtime stderr should be JSON");
    assert_eq!(error["status"], "error");
    assert_eq!(
        error["message"],
        "daemon log --lines must be greater than 0"
    );
}

#[test]
fn runtime_errors_follow_json_formatter_shape_for_root_json_flag() {
    let output = vinpst_command()
        .args(["--json", "status", "not-a-status"])
        .output()
        .expect("run failing hidden status command with root JSON flag");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error: serde_json::Value =
        serde_json::from_slice(&output.stderr).expect("runtime stderr should be JSON");
    assert_eq!(error["status"], "error");
    assert!(
        error["message"]
            .as_str()
            .is_some_and(|message| message.contains("parse status `not-a-status`"))
    );
}

#[test]
fn runtime_errors_remain_plain_text_without_json_flag() {
    let output = vinpst_command()
        .args(["daemon", "log", "--lines", "0"])
        .output()
        .expect("run failing daemon log without JSON flag");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("runtime stderr should be UTF-8");
    assert!(stderr.starts_with("Error: daemon log --lines must be greater than 0"));
}

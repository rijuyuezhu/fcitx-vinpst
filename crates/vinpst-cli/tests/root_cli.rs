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

//! Integration tests for CLI help output.

mod common;

use common::{assert_stdout_success, vinput_command};

#[test]
fn help_lists_diagnostic_commands() {
    let output = vinput_command()
        .arg("--help")
        .output()
        .expect("run vinput --help");

    let stdout = assert_stdout_success(output, "help output");
    assert!(stdout.contains("asr-state"));
    assert!(stdout.contains("audio-devices"));
    assert!(stdout.contains("doctor"));
    assert!(stdout.contains("protocol"));
    assert!(stdout.contains("registry"));
    assert!(stdout.contains("model"));
    assert!(stdout.contains("activation-service"));
}

#[test]
fn audio_devices_help_lists_config_option() {
    let output = vinput_command()
        .args(["audio-devices", "--help"])
        .output()
        .expect("run vinput audio-devices --help");

    let stdout = assert_stdout_success(output, "help output");
    assert!(stdout.contains("--config"));
    assert!(stdout.contains("capture-device diagnostics"));
}

#[test]
fn asr_state_help_lists_config_option() {
    let output = vinput_command()
        .args(["asr-state", "--help"])
        .output()
        .expect("run vinput asr-state --help");

    let stdout = assert_stdout_success(output, "help output");
    assert!(stdout.contains("--config"));
    assert!(stdout.contains("diagnostics from config"));
}

#[test]
fn activation_service_help_lists_daemon_options() {
    let output = vinput_command()
        .args(["activation-service", "--help"])
        .output()
        .expect("run vinput activation-service --help");

    let stdout = assert_stdout_success(output, "help output");
    assert!(stdout.contains("--daemon"));
    assert!(stdout.contains("--configured-backends"));
    assert!(stdout.contains("--audio-backend"));
    assert!(stdout.contains("--output"));
    assert!(stdout.contains("--user"));
    assert!(stdout.contains("--remove-user"));
    assert!(stdout.contains("--user-status"));
}

#[test]
fn doctor_help_lists_config_option() {
    let output = vinput_command()
        .args(["doctor", "--help"])
        .output()
        .expect("run vinput doctor --help");

    let stdout = assert_stdout_success(output, "help output");
    assert!(stdout.contains("--config"));
    assert!(stdout.contains("combined local diagnostics"));
}

#[test]
fn model_list_help_lists_registry_options() {
    let output = vinput_command()
        .args(["model", "list", "--help"])
        .output()
        .expect("run vinput model list --help");

    let stdout = assert_stdout_success(output, "help output");
    assert!(stdout.contains("--available"));
    assert!(stdout.contains("--registry"));
    assert!(stdout.contains("--i18n"));
    assert!(stdout.contains("--config"));
    assert!(stdout.contains("--json"));
}

#[test]
fn model_info_help_lists_registry_options() {
    let output = vinput_command()
        .args(["model", "info", "--help"])
        .output()
        .expect("run vinput model info --help");

    let stdout = assert_stdout_success(output, "help output");
    assert!(stdout.contains("<ID>"));
    assert!(stdout.contains("--registry"));
    assert!(stdout.contains("--i18n"));
    assert!(stdout.contains("--config"));
    assert!(stdout.contains("--json"));
}

#[test]
fn model_install_help_lists_dry_run_and_path_options() {
    let output = vinput_command()
        .args(["model", "install", "--help"])
        .output()
        .expect("run vinput model install --help");

    let stdout = assert_stdout_success(output, "help output");
    assert!(stdout.contains("<ID>"));
    assert!(stdout.contains("--registry"));
    assert!(stdout.contains("--model-root"));
    assert!(stdout.contains("--staging-root"));
    assert!(stdout.contains("--dry-run"));
    assert!(stdout.contains("--json"));
}

#[test]
fn model_legacy_aliases_accept_help() {
    let list_output = vinput_command()
        .args(["model", "ls", "--help"])
        .output()
        .expect("run vinput model ls --help");
    let list_stdout = assert_stdout_success(list_output, "model ls help output");
    assert!(list_stdout.contains("--available"));
    assert!(list_stdout.contains("--registry"));

    let add_output = vinput_command()
        .args(["model", "add", "--help"])
        .output()
        .expect("run vinput model add --help");
    let add_stdout = assert_stdout_success(add_output, "model add help output");
    assert!(add_stdout.contains("<ID>"));
    assert!(add_stdout.contains("--dry-run"));
    assert!(add_stdout.contains("--model-root"));
}

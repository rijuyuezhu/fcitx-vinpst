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
    assert!(stdout.contains("init"));
    assert!(stdout.contains("asr-state"));
    assert!(stdout.contains("audio-devices"));
    assert!(stdout.contains("doctor"));
    assert!(stdout.contains("protocol"));
    assert!(stdout.contains("registry"));
    assert!(stdout.contains("device"));
    assert!(stdout.contains("provider"));
    assert!(stdout.contains("model"));
    assert!(stdout.contains("daemon"));
    assert!(stdout.contains("recording"));
    assert!(stdout.contains("activation-service"));
}

#[test]
fn init_help_lists_first_run_options() {
    let output = vinput_command()
        .args(["init", "--help"])
        .output()
        .expect("run vinput init --help");

    let stdout = assert_stdout_success(output, "init help output");
    assert!(stdout.contains("--config"));
    assert!(stdout.contains("--model-root"));
    assert!(stdout.contains("--cache-root"));
    assert!(stdout.contains("--force"));
    assert!(stdout.contains("--dry-run"));
    assert!(stdout.contains("--json"));
}

#[test]
fn config_get_set_edit_help_lists_pointer_and_write_options() {
    let get_output = vinput_command()
        .args(["config", "get", "--help"])
        .output()
        .expect("run vinput config get --help");
    let get_stdout = assert_stdout_success(get_output, "config get help output");
    assert!(get_stdout.contains("<POINTER>"));
    assert!(get_stdout.contains("--config"));
    assert!(get_stdout.contains("--json"));

    let set_output = vinput_command()
        .args(["config", "set", "--help"])
        .output()
        .expect("run vinput config set --help");
    let set_stdout = assert_stdout_success(set_output, "config set help output");
    assert!(set_stdout.contains("<POINTER>"));
    assert!(set_stdout.contains("<VALUE>"));
    assert!(set_stdout.contains("--config"));
    assert!(set_stdout.contains("--output"));
    assert!(set_stdout.contains("--in-place"));
    assert!(set_stdout.contains("--dry-run"));
    assert!(set_stdout.contains("--json"));

    let edit_output = vinput_command()
        .args(["config", "edit", "--help"])
        .output()
        .expect("run vinput config edit --help");
    let edit_stdout = assert_stdout_success(edit_output, "config edit help output");
    assert!(edit_stdout.contains("--config"));
    assert!(edit_stdout.contains("--editor"));
    assert!(edit_stdout.contains("--dry-run"));
    assert!(edit_stdout.contains("--json"));
}

#[test]
fn device_help_lists_list_and_use_options() {
    let root_output = vinput_command()
        .args(["device", "--help"])
        .output()
        .expect("run vinput device --help");
    let root_stdout = assert_stdout_success(root_output, "device help output");
    assert!(root_stdout.contains("list"));
    assert!(root_stdout.contains("use"));

    let list_output = vinput_command()
        .args(["device", "list", "--help"])
        .output()
        .expect("run vinput device list --help");
    let list_stdout = assert_stdout_success(list_output, "device list help output");
    assert!(list_stdout.contains("--config"));
    assert!(list_stdout.contains("--json"));

    let use_output = vinput_command()
        .args(["device", "use", "--help"])
        .output()
        .expect("run vinput device use --help");
    let use_stdout = assert_stdout_success(use_output, "device use help output");
    assert!(use_stdout.contains("<TARGET>"));
    assert!(use_stdout.contains("--config"));
    assert!(use_stdout.contains("--output"));
    assert!(use_stdout.contains("--in-place"));
    assert!(use_stdout.contains("--dry-run"));
    assert!(use_stdout.contains("--json"));
}

#[test]
fn provider_help_lists_list_options() {
    let root_output = vinput_command()
        .args(["provider", "--help"])
        .output()
        .expect("run vinput provider --help");
    let root_stdout = assert_stdout_success(root_output, "provider help output");
    assert!(root_stdout.contains("list"));

    let list_output = vinput_command()
        .args(["provider", "list", "--help"])
        .output()
        .expect("run vinput provider list --help");
    let list_stdout = assert_stdout_success(list_output, "provider list help output");
    assert!(list_stdout.contains("--config"));
    assert!(list_stdout.contains("--json"));
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
    assert!(stdout.contains("--installed"));
    assert!(stdout.contains("--model-root"));
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

#[test]
fn model_use_help_lists_dry_run_and_config_options() {
    let output = vinput_command()
        .args(["model", "use", "--help"])
        .output()
        .expect("run vinput model use --help");

    let stdout = assert_stdout_success(output, "model use help output");
    assert!(stdout.contains("<SELECTOR>"));
    assert!(stdout.contains("--registry"));
    assert!(stdout.contains("--config"));
    assert!(stdout.contains("--provider"));
    assert!(stdout.contains("--output"));
    assert!(stdout.contains("--in-place"));
    assert!(stdout.contains("--model-root"));
    assert!(stdout.contains("--reload-daemon"));
    assert!(stdout.contains("--dry-run"));
    assert!(stdout.contains("--json"));
}

#[test]
fn model_remove_help_lists_dry_run_and_registry_options() {
    let output = vinput_command()
        .args(["model", "remove", "--help"])
        .output()
        .expect("run vinput model remove --help");

    let stdout = assert_stdout_success(output, "model remove help output");
    assert!(stdout.contains("<SELECTOR>"));
    assert!(stdout.contains("--registry"));
    assert!(stdout.contains("--model-root"));
    assert!(stdout.contains("--dry-run"));
    assert!(stdout.contains("--yes"));
    assert!(stdout.contains("--json"));

    let alias_output = vinput_command()
        .args(["model", "rm", "--help"])
        .output()
        .expect("run vinput model rm --help");
    let alias_stdout = assert_stdout_success(alias_output, "model rm help output");
    assert!(alias_stdout.contains("--dry-run"));
}

#[test]
fn daemon_reload_asr_help_lists_dry_run_and_json_options() {
    let output = vinput_command()
        .args(["daemon", "reload-asr", "--help"])
        .output()
        .expect("run vinput daemon reload-asr --help");

    let stdout = assert_stdout_success(output, "daemon reload-asr help output");
    assert!(stdout.contains("--dry-run"));
    assert!(stdout.contains("--json"));
}

#[test]
fn daemon_status_help_lists_dry_run_and_json_options() {
    let output = vinput_command()
        .args(["daemon", "status", "--help"])
        .output()
        .expect("run vinput daemon status --help");

    let stdout = assert_stdout_success(output, "daemon status help output");
    assert!(stdout.contains("--dry-run"));
    assert!(stdout.contains("--json"));
}

#[test]
fn recording_help_lists_dry_run_options() {
    for command in ["start", "stop", "toggle"] {
        let output = vinput_command()
            .args(["recording", command, "--help"])
            .output()
            .expect("run vinput recording command --help");
        let stdout = assert_stdout_success(output, "recording help output");
        assert!(stdout.contains("--dry-run"));
        assert!(stdout.contains("--json"));
    }
}

#[test]
fn daemon_start_help_lists_dry_run_and_json_options() {
    let output = vinput_command()
        .args(["daemon", "start", "--help"])
        .output()
        .expect("run vinput daemon start --help");

    let stdout = assert_stdout_success(output, "daemon start help output");
    assert!(stdout.contains("--dry-run"));
    assert!(stdout.contains("--json"));
}

#[test]
fn daemon_user_service_help_lists_dry_run_and_json_options() {
    for command in ["stop", "restart", "log"] {
        let output = vinput_command()
            .args(["daemon", command, "--help"])
            .output()
            .expect("run vinput daemon user-service command --help");
        let stdout = assert_stdout_success(output, "daemon user-service help output");
        assert!(stdout.contains("--dry-run"));
        assert!(stdout.contains("--json"));
    }
}

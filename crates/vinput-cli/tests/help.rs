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
    assert!(stdout.contains("llm"));
    assert!(stdout.contains("adapter"));
    assert!(stdout.contains("scene"));
    assert!(stdout.contains("hotword"));
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
fn llm_and_adapter_help_list_options() {
    let llm_root_output = vinput_command()
        .args(["llm", "--help"])
        .output()
        .expect("run vinput llm --help");
    let llm_root_stdout = assert_stdout_success(llm_root_output, "llm help output");
    assert!(llm_root_stdout.contains("list"));
    assert!(llm_root_stdout.contains("add"));
    assert!(llm_root_stdout.contains("edit"));
    assert!(llm_root_stdout.contains("test"));
    assert!(llm_root_stdout.contains("remove"));

    let llm_list_output = vinput_command()
        .args(["llm", "list", "--help"])
        .output()
        .expect("run vinput llm list --help");
    let llm_list_stdout = assert_stdout_success(llm_list_output, "llm list help output");
    assert!(llm_list_stdout.contains("--config"));
    assert!(llm_list_stdout.contains("--json"));

    let llm_add_output = vinput_command()
        .args(["llm", "add", "--help"])
        .output()
        .expect("run vinput llm add --help");
    let llm_add_stdout = assert_stdout_success(llm_add_output, "llm add help output");
    assert!(llm_add_stdout.contains("<ID>"));
    assert!(llm_add_stdout.contains("--base-url"));
    assert!(llm_add_stdout.contains("--api-key"));
    assert!(llm_add_stdout.contains("--model"));
    assert!(llm_add_stdout.contains("--extra-body"));
    assert!(llm_add_stdout.contains("--config"));
    assert!(llm_add_stdout.contains("--output"));
    assert!(llm_add_stdout.contains("--in-place"));
    assert!(llm_add_stdout.contains("--dry-run"));
    assert!(llm_add_stdout.contains("--json"));

    let llm_edit_output = vinput_command()
        .args(["llm", "edit", "--help"])
        .output()
        .expect("run vinput llm edit --help");
    let llm_edit_stdout = assert_stdout_success(llm_edit_output, "llm edit help output");
    assert!(llm_edit_stdout.contains("<ID>"));
    assert!(llm_edit_stdout.contains("--base-url"));
    assert!(llm_edit_stdout.contains("--api-key"));
    assert!(llm_edit_stdout.contains("--clear-api-key"));
    assert!(llm_edit_stdout.contains("--model"));
    assert!(llm_edit_stdout.contains("--clear-model"));
    assert!(llm_edit_stdout.contains("--extra-body"));
    assert!(llm_edit_stdout.contains("--clear-extra-body"));
    assert!(llm_edit_stdout.contains("--config"));
    assert!(llm_edit_stdout.contains("--output"));
    assert!(llm_edit_stdout.contains("--in-place"));
    assert!(llm_edit_stdout.contains("--dry-run"));
    assert!(llm_edit_stdout.contains("--json"));

    let llm_test_output = vinput_command()
        .args(["llm", "test", "--help"])
        .output()
        .expect("run vinput llm test --help");
    let llm_test_stdout = assert_stdout_success(llm_test_output, "llm test help output");
    assert!(llm_test_stdout.contains("<ID>"));
    assert!(llm_test_stdout.contains("--text"));
    assert!(llm_test_stdout.contains("--timeout-ms"));
    assert!(llm_test_stdout.contains("--config"));
    assert!(llm_test_stdout.contains("--dry-run"));
    assert!(llm_test_stdout.contains("--json"));

    let llm_remove_output = vinput_command()
        .args(["llm", "remove", "--help"])
        .output()
        .expect("run vinput llm remove --help");
    let llm_remove_stdout = assert_stdout_success(llm_remove_output, "llm remove help output");
    assert!(llm_remove_stdout.contains("<ID>"));
    assert!(llm_remove_stdout.contains("--config"));
    assert!(llm_remove_stdout.contains("--output"));
    assert!(llm_remove_stdout.contains("--in-place"));
    assert!(llm_remove_stdout.contains("--dry-run"));
    assert!(llm_remove_stdout.contains("--json"));

    let adapter_root_output = vinput_command()
        .args(["adapter", "--help"])
        .output()
        .expect("run vinput adapter --help");
    let adapter_root_stdout = assert_stdout_success(adapter_root_output, "adapter help output");
    assert!(adapter_root_stdout.contains("list"));
    assert!(adapter_root_stdout.contains("add"));
    assert!(adapter_root_stdout.contains("edit"));
    assert!(adapter_root_stdout.contains("install-plan"));
    assert!(adapter_root_stdout.contains("start"));
    assert!(adapter_root_stdout.contains("stop"));
    assert!(adapter_root_stdout.contains("remove"));

    let adapter_list_output = vinput_command()
        .args(["adapter", "list", "--help"])
        .output()
        .expect("run vinput adapter list --help");
    let adapter_list_stdout =
        assert_stdout_success(adapter_list_output, "adapter list help output");
    assert!(adapter_list_stdout.contains("--available"));
    assert!(adapter_list_stdout.contains("--registry"));
    assert!(adapter_list_stdout.contains("--config"));
    assert!(adapter_list_stdout.contains("--json"));
}

#[test]
fn scene_help_lists_list_and_use_options() {
    let root_output = vinput_command()
        .args(["scene", "--help"])
        .output()
        .expect("run vinput scene --help");
    let root_stdout = assert_stdout_success(root_output, "scene help output");
    assert!(root_stdout.contains("list"));
    assert!(root_stdout.contains("add"));
    assert!(root_stdout.contains("edit"));
    assert!(root_stdout.contains("use"));
    assert!(root_stdout.contains("remove"));

    let list_output = vinput_command()
        .args(["scene", "list", "--help"])
        .output()
        .expect("run vinput scene list --help");
    let list_stdout = assert_stdout_success(list_output, "scene list help output");
    assert!(list_stdout.contains("--config"));
    assert!(list_stdout.contains("--json"));

    let add_output = vinput_command()
        .args(["scene", "add", "--help"])
        .output()
        .expect("run vinput scene add --help");
    let add_stdout = assert_stdout_success(add_output, "scene add help output");
    assert!(add_stdout.contains("<ID>"));
    assert!(add_stdout.contains("--label"));
    assert!(add_stdout.contains("--prompt"));
    assert!(add_stdout.contains("--provider-id"));
    assert!(add_stdout.contains("--model"));
    assert!(add_stdout.contains("--candidate-count"));
    assert!(add_stdout.contains("--timeout-ms"));
    assert!(add_stdout.contains("--context-lines"));
    assert!(add_stdout.contains("--config"));
    assert!(add_stdout.contains("--output"));
    assert!(add_stdout.contains("--in-place"));
    assert!(add_stdout.contains("--dry-run"));
    assert!(add_stdout.contains("--json"));

    let edit_output = vinput_command()
        .args(["scene", "edit", "--help"])
        .output()
        .expect("run vinput scene edit --help");
    let edit_stdout = assert_stdout_success(edit_output, "scene edit help output");
    assert!(edit_stdout.contains("<ID>"));
    assert!(edit_stdout.contains("--label"));
    assert!(edit_stdout.contains("--prompt"));
    assert!(edit_stdout.contains("--clear-prompt"));
    assert!(edit_stdout.contains("--provider-id"));
    assert!(edit_stdout.contains("--clear-provider-id"));
    assert!(edit_stdout.contains("--model"));
    assert!(edit_stdout.contains("--clear-model"));
    assert!(edit_stdout.contains("--candidate-count"));
    assert!(edit_stdout.contains("--timeout-ms"));
    assert!(edit_stdout.contains("--clear-timeout"));
    assert!(edit_stdout.contains("--context-lines"));
    assert!(edit_stdout.contains("--config"));
    assert!(edit_stdout.contains("--output"));
    assert!(edit_stdout.contains("--in-place"));
    assert!(edit_stdout.contains("--dry-run"));
    assert!(edit_stdout.contains("--json"));

    let use_output = vinput_command()
        .args(["scene", "use", "--help"])
        .output()
        .expect("run vinput scene use --help");
    let use_stdout = assert_stdout_success(use_output, "scene use help output");
    assert!(use_stdout.contains("<ID>"));
    assert!(use_stdout.contains("--config"));
    assert!(use_stdout.contains("--output"));
    assert!(use_stdout.contains("--in-place"));
    assert!(use_stdout.contains("--dry-run"));
    assert!(use_stdout.contains("--json"));

    let remove_output = vinput_command()
        .args(["scene", "remove", "--help"])
        .output()
        .expect("run vinput scene remove --help");
    let remove_stdout = assert_stdout_success(remove_output, "scene remove help output");
    assert!(remove_stdout.contains("<ID>"));
    assert!(remove_stdout.contains("--config"));
    assert!(remove_stdout.contains("--output"));
    assert!(remove_stdout.contains("--in-place"));
    assert!(remove_stdout.contains("--dry-run"));
    assert!(remove_stdout.contains("--json"));
}

#[test]
fn provider_help_lists_list_and_use_options() {
    let root_output = vinput_command()
        .args(["provider", "--help"])
        .output()
        .expect("run vinput provider --help");
    let root_stdout = assert_stdout_success(root_output, "provider help output");
    assert!(root_stdout.contains("list"));
    assert!(root_stdout.contains("use"));
    assert!(root_stdout.contains("add"));
    assert!(root_stdout.contains("edit"));
    assert!(root_stdout.contains("remove"));

    let list_output = vinput_command()
        .args(["provider", "list", "--help"])
        .output()
        .expect("run vinput provider list --help");
    let list_stdout = assert_stdout_success(list_output, "provider list help output");
    assert!(list_stdout.contains("--config"));
    assert!(list_stdout.contains("--json"));

    let add_output = vinput_command()
        .args(["provider", "add", "--help"])
        .output()
        .expect("run vinput provider add --help");
    let add_stdout = assert_stdout_success(add_output, "provider add help output");
    assert!(add_stdout.contains("<ID>"));
    assert!(add_stdout.contains("--type"));
    assert!(add_stdout.contains("--model"));
    assert!(add_stdout.contains("--hotwords-file"));
    assert!(add_stdout.contains("--command"));
    assert!(add_stdout.contains("--arg"));
    assert!(add_stdout.contains("--env"));
    assert!(add_stdout.contains("--endpoint"));
    assert!(add_stdout.contains("--timeout-ms"));
    assert!(add_stdout.contains("--config"));
    assert!(add_stdout.contains("--output"));
    assert!(add_stdout.contains("--in-place"));
    assert!(add_stdout.contains("--dry-run"));
    assert!(add_stdout.contains("--json"));

    let edit_output = vinput_command()
        .args(["provider", "edit", "--help"])
        .output()
        .expect("run vinput provider edit --help");
    let edit_stdout = assert_stdout_success(edit_output, "provider edit help output");
    assert!(edit_stdout.contains("<ID>"));
    assert!(edit_stdout.contains("--type"));
    assert!(edit_stdout.contains("--model"));
    assert!(edit_stdout.contains("--clear-model"));
    assert!(edit_stdout.contains("--hotwords-file"));
    assert!(edit_stdout.contains("--clear-hotwords-file"));
    assert!(edit_stdout.contains("--command"));
    assert!(edit_stdout.contains("--clear-command"));
    assert!(edit_stdout.contains("--arg"));
    assert!(edit_stdout.contains("--clear-args"));
    assert!(edit_stdout.contains("--env"));
    assert!(edit_stdout.contains("--clear-env"));
    assert!(edit_stdout.contains("--endpoint"));
    assert!(edit_stdout.contains("--clear-endpoint"));
    assert!(edit_stdout.contains("--timeout-ms"));
    assert!(edit_stdout.contains("--clear-timeout"));
    assert!(edit_stdout.contains("--config"));
    assert!(edit_stdout.contains("--output"));
    assert!(edit_stdout.contains("--in-place"));
    assert!(edit_stdout.contains("--dry-run"));
    assert!(edit_stdout.contains("--json"));

    let use_output = vinput_command()
        .args(["provider", "use", "--help"])
        .output()
        .expect("run vinput provider use --help");
    let use_stdout = assert_stdout_success(use_output, "provider use help output");
    assert!(use_stdout.contains("<ID>"));
    assert!(use_stdout.contains("--config"));
    assert!(use_stdout.contains("--output"));
    assert!(use_stdout.contains("--in-place"));
    assert!(use_stdout.contains("--dry-run"));
    assert!(use_stdout.contains("--json"));

    let remove_output = vinput_command()
        .args(["provider", "remove", "--help"])
        .output()
        .expect("run vinput provider remove --help");
    let remove_stdout = assert_stdout_success(remove_output, "provider remove help output");
    assert!(remove_stdout.contains("<ID>"));
    assert!(remove_stdout.contains("--config"));
    assert!(remove_stdout.contains("--output"));
    assert!(remove_stdout.contains("--in-place"));
    assert!(remove_stdout.contains("--dry-run"));
    assert!(remove_stdout.contains("--json"));
}

#[test]
fn hotword_help_lists_get_options() {
    let root_output = vinput_command()
        .args(["hotword", "--help"])
        .output()
        .expect("run vinput hotword --help");
    let root_stdout = assert_stdout_success(root_output, "hotword help output");
    assert!(root_stdout.contains("get"));
    assert!(root_stdout.contains("set"));
    assert!(root_stdout.contains("clear"));
    assert!(root_stdout.contains("edit"));

    let get_output = vinput_command()
        .args(["hotword", "get", "--help"])
        .output()
        .expect("run vinput hotword get --help");
    let get_stdout = assert_stdout_success(get_output, "hotword get help output");
    assert!(get_stdout.contains("--provider"));
    assert!(get_stdout.contains("--config"));
    assert!(get_stdout.contains("--json"));

    let set_output = vinput_command()
        .args(["hotword", "set", "--help"])
        .output()
        .expect("run vinput hotword set --help");
    let set_stdout = assert_stdout_success(set_output, "hotword set help output");
    assert!(set_stdout.contains("<PATH>"));
    assert!(set_stdout.contains("--provider"));
    assert!(set_stdout.contains("--config"));
    assert!(set_stdout.contains("--output"));
    assert!(set_stdout.contains("--in-place"));
    assert!(set_stdout.contains("--dry-run"));
    assert!(set_stdout.contains("--json"));

    let clear_output = vinput_command()
        .args(["hotword", "clear", "--help"])
        .output()
        .expect("run vinput hotword clear --help");
    let clear_stdout = assert_stdout_success(clear_output, "hotword clear help output");
    assert!(clear_stdout.contains("--provider"));
    assert!(clear_stdout.contains("--config"));
    assert!(clear_stdout.contains("--output"));
    assert!(clear_stdout.contains("--in-place"));
    assert!(clear_stdout.contains("--dry-run"));
    assert!(clear_stdout.contains("--json"));

    let edit_output = vinput_command()
        .args(["hotword", "edit", "--help"])
        .output()
        .expect("run vinput hotword edit --help");
    let edit_stdout = assert_stdout_success(edit_output, "hotword edit help output");
    assert!(edit_stdout.contains("--provider"));
    assert!(edit_stdout.contains("--config"));
    assert!(edit_stdout.contains("--editor"));
    assert!(edit_stdout.contains("--dry-run"));
    assert!(edit_stdout.contains("--json"));
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

#[test]
fn recording_help_lists_status_options() {
    let root_output = vinput_command()
        .args(["recording", "--help"])
        .output()
        .expect("run vinput recording --help");
    let root_stdout = assert_stdout_success(root_output, "recording help output");
    assert!(root_stdout.contains("start"));
    assert!(root_stdout.contains("stop"));
    assert!(root_stdout.contains("toggle"));
    assert!(root_stdout.contains("status"));

    let status_output = vinput_command()
        .args(["recording", "status", "--help"])
        .output()
        .expect("run vinput recording status --help");
    let status_stdout = assert_stdout_success(status_output, "recording status help output");
    assert!(status_stdout.contains("--dry-run"));
    assert!(status_stdout.contains("--json"));
}

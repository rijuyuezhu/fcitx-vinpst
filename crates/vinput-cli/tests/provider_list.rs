//! Integration tests for ASR provider listing and selection CLI paths.

mod common;

use std::{fs, path::Path};

use common::{assert_json_success, assert_stdout_success, vinput_command, write_temp_json};

#[test]
fn provider_list_json_reports_bundled_default_active_provider() {
    let output = vinput_command()
        .args(["provider", "list", "--json"])
        .output()
        .expect("run vinput provider list --json");

    let value = assert_json_success(output, "provider list json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["source"], "bundled-default");
    assert_eq!(value["config_path"], serde_json::Value::Null);
    assert_eq!(value["active_provider"], "sherpa-onnx");
    assert_eq!(value["provider_count"], 1);

    let providers = value["providers"].as_array().unwrap();
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0]["id"], "sherpa-onnx");
    assert_eq!(providers[0]["type"], "local");
    assert_eq!(providers[0]["active"], true);
    assert_eq!(providers[0]["timeout_ms"], 15000);
    assert_eq!(providers[0]["hotwords_file_configured"], false);
    assert_eq!(providers[0]["command_configured"], false);
    assert_eq!(providers[0]["endpoint_configured"], false);
}

#[test]
fn provider_list_json_reports_multiple_provider_kinds_and_active_marker() {
    let path = write_provider_fixture("vinput-provider-list");

    let output = vinput_command()
        .args(["provider", "ls", "--config"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run vinput provider ls --json");
    fs::remove_file(&path).expect("remove temporary provider config");

    let value = assert_json_success(output, "provider list json with fixture");
    assert_eq!(value["source"], "file");
    assert_eq!(value["config_path"], path.to_string_lossy().as_ref());
    assert_eq!(value["active_provider"], "cmd");
    assert_eq!(value["provider_count"], 3);

    let providers = value["providers"].as_array().unwrap();
    assert_eq!(providers[0]["id"], "local");
    assert_eq!(providers[0]["type"], "local");
    assert_eq!(providers[0]["active"], false);
    assert_eq!(providers[0]["model"], "/tmp/model");
    assert_eq!(providers[0]["hotwords_file_configured"], true);

    assert_eq!(providers[1]["id"], "cmd");
    assert_eq!(providers[1]["type"], "command");
    assert_eq!(providers[1]["active"], true);
    assert_eq!(providers[1]["command_configured"], true);
    assert_eq!(providers[1]["args_count"], 1);
    assert_eq!(providers[1]["env_count"], 1);
    assert_eq!(providers[1]["timeout_ms"], 20000);

    assert_eq!(providers[2]["id"], "remote");
    assert_eq!(providers[2]["type"], "remote");
    assert_eq!(providers[2]["endpoint_configured"], true);
}

#[test]
fn provider_list_text_prints_table_and_active_marker() {
    let path = write_provider_fixture("vinput-provider-list-text");

    let output = vinput_command()
        .args(["provider", "list", "--config"])
        .arg(&path)
        .output()
        .expect("run vinput provider list text");
    fs::remove_file(&path).expect("remove temporary provider config");

    let stdout = assert_stdout_success(output, "provider list text");
    assert!(stdout.contains("source: file"));
    assert!(stdout.contains("active_provider: cmd"));
    assert!(stdout.contains("provider_count: 3"));
    assert!(stdout.contains("active\tid\ttype\tmodel\thotwords\tcommand\tendpoint\ttimeout_ms"));
    assert!(stdout.contains("\tlocal\tlocal\t/tmp/model\tyes\tno\tno\t-"));
    assert!(stdout.contains("*\tcmd\tcommand\t-\tno\tyes\tno\t20000"));
}

#[test]
fn provider_use_dry_run_json_validates_existing_provider_without_writing() {
    let path = write_provider_fixture("vinput-provider-use-dry-run");
    let before = fs::read_to_string(&path).expect("read original provider config");

    let output = vinput_command()
        .args(["provider", "use", "remote", "--config"])
        .arg(&path)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinput provider use dry-run");

    let value = assert_json_success(output, "provider use dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["before"], "cmd");
    assert_eq!(value["after"], "remote");
    assert_eq!(value["provider_type"], "remote");
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged provider config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary provider config");
}

#[test]
fn provider_use_text_dry_run_outputs_expected_fields() {
    let path = write_provider_fixture("vinput-provider-use-text-dry-run");

    let output = vinput_command()
        .args(["provider", "use", "remote", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput provider use text dry-run");
    fs::remove_file(&path).expect("remove temporary provider config");

    let stdout = assert_stdout_success(output, "provider use text dry-run");
    assert!(stdout.contains("dry_run: true"));
    assert!(stdout.contains("source: file"));
    assert!(stdout.contains("before: cmd"));
    assert!(stdout.contains("after: remote"));
    assert!(stdout.contains("provider_type: remote"));
    assert!(stdout.contains("will_write_config: false"));
    assert!(stdout.contains("wrote_config: false"));
}

#[test]
fn provider_use_output_writes_valid_config_without_overwriting_input() {
    let root = unique_temp_dir("vinput-provider-use-output");
    let config_path = root.join("config.json");
    fs::write(&config_path, provider_fixture_json()).expect("write provider config");
    let output_path = root.join("out/provider.json");
    let before = fs::read_to_string(&config_path).expect("read original provider config");

    let output = vinput_command()
        .args(["provider", "use", "local", "--config"])
        .arg(&config_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinput provider use --output");

    let value = assert_json_success(output, "provider use output json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    assert_eq!(read_json(&output_path)["asr"]["active_provider"], "local");
    fs::remove_dir_all(root).expect("remove provider output fixture dir");
}

#[test]
fn provider_use_in_place_writes_backup() {
    let root = unique_temp_dir("vinput-provider-use-in-place");
    let config_path = root.join("config.json");
    fs::write(&config_path, provider_fixture_json()).expect("write provider config");
    let backup_path = root.join("config.json.bak");
    let before = fs::read_to_string(&config_path).expect("read original provider config");

    let output = vinput_command()
        .args(["provider", "use", "local", "--config"])
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinput provider use --in-place");

    let value = assert_json_success(output, "provider use in-place json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&backup_path).expect("read provider backup config"),
        before
    );
    assert_eq!(read_json(&config_path)["asr"]["active_provider"], "local");
    fs::remove_dir_all(root).expect("remove provider in-place fixture dir");
}

#[test]
fn provider_use_rejects_empty_missing_and_missing_write_target() {
    let path = write_provider_fixture("vinput-provider-use-errors");

    let empty = vinput_command()
        .args(["provider", "use", "   ", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput provider use empty id");
    assert!(!empty.status.success());
    let stderr = String::from_utf8(empty.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider id cannot be empty"));

    let missing = vinput_command()
        .args(["provider", "use", "missing", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput provider use missing id");
    assert!(!missing.status.success());
    let stderr = String::from_utf8(missing.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider `missing` not found"));

    let missing_target = vinput_command()
        .args(["provider", "use", "local", "--config"])
        .arg(&path)
        .output()
        .expect("run vinput provider use without write target");
    assert!(!missing_target.status.success());
    let stderr = String::from_utf8(missing_target.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("config set writes require --output <path> or --in-place"));

    fs::remove_file(&path).expect("remove temporary provider config");
}

#[test]
fn provider_add_dry_run_json_validates_local_provider_without_writing() {
    let path = write_provider_fixture("vinput-provider-add-dry-run");
    let before = fs::read_to_string(&path).expect("read original provider config");

    let output = vinput_command()
        .args(["provider", "add", "extra", "--config"])
        .arg(&path)
        .args([
            "--model",
            "extra-model",
            "--hotwords-file",
            "/tmp/extra-hotwords.txt",
        ])
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinput provider add dry-run");

    let value = assert_json_success(output, "provider add dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["provider_id"], "extra");
    assert_eq!(value["provider_type"], "local");
    assert_eq!(value["active_provider"], "cmd");
    assert_eq!(value["before_provider_count"], 3);
    assert_eq!(value["after_provider_count"], 4);
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged provider config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary provider config");
}

#[test]
fn provider_add_text_dry_run_outputs_expected_fields() {
    let path = write_provider_fixture("vinput-provider-add-text-dry-run");

    let output = vinput_command()
        .args(["provider", "add", "extra", "--config"])
        .arg(&path)
        .args(["--model", "extra-model", "--dry-run"])
        .output()
        .expect("run vinput provider add text dry-run");
    fs::remove_file(&path).expect("remove temporary provider config");

    let stdout = assert_stdout_success(output, "provider add text dry-run");
    assert!(stdout.contains("dry_run: true"));
    assert!(stdout.contains("source: file"));
    assert!(stdout.contains("provider_id: extra"));
    assert!(stdout.contains("provider_type: local"));
    assert!(stdout.contains("active_provider: cmd"));
    assert!(stdout.contains("before_provider_count: 3"));
    assert!(stdout.contains("after_provider_count: 4"));
    assert!(stdout.contains("will_write_config: false"));
    assert!(stdout.contains("wrote_config: false"));
}

#[test]
fn provider_add_output_writes_command_provider_without_overwriting_input() {
    let root = unique_temp_dir("vinput-provider-add-output");
    let config_path = root.join("config.json");
    fs::write(&config_path, provider_fixture_json()).expect("write provider config");
    let output_path = root.join("out/provider.json");
    let before = fs::read_to_string(&config_path).expect("read original provider config");

    let output = vinput_command()
        .args([
            "provider",
            "add",
            "cmd2",
            "--type",
            "command",
            "--command",
            "helper",
            "--config",
        ])
        .arg(&config_path)
        .args([
            "--arg=--json",
            "--env",
            "TOKEN=redacted",
            "--timeout-ms",
            "30000",
        ])
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinput provider add command --output");

    let value = assert_json_success(output, "provider add output json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["provider_id"], "cmd2");
    assert_eq!(value["provider_type"], "command");
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    let json = read_json(&output_path);
    let provider = json["asr"]["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|provider| provider["id"] == "cmd2")
        .expect("added command provider");
    assert_eq!(provider["type"], "command");
    assert_eq!(provider["command"], "helper");
    assert_eq!(provider["args"][0], "--json");
    assert_eq!(provider["env"]["TOKEN"], "redacted");
    assert_eq!(provider["timeout_ms"], 30000);
    fs::remove_dir_all(root).expect("remove provider add output fixture dir");
}

#[test]
fn provider_add_in_place_writes_remote_provider_and_backup() {
    let root = unique_temp_dir("vinput-provider-add-in-place");
    let config_path = root.join("config.json");
    fs::write(&config_path, provider_fixture_json()).expect("write provider config");
    let backup_path = root.join("config.json.bak");
    let before = fs::read_to_string(&config_path).expect("read original provider config");

    let output = vinput_command()
        .args([
            "provider",
            "add",
            "cloud",
            "--type",
            "remote",
            "--endpoint",
            "https://asr.example.test",
            "--config",
        ])
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinput provider add remote --in-place");

    let value = assert_json_success(output, "provider add in-place json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["provider_id"], "cloud");
    assert_eq!(value["provider_type"], "remote");
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&backup_path).expect("read provider backup config"),
        before
    );
    let json = read_json(&config_path);
    let provider = json["asr"]["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|provider| provider["id"] == "cloud")
        .expect("added remote provider");
    assert_eq!(provider["type"], "remote");
    assert_eq!(provider["endpoint"], "https://asr.example.test");
    fs::remove_dir_all(root).expect("remove provider add in-place fixture dir");
}

#[test]
fn provider_add_rejects_invalid_duplicate_and_missing_write_target() {
    let path = write_provider_fixture("vinput-provider-add-errors");

    let empty = vinput_command()
        .args(["provider", "add", "   ", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput provider add empty id");
    assert!(!empty.status.success());
    let stderr = String::from_utf8(empty.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider id cannot be empty"));

    let duplicate = vinput_command()
        .args(["provider", "add", "cmd", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput provider add duplicate id");
    assert!(!duplicate.status.success());
    let stderr = String::from_utf8(duplicate.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider `cmd` already exists"));

    let invalid_type = vinput_command()
        .args(["provider", "add", "new", "--type", "bad", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput provider add invalid type");
    assert!(!invalid_type.status.success());
    let stderr = String::from_utf8(invalid_type.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("unsupported ASR provider type `bad`"));

    let missing_command = vinput_command()
        .args(["provider", "add", "cmd2", "--type", "command", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput provider add command without command");
    assert!(!missing_command.status.success());
    let stderr = String::from_utf8(missing_command.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("command ASR provider `cmd2` must configure a command"));

    let missing_endpoint = vinput_command()
        .args(["provider", "add", "cloud", "--type", "remote", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput provider add remote without endpoint");
    assert!(!missing_endpoint.status.success());
    let stderr = String::from_utf8(missing_endpoint.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("remote ASR provider `cloud` must configure an endpoint"));

    let bad_env = vinput_command()
        .args([
            "provider",
            "add",
            "cmd3",
            "--type",
            "command",
            "--command",
            "helper",
            "--env",
            "TOKEN",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput provider add invalid env");
    assert!(!bad_env.status.success());
    let stderr = String::from_utf8(bad_env.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("provider env `TOKEN` is not KEY=VALUE"));

    let missing_target = vinput_command()
        .args(["provider", "add", "new", "--config"])
        .arg(&path)
        .output()
        .expect("run vinput provider add without write target");
    assert!(!missing_target.status.success());
    let stderr = String::from_utf8(missing_target.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("config set writes require --output <path> or --in-place"));

    fs::remove_file(&path).expect("remove temporary provider config");
}

#[test]
fn provider_edit_dry_run_json_updates_command_provider_without_writing() {
    let path = write_provider_fixture("vinput-provider-edit-dry-run");
    let before = fs::read_to_string(&path).expect("read original provider config");

    let output = vinput_command()
        .args(["provider", "edit", "cmd", "--config"])
        .arg(&path)
        .args([
            "--command",
            "helper2",
            "--arg=--stream",
            "--env",
            "TOKEN=new",
            "--timeout-ms",
            "31000",
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("run vinput provider edit dry-run");

    let value = assert_json_success(output, "provider edit dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["provider_id"], "cmd");
    assert_eq!(value["before_provider_type"], "command");
    assert_eq!(value["after_provider_type"], "command");
    assert_eq!(value["active_provider"], "cmd");
    assert_eq!(value["wrote_config"], false);
    let changed = value["changed_fields"].as_array().unwrap();
    assert!(changed.iter().any(|field| field == "command"));
    assert!(changed.iter().any(|field| field == "args"));
    assert!(changed.iter().any(|field| field == "env"));
    assert!(changed.iter().any(|field| field == "timeout_ms"));
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged provider config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary provider config");
}

#[test]
fn provider_edit_text_dry_run_outputs_expected_fields() {
    let path = write_provider_fixture("vinput-provider-edit-text-dry-run");

    let output = vinput_command()
        .args(["provider", "edit", "local", "--config"])
        .arg(&path)
        .args(["--model", "/tmp/new-model", "--dry-run"])
        .output()
        .expect("run vinput provider edit text dry-run");
    fs::remove_file(&path).expect("remove temporary provider config");

    let stdout = assert_stdout_success(output, "provider edit text dry-run");
    assert!(stdout.contains("dry_run: true"));
    assert!(stdout.contains("source: file"));
    assert!(stdout.contains("provider_id: local"));
    assert!(stdout.contains("before_provider_type: local"));
    assert!(stdout.contains("after_provider_type: local"));
    assert!(stdout.contains("active_provider: cmd"));
    assert!(stdout.contains("changed_fields: model"));
    assert!(stdout.contains("will_write_config: false"));
    assert!(stdout.contains("wrote_config: false"));
}

#[test]
fn provider_edit_output_writes_valid_config_without_overwriting_input() {
    let root = unique_temp_dir("vinput-provider-edit-output");
    let config_path = root.join("config.json");
    fs::write(&config_path, provider_fixture_json()).expect("write provider config");
    let output_path = root.join("out/provider.json");
    let before = fs::read_to_string(&config_path).expect("read original provider config");

    let output = vinput_command()
        .args(["provider", "edit", "local", "--config"])
        .arg(&config_path)
        .args(["--model", "/tmp/new-model", "--clear-hotwords-file"])
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinput provider edit --output");

    let value = assert_json_success(output, "provider edit output json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["provider_id"], "local");
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    let json = read_json(&output_path);
    assert_eq!(json["asr"]["providers"][0]["model"], "/tmp/new-model");
    assert!(
        json["asr"]["providers"][0]
            .as_object()
            .unwrap()
            .get("hotwords_file")
            .is_none()
    );
    fs::remove_dir_all(root).expect("remove provider edit output fixture dir");
}

#[test]
fn provider_edit_in_place_writes_remote_provider_and_backup() {
    let root = unique_temp_dir("vinput-provider-edit-in-place");
    let config_path = root.join("config.json");
    fs::write(&config_path, provider_fixture_json()).expect("write provider config");
    let backup_path = root.join("config.json.bak");
    let before = fs::read_to_string(&config_path).expect("read original provider config");

    let output = vinput_command()
        .args(["provider", "edit", "remote", "--config"])
        .arg(&config_path)
        .args([
            "--endpoint",
            "https://new-asr.example.test",
            "--model",
            "cloud-v2",
            "--in-place",
            "--json",
        ])
        .output()
        .expect("run vinput provider edit --in-place");

    let value = assert_json_success(output, "provider edit in-place json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["provider_id"], "remote");
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&backup_path).expect("read provider edit backup config"),
        before
    );
    let json = read_json(&config_path);
    assert_eq!(
        json["asr"]["providers"][2]["endpoint"],
        "https://new-asr.example.test"
    );
    assert_eq!(json["asr"]["providers"][2]["model"], "cloud-v2");
    fs::remove_dir_all(root).expect("remove provider edit in-place fixture dir");
}

#[test]
fn provider_edit_rejects_invalid_missing_noop_conflicts_and_invalid_config() {
    let path = write_provider_fixture("vinput-provider-edit-errors");

    let empty = vinput_command()
        .args(["provider", "edit", "   ", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput provider edit empty id");
    assert!(!empty.status.success());
    let stderr = String::from_utf8(empty.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider id cannot be empty"));

    let missing = vinput_command()
        .args(["provider", "edit", "missing", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput provider edit missing id");
    assert!(!missing.status.success());
    let stderr = String::from_utf8(missing.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider `missing` not found"));

    let noop = vinput_command()
        .args(["provider", "edit", "local", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput provider edit without field changes");
    assert!(!noop.status.success());
    let stderr = String::from_utf8(noop.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("provider edit requires at least one field change"));

    let conflict = vinput_command()
        .args([
            "provider",
            "edit",
            "local",
            "--model",
            "new",
            "--clear-model",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput provider edit conflicting model flags");
    assert!(!conflict.status.success());
    let stderr = String::from_utf8(conflict.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("provider edit cannot combine --model and --clear-model"));

    let invalid_command = vinput_command()
        .args(["provider", "edit", "cmd", "--clear-command", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput provider edit invalid command provider");
    assert!(!invalid_command.status.success());
    let stderr = String::from_utf8(invalid_command.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("command ASR provider `cmd` must configure a command"));

    let invalid_remote = vinput_command()
        .args(["provider", "edit", "remote", "--clear-endpoint", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput provider edit invalid remote provider");
    assert!(!invalid_remote.status.success());
    let stderr = String::from_utf8(invalid_remote.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("remote ASR provider `remote` must configure an endpoint"));

    fs::remove_file(&path).expect("remove temporary provider config");
}

#[test]
fn provider_remove_dry_run_json_validates_inactive_provider_without_writing() {
    let path = write_provider_fixture("vinput-provider-remove-dry-run");
    let before = fs::read_to_string(&path).expect("read original provider config");

    let output = vinput_command()
        .args(["provider", "remove", "remote", "--config"])
        .arg(&path)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinput provider remove dry-run");

    let value = assert_json_success(output, "provider remove dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["removed_provider_id"], "remote");
    assert_eq!(value["removed_provider_type"], "remote");
    assert_eq!(value["active_provider"], "cmd");
    assert_eq!(value["before_provider_count"], 3);
    assert_eq!(value["after_provider_count"], 2);
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged provider config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary provider config");
}

#[test]
fn provider_remove_text_dry_run_outputs_expected_fields() {
    let path = write_provider_fixture("vinput-provider-remove-text-dry-run");

    let output = vinput_command()
        .args(["provider", "remove", "remote", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput provider remove text dry-run");
    fs::remove_file(&path).expect("remove temporary provider config");

    let stdout = assert_stdout_success(output, "provider remove text dry-run");
    assert!(stdout.contains("dry_run: true"));
    assert!(stdout.contains("source: file"));
    assert!(stdout.contains("removed_provider_id: remote"));
    assert!(stdout.contains("removed_provider_type: remote"));
    assert!(stdout.contains("active_provider: cmd"));
    assert!(stdout.contains("before_provider_count: 3"));
    assert!(stdout.contains("after_provider_count: 2"));
    assert!(stdout.contains("will_write_config: false"));
    assert!(stdout.contains("wrote_config: false"));
}

#[test]
fn provider_remove_output_writes_valid_config_without_overwriting_input() {
    let root = unique_temp_dir("vinput-provider-remove-output");
    let config_path = root.join("config.json");
    fs::write(&config_path, provider_fixture_json()).expect("write provider config");
    let output_path = root.join("out/provider.json");
    let before = fs::read_to_string(&config_path).expect("read original provider config");

    let output = vinput_command()
        .args(["provider", "remove", "local", "--config"])
        .arg(&config_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinput provider remove --output");

    let value = assert_json_success(output, "provider remove output json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    let providers = read_json(&output_path)["asr"]["providers"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(providers.len(), 2);
    assert!(providers.iter().all(|provider| provider["id"] != "local"));
    fs::remove_dir_all(root).expect("remove provider remove output fixture dir");
}

#[test]
fn provider_remove_in_place_writes_backup() {
    let root = unique_temp_dir("vinput-provider-remove-in-place");
    let config_path = root.join("config.json");
    fs::write(&config_path, provider_fixture_json()).expect("write provider config");
    let backup_path = root.join("config.json.bak");
    let before = fs::read_to_string(&config_path).expect("read original provider config");

    let output = vinput_command()
        .args(["provider", "remove", "remote", "--config"])
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinput provider remove --in-place");

    let value = assert_json_success(output, "provider remove in-place json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&backup_path).expect("read provider backup config"),
        before
    );
    let providers = read_json(&config_path)["asr"]["providers"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(providers.len(), 2);
    assert!(providers.iter().all(|provider| provider["id"] != "remote"));
    fs::remove_dir_all(root).expect("remove provider remove in-place fixture dir");
}

#[test]
fn provider_remove_rejects_empty_missing_active_and_missing_write_target() {
    let path = write_provider_fixture("vinput-provider-remove-errors");

    let empty = vinput_command()
        .args(["provider", "remove", "   ", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput provider remove empty id");
    assert!(!empty.status.success());
    let stderr = String::from_utf8(empty.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider id cannot be empty"));

    let missing = vinput_command()
        .args(["provider", "remove", "missing", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput provider remove missing id");
    assert!(!missing.status.success());
    let stderr = String::from_utf8(missing.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider `missing` not found"));

    let active = vinput_command()
        .args(["provider", "remove", "cmd", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput provider remove active id");
    assert!(!active.status.success());
    let stderr = String::from_utf8(active.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("refusing to remove active ASR provider `cmd`"));

    let missing_target = vinput_command()
        .args(["provider", "remove", "local", "--config"])
        .arg(&path)
        .output()
        .expect("run vinput provider remove without write target");
    assert!(!missing_target.status.success());
    let stderr = String::from_utf8(missing_target.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("config set writes require --output <path> or --in-place"));

    fs::remove_file(&path).expect("remove temporary provider config");
}

fn write_provider_fixture(prefix: &str) -> std::path::PathBuf {
    write_temp_json(prefix, provider_fixture_json())
}

fn provider_fixture_json() -> &'static str {
    r#"
    {
      "version": 1,
      "asr": {
        "active_provider": "cmd",
        "providers": [
          {"id":"local","type":"local","model":"/tmp/model","hotwords_file":"/tmp/hotwords.txt"},
          {"id":"cmd","type":"command","command":"helper","args":["--json"],"env":{"TOKEN":"redacted"},"timeout_ms":20000},
          {"id":"remote","type":"remote","endpoint":"https://asr.example.test","model":"cloud"}
        ]
      },
      "scenes": {
        "active_scene": "raw",
        "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
      }
    }
    "#
}

fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&path).expect("create unique temp dir");
    path
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read json file"))
        .expect("parse json file")
}

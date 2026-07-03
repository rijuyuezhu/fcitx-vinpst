//! Integration tests for config JSON-pointer get/set commands.

mod common;

use common::{assert_json_success, assert_stdout_success, vinput_command, workspace_file};

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
    std::fs::create_dir_all(&path).expect("create unique temp dir");
    path
}

fn copy_default_config(root: &std::path::Path) -> std::path::PathBuf {
    let config_path = root.join("config.json");
    std::fs::copy(workspace_file("data/default-config.json"), &config_path)
        .expect("copy default config");
    config_path
}

fn read_json(path: &std::path::Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).expect("read json file"))
        .expect("parse json file")
}

#[test]
fn config_get_json_reads_existing_pointer() {
    let output = vinput_command()
        .args([
            "config",
            "get",
            "/asr/active_provider",
            "--config",
            workspace_file("data/default-config.json")
                .to_str()
                .expect("path utf8"),
            "--json",
        ])
        .output()
        .expect("run vinput config get json");

    let value = assert_json_success(output, "config get json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["pointer"], "/asr/active_provider");
    assert_eq!(value["value"], "sherpa-onnx");
}

#[test]
fn config_get_text_prints_scalar_value() {
    let output = vinput_command()
        .args(["config", "get", "/global/default_language", "--config"])
        .arg(workspace_file("data/default-config.json"))
        .output()
        .expect("run vinput config get text");

    let stdout = assert_stdout_success(output, "config get text");
    assert_eq!(stdout, "zh\n");
}

#[test]
fn config_set_dry_run_json_validates_without_writing() {
    let root = unique_temp_dir("vinput-cli-config-set-dry-run");
    let config_path = copy_default_config(&root);
    let before = std::fs::read_to_string(&config_path).expect("read original config");

    let output = vinput_command()
        .args([
            "config",
            "set",
            "/global/default_language",
            "en",
            "--config",
        ])
        .arg(&config_path)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinput config set dry-run json");

    let value = assert_json_success(output, "config set dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["wrote_config"], false);
    assert_eq!(value["source"], "file");
    assert_eq!(value["pointer"], "/global/default_language");
    assert_eq!(value["parsed_value_kind"], "string");
    assert_eq!(value["before"], "zh");
    assert_eq!(value["after"], "en");
    assert_eq!(
        std::fs::read_to_string(&config_path).expect("read unchanged config"),
        before
    );
}

#[test]
fn config_set_output_writes_validated_config_without_overwriting_input() {
    let root = unique_temp_dir("vinput-cli-config-set-output");
    let config_path = copy_default_config(&root);
    let output_path = root.join("out/updated.json");
    let original = std::fs::read_to_string(&config_path).expect("read original config");

    let output = vinput_command()
        .args([
            "config",
            "set",
            "/global/capture_device",
            "object:42",
            "--config",
        ])
        .arg(&config_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinput config set output");

    let value = assert_json_success(output, "config set output json");
    assert_eq!(value["dry_run"], false);
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        std::fs::read_to_string(&config_path).expect("read original config after output write"),
        original
    );
    assert_eq!(
        read_json(&output_path)["global"]["capture_device"],
        "object:42"
    );

    let validate = vinput_command()
        .args(["config", "validate"])
        .arg(&output_path)
        .output()
        .expect("validate output config");
    assert_json_success(validate, "updated output config validate");
}

#[test]
fn config_set_in_place_writes_backup() {
    let root = unique_temp_dir("vinput-cli-config-set-in-place");
    let config_path = copy_default_config(&root);
    let backup_path = root.join("config.json.bak");
    let original = std::fs::read_to_string(&config_path).expect("read original config");

    let output = vinput_command()
        .args(["config", "set", "/asr/normalize_audio", "false", "--config"])
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinput config set in-place");

    let value = assert_json_success(output, "config set in-place json");
    assert_eq!(value["dry_run"], false);
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());
    assert_eq!(value["parsed_value_kind"], "bool");
    assert_eq!(
        std::fs::read_to_string(&backup_path).expect("read backup config"),
        original
    );
    assert_eq!(read_json(&config_path)["asr"]["normalize_audio"], false);
}

#[test]
fn config_set_rejects_invalid_updated_config() {
    let root = unique_temp_dir("vinput-cli-config-set-invalid");
    let config_path = copy_default_config(&root);
    let before = std::fs::read_to_string(&config_path).expect("read original config");

    let output = vinput_command()
        .args([
            "config",
            "set",
            "/asr/active_provider",
            "missing-provider",
            "--config",
        ])
        .arg(&config_path)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run invalid vinput config set");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("validate updated config"));
    assert!(stderr.contains("missing-provider"));
    assert_eq!(
        std::fs::read_to_string(&config_path).expect("read unchanged config"),
        before
    );
}

#[test]
fn config_set_requires_existing_json_pointer_and_explicit_write_target() {
    let root = unique_temp_dir("vinput-cli-config-set-errors");
    let config_path = copy_default_config(&root);

    let missing_pointer = vinput_command()
        .args(["config", "set", "/global/missing", "value", "--config"])
        .arg(&config_path)
        .arg("--dry-run")
        .output()
        .expect("run config set missing pointer");
    assert!(!missing_pointer.status.success());
    let stderr = String::from_utf8(missing_pointer.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("config pointer `/global/missing` not found"));

    let missing_target = vinput_command()
        .args([
            "config",
            "set",
            "/global/default_language",
            "en",
            "--config",
        ])
        .arg(&config_path)
        .output()
        .expect("run config set without write target");
    assert!(!missing_target.status.success());
    let stderr = String::from_utf8(missing_target.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("config set writes require --output <path> or --in-place"));
}

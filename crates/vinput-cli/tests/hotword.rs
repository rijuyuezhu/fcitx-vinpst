//! Integration tests for ASR hotword inspection and mutation CLI paths.

mod common;

use std::{fs, path::Path};

use common::{assert_json_success, assert_stdout_success, vinput_command, write_temp_json};

#[test]
fn hotword_get_json_reports_bundled_default_active_provider() {
    let output = vinput_command()
        .args(["hotword", "get", "--json"])
        .output()
        .expect("run vinput hotword get --json");

    let value = assert_json_success(output, "hotword get json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["source"], "bundled-default");
    assert_eq!(value["config_path"], serde_json::Value::Null);
    assert_eq!(value["active_provider"], "sherpa-onnx");
    assert_eq!(value["provider_id"], "sherpa-onnx");
    assert_eq!(value["provider_type"], "local");
    assert_eq!(value["active"], true);
    assert_eq!(value["supported"], true);
    assert_eq!(value["configured"], false);
    assert_eq!(value["hotwords_file"], serde_json::Value::Null);
}

#[test]
fn hotword_get_json_reports_selected_provider_hotwords_file() {
    let path = write_hotword_fixture("vinput-hotword-get-json");

    let output = vinput_command()
        .args(["hotword", "get", "--provider", "local", "--config"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run vinput hotword get selected provider");
    fs::remove_file(&path).expect("remove temporary hotword config");

    let value = assert_json_success(output, "hotword get json selected provider");
    assert_eq!(value["source"], "file");
    assert_eq!(value["config_path"], path.to_string_lossy().as_ref());
    assert_eq!(value["active_provider"], "cmd");
    assert_eq!(value["provider_id"], "local");
    assert_eq!(value["provider_type"], "local");
    assert_eq!(value["active"], false);
    assert_eq!(value["supported"], true);
    assert_eq!(value["configured"], true);
    assert_eq!(value["hotwords_file"], "/tmp/hotwords.txt");
}

#[test]
fn hotword_get_json_reports_remote_provider_as_unsupported() {
    let path = write_hotword_fixture("vinput-hotword-get-remote");

    let output = vinput_command()
        .args(["hotword", "get", "--provider", "remote", "--config"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run vinput hotword get remote provider");
    fs::remove_file(&path).expect("remove temporary hotword config");

    let value = assert_json_success(output, "hotword get json remote provider");
    assert_eq!(value["provider_id"], "remote");
    assert_eq!(value["provider_type"], "remote");
    assert_eq!(value["supported"], false);
    assert_eq!(value["configured"], false);
    assert_eq!(value["hotwords_file"], serde_json::Value::Null);
}

#[test]
fn hotword_get_text_reports_active_provider_hotwords_file() {
    let path = write_hotword_fixture("vinput-hotword-get-text");

    let output = vinput_command()
        .args(["hotword", "get", "--config"])
        .arg(&path)
        .output()
        .expect("run vinput hotword get text");
    fs::remove_file(&path).expect("remove temporary hotword config");

    let stdout = assert_stdout_success(output, "hotword get text");
    assert!(stdout.contains("source: file"));
    assert!(stdout.contains("active_provider: cmd"));
    assert!(stdout.contains("provider_id: cmd"));
    assert!(stdout.contains("provider_type: command"));
    assert!(stdout.contains("active: true"));
    assert!(stdout.contains("supported: true"));
    assert!(stdout.contains("configured: yes"));
    assert!(stdout.contains("hotwords_file: /tmp/cmd-hotwords.txt"));
}

#[test]
fn hotword_get_rejects_empty_and_missing_provider() {
    let path = write_hotword_fixture("vinput-hotword-get-errors");

    let empty = vinput_command()
        .args(["hotword", "get", "--provider", "   ", "--config"])
        .arg(&path)
        .output()
        .expect("run vinput hotword get empty provider");
    assert!(!empty.status.success());
    let stderr = String::from_utf8(empty.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider id cannot be empty"));

    let missing = vinput_command()
        .args(["hotword", "get", "--provider", "missing", "--config"])
        .arg(&path)
        .output()
        .expect("run vinput hotword get missing provider");
    assert!(!missing.status.success());
    let stderr = String::from_utf8(missing.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider `missing` not found"));

    fs::remove_file(&path).expect("remove temporary hotword config");
}

#[test]
fn hotword_set_dry_run_json_validates_without_writing() {
    let path = write_hotword_fixture("vinput-hotword-set-dry-run");
    let before = fs::read_to_string(&path).expect("read original hotword config");

    let output = vinput_command()
        .args(["hotword", "set", "/tmp/new-hotwords.txt", "--config"])
        .arg(&path)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinput hotword set dry-run");

    let value = assert_json_success(output, "hotword set dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["provider_id"], "cmd");
    assert_eq!(value["provider_type"], "command");
    assert_eq!(value["before"], "/tmp/cmd-hotwords.txt");
    assert_eq!(value["after"], "/tmp/new-hotwords.txt");
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged hotword config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary hotword config");
}

#[test]
fn hotword_set_output_writes_valid_config_without_overwriting_input() {
    let root = unique_temp_dir("vinput-hotword-set-output");
    let config_path = root.join("config.json");
    fs::write(&config_path, hotword_fixture_json()).expect("write hotword config");
    let output_path = root.join("out/hotword.json");
    let before = fs::read_to_string(&config_path).expect("read original hotword config");

    let output = vinput_command()
        .args([
            "hotword",
            "set",
            "/tmp/local-hotwords.txt",
            "--provider",
            "local",
            "--config",
        ])
        .arg(&config_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinput hotword set --output");

    let value = assert_json_success(output, "hotword set output json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    let json = read_json(&output_path);
    assert_eq!(
        json["asr"]["providers"][0]["hotwords_file"],
        "/tmp/local-hotwords.txt"
    );
    fs::remove_dir_all(root).expect("remove hotword output fixture dir");
}

#[test]
fn hotword_clear_in_place_writes_backup_and_removes_field() {
    let root = unique_temp_dir("vinput-hotword-clear-in-place");
    let config_path = root.join("config.json");
    fs::write(&config_path, hotword_fixture_json()).expect("write hotword config");
    let backup_path = root.join("config.json.bak");
    let before = fs::read_to_string(&config_path).expect("read original hotword config");

    let output = vinput_command()
        .args(["hotword", "clear", "--config"])
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinput hotword clear --in-place");

    let value = assert_json_success(output, "hotword clear in-place json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&backup_path).expect("read hotword backup config"),
        before
    );
    let json = read_json(&config_path);
    assert!(
        json["asr"]["providers"][1]
            .as_object()
            .unwrap()
            .get("hotwords_file")
            .is_none()
    );
    fs::remove_dir_all(root).expect("remove hotword in-place fixture dir");
}

#[test]
fn hotword_set_clear_reject_empty_remote_and_missing_write_target() {
    let path = write_hotword_fixture("vinput-hotword-mutation-errors");

    let empty = vinput_command()
        .args(["hotword", "set", "   ", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput hotword set empty path");
    assert!(!empty.status.success());
    let stderr = String::from_utf8(empty.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("hotwords file cannot be empty"));

    let remote = vinput_command()
        .args([
            "hotword",
            "set",
            "/tmp/hotwords.txt",
            "--provider",
            "remote",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput hotword set remote provider");
    assert!(!remote.status.success());
    let stderr = String::from_utf8(remote.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider `remote` does not support hotwords"));

    let missing_target = vinput_command()
        .args(["hotword", "clear", "--config"])
        .arg(&path)
        .output()
        .expect("run vinput hotword clear without write target");
    assert!(!missing_target.status.success());
    let stderr = String::from_utf8(missing_target.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("config set writes require --output <path> or --in-place"));

    fs::remove_file(&path).expect("remove temporary hotword config");
}

fn write_hotword_fixture(prefix: &str) -> std::path::PathBuf {
    write_temp_json(prefix, hotword_fixture_json())
}

fn hotword_fixture_json() -> &'static str {
    r#"
    {
      "version": 1,
      "asr": {
        "active_provider": "cmd",
        "providers": [
          {"id":"local","type":"local","model":"/tmp/model","hotwords_file":"/tmp/hotwords.txt"},
          {"id":"cmd","type":"command","command":"helper","hotwords_file":"/tmp/cmd-hotwords.txt"},
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

//! Integration tests for ASR hotword inspection CLI paths.

mod common;

use std::fs;

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

fn write_hotword_fixture(prefix: &str) -> std::path::PathBuf {
    write_temp_json(
        prefix,
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
        "#,
    )
}

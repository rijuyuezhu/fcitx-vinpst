//! Integration tests for ASR provider listing CLI paths.

mod common;

use std::fs;

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
    let path = write_temp_json(
        "vinput-provider-list",
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
        "#,
    );

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
    let path = write_temp_json(
        "vinput-provider-list-text",
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "cmd",
            "providers": [
              {"id":"local","type":"local","model":"fixture-model"},
              {"id":"cmd","type":"command","command":"helper"}
            ]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    );

    let output = vinput_command()
        .args(["provider", "list", "--config"])
        .arg(&path)
        .output()
        .expect("run vinput provider list text");
    fs::remove_file(&path).expect("remove temporary provider config");

    let stdout = assert_stdout_success(output, "provider list text");
    assert!(stdout.contains("source: file"));
    assert!(stdout.contains("active_provider: cmd"));
    assert!(stdout.contains("provider_count: 2"));
    assert!(stdout.contains("active\tid\ttype\tmodel\thotwords\tcommand\tendpoint\ttimeout_ms"));
    assert!(stdout.contains("\tlocal\tlocal\tfixture-model\tno\tno\tno\t-"));
    assert!(stdout.contains("*\tcmd\tcommand\t-\tno\tyes\tno\t-"));
}

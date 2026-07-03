//! Integration tests for LLM provider and text adapter list CLI paths.

mod common;

use std::fs;

use common::{assert_json_success, assert_stdout_success, vinput_command, write_temp_json};

#[test]
fn llm_list_json_reports_bundled_default_empty_providers() {
    let output = vinput_command()
        .args(["llm", "list", "--json"])
        .output()
        .expect("run vinput llm list --json");

    let value = assert_json_success(output, "llm list json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["source"], "bundled-default");
    assert_eq!(value["config_path"], serde_json::Value::Null);
    assert_eq!(value["provider_count"], 0);
    assert_eq!(value["providers"].as_array().unwrap().len(), 0);
}

#[test]
fn llm_list_json_reports_provider_metadata_without_secrets() {
    let path = write_llm_fixture("vinput-llm-list-json");

    let output = vinput_command()
        .args(["llm", "ls", "--config"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run vinput llm ls --json");
    fs::remove_file(&path).expect("remove temporary llm config");

    let value = assert_json_success(output, "llm list json with fixture");
    assert_eq!(value["source"], "file");
    assert_eq!(value["config_path"], path.to_string_lossy().as_ref());
    assert_eq!(value["provider_count"], 2);
    let providers = value["providers"].as_array().unwrap();
    assert_eq!(providers[0]["id"], "openai");
    assert_eq!(providers[0]["base_url_configured"], true);
    assert_eq!(providers[0]["api_key_configured"], true);
    assert_eq!(providers[0]["model"], "gpt-4o-mini");
    assert_eq!(providers[0]["extra_body_configured"], true);
    assert_eq!(providers[0]["extra_field_count"], 0);
    assert_eq!(providers[1]["id"], "local");
    assert_eq!(providers[1]["api_key_configured"], false);
}

#[test]
fn llm_list_text_prints_table_without_secret_values() {
    let path = write_llm_fixture("vinput-llm-list-text");

    let output = vinput_command()
        .args(["llm", "list", "--config"])
        .arg(&path)
        .output()
        .expect("run vinput llm list text");
    fs::remove_file(&path).expect("remove temporary llm config");

    let stdout = assert_stdout_success(output, "llm list text");
    assert!(stdout.contains("source: file"));
    assert!(stdout.contains("provider_count: 2"));
    assert!(stdout.contains("id	base_url	api_key	model	extra_body	extra_fields"));
    assert!(stdout.contains("openai	yes	yes	gpt-4o-mini	yes	0"));
    assert!(stdout.contains("local	yes	no	-	no	0"));
    assert!(!stdout.contains("secret-token"));
}

#[test]
fn adapter_list_json_reports_bundled_default_empty_adapters() {
    let output = vinput_command()
        .args(["adapter", "list", "--json"])
        .output()
        .expect("run vinput adapter list --json");

    let value = assert_json_success(output, "adapter list json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["source"], "bundled-default");
    assert_eq!(value["config_path"], serde_json::Value::Null);
    assert_eq!(value["adapter_count"], 0);
    assert_eq!(value["adapters"].as_array().unwrap().len(), 0);
}

#[test]
fn adapter_list_json_reports_adapter_metadata_without_secrets() {
    let path = write_llm_fixture("vinput-adapter-list-json");

    let output = vinput_command()
        .args(["adapter", "ls", "--config"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run vinput adapter ls --json");
    fs::remove_file(&path).expect("remove temporary adapter config");

    let value = assert_json_success(output, "adapter list json with fixture");
    assert_eq!(value["source"], "file");
    assert_eq!(value["config_path"], path.to_string_lossy().as_ref());
    assert_eq!(value["adapter_count"], 2);
    let adapters = value["adapters"].as_array().unwrap();
    assert_eq!(adapters[0]["id"], "command-adapter");
    assert_eq!(adapters[0]["command_configured"], true);
    assert_eq!(adapters[0]["args_count"], 2);
    assert_eq!(adapters[0]["env_count"], 1);
    assert_eq!(adapters[0]["working_dir_configured"], true);
    assert_eq!(adapters[1]["id"], "simple-adapter");
    assert_eq!(adapters[1]["args_count"], 0);
}

#[test]
fn adapter_list_text_prints_table_without_secret_values() {
    let path = write_llm_fixture("vinput-adapter-list-text");

    let output = vinput_command()
        .args(["adapter", "list", "--config"])
        .arg(&path)
        .output()
        .expect("run vinput adapter list text");
    fs::remove_file(&path).expect("remove temporary adapter config");

    let stdout = assert_stdout_success(output, "adapter list text");
    assert!(stdout.contains("source: file"));
    assert!(stdout.contains("adapter_count: 2"));
    assert!(stdout.contains("id	command	args	env	working_dir	extra_fields"));
    assert!(stdout.contains("command-adapter	yes	2	1	yes	0"));
    assert!(stdout.contains("simple-adapter	yes	0	0	no	0"));
    assert!(!stdout.contains("secret-token"));
}

fn write_llm_fixture(prefix: &str) -> std::path::PathBuf {
    write_temp_json(prefix, llm_fixture_json())
}

fn llm_fixture_json() -> &'static str {
    r#"
    {
      "version": 1,
      "asr": {
        "active_provider": "p",
        "providers": [{"id":"p","type":"local"}]
      },
      "llm": {
        "providers": [
          {"id":"openai","base_url":"https://llm.example.test/v1","api_key":"secret-token","model":"gpt-4o-mini","extra_body":{"temperature":0.2}},
          {"id":"local","base_url":"http://127.0.0.1:11434/v1"}
        ],
        "adapters": [
          {"id":"command-adapter","command":"adapter-helper","args":["--token","secret-token"],"env":{"TOKEN":"secret-token"},"working_dir":"/tmp"},
          {"id":"simple-adapter","command":"simple-helper"}
        ]
      },
      "scenes": {
        "active_scene": "raw",
        "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
      }
    }
    "#
}

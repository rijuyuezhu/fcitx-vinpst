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
fn llm_add_dry_run_json_validates_without_writing() {
    let path = write_llm_fixture("vinput-llm-add-dry-run");
    let before = fs::read_to_string(&path).expect("read original llm config");

    let output = vinput_command()
        .args([
            "llm",
            "add",
            "anthropic",
            "--base-url",
            "https://llm.example.test/anthropic",
            "--config",
        ])
        .arg(&path)
        .args([
            "--api-key",
            "secret-token",
            "--model",
            "claude-test",
            "--extra-body",
            r#"{"temperature":0.1}"#,
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("run vinput llm add dry-run");

    let value = assert_json_success(output, "llm add dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["provider_id"], "anthropic");
    assert_eq!(value["before_provider_count"], 2);
    assert_eq!(value["after_provider_count"], 3);
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged llm config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary llm config");
}

#[test]
fn llm_add_text_dry_run_outputs_expected_fields() {
    let path = write_llm_fixture("vinput-llm-add-text-dry-run");

    let output = vinput_command()
        .args([
            "llm",
            "add",
            "anthropic",
            "--base-url",
            "https://llm.example.test/anthropic",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput llm add text dry-run");
    fs::remove_file(&path).expect("remove temporary llm config");

    let stdout = assert_stdout_success(output, "llm add text dry-run");
    assert!(stdout.contains("dry_run: true"));
    assert!(stdout.contains("source: file"));
    assert!(stdout.contains("provider_id: anthropic"));
    assert!(stdout.contains("before_provider_count: 2"));
    assert!(stdout.contains("after_provider_count: 3"));
    assert!(stdout.contains("will_write_config: false"));
    assert!(stdout.contains("wrote_config: false"));
    assert!(!stdout.contains("secret-token"));
}

#[test]
fn llm_add_output_writes_valid_config_without_overwriting_input() {
    let root = unique_temp_dir("vinput-llm-add-output");
    let config_path = root.join("config.json");
    fs::write(&config_path, llm_fixture_json()).expect("write llm config");
    let output_path = root.join("out/llm.json");
    let before = fs::read_to_string(&config_path).expect("read original llm config");

    let output = vinput_command()
        .args([
            "llm",
            "add",
            "anthropic",
            "--base-url",
            "https://llm.example.test/anthropic",
            "--config",
        ])
        .arg(&config_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinput llm add --output");

    let value = assert_json_success(output, "llm add output json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    let providers = read_json(&output_path)["llm"]["providers"]
        .as_array()
        .unwrap()
        .clone();
    assert!(
        providers
            .iter()
            .any(|provider| provider["id"] == "anthropic")
    );
    fs::remove_dir_all(root).expect("remove llm add output fixture dir");
}

#[test]
fn llm_edit_dry_run_json_validates_without_writing() {
    let path = write_llm_fixture("vinput-llm-edit-dry-run");
    let before = fs::read_to_string(&path).expect("read original llm config");

    let output = vinput_command()
        .args(["llm", "edit", "openai", "--config"])
        .arg(&path)
        .args([
            "--base-url",
            "https://new-llm.example.test/v1",
            "--api-key",
            "new-secret-token",
            "--clear-model",
            "--extra-body",
            r#"{"temperature":0.3}"#,
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("run vinput llm edit dry-run");

    let value = assert_json_success(output, "llm edit dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["provider_id"], "openai");
    let changed = value["changed_fields"].as_array().unwrap();
    assert!(changed.iter().any(|field| field == "base_url"));
    assert!(changed.iter().any(|field| field == "api_key"));
    assert!(changed.iter().any(|field| field == "model"));
    assert!(changed.iter().any(|field| field == "extra_body"));
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged llm config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary llm config");
}

#[test]
fn llm_edit_text_dry_run_outputs_expected_fields_without_secrets() {
    let path = write_llm_fixture("vinput-llm-edit-text-dry-run");

    let output = vinput_command()
        .args([
            "llm",
            "edit",
            "openai",
            "--api-key",
            "new-secret-token",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput llm edit text dry-run");
    fs::remove_file(&path).expect("remove temporary llm config");

    let stdout = assert_stdout_success(output, "llm edit text dry-run");
    assert!(stdout.contains("dry_run: true"));
    assert!(stdout.contains("source: file"));
    assert!(stdout.contains("provider_id: openai"));
    assert!(stdout.contains("changed_fields: api_key"));
    assert!(stdout.contains("will_write_config: false"));
    assert!(stdout.contains("wrote_config: false"));
    assert!(!stdout.contains("new-secret-token"));
}

#[test]
fn llm_edit_output_writes_valid_config_without_overwriting_input() {
    let root = unique_temp_dir("vinput-llm-edit-output");
    let config_path = root.join("config.json");
    fs::write(&config_path, llm_fixture_json()).expect("write llm config");
    let output_path = root.join("out/llm.json");
    let before = fs::read_to_string(&config_path).expect("read original llm config");

    let output = vinput_command()
        .args([
            "llm",
            "edit",
            "openai",
            "--base-url",
            "https://new-llm.example.test/v1",
            "--clear-model",
            "--config",
        ])
        .arg(&config_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinput llm edit --output");

    let value = assert_json_success(output, "llm edit output json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    let json = read_json(&output_path);
    assert_eq!(
        json["llm"]["providers"][0]["base_url"],
        "https://new-llm.example.test/v1"
    );
    assert!(
        json["llm"]["providers"][0]
            .as_object()
            .unwrap()
            .get("model")
            .is_none()
    );
    fs::remove_dir_all(root).expect("remove llm edit output fixture dir");
}

#[test]
fn llm_edit_in_place_writes_backup_and_clears_extra_body() {
    let root = unique_temp_dir("vinput-llm-edit-in-place");
    let config_path = root.join("config.json");
    fs::write(&config_path, llm_fixture_json()).expect("write llm config");
    let backup_path = root.join("config.json.bak");
    let before = fs::read_to_string(&config_path).expect("read original llm config");

    let output = vinput_command()
        .args(["llm", "edit", "openai", "--clear-extra-body", "--config"])
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinput llm edit --in-place");

    let value = assert_json_success(output, "llm edit in-place json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&backup_path).expect("read llm edit backup config"),
        before
    );
    let json = read_json(&config_path);
    assert!(
        json["llm"]["providers"][0]
            .as_object()
            .unwrap()
            .get("extra_body")
            .is_none()
    );
    fs::remove_dir_all(root).expect("remove llm edit in-place fixture dir");
}

#[test]
fn llm_edit_rejects_invalid_inputs() {
    let path = write_llm_fixture("vinput-llm-edit-errors");

    let missing = vinput_command()
        .args([
            "llm",
            "edit",
            "missing",
            "--base-url",
            "https://missing.example.test",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput llm edit missing id");
    assert!(!missing.status.success());
    let stderr = String::from_utf8(missing.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("LLM provider `missing` not found"));

    let noop = vinput_command()
        .args(["llm", "edit", "openai", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput llm edit without field changes");
    assert!(!noop.status.success());
    let stderr = String::from_utf8(noop.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("LLM provider edit requires at least one field change"));

    let conflict = vinput_command()
        .args([
            "llm",
            "edit",
            "openai",
            "--api-key",
            "new",
            "--clear-api-key",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput llm edit conflicting api key flags");
    assert!(!conflict.status.success());
    let stderr = String::from_utf8(conflict.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("LLM provider edit cannot combine --api-key and --clear-api-key"));

    let invalid_extra = vinput_command()
        .args(["llm", "edit", "openai", "--extra-body", "[]", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput llm edit invalid extra body");
    assert!(!invalid_extra.status.success());
    let stderr = String::from_utf8(invalid_extra.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("LLM provider --extra-body must be a JSON object"));

    fs::remove_file(&path).expect("remove temporary llm config");
}

#[test]
fn llm_remove_dry_run_json_validates_without_writing() {
    let path = write_llm_fixture("vinput-llm-remove-dry-run");
    let before = fs::read_to_string(&path).expect("read original llm config");

    let output = vinput_command()
        .args(["llm", "remove", "local", "--config"])
        .arg(&path)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinput llm remove dry-run");

    let value = assert_json_success(output, "llm remove dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["removed_provider_id"], "local");
    assert_eq!(value["before_provider_count"], 2);
    assert_eq!(value["after_provider_count"], 1);
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged llm config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary llm config");
}

#[test]
fn llm_remove_in_place_writes_backup() {
    let root = unique_temp_dir("vinput-llm-remove-in-place");
    let config_path = root.join("config.json");
    fs::write(&config_path, llm_fixture_json()).expect("write llm config");
    let backup_path = root.join("config.json.bak");
    let before = fs::read_to_string(&config_path).expect("read original llm config");

    let output = vinput_command()
        .args(["llm", "remove", "local", "--config"])
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinput llm remove --in-place");

    let value = assert_json_success(output, "llm remove in-place json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&backup_path).expect("read llm backup config"),
        before
    );
    let providers = read_json(&config_path)["llm"]["providers"]
        .as_array()
        .unwrap()
        .clone();
    assert!(providers.iter().all(|provider| provider["id"] != "local"));
    fs::remove_dir_all(root).expect("remove llm remove in-place fixture dir");
}

#[test]
fn llm_mutations_reject_invalid_inputs() {
    let path = write_llm_fixture("vinput-llm-mutation-errors");

    let duplicate = vinput_command()
        .args([
            "llm",
            "add",
            "openai",
            "--base-url",
            "https://duplicate.example.test",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput llm add duplicate id");
    assert!(!duplicate.status.success());
    let stderr = String::from_utf8(duplicate.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("LLM provider `openai` already exists"));

    let empty_base_url = vinput_command()
        .args(["llm", "add", "new", "--base-url", "   ", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput llm add empty base url");
    assert!(!empty_base_url.status.success());
    let stderr = String::from_utf8(empty_base_url.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("LLM provider base URL cannot be empty"));

    let invalid_extra_body = vinput_command()
        .args([
            "llm",
            "add",
            "new",
            "--base-url",
            "https://new.example.test",
            "--extra-body",
            "[]",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput llm add invalid extra body");
    assert!(!invalid_extra_body.status.success());
    let stderr = String::from_utf8(invalid_extra_body.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("LLM provider --extra-body must be a JSON object"));

    let missing = vinput_command()
        .args(["llm", "remove", "missing", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput llm remove missing id");
    assert!(!missing.status.success());
    let stderr = String::from_utf8(missing.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("LLM provider `missing` not found"));

    let referenced = vinput_command()
        .args(["llm", "remove", "openai", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput llm remove provider used by scene");
    assert!(!referenced.status.success());
    let stderr = String::from_utf8(referenced.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("references unknown LLM provider `openai`"));

    let missing_target = vinput_command()
        .args([
            "llm",
            "add",
            "new",
            "--base-url",
            "https://new.example.test",
            "--config",
        ])
        .arg(&path)
        .output()
        .expect("run vinput llm add without write target");
    assert!(!missing_target.status.success());
    let stderr = String::from_utf8(missing_target.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("config set writes require --output <path> or --in-place"));

    fs::remove_file(&path).expect("remove temporary llm config");
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

fn read_json(path: &std::path::Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read json file"))
        .expect("parse json file")
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
        "definitions": [{"id":"raw","label":"Raw","provider_id":"openai","candidate_count":0}]
      }
    }
    "#
}

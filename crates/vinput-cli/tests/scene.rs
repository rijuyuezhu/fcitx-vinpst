//! Integration tests for recognition scene listing and selection CLI paths.

mod common;

use std::{fs, path::Path};

use common::{assert_json_success, assert_stdout_success, vinput_command, write_temp_json};

#[test]
fn scene_list_json_reports_bundled_default_active_scene() {
    let output = vinput_command()
        .args(["scene", "list", "--json"])
        .output()
        .expect("run vinput scene list --json");

    let value = assert_json_success(output, "scene list json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["source"], "bundled-default");
    assert_eq!(value["config_path"], serde_json::Value::Null);
    assert_eq!(value["active_scene"], "__raw__");
    assert_eq!(value["scene_count"], 2);

    let scenes = value["scenes"].as_array().unwrap();
    assert_eq!(scenes[0]["id"], "__raw__");
    assert_eq!(scenes[0]["active"], true);
    assert_eq!(scenes[0]["candidate_count"], 0);
    assert_eq!(scenes[1]["id"], "__command__");
    assert_eq!(scenes[1]["prompt_configured"], true);
}

#[test]
fn scene_list_json_reports_scene_metadata() {
    let path = write_scene_fixture("vinput-scene-list");

    let output = vinput_command()
        .args(["scene", "ls", "--config"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run vinput scene ls --json");
    fs::remove_file(&path).expect("remove temporary scene config");

    let value = assert_json_success(output, "scene list json with fixture");
    assert_eq!(value["source"], "file");
    assert_eq!(value["config_path"], path.to_string_lossy().as_ref());
    assert_eq!(value["active_scene"], "rewrite");
    assert_eq!(value["scene_count"], 5);

    let scenes = value["scenes"].as_array().unwrap();
    assert_eq!(scenes[0]["id"], "raw");
    assert_eq!(scenes[0]["active"], false);
    assert_eq!(scenes[0]["prompt_configured"], false);
    assert_eq!(scenes[1]["id"], "rewrite");
    assert_eq!(scenes[1]["active"], true);
    assert_eq!(scenes[1]["provider_id"], "openai");
    assert_eq!(scenes[1]["model"], "gpt-scene");
    assert_eq!(scenes[1]["candidate_count"], 2);
    assert_eq!(scenes[1]["timeout_ms"], 2500);
    assert_eq!(scenes[1]["context_lines"], 4);
}

#[test]
fn scene_list_text_prints_table_and_active_marker() {
    let path = write_scene_fixture("vinput-scene-list-text");

    let output = vinput_command()
        .args(["scene", "list", "--config"])
        .arg(&path)
        .output()
        .expect("run vinput scene list text");
    fs::remove_file(&path).expect("remove temporary scene config");

    let stdout = assert_stdout_success(output, "scene list text");
    assert!(stdout.contains("source: file"));
    assert!(stdout.contains("active_scene: rewrite"));
    assert!(stdout.contains("scene_count: 5"));
    assert!(stdout.contains(
        "active\tid\tlabel\tprompt\tprovider\tmodel\tcandidates\ttimeout_ms\tcontext_lines"
    ));
    assert!(stdout.contains("\traw\tRaw\tno\t-\t-\t0\t-\t0"));
    assert!(stdout.contains("*\trewrite\tRewrite\tyes\topenai\tgpt-scene\t2\t2500\t4"));
}

#[test]
fn scene_use_dry_run_json_validates_existing_scene_without_writing() {
    let path = write_scene_fixture("vinput-scene-use-dry-run");
    let before = fs::read_to_string(&path).expect("read original scene config");

    let output = vinput_command()
        .args(["scene", "use", "raw", "--config"])
        .arg(&path)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinput scene use dry-run");

    let value = assert_json_success(output, "scene use dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["before"], "rewrite");
    assert_eq!(value["after"], "raw");
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged scene config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary scene config");
}

#[test]
fn scene_use_text_dry_run_outputs_expected_fields() {
    let path = write_scene_fixture("vinput-scene-use-text-dry-run");

    let output = vinput_command()
        .args(["scene", "use", "command", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput scene use text dry-run");
    fs::remove_file(&path).expect("remove temporary scene config");

    let stdout = assert_stdout_success(output, "scene use text dry-run");
    assert!(stdout.contains("dry_run: true"));
    assert!(stdout.contains("source: file"));
    assert!(stdout.contains("before: rewrite"));
    assert!(stdout.contains("after: command"));
    assert!(stdout.contains("will_write_config: false"));
    assert!(stdout.contains("wrote_config: false"));
}

#[test]
fn scene_use_output_writes_valid_config_without_overwriting_input() {
    let root = unique_temp_dir("vinput-scene-use-output");
    let config_path = root.join("config.json");
    fs::write(&config_path, scene_fixture_json()).expect("write scene config");
    let output_path = root.join("out/scene.json");
    let before = fs::read_to_string(&config_path).expect("read original scene config");

    let output = vinput_command()
        .args(["scene", "use", "command", "--config"])
        .arg(&config_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinput scene use --output");

    let value = assert_json_success(output, "scene use output json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    assert_eq!(read_json(&output_path)["scenes"]["active_scene"], "command");
    fs::remove_dir_all(root).expect("remove scene use output fixture dir");
}

#[test]
fn scene_use_in_place_writes_backup() {
    let root = unique_temp_dir("vinput-scene-use-in-place");
    let config_path = root.join("config.json");
    fs::write(&config_path, scene_fixture_json()).expect("write scene config");
    let backup_path = root.join("config.json.bak");
    let before = fs::read_to_string(&config_path).expect("read original scene config");

    let output = vinput_command()
        .args(["scene", "use", "raw", "--config"])
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinput scene use --in-place");

    let value = assert_json_success(output, "scene use in-place json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&backup_path).expect("read scene backup config"),
        before
    );
    assert_eq!(read_json(&config_path)["scenes"]["active_scene"], "raw");
    fs::remove_dir_all(root).expect("remove scene use in-place fixture dir");
}

#[test]
fn scene_use_rejects_empty_missing_and_missing_write_target() {
    let path = write_scene_fixture("vinput-scene-use-errors");

    let empty = vinput_command()
        .args(["scene", "use", "   ", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput scene use empty id");
    assert!(!empty.status.success());
    let stderr = String::from_utf8(empty.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("scene id cannot be empty"));

    let missing = vinput_command()
        .args(["scene", "use", "missing", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinput scene use missing id");
    assert!(!missing.status.success());
    let stderr = String::from_utf8(missing.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("scene `missing` not found"));

    let missing_target = vinput_command()
        .args(["scene", "use", "raw", "--config"])
        .arg(&path)
        .output()
        .expect("run vinput scene use without write target");
    assert!(!missing_target.status.success());
    let stderr = String::from_utf8(missing_target.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("config set writes require --output <path> or --in-place"));

    fs::remove_file(&path).expect("remove temporary scene config");
}

fn write_scene_fixture(prefix: &str) -> std::path::PathBuf {
    write_temp_json(prefix, scene_fixture_json())
}

fn scene_fixture_json() -> &'static str {
    r#"
    {
      "version": 1,
      "asr": {
        "active_provider": "p",
        "providers": [{"id":"p","type":"local"}]
      },
      "llm": {
        "providers": [{"id":"openai","base_url":"https://llm.example.test/v1"}]
      },
      "scenes": {
        "active_scene": "rewrite",
        "definitions": [
          {"id":"raw","label":"Raw","candidate_count":0},
          {"id":"rewrite","label":"Rewrite","prompt":"Polish text","provider_id":"openai","model":"gpt-scene","candidate_count":2,"timeout_ms":2500,"context_lines":4},
          {"id":"command","label":"Command","prompt":"Apply command","candidate_count":1}
        ]
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

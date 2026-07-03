//! Integration tests for live registry model list CLI paths.

mod common;

use common::{assert_json_success, assert_stdout_success, vinput_command, workspace_file};

fn live_models_fixture() -> std::path::PathBuf {
    let path = workspace_file("crates/vinput-registry/tests/fixtures/live-models-sensevoice.json");
    assert!(path.exists(), "live model registry fixture should exist");
    path
}

fn live_i18n_fixture() -> std::path::PathBuf {
    let path = workspace_file("crates/vinput-registry/tests/fixtures/live-i18n-zh-cn.json");
    assert!(path.exists(), "live registry i18n fixture should exist");
    path
}

#[test]
fn model_list_json_accepts_live_sensevoice_fixture() {
    let output = vinput_command()
        .args(["model", "list", "--registry"])
        .arg(live_models_fixture())
        .args(["--i18n"])
        .arg(live_i18n_fixture())
        .arg("--json")
        .output()
        .expect("run vinput model list --json");

    let value = assert_json_success(output, "model list json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["source"]["kind"], "file");
    assert_eq!(value["source"]["mirror_count"], 3);
    assert_eq!(value["i18n"]["kind"], "file");
    assert_eq!(value["model_count"], 1);

    let model = &value["models"][0];
    assert_eq!(
        model["id"],
        "model.sherpa-onnx.sense-voice-zh-en-ja-ko-yue-int8"
    );
    assert_eq!(model["short_id"], "onnx-sv-zh-int8-off");
    assert_eq!(model["language"], "zh");
    assert_eq!(model["size_bytes"], 165_675_008);
    assert_eq!(model["backend"], "sherpa-offline");
    assert_eq!(model["family"], "sense_voice");
    assert_eq!(model["runtime"], "offline");
    assert_eq!(model["supported"], true);
    assert_eq!(model["support"], "supported");
    assert_eq!(model["title"], "SenseVoice 五语");
    assert_eq!(
        model["description"],
        "SenseVoice 多语言模型，支持中文、英文、日语、韩语和粤语。"
    );
    assert_eq!(model["url_count"], 3);
    assert_eq!(
        model["sha256"],
        "7305f7905bfcf77fa0b39388a313f3da35c68d971661a65475b56fb2162c8e63"
    );
}

#[test]
fn model_list_text_prints_source_columns_and_support_marker() {
    let output = vinput_command()
        .args(["model", "list", "--registry"])
        .arg(live_models_fixture())
        .args(["--i18n"])
        .arg(live_i18n_fixture())
        .output()
        .expect("run vinput model list");

    let stdout = assert_stdout_success(output, "model list text");
    assert!(stdout.contains("registry_source: file:"));
    assert!(stdout.contains("i18n_source: file:"));
    assert!(stdout.contains("id\tshort_id\tlanguage\tsize\tbackend\tfamily\tsupport\ttitle"));
    assert!(stdout.contains("model.sherpa-onnx.sense-voice-zh-en-ja-ko-yue-int8"));
    assert!(stdout.contains("onnx-sv-zh-int8-off"));
    assert!(stdout.contains("sherpa-offline\tsense_voice\tsupported\tSenseVoice 五语"));
}

#[test]
fn model_list_text_falls_back_to_short_id_without_i18n() {
    let output = vinput_command()
        .args(["model", "list", "--registry"])
        .arg(live_models_fixture())
        .output()
        .expect("run vinput model list without i18n");

    let stdout = assert_stdout_success(output, "model list text without i18n");
    assert!(stdout.contains("i18n_source: none"));
    assert!(stdout.contains("onnx-sv-zh-int8-off"));
    assert!(!stdout.contains("SenseVoice 五语"));
}

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

#[test]
fn model_info_json_accepts_short_id_and_includes_raw_metadata() {
    let output = vinput_command()
        .args(["model", "info", "onnx-sv-zh-int8-off", "--registry"])
        .arg(live_models_fixture())
        .args(["--i18n"])
        .arg(live_i18n_fixture())
        .arg("--json")
        .output()
        .expect("run vinput model info --json");

    let value = assert_json_success(output, "model info json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["source"]["kind"], "file");
    assert_eq!(value["i18n"]["kind"], "file");

    let model = &value["model"];
    assert_eq!(
        model["id"],
        "model.sherpa-onnx.sense-voice-zh-en-ja-ko-yue-int8"
    );
    assert_eq!(model["short_id"], "onnx-sv-zh-int8-off");
    assert_eq!(model["title"], "SenseVoice 五语");
    assert_eq!(model["backend"], "sherpa-offline");
    assert_eq!(model["family"], "sense_voice");
    assert_eq!(model["runtime"], "offline");
    assert_eq!(model["support"], "supported");
    assert_eq!(
        model["vinput_model"]["model"]["sense_voice"]["model"],
        "model.int8.onnx"
    );
    assert_eq!(
        model["vinput_model"]["model"]["sense_voice"]["use_itn"],
        true
    );
}

#[test]
fn model_info_text_accepts_full_id() {
    let output = vinput_command()
        .args([
            "model",
            "info",
            "model.sherpa-onnx.sense-voice-zh-en-ja-ko-yue-int8",
            "--registry",
        ])
        .arg(live_models_fixture())
        .args(["--i18n"])
        .arg(live_i18n_fixture())
        .output()
        .expect("run vinput model info text");

    let stdout = assert_stdout_success(output, "model info text");
    assert!(stdout.contains("registry_source: file:"));
    assert!(stdout.contains("id: model.sherpa-onnx.sense-voice-zh-en-ja-ko-yue-int8"));
    assert!(stdout.contains("short_id: onnx-sv-zh-int8-off"));
    assert!(stdout.contains("title: SenseVoice 五语"));
    assert!(stdout.contains("backend: sherpa-offline"));
    assert!(stdout.contains("family: sense_voice"));
    assert!(stdout.contains("support: supported"));
    assert!(stdout.contains("urls:"));
}

#[test]
fn model_info_rejects_unknown_id_or_short_id() {
    let output = vinput_command()
        .args(["model", "info", "missing-model", "--registry"])
        .arg(live_models_fixture())
        .output()
        .expect("run vinput model info unknown id");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("unknown model id or short_id `missing-model`"));
}

#[test]
fn model_install_dry_run_json_plans_target_and_archive_without_mutation() {
    let output = vinput_command()
        .args(["model", "install", "onnx-sv-zh-int8-off", "--registry"])
        .arg(live_models_fixture())
        .args(["--i18n"])
        .arg(live_i18n_fixture())
        .args(["--model-root", "/tmp/vinput-models"])
        .args(["--staging-root", "/tmp/vinput-stage"])
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinput model install --dry-run --json");

    let value = assert_json_success(output, "model install dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["will_download"], false);
    assert_eq!(value["will_extract"], false);
    assert_eq!(value["will_write_config"], false);
    assert_eq!(value["model"]["short_id"], "onnx-sv-zh-int8-off");
    assert_eq!(
        value["archive"]["file_name"],
        "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09.tar.bz2"
    );
    assert_eq!(value["archive"]["format"], "tar_bz2");
    assert_eq!(value["archive"]["supported"], false);
    assert_eq!(value["archive"]["supported_formats"][0], "tar");
    assert_eq!(value["archive"]["size_bytes"], 165_675_008);
    assert_eq!(
        value["target"]["model_dir"],
        "/tmp/vinput-models/onnx-sv-zh-int8-off"
    );
    assert_eq!(
        value["target"]["metadata_path"],
        "/tmp/vinput-models/onnx-sv-zh-int8-off/vinput-model.json"
    );
    assert_eq!(
        value["staging"]["archive_path"],
        "/tmp/vinput-stage/onnx-sv-zh-int8-off/archives/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09.tar.bz2"
    );
}

#[test]
fn model_install_dry_run_text_reports_no_side_effects() {
    let output = vinput_command()
        .args(["model", "install", "onnx-sv-zh-int8-off", "--registry"])
        .arg(live_models_fixture())
        .args(["--model-root", "/tmp/vinput-models"])
        .args(["--staging-root", "/tmp/vinput-stage"])
        .arg("--dry-run")
        .output()
        .expect("run vinput model install --dry-run");

    let stdout = assert_stdout_success(output, "model install dry-run text");
    assert!(stdout.contains("dry_run: true"));
    assert!(stdout.contains("target_model_dir: /tmp/vinput-models/onnx-sv-zh-int8-off"));
    assert!(stdout.contains("archive_format: tar_bz2"));
    assert!(stdout.contains("archive_supported: false"));
    assert!(stdout.contains("will_download: false"));
    assert!(stdout.contains("will_extract: false"));
    assert!(stdout.contains("will_write_config: false"));
}

#[test]
fn model_install_without_dry_run_is_rejected_until_real_install_exists() {
    let output = vinput_command()
        .args(["model", "install", "onnx-sv-zh-int8-off", "--registry"])
        .arg(live_models_fixture())
        .output()
        .expect("run vinput model install without dry-run");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains(
        "real model install is not implemented yet; rerun with --dry-run to inspect the install plan"
    ));
}

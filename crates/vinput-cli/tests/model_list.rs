//! Integration tests for live registry model list CLI paths.

mod common;

use common::{
    assert_json_success, assert_stdout_success, vinput_command, workspace_file, write_temp_json,
};

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
fn model_ls_available_alias_matches_live_registry_list() {
    let output = vinput_command()
        .args(["model", "ls", "--available", "--registry"])
        .arg(live_models_fixture())
        .args(["--i18n"])
        .arg(live_i18n_fixture())
        .arg("--json")
        .output()
        .expect("run vinput model ls --available --json");

    let value = assert_json_success(output, "model ls available json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["model_count"], 1);
    assert_eq!(value["models"][0]["short_id"], "onnx-sv-zh-int8-off");
    assert_eq!(value["models"][0]["title"], "SenseVoice 五语");
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
    assert_eq!(value["archive"]["supported"], true);
    assert_eq!(value["archive"]["supported_formats"][0], "tar");
    assert_eq!(value["archive"]["supported_formats"][2], "tar_bz2");
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
    assert!(stdout.contains("archive_supported: true"));
    assert!(stdout.contains("will_download: false"));
    assert!(stdout.contains("will_extract: false"));
    assert!(stdout.contains("will_write_config: false"));
}

#[test]
fn model_add_alias_matches_install_dry_run() {
    let output = vinput_command()
        .args(["model", "add", "onnx-sv-zh-int8-off", "--registry"])
        .arg(live_models_fixture())
        .args(["--model-root", "/tmp/vinput-models"])
        .args(["--staging-root", "/tmp/vinput-stage"])
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinput model add --dry-run --json");

    let value = assert_json_success(output, "model add alias dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["model"]["short_id"], "onnx-sv-zh-int8-off");
    assert_eq!(value["archive"]["format"], "tar_bz2");
    assert_eq!(value["will_write_config"], false);
}

#[test]
fn model_install_without_dry_run_downloads_local_archive_without_config_mutation() {
    let temp_root = unique_temp_dir("vinput-cli-model-install");
    std::fs::create_dir_all(&temp_root).expect("create temp root");

    let archive =
        build_test_tar_archive(&[("model.int8.onnx", b"onnx"), ("tokens.txt", b"tokens")]);
    let archive_sha256 = vinput_registry::sha256_hex(&archive);
    let (url, handle) = serve_single_binary_response(archive);
    let registry_path = write_temp_json(
        "live-model-install",
        &serde_json::json!({
            "version": 2,
            "items": [
                {
                    "id": "model.test.install",
                    "short_id": "test-install",
                    "urls": [url],
                    "sha256": archive_sha256,
                    "size_bytes": 123,
                    "language": "zh",
                    "vinput_model": {
                        "backend": "sherpa-offline",
                        "family": "sense_voice",
                        "language": "zh",
                        "runtime": "offline",
                        "size_bytes": 123,
                        "supports_hotwords": false,
                        "model": {
                            "tokens": "tokens.txt",
                            "sense_voice": {
                                "model": "model.int8.onnx",
                                "language": "zh",
                                "use_itn": true
                            }
                        }
                    }
                }
            ]
        })
        .to_string(),
    );
    let model_root = temp_root.join("models");
    let staging_root = temp_root.join("stage");

    let output = vinput_command()
        .args(["model", "install", "test-install", "--registry"])
        .arg(&registry_path)
        .arg("--model-root")
        .arg(&model_root)
        .arg("--staging-root")
        .arg(&staging_root)
        .arg("--json")
        .output()
        .expect("run vinput model install");

    let value = assert_json_success(output, "model install json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], false);
    assert_eq!(value["will_write_config"], false);
    assert_eq!(value["install"]["checksum_verified"], true);
    assert_eq!(value["install"]["file_count"], 2);
    assert_eq!(
        value["install"]["model_dir"],
        model_root.join("test-install").to_string_lossy().as_ref()
    );
    assert_eq!(
        std::fs::read_to_string(model_root.join("test-install/model.int8.onnx")).unwrap(),
        "onnx"
    );
    assert_eq!(
        std::fs::read_to_string(model_root.join("test-install/tokens.txt")).unwrap(),
        "tokens"
    );
    let metadata: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(model_root.join("test-install/vinput-model.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(metadata["backend"], "sherpa-offline");
    assert_eq!(metadata["model"]["sense_voice"]["model"], "model.int8.onnx");

    let request = handle.join().expect("HTTP thread should finish");
    assert!(request.starts_with("GET /model.tar HTTP/1.1"));
    let _ = std::fs::remove_file(registry_path);
    let _ = std::fs::remove_dir_all(temp_root);
}

fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos()
    ));
    path
}

fn serve_single_binary_response(bytes: Vec<u8>) -> (String, std::thread::JoinHandle<String>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind test HTTP server");
    let url = format!("http://{}/model.tar", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept HTTP request");
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        loop {
            let read = std::io::Read::read(&mut stream, &mut buffer).expect("read HTTP request");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        let response_header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            bytes.len()
        );
        std::io::Write::write_all(&mut stream, response_header.as_bytes())
            .expect("write HTTP response header");
        std::io::Write::write_all(&mut stream, &bytes).expect("write HTTP response body");
        String::from_utf8_lossy(&request).into_owned()
    });
    (url, handle)
}

fn build_test_tar_archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut output = Vec::new();
    for (path, data) in entries {
        write_raw_tar_file(&mut output, path, data);
    }
    output.extend_from_slice(&[0_u8; 1024]);
    output
}

fn write_raw_tar_file(output: &mut Vec<u8>, path: &str, data: &[u8]) {
    assert!(path.len() <= 100, "test tar path is too long");
    let mut header = [0_u8; 512];
    header[..path.len()].copy_from_slice(path.as_bytes());
    write_tar_octal(&mut header[100..108], 0o644);
    write_tar_octal(&mut header[108..116], 0);
    write_tar_octal(&mut header[116..124], 0);
    write_tar_octal(&mut header[124..136], data.len() as u64);
    write_tar_octal(&mut header[136..148], 0);
    header[148..156].fill(b' ');
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let checksum = header.iter().map(|byte| u32::from(*byte)).sum::<u32>();
    let checksum_text = format!("{checksum:06o}\0 ");
    header[148..156].copy_from_slice(checksum_text.as_bytes());
    output.extend_from_slice(&header);
    output.extend_from_slice(data);
    let padding = (512 - (data.len() % 512)) % 512;
    output.extend(std::iter::repeat_n(0_u8, padding));
}

fn write_tar_octal(field: &mut [u8], value: u64) {
    let width = field.len() - 1;
    let text = format!("{value:0width$o}\0");
    field.copy_from_slice(text.as_bytes());
}

#[test]
fn model_use_dry_run_json_previews_config_patch_for_registry_model() {
    let output = vinput_command()
        .args(["model", "use", "onnx-sv-zh-int8-off", "--registry"])
        .arg(live_models_fixture())
        .args(["--i18n"])
        .arg(live_i18n_fixture())
        .args(["--model-root", "/tmp/vinput-models"])
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinput model use --dry-run --json");

    let value = assert_json_success(output, "model use dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["will_write_config"], false);
    assert_eq!(value["selector"]["kind"], "registry");
    assert_eq!(
        value["selector"]["resolved_short_id"],
        "onnx-sv-zh-int8-off"
    );
    assert_eq!(value["selector"]["title"], "SenseVoice 五语");
    assert_eq!(
        value["patch"]["asr.active_provider"]["before"],
        "sherpa-onnx"
    );
    assert_eq!(
        value["patch"]["asr.active_provider"]["after"],
        "sherpa-onnx"
    );
    assert_eq!(
        value["patch"]["asr.providers[].model"]["provider_id"],
        "sherpa-onnx"
    );
    assert_eq!(
        value["patch"]["asr.providers[].model"]["provider_type"],
        "local"
    );
    assert_eq!(
        value["patch"]["asr.providers[].model"]["before"],
        serde_json::Value::Null
    );
    assert_eq!(
        value["patch"]["asr.providers[].model"]["after"],
        "/tmp/vinput-models/onnx-sv-zh-int8-off"
    );
}

#[test]
fn model_use_dry_run_text_accepts_installed_path_without_registry() {
    let output = vinput_command()
        .args(["model", "use", "/tmp/vinput-models/custom", "--dry-run"])
        .output()
        .expect("run vinput model use path --dry-run");

    let stdout = assert_stdout_success(output, "model use path dry-run text");
    assert!(stdout.contains("dry_run: true"));
    assert!(stdout.contains("selector_kind: path"));
    assert!(stdout.contains("provider_id: sherpa-onnx"));
    assert!(stdout.contains("model_before: -"));
    assert!(stdout.contains("model_after: /tmp/vinput-models/custom"));
    assert!(stdout.contains("will_write_config: false"));
}

#[test]
fn model_use_without_dry_run_is_rejected_until_config_mutation_exists() {
    let output = vinput_command()
        .args(["model", "use", "/tmp/vinput-models/custom"])
        .output()
        .expect("run vinput model use without dry-run");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains(
        "real model use is not implemented yet; rerun with --dry-run to inspect the config patch"
    ));
}

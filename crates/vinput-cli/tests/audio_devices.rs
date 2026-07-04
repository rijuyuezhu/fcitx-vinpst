//! Integration tests for audio device diagnostics CLI paths.

mod common;

use std::fs;

use common::{assert_json_success, vinput_command, write_temp_json};

#[test]
fn audio_devices_reports_default_capture_target_and_backend() {
    let output = vinput_command()
        .arg("audio-devices")
        .output()
        .expect("run vinput audio-devices");

    let value = assert_json_success(output, "audio devices summary");
    assert_eq!(value["ok"], true);
    assert_eq!(value["capture_device"], "default");
    assert_eq!(value["capture_target"]["kind"], "default");
    assert_eq!(
        value["capture_target"]["target_object"],
        serde_json::Value::Null
    );
    assert_eq!(
        value["backend"],
        if cfg!(feature = "pipewire-backend") {
            "pipewire"
        } else {
            "unavailable"
        }
    );
    assert!(value["live"].is_boolean());
    let devices = value["devices"].as_array().unwrap();
    if value["live"] == true {
        assert_eq!(value["enumeration_error"], serde_json::Value::Null);
    } else {
        assert_eq!(devices.len(), 0);
    }
    if cfg!(feature = "pipewire-backend") {
        assert!(value["enumeration_error"].is_null() || value["enumeration_error"].is_string());
    } else {
        assert_eq!(value["enumeration_error"], serde_json::Value::Null);
    }
}

#[test]
fn audio_devices_preserves_configured_capture_target_object() {
    let path = write_temp_json(
        "vinput-audio-devices",
        r#"
        {
          "version": 1,
          "global": {"capture_device": "  alsa_input.usb-mic  "},
          "asr": {
            "active_provider": "p",
            "providers": [{"id":"p","type":"local"}]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    );

    let output = vinput_command()
        .args(["audio-devices", "--config"])
        .arg(&path)
        .output()
        .expect("run vinput audio-devices with config");
    fs::remove_file(&path).expect("remove temporary config fixture");

    let value = assert_json_success(output, "audio devices summary");
    assert_eq!(value["capture_device"], "  alsa_input.usb-mic  ");
    assert_eq!(value["capture_target"]["kind"], "object");
    assert_eq!(
        value["capture_target"]["target_object"],
        "alsa_input.usb-mic"
    );
}

#[cfg(feature = "pipewire-backend")]
#[test]
fn audio_devices_reports_pipewire_enumeration_error_without_failing() {
    let config_dir = std::env::temp_dir().join(format!(
        "vinput-missing-pipewire-config-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    fs::create_dir(&config_dir).expect("create empty PipeWire config dir");

    let output = vinput_command()
        .env("PIPEWIRE_CONFIG_DIR", &config_dir)
        .env("XDG_CONFIG_HOME", &config_dir)
        .env("XDG_DATA_DIRS", &config_dir)
        .arg("audio-devices")
        .output()
        .expect("run vinput audio-devices without PipeWire client config");
    fs::remove_dir(&config_dir).expect("remove empty PipeWire config dir");

    let value = assert_json_success(output, "audio devices summary without PipeWire config");
    assert_eq!(value["ok"], true);
    assert_eq!(value["backend"], "pipewire");
    assert_eq!(value["live"], false);
    assert_eq!(value["devices"].as_array().unwrap().len(), 0);
    assert!(
        value["enumeration_error"]
            .as_str()
            .is_some_and(|message| message.contains("enumerate PipeWire audio sources"))
    );
}

#[test]
fn doctor_reports_combined_local_diagnostics() {
    let data_home = std::env::temp_dir().join(format!(
        "vinput-doctor-data-home-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    let addon_lib_dir = data_home.join("lib/fcitx5");

    let output = vinput_command()
        .env("XDG_DATA_HOME", &data_home)
        .env("VINPUT_USER_FCITX_LIB_DIR", &addon_lib_dir)
        .arg("doctor")
        .output()
        .expect("run vinput doctor");

    let value = assert_json_success(output, "doctor summary");
    assert_eq!(value["ok"], true);
    assert_eq!(value["config"]["ok"], true);
    assert_eq!(value["asr"]["target_provider_id"], "sherpa-onnx");
    assert_eq!(value["audio"]["ok"], true);
    assert_eq!(value["audio"]["capture_target"]["kind"], "default");
    assert_eq!(
        value["activation_service"]["user_service_path"],
        data_home
            .join("dbus-1")
            .join("services")
            .join("org.fcitx.Vinput.service")
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(value["activation_service"]["user_service_exists"], false);
    assert!(
        value["activation_service"]["next_steps"]
            .as_array()
            .expect("doctor activation service next steps")
            .iter()
            .any(|step| step
                .as_str()
                .is_some_and(|step| step.contains("daemon owner/procfs probes")))
    );
    assert_eq!(
        value["fcitx_addon"]["user_module_path"],
        addon_lib_dir
            .join("fcitx5-vinput.so")
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(value["fcitx_addon"]["user_module_exists"], false);
    assert_eq!(
        value["daemon_owner_probe"]["target_name"],
        vinput_protocol::dbus::SERVICE_BUS_NAME
    );
    assert!(
        value["daemon_owner_probe"]["process_fields"]
            .as_array()
            .expect("doctor daemon owner probe fields")
            .contains(&serde_json::json!("cmdline"))
    );
    assert_eq!(
        value["fcitx_addon"]["user_addon_metadata_path"],
        data_home
            .join("fcitx5")
            .join("addon")
            .join("vinput.conf")
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(value["fcitx_addon"]["user_addon_metadata_exists"], false);
    let next_steps = value["next_steps"].as_array().expect("doctor next steps");
    let next_steps_text = next_steps
        .iter()
        .map(|step| step.as_str().expect("doctor next step string"))
        .collect::<Vec<_>>()
        .join(
            "
",
        );
    assert!(next_steps_text.contains("vinput provider list"));
    assert!(next_steps_text.contains("vinput provider use sherpa-onnx"));
    assert!(next_steps_text.contains("vinput hotword get"));
    assert!(next_steps_text.contains("vinput device list"));
    assert!(next_steps_text.contains("vinput device use <target>"));
    assert!(next_steps_text.contains("daemon D-Bus owner/procfs probes"));
}

#[test]
fn doctor_reports_existing_user_activation_exec_line() {
    let data_home = std::env::temp_dir().join(format!(
        "vinput-doctor-service-home-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    let service_path = data_home
        .join("dbus-1")
        .join("services")
        .join("org.fcitx.Vinput.service");
    std::fs::create_dir_all(service_path.parent().unwrap()).expect("create service dir");
    std::fs::write(
        &service_path,
        "[D-BUS Service]\nName=org.fcitx.Vinput\nExec=/tmp/vinput-daemon --dbus --audio-backend pipewire\n",
    )
    .expect("write user activation service");

    let output = vinput_command()
        .env("XDG_DATA_HOME", &data_home)
        .arg("doctor")
        .output()
        .expect("run vinput doctor");

    let value = assert_json_success(output, "doctor summary with user service");
    assert_eq!(value["activation_service"]["user_service_exists"], true);
    assert_eq!(
        value["activation_service"]["user_service_name"],
        "org.fcitx.Vinput"
    );
    assert_eq!(
        value["activation_service"]["user_service_name_matches"],
        true
    );
    assert_eq!(
        value["activation_service"]["user_service_exec"],
        "/tmp/vinput-daemon --dbus --audio-backend pipewire"
    );
    assert_eq!(
        value["activation_service"]["read_error"],
        serde_json::Value::Null
    );
    assert!(
        value["activation_service"]["next_steps"]
            .as_array()
            .expect("activation service next steps")
            .iter()
            .any(|step| step
                .as_str()
                .is_some_and(|step| step.contains("daemon start --dry-run")))
    );
    std::fs::remove_dir_all(data_home).expect("remove service fixture");
}

#[test]
fn doctor_reports_existing_user_fcitx_addon_files() {
    let data_home = std::env::temp_dir().join(format!(
        "vinput-doctor-addon-home-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    let addon_lib_dir = data_home.join("lib/fcitx5");
    let module_path = addon_lib_dir.join("fcitx5-vinput.so");
    let metadata_path = data_home.join("fcitx5").join("addon").join("vinput.conf");
    std::fs::create_dir_all(&addon_lib_dir).expect("create module dir");
    std::fs::create_dir_all(metadata_path.parent().unwrap()).expect("create metadata dir");
    std::fs::write(&module_path, "fake module").expect("write fake addon module");
    std::fs::write(
        &metadata_path,
        "[Addon]\nLibrary=fcitx5-vinput\nType=SharedLibrary\n",
    )
    .expect("write addon metadata");

    let output = vinput_command()
        .env("XDG_DATA_HOME", &data_home)
        .env("VINPUT_USER_FCITX_LIB_DIR", &addon_lib_dir)
        .arg("doctor")
        .output()
        .expect("run vinput doctor");

    let value = assert_json_success(output, "doctor summary with user addon");
    assert_eq!(value["fcitx_addon"]["user_module_exists"], true);
    assert_eq!(value["fcitx_addon"]["user_addon_metadata_exists"], true);
    assert_eq!(value["fcitx_addon"]["user_addon_library"], "fcitx5-vinput");
    assert_eq!(value["fcitx_addon"]["user_addon_library_matches"], true);
    assert_eq!(value["fcitx_addon"]["user_addon_type"], "SharedLibrary");
    assert_eq!(value["fcitx_addon"]["read_error"], serde_json::Value::Null);
    std::fs::remove_dir_all(data_home).expect("remove addon fixture");
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

fn copy_default_config(root: &std::path::Path) -> std::path::PathBuf {
    let config_path = root.join("config.json");
    fs::copy(
        common::workspace_file("data/default-config.json"),
        &config_path,
    )
    .expect("copy default config");
    config_path
}

#[test]
fn device_list_json_reports_config_source_and_audio_summary() {
    let root = unique_temp_dir("vinput-device-list-json");
    let config_path = copy_default_config(&root);

    let output = vinput_command()
        .args(["device", "list", "--config"])
        .arg(&config_path)
        .arg("--json")
        .output()
        .expect("run vinput device list --json");

    let value = assert_json_success(output, "device list json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["config_path"], config_path.to_string_lossy().as_ref());
    assert_eq!(value["audio"]["ok"], true);
    assert_eq!(value["audio"]["capture_device"], "default");
    assert_eq!(value["audio"]["capture_target"]["kind"], "default");
}

#[test]
fn device_list_text_includes_default_target() {
    let output = vinput_command()
        .args(["device", "list"])
        .output()
        .expect("run vinput device list text");

    let stdout = common::assert_stdout_success(output, "device list text");
    assert!(stdout.contains("source: bundled-default"));
    assert!(stdout.contains("capture_device: default"));
    assert!(stdout.contains("target\tid\tname\tdescription"));
    assert!(stdout.contains("default\t-\tdefault\tDefault capture source"));
}

#[test]
fn device_use_dry_run_json_validates_without_writing() {
    let root = unique_temp_dir("vinput-device-use-dry-run");
    let config_path = copy_default_config(&root);
    let before = fs::read_to_string(&config_path).expect("read original config");

    let output = vinput_command()
        .args(["device", "use", "alsa_input.usb-mic", "--config"])
        .arg(&config_path)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinput device use dry-run");

    let value = assert_json_success(output, "device use dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["before"], "default");
    assert_eq!(value["after"], "alsa_input.usb-mic");
    assert_eq!(value["capture_target"]["kind"], "object");
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&config_path).expect("read unchanged config"),
        before
    );
}

#[test]
fn device_use_output_writes_valid_config_without_overwriting_input() {
    let root = unique_temp_dir("vinput-device-use-output");
    let config_path = copy_default_config(&root);
    let output_path = root.join("out/device.json");
    let before = fs::read_to_string(&config_path).expect("read original config");

    let output = vinput_command()
        .args(["device", "use", "alsa_input.output-mic", "--config"])
        .arg(&config_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinput device use --output");

    let value = assert_json_success(output, "device use output json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    assert_eq!(
        read_json(&output_path)["global"]["capture_device"],
        "alsa_input.output-mic"
    );
}

#[test]
fn device_use_in_place_writes_backup() {
    let root = unique_temp_dir("vinput-device-use-in-place");
    let config_path = copy_default_config(&root);
    let backup_path = root.join("config.json.bak");
    let before = fs::read_to_string(&config_path).expect("read original config");

    let output = vinput_command()
        .args(["device", "use", "default", "--config"])
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinput device use --in-place");

    let value = assert_json_success(output, "device use in-place json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&backup_path).expect("read backup config"),
        before
    );
    assert_eq!(
        read_json(&config_path)["global"]["capture_device"],
        "default"
    );
}

#[test]
fn device_use_rejects_empty_target_and_missing_write_target() {
    let root = unique_temp_dir("vinput-device-use-errors");
    let config_path = copy_default_config(&root);

    let empty = vinput_command()
        .args(["device", "use", "   ", "--config"])
        .arg(&config_path)
        .arg("--dry-run")
        .output()
        .expect("run vinput device use empty target");
    assert!(!empty.status.success());
    let stderr = String::from_utf8(empty.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("capture device cannot be empty"));

    let missing_target = vinput_command()
        .args(["device", "use", "alsa_input.usb-mic", "--config"])
        .arg(&config_path)
        .output()
        .expect("run vinput device use without write target");
    assert!(!missing_target.status.success());
    let stderr = String::from_utf8(missing_target.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("config set writes require --output <path> or --in-place"));
}

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
    assert_eq!(
        value["fcitx_addon"]["user_module_path"],
        addon_lib_dir
            .join("fcitx5-vinput.so")
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(value["fcitx_addon"]["user_module_exists"], false);
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

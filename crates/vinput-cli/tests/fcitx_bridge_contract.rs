//! Regression tests for the retained C++ Fcitx5 bridge D-Bus contract.

mod common;

use common::workspace_file;
use vinput_protocol::{ServiceStatus, dbus};

fn cpp_constant(header: &str, name: &str) -> String {
    let needle = format!("{name} =");
    let start = header
        .find(&needle)
        .unwrap_or_else(|| panic!("C++ bridge contract should define {name}"));
    let suffix = &header[start + needle.len()..];
    let first_quote = suffix
        .find('"')
        .unwrap_or_else(|| panic!("C++ bridge contract constant {name} should be a string"));
    let value = &suffix[first_quote + 1..];
    let second_quote = value
        .find('"')
        .unwrap_or_else(|| panic!("C++ bridge contract constant {name} should terminate"));
    value[..second_quote].to_owned()
}

#[test]
fn cpp_bridge_dbus_contract_matches_rust_protocol() {
    let header = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/include/vinput_fcitx_bridge/dbus_contract.h",
    ))
    .expect("read C++ bridge D-Bus contract header");

    for (name, expected) in [
        ("kFcitxBusName", dbus::FCITX_BUS_NAME),
        ("kServiceBusName", dbus::SERVICE_BUS_NAME),
        ("kServiceObjectPath", dbus::SERVICE_OBJECT_PATH),
        ("kServiceInterface", dbus::SERVICE_INTERFACE),
        (
            "kFrontendNotifierObjectPath",
            dbus::FRONTEND_NOTIFIER_OBJECT_PATH,
        ),
        (
            "kFrontendNotifierInterface",
            dbus::FRONTEND_NOTIFIER_INTERFACE,
        ),
        ("kMethodStartRecording", dbus::method::START_RECORDING),
        (
            "kMethodStartCommandRecording",
            dbus::method::START_COMMAND_RECORDING,
        ),
        ("kMethodStopRecording", dbus::method::STOP_RECORDING),
        ("kMethodGetStatus", dbus::method::GET_STATUS),
        (
            "kMethodGetAsrBackendState",
            dbus::method::GET_ASR_BACKEND_STATE,
        ),
        ("kMethodReloadAsrBackend", dbus::method::RELOAD_ASR_BACKEND),
        ("kMethodStartAdapter", dbus::method::START_ADAPTER),
        ("kMethodStopAdapter", dbus::method::STOP_ADAPTER),
        ("kMethodNotify", dbus::method::NOTIFY),
        ("kSignalRecognitionResult", dbus::signal::RECOGNITION_RESULT),
        (
            "kSignalRecognitionPartial",
            dbus::signal::RECOGNITION_PARTIAL,
        ),
        ("kSignalStatusChanged", dbus::signal::STATUS_CHANGED),
        (
            "kSignalDaemonNotification",
            dbus::signal::DAEMON_NOTIFICATION,
        ),
        ("kErrorOperationFailed", dbus::error::OPERATION_FAILED),
        ("kStatusIdle", ServiceStatus::Idle.as_wire_str()),
        ("kStatusRecording", ServiceStatus::Recording.as_wire_str()),
        ("kStatusInferring", ServiceStatus::Inferring.as_wire_str()),
        (
            "kStatusPostprocessing",
            ServiceStatus::Postprocessing.as_wire_str(),
        ),
        ("kStatusError", ServiceStatus::Error.as_wire_str()),
    ] {
        assert_eq!(cpp_constant(&header, name), expected, "{name} should match");
    }
}

#[test]
fn cpp_frontend_hotkey_config_remains_persistent_and_configurable() {
    let addon_metadata =
        std::fs::read_to_string(workspace_file("cpp/fcitx5-addon/vinput-addon.conf.in"))
            .expect("read addon metadata");
    assert!(addon_metadata.contains("Configurable=True"));

    let config_header = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/include/vinput_fcitx_bridge/fcitx_config.h",
    ))
    .expect("read frontend config header");
    let config_source =
        std::fs::read_to_string(workspace_file("cpp/fcitx5-addon/src/fcitx_config.cpp"))
            .expect("read frontend config source");
    let trigger_mode_header = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/include/vinput_fcitx_bridge/fcitx_trigger_mode.h",
    ))
    .expect("read trigger mode header");
    let addon_header = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/include/vinput_fcitx_bridge/fcitx_addon.h",
    ))
    .expect("read addon header");

    for required in [
        "conf/vinput.conf",
        "fcitx::KeyList normal_triggers",
        "fcitx::KeyList command_triggers",
        "fcitx::KeyList scene_menu_triggers",
        "fcitx::KeyList asr_menu_triggers",
        "enum class TriggerMode : std::uint8_t",
        "TriggerMode trigger_mode",
    ] {
        assert!(
            config_header.contains(required),
            "frontend config header should pin {required}"
        );
    }
    for required in [
        "\"TriggerKey\"",
        "\"CommandKeys\"",
        "\"SceneMenuKey\"",
        "\"AsrMenuKey\"",
        "\"TriggerMode\"",
        "safeSaveAsIni",
        "readAsIni",
    ] {
        assert!(
            config_source.contains(required),
            "frontend config source should pin {required}"
        );
    }
    for required in [
        "kTriggerDebounce = std::chrono::milliseconds(80)",
        "kTriggerHoldThreshold = std::chrono::milliseconds(300)",
        "kTriggerReleaseTail = std::chrono::milliseconds(500)",
        "class TriggerModeController",
        "ScheduleNormalStart",
        "ScheduleStop",
    ] {
        assert!(
            trigger_mode_header.contains(required),
            "trigger mode header should pin {required}"
        );
    }
    for required in [
        "void reloadConfig() override",
        "void save() override",
        "const fcitx::Configuration *getConfig() const override",
        "void setConfig(const fcitx::RawConfig &config) override",
    ] {
        assert!(
            addon_header.contains(required),
            "addon config API should pin {required}"
        );
    }
}

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
        (
            "kMethodGetAsrDisplayMenuState",
            dbus::method::GET_ASR_DISPLAY_MENU_STATE,
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
    let addon_header = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/include/vinput_fcitx_bridge/fcitx_addon.h",
    ))
    .expect("read addon header");
    let addon_source =
        std::fs::read_to_string(workspace_file("cpp/fcitx5-addon/src/fcitx_addon.cpp"))
            .expect("read addon source");

    for required in [
        "conf/vinput.conf",
        "fcitx::KeyList normal_triggers",
        "fcitx::KeyList command_triggers",
        "fcitx::KeyList scene_menu_triggers",
        "fcitx::KeyList asr_menu_triggers",
        "fcitx::KeyList page_prev_keys",
        "fcitx::KeyList page_next_keys",
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
        "\"PagePrevKeys\"",
        "\"PageNextKeys\"",
        "\"TriggerMode\"",
        "safeSaveAsIni",
        "readAsIni",
    ] {
        assert!(
            config_source.contains(required),
            "frontend config source should pin {required}"
        );
    }
    assert_eq!(
        addon_source
            .matches("key.checkKeyList(frontend_settings_.page_prev_keys)")
            .count(),
        2,
        "scene and ASR menus should both use configured previous-page keys"
    );
    assert_eq!(
        addon_source
            .matches("key.checkKeyList(frontend_settings_.page_next_keys)")
            .count(),
        2,
        "scene and ASR menus should both use configured next-page keys"
    );
    assert!(!addon_source.contains("IsKey(key, FcitxKey_Page_Up)"));
    assert!(!addon_source.contains("IsKey(key, FcitxKey_Page_Down)"));

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

#[test]
fn cpp_frontend_trigger_mode_keeps_legacy_timing_contract() {
    let header = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/include/vinput_fcitx_bridge/fcitx_trigger_mode.h",
    ))
    .expect("read trigger mode header");
    for required in [
        "kTriggerDebounce = std::chrono::milliseconds(80)",
        "kTriggerHoldThreshold = std::chrono::milliseconds(300)",
        "kTriggerReleaseTail = std::chrono::milliseconds(500)",
        "class TriggerModeController",
        "ScheduleNormalStart",
        "ScheduleStop",
    ] {
        assert!(
            header.contains(required),
            "trigger mode header should pin {required}"
        );
    }
}

#[test]
fn cpp_frontend_menus_keep_legacy_search_contract() {
    let filter_header = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/include/vinput_fcitx_bridge/fcitx_menu_filter.h",
    ))
    .expect("read menu filter header");
    let addon_source =
        std::fs::read_to_string(workspace_file("cpp/fcitx5-addon/src/fcitx_addon.cpp"))
            .expect("read addon source");

    for required in [
        "class MenuFilterState",
        "void Backspace()",
        "void DeleteLastWord()",
        "bool Matches(std::string_view search_text) const",
        "IsPrintableMenuInput",
        "MenuKeyToUtf8",
    ] {
        assert!(
            filter_header.contains(required),
            "menu filter header should pin {required}"
        );
    }
    for required in [
        "void FcitxVinputAddon::RebuildSceneMenu()",
        "void FcitxVinputAddon::RebuildAsrMenu()",
        r#"scene_menu_filter_.Matches(scene.label + " " + scene.id)"#,
        "const auto search_text = label +",
        "target.provider_id",
        "target.kind",
        "target.item_id",
        "target.model_value",
        "target.display_title",
        "GetAsrDisplayMenuState",
        "scene_menu_filter_.AppendText(MenuKeyToUtf8(key))",
        "asr_menu_filter_.AppendText(MenuKeyToUtf8(key))",
        "IsMenuCtrlShortcut(key, FcitxKey_w)",
        "IsMenuCtrlShortcut(key, FcitxKey_u)",
        r#""Scenes /filter""#,
        r#"FrontendText("Models /filter")"#,
    ] {
        assert!(
            addon_source.contains(required),
            "searchable frontend menus should pin {required}"
        );
    }
    assert_eq!(
        addon_source.matches("IsPrintableMenuInput(").count(),
        3,
        "shared handling plus scene and ASR handlers should pin printable filter input"
    );
}

#[test]
fn cpp_frontend_i18n_builds_loads_and_installs_chinese_catalog() {
    let header = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/include/vinput_fcitx_bridge/fcitx_i18n.h",
    ))
    .expect("read frontend i18n header");
    let source = std::fs::read_to_string(workspace_file("cpp/fcitx5-addon/src/fcitx_i18n.cpp"))
        .expect("read frontend i18n source");
    let cmake = std::fs::read_to_string(workspace_file("cpp/fcitx5-addon/CMakeLists.txt"))
        .expect("read addon CMake file");
    let addon_source =
        std::fs::read_to_string(workspace_file("cpp/fcitx5-addon/src/fcitx_addon.cpp"))
            .expect("read addon source");
    let candidates =
        std::fs::read_to_string(workspace_file("cpp/fcitx5-addon/src/fcitx_candidates.cpp"))
            .expect("read candidate source");

    for required in [
        "kFrontendTranslationDomain = \"fcitx5-vinput\"",
        "VINPUT_FCITX_LOCALEDIR",
        "InitFrontendI18n",
        "FrontendText",
        "FrontendCountText",
        "FrontendPageText",
    ] {
        assert!(
            header.contains(required),
            "i18n header should pin {required}"
        );
    }
    for required in [
        "fcitx::registerDomain",
        "fcitx::translateDomain",
        "VINPUT_FCITX_BUILD_LOCALEDIR",
        "VINPUT_FCITX_INSTALL_LOCALEDIR",
    ] {
        assert!(
            source.contains(required),
            "i18n source should pin {required}"
        );
    }
    for required in [
        "find_package(Gettext REQUIRED)",
        "vinput_fcitx_translations",
        "zh_CN/LC_MESSAGES/fcitx5-vinput.mo",
        "vinput_fcitx_bridge_i18n_smoke",
    ] {
        assert!(
            cmake.contains(required),
            "addon CMake should pin {required}"
        );
    }
    for required in [
        r#"FrontendText("Scenes /filter")"#,
        r#"FrontendText("Models /filter")"#,
        r#"FrontendText("Current: ")"#,
        r#"FrontendText("Loading: ")"#,
        r#"FrontendText("Error: ")"#,
    ] {
        assert!(
            addon_source.contains(required),
            "addon labels should pin {required}"
        );
    }
    for required in [
        r#"FrontendCountText("Choose Result (%zu)", count)"#,
        r#"FrontendText("Original")"#,
        r#"FrontendText("Voice Command")"#,
    ] {
        assert!(
            candidates.contains(required),
            "candidate labels should pin {required}"
        );
    }
}

#[test]
fn cpp_frontend_notifications_keep_legacy_presenter_contract() {
    let header = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/include/vinput_fcitx_bridge/fcitx_notifications.h",
    ))
    .expect("read notification header");
    let source = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/src/fcitx_notifications.cpp",
    ))
    .expect("read notification source");
    let addon_source =
        std::fs::read_to_string(workspace_file("cpp/fcitx5-addon/src/fcitx_addon.cpp"))
            .expect("read addon source");
    let cmake = std::fs::read_to_string(workspace_file("cpp/fcitx5-addon/CMakeLists.txt"))
        .expect("read addon CMake file");

    for required in [
        "kInfoNotificationTimeoutMs = 3000",
        "kErrorNotificationTimeoutMs = 5000",
        "enum class FrontendNotificationKind",
        "BuildFrontendNotification",
        "SendFrontendNotification",
    ] {
        assert!(
            header.contains(required),
            "notification header should pin {required}"
        );
    }
    for required in [
        "Fcitx5::Module::Notifications",
        "vinput_fcitx_bridge_notifications_smoke",
    ] {
        assert!(
            cmake.contains(required),
            "addon CMake should pin {required}"
        );
    }
    for required in [
        "dialog-information",
        "dialog-warning",
        "dialog-error",
        "fcitx::INotifications::sendNotification",
        "addon(\"notifications\", true)",
        "vinput: %s: %s",
    ] {
        assert!(
            source.contains(required),
            "notification source should pin {required}"
        );
    }
    for required in [
        "Notify(FrontendNotificationKind::Error, display_outcome.text)",
        r#"FrontendValueText("Switched scene to '%s'.", scene.label)"#,
        r#"FrontendValueText("ASR switch requested for '%s'.", display_title)"#,
        "Notify(FrontendNotificationKind::Info, message)",
    ] {
        assert!(
            addon_source.contains(required),
            "addon notification path should pin {required}"
        );
    }
}

#[test]
fn cpp_frontend_recovers_cross_client_daemon_status() {
    let bridge_header = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/include/vinput_fcitx_bridge/frontend_bridge.h",
    ))
    .expect("read frontend bridge header");
    let client_header = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/include/vinput_fcitx_bridge/sd_bus_daemon_client.h",
    ))
    .expect("read sd-bus client header");
    let client_source = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/src/sd_bus_daemon_client.cpp",
    ))
    .expect("read sd-bus client source");
    let addon_source =
        std::fs::read_to_string(workspace_file("cpp/fcitx5-addon/src/fcitx_addon.cpp"))
            .expect("read addon source");
    let addon_smoke = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/tests/fcitx_addon_dbus_smoke.cpp",
    ))
    .expect("read addon D-Bus smoke");

    assert!(bridge_header.contains("AdoptRecording"));
    assert!(client_header.contains("GetStatus(std::string *status"));
    assert!(client_source.contains("CallStringReply(dbus::kMethodGetStatus"));
    for required in [
        "ReconcileDaemonStatusBeforeStart",
        "client->GetStatus(&status, &error)",
        "bridge_.AdoptRecording(false, active_scene_id_)",
        "PresentRemoteDaemonStatus",
        "remote_status_ic_",
        "status == dbus::kStatusInferring",
        "status == dbus::kStatusPostprocessing",
    ] {
        assert!(
            addon_source.contains(required),
            "addon status recovery should pin {required}"
        );
    }
    for required in [
        "external normal start failed",
        "cross-client normal takeover",
        r#"external_status != "recording""#,
        r#"external_status != "idle""#,
    ] {
        assert!(
            addon_smoke.contains(required),
            "addon D-Bus smoke should pin {required}"
        );
    }
}

#[test]
fn cpp_frontend_forwards_daemon_notification_signals() {
    let header = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/include/vinput_fcitx_bridge/fcitx_daemon_signal_monitor.h",
    ))
    .expect("read daemon signal monitor header");
    let source = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/src/fcitx_daemon_signal_monitor.cpp",
    ))
    .expect("read daemon signal monitor source");
    let addon_source =
        std::fs::read_to_string(workspace_file("cpp/fcitx5-addon/src/fcitx_addon.cpp"))
            .expect("read addon source");
    let cmake = std::fs::read_to_string(workspace_file("cpp/fcitx5-addon/CMakeLists.txt"))
        .expect("read addon CMake file");

    for required in [
        "struct DaemonNotificationPayload",
        "ClassifyDaemonNotification",
        "RenderDaemonNotification",
        "ComposeDaemonStatusPreedit",
        "struct DaemonSignalCallbacks",
        "service_availability_changed",
        "service_owner_",
        "owner_change_slot_",
        "class FcitxDaemonSignalMonitor",
    ] {
        assert!(
            header.contains(required),
            "signal header should pin {required}"
        );
    }
    for required in [
        "NameOwnerChanged",
        "serviceOwner",
        "UpdateServiceOwner",
        "message.sender() == service_owner_",
        "dbus::kSignalStatusChanged",
        "dbus::kSignalRecognitionPartial",
        "dbus::kSignalDaemonNotification",
        "AddStringSignalMatch",
        "fcitx::dbus::MatchRule",
        "std::tuple<std::string, std::string, std::string, std::string>",
        "callbacks_.notification(payload)",
    ] {
        assert!(
            source.contains(required),
            "signal source should pin {required}"
        );
    }
    for required in [
        "SetupDaemonSignalMonitor",
        "HandleDaemonAvailability",
        "Voice input daemon is unavailable.",
        "ApplyBridgeOutcome(active_ic, error)",
        "HandleDaemonStatus",
        "HandleRecognitionPartial",
        "UpdateLivePreedit",
        "ComposeDaemonStatusPreedit",
        "live_partial_text_",
        "HandleDaemonNotification",
        "bridge_.Reset()",
        "trigger_mode_controller_.RecordingStopped()",
        "Notify(kind, message)",
    ] {
        assert!(
            addon_source.contains(required),
            "addon signal path should pin {required}"
        );
    }
    for required in [
        "Fcitx5::Module::DBus",
        "vinput_fcitx_bridge_daemon_signal_monitor_smoke",
        "dbus-run-session",
    ] {
        assert!(
            cmake.contains(required),
            "addon CMake should pin {required}"
        );
    }
}

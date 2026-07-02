//! Integration tests for protocol inspection CLI output.

mod common;

use common::{assert_json_success, assert_stdout_success, vinput_command};
use vinput_protocol::{RecognitionPayload, ServiceStatus, dbus};

const RAW_PAYLOAD_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/recognition/raw.json"
));
const MENU_PAYLOAD_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/recognition/menu.json"
));
const SENTINEL_PAYLOAD_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/recognition/sentinel.json"
));

fn fixture_json(input: &str) -> &str {
    input.trim_end()
}

#[test]
fn shared_recognition_fixtures_roundtrip_through_protocol_crate() {
    for fixture in [RAW_PAYLOAD_JSON, MENU_PAYLOAD_JSON, SENTINEL_PAYLOAD_JSON] {
        let fixture = fixture_json(fixture);
        let payload = RecognitionPayload::from_json_str(fixture).unwrap();

        assert_eq!(payload.to_json_string().unwrap(), fixture);
    }
}

#[test]
fn protocol_prints_service_dbus_contract() {
    let output = vinput_command()
        .args(["protocol"])
        .output()
        .expect("run vinput protocol");

    let value = assert_json_success(output, "protocol output");
    assert_eq!(value["service_bus_name"], "org.fcitx.Vinput");
    assert_eq!(value["service_object_path"], "/org/fcitx/Vinput");
    assert_eq!(value["service_interface"], "org.fcitx.Vinput.Service");
    assert_eq!(value["frontend_notifier_method"], "Notify");
    assert_eq!(
        value["operation_failed_error"],
        "org.fcitx.Vinput.Error.OperationFailed"
    );
    assert_eq!(value["error_info_signature"], "ssss");
    assert_eq!(
        value["methods"],
        serde_json::to_value(dbus::SERVICE_METHODS).unwrap()
    );
    assert_eq!(
        value["legacy_methods"],
        serde_json::to_value(dbus::LEGACY_SERVICE_METHODS).unwrap()
    );
    assert_eq!(
        value["diagnostic_extension_methods"],
        serde_json::to_value(dbus::DIAGNOSTIC_EXTENSION_METHODS).unwrap()
    );
    assert!(
        !value["legacy_methods"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("GetTextAdapterState"))
    );
    assert_eq!(
        value["signals"],
        serde_json::to_value(dbus::SERVICE_SIGNALS).unwrap()
    );
    assert_eq!(
        value["statuses"],
        serde_json::to_value(ServiceStatus::WIRE_VALUES).unwrap()
    );
}

#[test]
fn activation_service_prints_configured_exec_line() {
    let output = vinput_command()
        .args([
            "activation-service",
            "--daemon",
            "/opt/vinput daemon/bin/vinput-daemon",
            "--configured-backends",
            "--config",
            "/tmp/vinput config.json",
            "--audio-backend",
            "pipewire",
            "--daemon-arg=--log-level",
            "--daemon-arg=debug",
        ])
        .output()
        .expect("run vinput activation-service");

    let stdout = assert_stdout_success(output, "activation service output");
    assert_eq!(
        stdout,
        "[D-BUS Service]\nName=org.fcitx.Vinput\nExec='/opt/vinput daemon/bin/vinput-daemon' --dbus --configured-backends --config '/tmp/vinput config.json' --audio-backend pipewire --log-level debug\n"
    );
}

#[test]
fn activation_service_user_writes_xdg_data_home_service() {
    let mut data_home = std::env::temp_dir();
    data_home.push(format!(
        "vinput-cli-user-service-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));

    let output = vinput_command()
        .env("XDG_DATA_HOME", &data_home)
        .args([
            "activation-service",
            "--daemon",
            "/usr/bin/vinput-daemon",
            "--user",
        ])
        .output()
        .expect("run vinput activation-service --user");

    let stdout = assert_stdout_success(output, "activation service user output");
    assert!(stdout.is_empty());
    let service_path = data_home
        .join("dbus-1")
        .join("services")
        .join("org.fcitx.Vinput.service");
    let service = std::fs::read_to_string(&service_path).expect("read generated user service");
    assert_eq!(
        service,
        "[D-BUS Service]\nName=org.fcitx.Vinput\nExec=/usr/bin/vinput-daemon --dbus\n"
    );
    std::fs::remove_dir_all(data_home).expect("remove generated user service fixture");
}

#[test]
fn activation_service_remove_user_deletes_xdg_data_home_service() {
    let mut data_home = std::env::temp_dir();
    data_home.push(format!(
        "vinput-cli-remove-user-service-{}-{}",
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
    std::fs::write(&service_path, "stale service").expect("write stale service");

    let output = vinput_command()
        .env("XDG_DATA_HOME", &data_home)
        .args(["activation-service", "--remove-user"])
        .output()
        .expect("run vinput activation-service --remove-user");

    let value = assert_json_success(output, "remove user activation service output");
    assert_eq!(value["ok"], true);
    assert_eq!(value["removed"], true);
    assert_eq!(
        value["user_service_path"],
        service_path.to_string_lossy().as_ref()
    );
    assert!(!service_path.exists());
    std::fs::remove_dir_all(data_home).expect("remove service fixture");
}

#[test]
fn activation_service_user_status_reports_existing_service() {
    let mut data_home = std::env::temp_dir();
    data_home.push(format!(
        "vinput-cli-user-status-service-{}-{}",
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
        "[D-BUS Service]\nName=org.fcitx.Vinput\nExec=/usr/bin/vinput-daemon --dbus\n",
    )
    .expect("write service file");

    let output = vinput_command()
        .env("XDG_DATA_HOME", &data_home)
        .args(["activation-service", "--user-status"])
        .output()
        .expect("run vinput activation-service --user-status");

    let value = assert_json_success(output, "user activation status output");
    assert_eq!(value["user_service_exists"], true);
    assert_eq!(value["user_service_name"], "org.fcitx.Vinput");
    assert_eq!(value["user_service_name_matches"], true);
    assert_eq!(value["user_service_exec"], "/usr/bin/vinput-daemon --dbus");
    std::fs::remove_dir_all(data_home).expect("remove service fixture");
}

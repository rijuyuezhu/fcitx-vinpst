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
    assert!(
        !value["legacy_methods"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("GetRuntimeStatus"))
    );
    assert!(
        value["diagnostic_extension_methods"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("GetRuntimeStatus"))
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

#[test]
fn daemon_reload_asr_dry_run_prints_dbus_plan_json() {
    let output = vinput_command()
        .args(["daemon", "reload-asr", "--dry-run", "--json"])
        .output()
        .expect("run vinput daemon reload-asr --dry-run --json");

    let value = assert_json_success(output, "daemon reload-asr dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["will_call_dbus"], false);
    assert_eq!(value["dbus"]["service"], dbus::SERVICE_BUS_NAME);
    assert_eq!(value["dbus"]["object_path"], dbus::SERVICE_OBJECT_PATH);
    assert_eq!(value["dbus"]["interface"], dbus::SERVICE_INTERFACE);
    assert_eq!(value["dbus"]["method"], dbus::method::RELOAD_ASR_BACKEND);
}

#[test]
fn daemon_reload_asr_text_dry_run_prints_dbus_plan() {
    let text_output = vinput_command()
        .args(["daemon", "reload-asr", "--dry-run"])
        .output()
        .expect("run vinput daemon reload-asr --dry-run");
    let stdout = assert_stdout_success(text_output, "daemon reload-asr dry-run text");
    assert!(stdout.contains("dry_run: true"));
    assert!(stdout.contains("will_call_dbus: false"));
    assert!(stdout.contains("called: false"));
    assert!(stdout.contains("service: org.fcitx.Vinput"));
    assert!(stdout.contains("method: ReloadAsrBackend"));
}

#[test]
fn daemon_status_dry_run_prints_dbus_plan_json() {
    let output = vinput_command()
        .args(["daemon", "status", "--dry-run", "--json"])
        .output()
        .expect("run vinput daemon status --dry-run --json");

    let value = assert_json_success(output, "daemon status dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["will_call_dbus"], false);
    assert_eq!(value["dbus"]["service"], dbus::SERVICE_BUS_NAME);
    assert_eq!(value["dbus"]["object_path"], dbus::SERVICE_OBJECT_PATH);
    assert_eq!(value["dbus"]["interface"], dbus::SERVICE_INTERFACE);
    let methods = value["dbus"]["methods"].as_array().unwrap();
    assert!(methods.contains(&serde_json::json!(dbus::method::GET_STATUS)));
    assert!(methods.contains(&serde_json::json!(dbus::method::GET_ASR_BACKEND_STATE)));
    assert!(methods.contains(&serde_json::json!(dbus::method::GET_RUNTIME_STATUS)));
    let reports = value["reports"].as_array().unwrap();
    assert!(reports.contains(&serde_json::json!("service_status")));
    assert!(reports.contains(&serde_json::json!("bus_owner")));
    assert!(reports.contains(&serde_json::json!("asr_backend")));
    assert!(reports.contains(&serde_json::json!("runtime_status")));
    assert!(reports.contains(&serde_json::json!("text_adapters")));
    assert_eq!(value["owner_probe"]["target_name"], dbus::SERVICE_BUS_NAME);
    let owner_methods = value["owner_probe"]["methods"].as_array().unwrap();
    assert!(owner_methods.contains(&serde_json::json!("GetNameOwner")));
    assert!(owner_methods.contains(&serde_json::json!("GetConnectionUnixProcessID")));
    assert!(value["next_steps"].as_array().unwrap().iter().any(|step| {
        step.as_str()
            .is_some_and(|step| step.contains("without --dry-run"))
    }));
}

#[test]
fn daemon_status_text_dry_run_prints_dbus_plan() {
    let output = vinput_command()
        .args(["daemon", "status", "--dry-run"])
        .output()
        .expect("run vinput daemon status --dry-run");
    let stdout = assert_stdout_success(output, "daemon status dry-run text");
    assert!(stdout.contains("dry_run: true"));
    assert!(stdout.contains("will_call_dbus: false"));
    assert!(stdout.contains("service: org.fcitx.Vinput"));
    assert!(stdout.contains("GetStatus"));
    assert!(stdout.contains("GetAsrBackendState"));
    assert!(stdout.contains("GetRuntimeStatus"));
    assert!(stdout.contains(
        "reports: service_status, bus_owner, asr_backend, runtime_status, text_adapters"
    ));
    assert!(
        stdout
            .contains("owner_probe: GetNameOwner, GetConnectionUnixProcessID, procfs exe/cmdline")
    );
    assert!(stdout.contains("next_step: run vinput daemon status without --dry-run"));
}

#[test]
fn recording_start_dry_run_prints_dbus_plan_json() {
    let output = vinput_command()
        .args([
            "recording",
            "start",
            "--selected-text",
            "hello",
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("run vinput recording start --dry-run --json");

    let value = assert_json_success(output, "recording start dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["action"], "start");
    assert_eq!(value["will_call_dbus"], false);
    assert_eq!(value["dbus"]["service"], dbus::SERVICE_BUS_NAME);
    assert_eq!(
        value["dbus"]["methods"][0],
        dbus::method::START_COMMAND_RECORDING
    );
    assert_eq!(value["args"]["selected_text_present"], true);
}

#[test]
fn recording_stop_and_toggle_dry_run_print_text_plans() {
    let stop = vinput_command()
        .args(["recording", "stop", "--scene", "demo", "--dry-run"])
        .output()
        .expect("run vinput recording stop --dry-run");
    let stop_stdout = assert_stdout_success(stop, "recording stop dry-run text");
    assert!(stop_stdout.contains("action: stop"));
    assert!(stop_stdout.contains("StopRecording"));
    assert!(stop_stdout.contains("scene: demo"));

    let toggle = vinput_command()
        .args(["recording", "toggle", "--dry-run"])
        .output()
        .expect("run vinput recording toggle --dry-run");
    let toggle_stdout = assert_stdout_success(toggle, "recording toggle dry-run text");
    assert!(toggle_stdout.contains("action: toggle"));
    assert!(toggle_stdout.contains("GetStatus"));
    assert!(toggle_stdout.contains("StartRecording"));
    assert!(toggle_stdout.contains("StopRecording"));
}

#[test]
fn daemon_start_dry_run_prints_activation_plan_json() {
    let output = vinput_command()
        .args(["daemon", "start", "--dry-run", "--json"])
        .output()
        .expect("run vinput daemon start --dry-run --json");

    let value = assert_json_success(output, "daemon start dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["action"], "start");
    assert_eq!(value["will_call_dbus"], false);
    assert_eq!(value["activation"]["strategy"], "dbus-service-activation");
    assert_eq!(
        value["activation"]["trigger_method"],
        dbus::method::GET_STATUS
    );
    assert_eq!(value["dbus"]["service"], dbus::SERVICE_BUS_NAME);
    assert_eq!(value["dbus"]["object_path"], dbus::SERVICE_OBJECT_PATH);
    assert_eq!(value["dbus"]["interface"], dbus::SERVICE_INTERFACE);
    assert_eq!(value["dbus"]["method"], dbus::method::GET_STATUS);
    assert_eq!(value["owner_probe"]["target_name"], dbus::SERVICE_BUS_NAME);
    assert!(
        value["owner_probe"]["methods"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("GetNameOwner"))
    );
    assert!(value["next_steps"].as_array().unwrap().iter().any(|step| {
        step.as_str()
            .is_some_and(|step| step.contains("daemon status"))
    }));
}

#[test]
fn daemon_start_text_dry_run_prints_activation_plan() {
    let output = vinput_command()
        .args(["daemon", "start", "--dry-run"])
        .output()
        .expect("run vinput daemon start --dry-run");
    let stdout = assert_stdout_success(output, "daemon start dry-run text");
    assert!(stdout.contains("dry_run: true"));
    assert!(stdout.contains("action: start"));
    assert!(stdout.contains("strategy: dbus-service-activation"));
    assert!(stdout.contains("method: GetStatus"));
    assert!(stdout.contains("service: org.fcitx.Vinput"));
    assert!(
        stdout
            .contains("owner_probe: GetNameOwner, GetConnectionUnixProcessID, procfs exe/cmdline")
    );
    assert!(
        stdout.contains("next_step: run vinput daemon status to inspect live D-Bus/runtime state")
    );
}

#[test]
fn daemon_user_service_dry_run_commands_print_plans_json() {
    for (command, expected) in [
        ("stop", "systemctl --user stop fcitx-vinput.service"),
        ("restart", "systemctl --user restart fcitx-vinput.service"),
        ("log", "journalctl --user -u fcitx-vinput.service"),
    ] {
        let output = vinput_command()
            .args(["daemon", command, "--dry-run", "--json"])
            .output()
            .expect("run vinput daemon user-service command --dry-run --json");

        let value = assert_json_success(output, "daemon user service dry-run json");
        assert_eq!(value["ok"], true);
        assert_eq!(value["dry_run"], true);
        assert_eq!(value["action"], command);
        assert_eq!(value["will_mutate_user_service"], false);
        assert_eq!(value["strategy"], "systemd-user-service");
        assert_eq!(value["command"], expected);
        assert!(value["next_steps"].as_array().unwrap().iter().any(|step| {
            step.as_str()
                .is_some_and(|step| step.contains("vinput daemon"))
        }));
    }
}

#[test]
fn daemon_log_lines_dry_run_adds_journalctl_limit() {
    let output = vinput_command()
        .args(["daemon", "log", "--lines", "42", "--dry-run", "--json"])
        .output()
        .expect("run vinput daemon log --lines --dry-run --json");

    let value = assert_json_success(output, "daemon log lines dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["action"], "log");
    assert_eq!(
        value["command"],
        "journalctl --user -u fcitx-vinput.service -n 42"
    );
    assert_eq!(
        value["command_argv"],
        serde_json::json!([
            "journalctl",
            "--user",
            "-u",
            "fcitx-vinput.service",
            "-n",
            "42"
        ])
    );
}

#[test]
fn daemon_log_lines_text_dry_run_prints_limit() {
    let output = vinput_command()
        .args(["daemon", "log", "--lines", "7", "--dry-run"])
        .output()
        .expect("run vinput daemon log --lines --dry-run");

    let stdout = assert_stdout_success(output, "daemon log lines dry-run text");
    assert!(stdout.contains("action: log"));
    assert!(stdout.contains("journalctl --user -u fcitx-vinput.service -n 7"));
}

#[test]
fn daemon_log_lines_real_command_reports_limited_argv() {
    let output = vinput_command()
        .env("VINPUT_DAEMON_JOURNALCTL", "/bin/echo")
        .args(["daemon", "log", "--lines", "3", "--json"])
        .output()
        .expect("run vinput daemon log --lines --json");

    let value = assert_json_success(output, "daemon log lines real json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], false);
    assert_eq!(value["action"], "log");
    assert_eq!(value["will_mutate_user_service"], false);
    assert_eq!(value["command_argv"][0], "/bin/echo");
    assert_eq!(value["stdout"], "--user -u fcitx-vinput.service -n 3\n");
}

#[test]
fn global_json_flag_forces_daemon_log_lines_json() {
    let output = vinput_command()
        .args(["-j", "daemon", "log", "--lines", "9", "--dry-run"])
        .output()
        .expect("run vinput -j daemon log --lines --dry-run");

    let value = assert_json_success(output, "global json daemon log lines dry-run");
    assert_eq!(value["ok"], true);
    assert_eq!(value["action"], "log");
    assert_eq!(
        value["command"],
        "journalctl --user -u fcitx-vinput.service -n 9"
    );
}

#[test]
fn daemon_log_lines_rejects_zero() {
    let output = vinput_command()
        .args(["daemon", "log", "--lines", "0", "--dry-run"])
        .output()
        .expect("run vinput daemon log --lines 0");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("daemon log --lines must be greater than 0"));
}

#[test]
fn daemon_user_service_real_commands_report_external_output_json() {
    for (command, expected_stdout, will_mutate) in [
        ("stop", "--user stop fcitx-vinput.service\n", true),
        ("restart", "--user restart fcitx-vinput.service\n", true),
        ("log", "--user -u fcitx-vinput.service\n", false),
    ] {
        let output = vinput_command()
            .env("VINPUT_DAEMON_SYSTEMCTL", "/bin/echo")
            .env("VINPUT_DAEMON_JOURNALCTL", "/bin/echo")
            .args(["daemon", command, "--json"])
            .output()
            .expect("run vinput daemon user-service command --json");

        let value = assert_json_success(output, "daemon user service real json");
        assert_eq!(value["ok"], true);
        assert_eq!(value["dry_run"], false);
        assert_eq!(value["action"], command);
        assert_eq!(value["will_mutate_user_service"], will_mutate);
        assert_eq!(value["strategy"], "systemd-user-service");
        assert_eq!(value["command_argv"][0], "/bin/echo");
        assert_eq!(value["exit_status"], 0);
        assert_eq!(value["stdout"], expected_stdout);
        assert_eq!(value["stderr"], "");
        assert!(value["next_steps"].as_array().unwrap().iter().any(|step| {
            step.as_str()
                .is_some_and(|step| step.contains("vinput daemon"))
        }));
    }
}

#[test]
fn daemon_log_dry_run_next_step_mentions_lines() {
    let output = vinput_command()
        .args(["daemon", "log", "--lines", "5", "--dry-run"])
        .output()
        .expect("run vinput daemon log --lines --dry-run text");

    let stdout = assert_stdout_success(output, "daemon log lines next-step text");
    assert!(stdout.contains("next_step: adjust --lines to inspect more or fewer journal entries"));
}

#[test]
fn daemon_stop_text_dry_run_prints_user_service_plan() {
    let output = vinput_command()
        .args(["daemon", "stop", "--dry-run"])
        .output()
        .expect("run vinput daemon stop --dry-run");
    let stdout = assert_stdout_success(output, "daemon stop dry-run text");
    assert!(stdout.contains("dry_run: true"));
    assert!(stdout.contains("action: stop"));
    assert!(stdout.contains("will_mutate_user_service: false"));
    assert!(stdout.contains("systemctl --user stop fcitx-vinput.service"));
    assert!(stdout.contains("next_step: run vinput daemon status to verify daemon availability"));
}

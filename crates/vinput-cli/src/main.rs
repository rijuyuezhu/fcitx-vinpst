//! `vinput` command-line prototype.

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use vinput_asr::AsrBackendFactory;
use vinput_audio::CaptureTarget;
use vinput_config::{AsrProviderKind, RegistryConfig, VinputConfig};
use vinput_protocol::{RecognitionPayload, ServiceStatus, dbus};
use vinput_registry::{
    ArchiveFormat, AssetEntry, AssetPlanSummary, LiveModelEntry, LiveModelInstallRequest,
    LiveModelInstallResult, LiveModelRegistry, LiveRegistryI18n, LiveVinputModelMetadata,
    PlannedAsset, RegistryIndex, RegistryTextSource, ReqwestRegistryAssetSource,
    ReqwestRegistryTextSource, install_live_model,
};

/// CLI for inspecting and controlling the vinput daemon.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

/// Config-related commands.
#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Validate a local config JSON file and print a summary.
    Validate {
        /// Path to a config JSON file.
        path: PathBuf,
        /// Explicitly print only summary fields.
        #[arg(long)]
        summary_only: bool,
    },
    /// Print, list, or write a bundled example config JSON file.
    Example {
        /// Example config to export. Omit with --list to show available examples.
        #[arg(value_enum, required_unless_present = "list")]
        kind: Option<ConfigExample>,
        /// List available example configs as JSON.
        #[arg(long, conflicts_with = "output")]
        list: bool,
        /// Write the example config to this path instead of stdout.
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ConfigExample {
    /// Upstream-compatible default config skeleton.
    Default,
    /// Deterministic command ASR/text adapter demo config.
    CommandDemo,
    /// Configured command ASR/text adapter demo intended for live `PipeWire` smoke.
    ConfiguredPipewireLive,
}

/// Registry-related commands.
#[derive(Debug, Subcommand)]
enum RegistryCommand {
    /// Validate a local registry index JSON file and print a summary.
    Validate {
        /// Path to a registry index JSON file.
        path: PathBuf,
    },
    /// Print planned registry assets using configured mirrors.
    Plan {
        /// Path to a registry index JSON file.
        path: PathBuf,
        /// Optional config JSON file that provides registry mirrors.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Only plan assets for this model id.
        #[arg(long, conflicts_with = "adapter")]
        model: Option<String>,
        /// Only plan assets for this adapter id.
        #[arg(long, conflicts_with = "model")]
        adapter: Option<String>,
        /// Print only the plan summary without per-asset rows.
        #[arg(long)]
        summary_only: bool,
    },
    /// Print a dry-run install plan without downloading assets.
    InstallPlan {
        /// Path to a registry index JSON file.
        path: PathBuf,
        /// Target root directory for planned asset installation.
        #[arg(long)]
        target_root: PathBuf,
        /// Optional config JSON file that provides registry mirrors.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Only plan assets for this model id.
        #[arg(long, conflicts_with = "adapter")]
        model: Option<String>,
        /// Only plan assets for this adapter id.
        #[arg(long, conflicts_with = "model")]
        adapter: Option<String>,
        /// Print only the install-plan summary without per-asset rows.
        #[arg(long)]
        summary_only: bool,
    },
}

/// Daemon-related commands backed by the D-Bus service contract.
#[derive(Debug, Subcommand)]
enum DaemonCommand {
    /// Query daemon status and runtime diagnostics over D-Bus.
    Status {
        /// Print the D-Bus call plan without contacting the daemon.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Reload the selected ASR backend on the running daemon.
    ReloadAsr {
        /// Print the D-Bus call plan without contacting the daemon.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
}

/// Recording control commands backed by the daemon D-Bus service contract.
#[derive(Debug, Subcommand)]
enum RecordingCommand {
    /// Start normal or command-mode recording.
    Start {
        /// Selected text context for command-mode recording.
        #[arg(long)]
        selected_text: Option<String>,
        /// Print the D-Bus call plan without contacting the daemon.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Stop recording and request a recognition result.
    Stop {
        /// Scene id forwarded to `StopRecording`. Defaults to an empty scene.
        #[arg(long)]
        scene: Option<String>,
        /// Print the D-Bus call plan without contacting the daemon.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Toggle recording by querying daemon status first.
    Toggle {
        /// Selected text context used when toggle starts command-mode recording.
        #[arg(long)]
        selected_text: Option<String>,
        /// Scene id used when toggle stops recording. Defaults to an empty scene.
        #[arg(long)]
        scene: Option<String>,
        /// Print the D-Bus call plan without contacting the daemon.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
}

/// Model-related commands backed by the live registry catalog.
#[derive(Debug, Subcommand)]
enum ModelCommand {
    /// List models from live registry/models.json metadata.
    #[command(alias = "ls")]
    List {
        /// Legacy-compatible flag for listing remote/available models.
        #[arg(short = 'a', long)]
        available: bool,
        /// List installed models from the managed model root instead of the live registry.
        #[arg(long)]
        installed: bool,
        /// Managed model root used by --installed. Defaults to $XDG_DATA_HOME/fcitx-vinput/models.
        #[arg(long)]
        model_root: Option<PathBuf>,
        /// Optional local live registry/models.json file. Omitted to fetch configured mirrors.
        #[arg(long)]
        registry: Option<PathBuf>,
        /// Optional local live i18n JSON file used for title/description fallback.
        #[arg(long)]
        i18n: Option<PathBuf>,
        /// Optional config JSON file that provides registry mirrors.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Live registry i18n locale to fetch when reading remote mirrors.
        #[arg(long, default_value = "zh_CN")]
        locale: String,
        /// Print machine-readable JSON instead of text table output.
        #[arg(long)]
        json: bool,
    },
    /// Show one live registry model by full id or short id.
    Info {
        /// Full model id or `short_id`.
        id: String,
        /// Optional local live registry/models.json file. Omitted to fetch configured mirrors.
        #[arg(long)]
        registry: Option<PathBuf>,
        /// Optional local live i18n JSON file used for title/description fallback.
        #[arg(long)]
        i18n: Option<PathBuf>,
        /// Optional config JSON file that provides registry mirrors.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Live registry i18n locale to fetch when reading remote mirrors.
        #[arg(long, default_value = "zh_CN")]
        locale: String,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Install a model from the live registry, or inspect the plan with --dry-run.
    #[command(alias = "add")]
    Install {
        /// Full model id or `short_id`.
        id: String,
        /// Optional local live registry/models.json file. Omitted to fetch configured mirrors.
        #[arg(long)]
        registry: Option<PathBuf>,
        /// Optional local live i18n JSON file used for title/description fallback.
        #[arg(long)]
        i18n: Option<PathBuf>,
        /// Optional config JSON file that provides registry mirrors.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Live registry i18n locale to fetch when reading remote mirrors.
        #[arg(long, default_value = "zh_CN")]
        locale: String,
        /// Managed model root. Defaults to $XDG_DATA_HOME/fcitx-vinput/models.
        #[arg(long)]
        model_root: Option<PathBuf>,
        /// Temporary staging root. Defaults to $XDG_CACHE_HOME/fcitx-vinput/model-install.
        #[arg(long)]
        staging_root: Option<PathBuf>,
        /// Print the install plan without downloading, extracting, or writing config.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Preview selecting the active local ASR model in config.
    Use {
        /// Full model id, `short_id`, installed model path, or managed model dir name.
        selector: String,
        /// Optional local live registry/models.json file for resolving `id`/`short_id`.
        #[arg(long)]
        registry: Option<PathBuf>,
        /// Optional local live i18n JSON file used for title/description output.
        #[arg(long)]
        i18n: Option<PathBuf>,
        /// Optional config JSON file to preview changing.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Live registry i18n locale to fetch when reading remote mirrors.
        #[arg(long, default_value = "zh_CN")]
        locale: String,
        /// ASR provider id to update. Defaults to the config active provider.
        #[arg(long)]
        provider: Option<String>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update --config in place and write a <config>.bak backup.
        #[arg(long)]
        in_place: bool,
        /// Managed model root. Defaults to $XDG_DATA_HOME/fcitx-vinput/models.
        #[arg(long)]
        model_root: Option<PathBuf>,
        /// Reload the running daemon ASR backend after writing config. Dry-run prints the planned call.
        #[arg(long)]
        reload_daemon: bool,
        /// Preview config changes without writing. Required until config mutation is implemented.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Preview removing a managed installed model directory.
    #[command(alias = "rm")]
    Remove {
        /// Full model id, `short_id`, managed model dir name, or installed path under model root.
        selector: String,
        /// Optional local live registry/models.json file for resolving `id`/`short_id`.
        #[arg(long)]
        registry: Option<PathBuf>,
        /// Optional local live i18n JSON file used for title/description output.
        #[arg(long)]
        i18n: Option<PathBuf>,
        /// Optional config JSON file that provides registry mirrors.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Live registry i18n locale to fetch when reading remote mirrors.
        #[arg(long, default_value = "zh_CN")]
        locale: String,
        /// Managed model root. Defaults to $XDG_DATA_HOME/fcitx-vinput/models.
        #[arg(long)]
        model_root: Option<PathBuf>,
        /// Print the removal plan without deleting anything.
        #[arg(long)]
        dry_run: bool,
        /// Confirm removal. Required for deleting the managed model directory.
        #[arg(long)]
        yes: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
}

/// Supported bootstrap commands.
#[derive(Debug, Subcommand)]
enum Command {
    /// Print stable D-Bus names and methods.
    Protocol,
    /// Inspect or validate vinput config metadata.
    Config {
        /// Config operation. Omitted to validate the bundled default config.
        #[command(subcommand)]
        command: Option<ConfigCommand>,
    },
    /// Inspect or validate registry metadata.
    Registry {
        /// Registry operation. Omitted to print URL resolution for the bundled config.
        #[command(subcommand)]
        command: Option<RegistryCommand>,
    },
    /// Control or inspect the running vinput daemon.
    Daemon {
        /// Daemon operation.
        #[command(subcommand)]
        command: DaemonCommand,
    },

    /// Control daemon recording over D-Bus.
    Recording {
        /// Recording operation.
        #[command(subcommand)]
        command: RecordingCommand,
    },
    /// Manage ASR models from the live registry catalog.
    Model {
        /// Model operation.
        #[command(subcommand)]
        command: ModelCommand,
    },
    /// Print ASR backend availability diagnostics from config.
    AsrState {
        /// Optional config JSON file. Omitted to inspect the bundled default config.
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Print capture-device diagnostics from config and optional live backend.
    AudioDevices {
        /// Optional config JSON file. Omitted to inspect the bundled default config.
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Print combined local diagnostics for config, ASR, audio, and activation setup.
    Doctor {
        /// Optional config JSON file. Omitted to inspect the bundled default config.
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Generate, install, or remove an org.fcitx.Vinput D-Bus activation service file.
    ActivationService {
        /// Path to the vinput-daemon executable used by D-Bus activation.
        #[arg(long, required_unless_present_any = ["remove_user", "user_status"])]
        daemon: Option<PathBuf>,
        /// Optional config JSON file passed to vinput-daemon.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Activate configured ASR/text backends instead of the mock runtime.
        #[arg(long)]
        configured_backends: bool,
        /// Optional audio backend passed to vinput-daemon, such as mock or pipewire.
        #[arg(long)]
        audio_backend: Option<String>,
        /// Extra argument forwarded to vinput-daemon; repeat for multiple arguments.
        #[arg(long = "daemon-arg")]
        daemon_args: Vec<String>,
        /// Write to the per-user D-Bus activation service path.
        #[arg(long, conflicts_with = "output")]
        user: bool,
        /// Remove the per-user D-Bus activation service file and print JSON status.
        #[arg(
            long,
            conflicts_with_all = [
                "daemon",
                "config",
                "configured_backends",
                "audio_backend",
                "daemon_args",
                "user",
                "user_status",
                "output"
            ]
        )]
        remove_user: bool,
        /// Print per-user D-Bus activation service status as JSON.
        #[arg(
            long,
            conflicts_with_all = [
                "daemon",
                "config",
                "configured_backends",
                "audio_backend",
                "daemon_args",
                "user",
                "output"
            ]
        )]
        user_status: bool,
        /// Write the service file to this path instead of stdout.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Create a recognition JSON payload for tests/manual inspection.
    MockResult {
        /// Commit text for the payload.
        text: String,
    },
    /// Parse a status string and print the normalized wire value.
    Status {
        /// Status string such as idle, recording, inferring, postprocessing, or error.
        status: String,
    },
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Protocol => print_protocol(),
        Command::Config { command } => match command {
            Some(ConfigCommand::Validate { path, summary_only }) => {
                validate_config_file(&path, summary_only)
            }
            Some(ConfigCommand::Example { kind, list, output }) => {
                handle_config_example(kind, list, output.as_deref())
            }
            None => validate_config(),
        },
        Command::Registry { command } => match command {
            Some(RegistryCommand::Validate { path }) => validate_registry_index(&path),
            Some(RegistryCommand::Plan {
                path,
                config,
                model,
                adapter,
                summary_only,
            }) => print_registry_plan(
                &path,
                config.as_ref(),
                model.as_deref(),
                adapter.as_deref(),
                summary_only,
            ),
            Some(RegistryCommand::InstallPlan {
                path,
                target_root,
                config,
                model,
                adapter,
                summary_only,
            }) => print_registry_install_plan(
                &path,
                &target_root,
                config.as_ref(),
                model.as_deref(),
                adapter.as_deref(),
                summary_only,
            ),
            None => print_registry_summary(),
        },
        Command::Daemon { command } => handle_daemon_command(&command),
        Command::Recording { command } => handle_recording_command(command),
        Command::Model { command } => handle_model_command(command),
        Command::AsrState { config } => print_asr_state(config.as_ref()),
        Command::AudioDevices { config } => print_audio_devices(config.as_ref()),
        Command::Doctor { config } => print_doctor(config.as_ref()),
        Command::ActivationService {
            daemon,
            config,
            configured_backends,
            audio_backend,
            daemon_args,
            user,
            remove_user,
            user_status,
            output,
        } => {
            if remove_user {
                remove_user_activation_service()
            } else if user_status {
                print_user_activation_service_status()
            } else {
                let daemon = daemon.context("--daemon is required unless --remove-user is set")?;
                write_activation_service(
                    &daemon,
                    config.as_deref(),
                    configured_backends,
                    audio_backend.as_deref(),
                    &daemon_args,
                    user,
                    output.as_deref(),
                )
            }
        }
        Command::MockResult { text } => {
            let payload = RecognitionPayload::raw(text);
            println!("{}", payload.to_json_string()?);
            Ok(())
        }
        Command::Status { status } => {
            let status = ServiceStatus::parse_wire(&status)
                .with_context(|| format!("parse status `{status}`"))?;
            println!("{status}");
            Ok(())
        }
    }
}

fn handle_recording_command(command: RecordingCommand) -> anyhow::Result<()> {
    match command {
        RecordingCommand::Start {
            selected_text,
            dry_run,
            json,
        } => print_recording_plan("start", selected_text.as_deref(), None, dry_run, json),
        RecordingCommand::Stop {
            scene,
            dry_run,
            json,
        } => print_recording_plan("stop", None, scene.as_deref(), dry_run, json),
        RecordingCommand::Toggle {
            selected_text,
            scene,
            dry_run,
            json,
        } => print_recording_plan(
            "toggle",
            selected_text.as_deref(),
            scene.as_deref(),
            dry_run,
            json,
        ),
    }
}

fn print_recording_plan(
    action: &str,
    selected_text: Option<&str>,
    scene: Option<&str>,
    dry_run: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    if !dry_run {
        anyhow::bail!(
            "recording {action} currently requires --dry-run until the D-Bus client is enabled"
        );
    }
    let output = recording_plan_json(action, selected_text, scene);
    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_recording_plan_text(action, selected_text, scene);
    }
    Ok(())
}

fn recording_plan_json(
    action: &str,
    selected_text: Option<&str>,
    scene: Option<&str>,
) -> serde_json::Value {
    let methods = match (action, selected_text.is_some()) {
        ("start", true) => vec![dbus::method::START_COMMAND_RECORDING],
        ("start", false) => vec![dbus::method::START_RECORDING],
        ("stop", _) => vec![dbus::method::STOP_RECORDING],
        ("toggle", true) => vec![
            dbus::method::GET_STATUS,
            dbus::method::START_COMMAND_RECORDING,
            dbus::method::STOP_RECORDING,
        ],
        ("toggle", false) => vec![
            dbus::method::GET_STATUS,
            dbus::method::START_RECORDING,
            dbus::method::STOP_RECORDING,
        ],
        _ => Vec::new(),
    };
    serde_json::json!({
        "ok": true,
        "dry_run": true,
        "action": action,
        "will_call_dbus": false,
        "dbus": {
            "service": dbus::SERVICE_BUS_NAME,
            "object_path": dbus::SERVICE_OBJECT_PATH,
            "interface": dbus::SERVICE_INTERFACE,
            "methods": methods,
        },
        "args": {
            "selected_text_present": selected_text.is_some(),
            "scene": scene.unwrap_or(""),
        },
    })
}

fn print_recording_plan_text(action: &str, selected_text: Option<&str>, scene: Option<&str>) {
    let output = recording_plan_json(action, selected_text, scene);
    println!("dry_run: true");
    println!("action: {action}");
    println!("will_call_dbus: false");
    println!("service: {}", dbus::SERVICE_BUS_NAME);
    println!("object_path: {}", dbus::SERVICE_OBJECT_PATH);
    println!("interface: {}", dbus::SERVICE_INTERFACE);
    println!(
        "methods: {}",
        output["dbus"]["methods"]
            .as_array()
            .map(|methods| methods
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join(", "))
            .unwrap_or_default()
    );
    println!("selected_text_present: {}", selected_text.is_some());
    println!("scene: {}", scene.unwrap_or(""));
}

fn handle_daemon_command(command: &DaemonCommand) -> anyhow::Result<()> {
    match command {
        DaemonCommand::Status { dry_run, json } => print_daemon_status(*dry_run, *json),
        DaemonCommand::ReloadAsr { dry_run, json } => print_daemon_reload_asr_plan(*dry_run, *json),
    }
}

type DaemonAsrBackendStateTuple = (
    String,
    String,
    String,
    String,
    String,
    bool,
    bool,
    Vec<String>,
);

fn print_daemon_status(dry_run: bool, json_output: bool) -> anyhow::Result<()> {
    if dry_run {
        let output = daemon_status_dry_run_json();
        if json_output {
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            print_daemon_status_dry_run_text();
        }
        return Ok(());
    }

    let snapshot = daemon_status_via_dbus()?;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&snapshot)?);
    } else {
        print_daemon_status_text(&snapshot);
    }
    Ok(())
}

fn daemon_status_dry_run_json() -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": true,
        "will_call_dbus": false,
        "dbus": daemon_status_dbus_plan_json(),
    })
}

fn daemon_status_dbus_plan_json() -> serde_json::Value {
    serde_json::json!({
        "service": dbus::SERVICE_BUS_NAME,
        "object_path": dbus::SERVICE_OBJECT_PATH,
        "interface": dbus::SERVICE_INTERFACE,
        "methods": [
            dbus::method::GET_STATUS,
            dbus::method::GET_ASR_BACKEND_STATE,
            dbus::method::GET_RUNTIME_STATUS,
        ],
    })
}

fn print_daemon_status_dry_run_text() {
    println!("dry_run: true");
    println!("will_call_dbus: false");
    println!("service: {}", dbus::SERVICE_BUS_NAME);
    println!("object_path: {}", dbus::SERVICE_OBJECT_PATH);
    println!("interface: {}", dbus::SERVICE_INTERFACE);
    println!(
        "methods: {}, {}, {}",
        dbus::method::GET_STATUS,
        dbus::method::GET_ASR_BACKEND_STATE,
        dbus::method::GET_RUNTIME_STATUS
    );
}

fn daemon_status_via_dbus() -> anyhow::Result<serde_json::Value> {
    let connection = zbus::blocking::Connection::session().context("connect to session bus")?;
    let proxy = daemon_service_proxy(&connection)?;
    let status: String = proxy
        .call(dbus::method::GET_STATUS, &())
        .context("call GetStatus on daemon D-Bus service")?;
    let asr: DaemonAsrBackendStateTuple = proxy
        .call(dbus::method::GET_ASR_BACKEND_STATE, &())
        .context("call GetAsrBackendState on daemon D-Bus service")?;
    let runtime_status_json: String = proxy
        .call(dbus::method::GET_RUNTIME_STATUS, &())
        .context("call GetRuntimeStatus on daemon D-Bus service")?;
    let runtime_status = serde_json::from_str::<serde_json::Value>(&runtime_status_json)
        .context("parse daemon runtime status JSON")?;
    Ok(serde_json::json!({
        "ok": true,
        "dry_run": false,
        "will_call_dbus": true,
        "dbus": daemon_status_dbus_plan_json(),
        "status": status,
        "asr_backend": {
            "target_provider_id": asr.0,
            "target_model_id": asr.1,
            "effective_provider_id": asr.2,
            "effective_model_id": asr.3,
            "last_error": asr.4,
            "reload_in_progress": asr.5,
            "has_effective_backend": asr.6,
            "remote_endpoints": asr.7,
        },
        "runtime_status": runtime_status,
    }))
}

fn print_daemon_status_text(snapshot: &serde_json::Value) {
    println!("status: {}", optional_json_str(&snapshot["status"]));
    println!(
        "target_provider_id: {}",
        optional_json_str(&snapshot["asr_backend"]["target_provider_id"])
    );
    println!(
        "effective_provider_id: {}",
        optional_json_str(&snapshot["asr_backend"]["effective_provider_id"])
    );
    println!(
        "reload_in_progress: {}",
        snapshot["asr_backend"]["reload_in_progress"]
            .as_bool()
            .unwrap_or(false)
    );
    println!(
        "has_effective_backend: {}",
        snapshot["asr_backend"]["has_effective_backend"]
            .as_bool()
            .unwrap_or(false)
    );
}

fn optional_json_str(value: &serde_json::Value) -> &str {
    value.as_str().unwrap_or("-")
}

fn daemon_service_proxy(
    connection: &zbus::blocking::Connection,
) -> anyhow::Result<zbus::blocking::Proxy<'_>> {
    zbus::blocking::Proxy::new(
        connection,
        dbus::SERVICE_BUS_NAME,
        dbus::SERVICE_OBJECT_PATH,
        dbus::SERVICE_INTERFACE,
    )
    .context("create daemon D-Bus proxy")
}

fn print_daemon_reload_asr_plan(dry_run: bool, json_output: bool) -> anyhow::Result<()> {
    if !dry_run {
        reload_asr_backend_via_dbus()?;
    }
    let output = daemon_reload_asr_output(dry_run);
    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("dry_run: {dry_run}");
        println!("will_call_dbus: {}", !dry_run);
        println!("called: {}", !dry_run);
        println!("service: {}", dbus::SERVICE_BUS_NAME);
        println!("object_path: {}", dbus::SERVICE_OBJECT_PATH);
        println!("interface: {}", dbus::SERVICE_INTERFACE);
        println!("method: {}", dbus::method::RELOAD_ASR_BACKEND);
    }
    Ok(())
}

fn daemon_reload_asr_output(dry_run: bool) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": dry_run,
        "will_call_dbus": !dry_run,
        "called": !dry_run,
        "dbus": {
            "service": dbus::SERVICE_BUS_NAME,
            "object_path": dbus::SERVICE_OBJECT_PATH,
            "interface": dbus::SERVICE_INTERFACE,
            "method": dbus::method::RELOAD_ASR_BACKEND,
        },
        "next_steps": [
            "run vinput daemon status to verify the selected ASR backend",
            "use vinput protocol to inspect the stable method contract"
        ],
    })
}

fn reload_asr_backend_via_dbus() -> anyhow::Result<()> {
    let connection = zbus::blocking::Connection::session().context("connect to session bus")?;
    let proxy = daemon_service_proxy(&connection)?;
    let _: () = proxy
        .call(dbus::method::RELOAD_ASR_BACKEND, &())
        .context("call ReloadAsrBackend on daemon D-Bus service")?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn handle_model_command(command: ModelCommand) -> anyhow::Result<()> {
    match command {
        ModelCommand::List {
            available,
            installed,
            model_root,
            registry,
            i18n,
            config,
            locale,
            json,
        } => handle_model_list_command(&ModelListOwnedRequest {
            available,
            installed,
            model_root,
            registry,
            i18n,
            config,
            locale,
            json_output: json,
        }),
        ModelCommand::Info {
            id,
            registry,
            i18n,
            config,
            locale,
            json,
        } => print_model_info(
            &id,
            registry.as_deref(),
            i18n.as_deref(),
            config.as_ref(),
            &locale,
            json,
        ),
        ModelCommand::Install {
            id,
            registry,
            i18n,
            config,
            locale,
            model_root,
            staging_root,
            dry_run,
            json,
        } => print_model_install_plan(ModelInstallPlanRequest {
            id_or_short_id: &id,
            registry_path: registry.as_deref(),
            i18n_path: i18n.as_deref(),
            config_path: config.as_ref(),
            locale: &locale,
            model_root: model_root.as_deref(),
            staging_root: staging_root.as_deref(),
            dry_run,
            json_output: json,
        }),
        ModelCommand::Use {
            selector,
            registry,
            i18n,
            config,
            locale,
            provider,
            output,
            in_place,
            model_root,
            reload_daemon,
            dry_run,
            json,
        } => print_model_use_preview(ModelUseRequest {
            selector: &selector,
            registry_path: registry.as_deref(),
            i18n_path: i18n.as_deref(),
            config_path: config.as_ref(),
            locale: &locale,
            provider: provider.as_deref(),
            output_path: output.as_deref(),
            in_place,
            model_root: model_root.as_deref(),
            reload_daemon,
            dry_run,
            json_output: json,
        }),
        ModelCommand::Remove {
            selector,
            registry,
            i18n,
            config,
            locale,
            model_root,
            dry_run,
            yes,
            json,
        } => print_model_remove_plan(ModelRemoveRequest {
            selector: &selector,
            registry_path: registry.as_deref(),
            i18n_path: i18n.as_deref(),
            config_path: config.as_ref(),
            locale: &locale,
            model_root: model_root.as_deref(),
            dry_run,
            yes,
            json_output: json,
        }),
    }
}

fn print_protocol() -> anyhow::Result<()> {
    let value = serde_json::json!({
        "service_bus_name": dbus::SERVICE_BUS_NAME,
        "service_object_path": dbus::SERVICE_OBJECT_PATH,
        "service_interface": dbus::SERVICE_INTERFACE,
        "frontend_notifier_object_path": dbus::FRONTEND_NOTIFIER_OBJECT_PATH,
        "frontend_notifier_interface": dbus::FRONTEND_NOTIFIER_INTERFACE,
        "frontend_notifier_method": dbus::method::NOTIFY,
        "operation_failed_error": dbus::error::OPERATION_FAILED,
        "error_info_signature": dbus::signature::ERROR_INFO,
        "methods": dbus::SERVICE_METHODS,
        "legacy_methods": dbus::LEGACY_SERVICE_METHODS,
        "diagnostic_extension_methods": dbus::DIAGNOSTIC_EXTENSION_METHODS,
        "signals": dbus::SERVICE_SIGNALS,
        "statuses": ServiceStatus::WIRE_VALUES,
    });
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn validate_config() -> anyhow::Result<()> {
    let config = VinputConfig::bundled_default().context("parse bundled config")?;
    config.validate().context("validate bundled config")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&config_summary_json(&config))?
    );
    Ok(())
}

fn print_asr_state(config_path: Option<&PathBuf>) -> anyhow::Result<()> {
    let config = match config_path {
        Some(path) => load_config_file(path)?,
        None => VinputConfig::bundled_default().context("parse bundled config")?,
    };
    config.validate().context("validate config for ASR state")?;
    let state = AsrBackendFactory::state_for_config(&config.asr);
    println!("{}", serde_json::to_string_pretty(&state)?);
    Ok(())
}

fn print_audio_devices(config_path: Option<&PathBuf>) -> anyhow::Result<()> {
    let config = match config_path {
        Some(path) => load_config_file(path)?,
        None => VinputConfig::bundled_default().context("parse bundled config")?,
    };
    config
        .validate()
        .context("validate config for audio device diagnostics")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&audio_devices_json(&config)?)?
    );
    Ok(())
}

fn print_doctor(config_path: Option<&PathBuf>) -> anyhow::Result<()> {
    let config = match config_path {
        Some(path) => load_config_file(path)?,
        None => VinputConfig::bundled_default().context("parse bundled config")?,
    };
    config.validate().context("validate config for doctor")?;
    let asr_state = AsrBackendFactory::state_for_config(&config.asr);
    let audio = audio_devices_json(&config)?;
    let activation_service = match user_activation_service_path() {
        Ok(path) => user_activation_service_json(&path),
        Err(error) => serde_json::json!({
            "user_service_path": null,
            "user_service_exists": false,
            "user_service_exec": null,
            "read_error": null,
            "path_error": format!("{error:#}"),
        }),
    };
    let summary = serde_json::json!({
        "ok": true,
        "config_path": config_path.map(|path| path.to_string_lossy().into_owned()),
        "config": config_summary_json(&config),
        "asr": asr_state,
        "audio": audio,
        "activation_service": activation_service,
        "fcitx_addon": user_fcitx_addon_json(),
    });
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn user_fcitx_addon_json() -> serde_json::Value {
    match user_fcitx_addon_paths() {
        Ok((module_path, metadata_path)) => fcitx_addon_status_json(&module_path, &metadata_path),
        Err(error) => serde_json::json!({
            "user_module_path": null,
            "user_module_exists": false,
            "user_addon_metadata_path": null,
            "user_addon_metadata_exists": false,
            "user_addon_library": null,
            "user_addon_library_matches": false,
            "user_addon_type": null,
            "read_error": null,
            "path_error": format!("{error:#}"),
        }),
    }
}

fn user_fcitx_addon_paths() -> anyhow::Result<(PathBuf, PathBuf)> {
    let lib_dir = match std::env::var_os("VINPUT_USER_FCITX_LIB_DIR") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => user_home()?.join(".local/lib/fcitx5"),
    };
    let metadata_dir = match std::env::var_os("VINPUT_USER_FCITX_ADDON_DIR") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => user_data_home()?.join("fcitx5").join("addon"),
    };
    Ok((
        lib_dir.join("fcitx5-vinput.so"),
        metadata_dir.join("vinput.conf"),
    ))
}

fn fcitx_addon_status_json(module_path: &Path, metadata_path: &Path) -> serde_json::Value {
    let module_exists = module_path.exists();
    if !metadata_path.exists() {
        return serde_json::json!({
            "user_module_path": module_path,
            "user_module_exists": module_exists,
            "user_addon_metadata_path": metadata_path,
            "user_addon_metadata_exists": false,
            "user_addon_library": null,
            "user_addon_library_matches": false,
            "user_addon_type": null,
            "read_error": null,
        });
    }

    match fs::read_to_string(metadata_path) {
        Ok(contents) => {
            let library = activation_service_field(&contents, "Library");
            serde_json::json!({
                "user_module_path": module_path,
                "user_module_exists": module_exists,
                "user_addon_metadata_path": metadata_path,
                "user_addon_metadata_exists": true,
                "user_addon_library": library,
                "user_addon_library_matches": library.as_deref() == Some("fcitx5-vinput"),
                "user_addon_type": activation_service_field(&contents, "Type"),
                "read_error": null,
            })
        }
        Err(error) => serde_json::json!({
            "user_module_path": module_path,
            "user_module_exists": module_exists,
            "user_addon_metadata_path": metadata_path,
            "user_addon_metadata_exists": true,
            "user_addon_library": null,
            "user_addon_library_matches": false,
            "user_addon_type": null,
            "read_error": error.to_string(),
        }),
    }
}

fn user_activation_service_json(path: &Path) -> serde_json::Value {
    let exists = path.exists();
    if !exists {
        return serde_json::json!({
            "user_service_path": path,
            "user_service_exists": false,
            "user_service_name": null,
            "user_service_name_matches": false,
            "user_service_exec": null,
            "read_error": null,
        });
    }

    match fs::read_to_string(path) {
        Ok(contents) => {
            let name = activation_service_field(&contents, "Name");
            serde_json::json!({
                "user_service_path": path,
                "user_service_exists": true,
                "user_service_name": name,
                "user_service_name_matches": name.as_deref() == Some(dbus::SERVICE_BUS_NAME),
                "user_service_exec": activation_service_field(&contents, "Exec"),
                "read_error": null,
            })
        }
        Err(error) => serde_json::json!({
            "user_service_path": path,
            "user_service_exists": true,
            "user_service_name": null,
            "user_service_name_matches": false,
            "user_service_exec": null,
            "read_error": error.to_string(),
        }),
    }
}

fn activation_service_field(contents: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    contents
        .lines()
        .find_map(|line| line.strip_prefix(&prefix).map(ToOwned::to_owned))
}

fn audio_devices_json(config: &VinputConfig) -> anyhow::Result<serde_json::Value> {
    let capture_target = CaptureTarget::from_config_value(&config.global.capture_device)
        .context("parse configured capture device")?;
    let audio_report = enumerate_audio_devices();
    Ok(serde_json::json!({
        "ok": true,
        "capture_device": config.global.capture_device,
        "capture_target": capture_target_json(&capture_target),
        "backend": audio_devices_backend_name(),
        "live": audio_report.live,
        "devices": audio_report.devices,
        "enumeration_error": audio_report.enumeration_error,
    }))
}

fn capture_target_json(target: &CaptureTarget) -> serde_json::Value {
    match target {
        CaptureTarget::Default => serde_json::json!({"kind": "default", "target_object": null}),
        CaptureTarget::Object(value) => {
            serde_json::json!({"kind": "object", "target_object": value})
        }
    }
}

fn write_activation_service(
    daemon: &Path,
    config: Option<&Path>,
    configured_backends: bool,
    audio_backend: Option<&str>,
    daemon_args: &[String],
    user: bool,
    output: Option<&Path>,
) -> anyhow::Result<()> {
    let mut args = vec!["--dbus".to_owned()];
    if configured_backends {
        args.push("--configured-backends".to_owned());
    }
    if let Some(config) = config {
        args.push("--config".to_owned());
        args.push(config.to_string_lossy().into_owned());
    }
    if let Some(audio_backend) = audio_backend {
        args.push("--audio-backend".to_owned());
        args.push(audio_backend.to_owned());
    }
    args.extend(daemon_args.iter().cloned());

    let mut exec_parts = Vec::with_capacity(args.len() + 1);
    exec_parts.push(quote_exec_arg(&daemon.to_string_lossy()));
    exec_parts.extend(args.iter().map(|arg| quote_exec_arg(arg)));

    let service = format!(
        "[D-BUS Service]\nName={}\nExec={}\n",
        dbus::SERVICE_BUS_NAME,
        exec_parts.join(" ")
    );

    let user_output;
    let output = if user {
        user_output = user_activation_service_path()?;
        Some(user_output.as_path())
    } else {
        output
    };

    if let Some(output) = output {
        if let Some(parent) = output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).with_context(|| {
                format!("create activation service directory `{}`", parent.display())
            })?;
        }
        fs::write(output, service)
            .with_context(|| format!("write activation service `{}`", output.display()))?;
    } else {
        print!("{service}");
    }
    Ok(())
}

fn print_user_activation_service_status() -> anyhow::Result<()> {
    let path = user_activation_service_path()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&user_activation_service_json(&path))?
    );
    Ok(())
}

fn remove_user_activation_service() -> anyhow::Result<()> {
    let path = user_activation_service_path()?;
    let existed = path.exists();
    if existed {
        fs::remove_file(&path)
            .with_context(|| format!("remove activation service `{}`", path.display()))?;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "ok": true,
            "removed": existed,
            "user_service_path": path,
        }))?
    );
    Ok(())
}

fn user_activation_service_path() -> anyhow::Result<PathBuf> {
    Ok(user_data_home()?
        .join("dbus-1")
        .join("services")
        .join("org.fcitx.Vinput.service"))
}

fn user_data_home() -> anyhow::Result<PathBuf> {
    match std::env::var_os("XDG_DATA_HOME") {
        Some(value) if !value.is_empty() => Ok(PathBuf::from(value)),
        _ => Ok(user_home()?.join(".local/share")),
    }
}

fn user_cache_home() -> anyhow::Result<PathBuf> {
    match std::env::var_os("XDG_CACHE_HOME") {
        Some(value) if !value.is_empty() => Ok(PathBuf::from(value)),
        _ => Ok(user_home()?.join(".cache")),
    }
}

fn default_model_root() -> anyhow::Result<PathBuf> {
    Ok(user_data_home()?.join("fcitx-vinput").join("models"))
}

fn default_model_install_staging_root() -> anyhow::Result<PathBuf> {
    Ok(user_cache_home()?
        .join("fcitx-vinput")
        .join("model-install"))
}

fn user_home() -> anyhow::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .context("resolve user path: HOME is unset and XDG_DATA_HOME is unset")?;
    Ok(PathBuf::from(home))
}

fn quote_exec_arg(value: &str) -> String {
    if value.is_empty() {
        return "''".to_owned();
    }
    if value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-' | b':' | b'=')
    }) {
        return value.to_owned();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

struct AudioDevicesReport {
    devices: Vec<vinput_audio::AudioDeviceInfo>,
    live: bool,
    enumeration_error: Option<String>,
}

#[cfg(feature = "pipewire-backend")]
fn enumerate_audio_devices() -> AudioDevicesReport {
    use vinput_audio::AudioDeviceEnumerator as _;

    let mut enumerator = vinput_audio::pipewire_backend::PipeWireDeviceEnumerator;
    match enumerator
        .enumerate_audio_sources()
        .context("enumerate PipeWire audio sources")
    {
        Ok(devices) => AudioDevicesReport {
            devices,
            live: true,
            enumeration_error: None,
        },
        Err(error) => AudioDevicesReport {
            devices: Vec::new(),
            live: false,
            enumeration_error: Some(format!("{error:#}")),
        },
    }
}

#[cfg(not(feature = "pipewire-backend"))]
fn enumerate_audio_devices() -> AudioDevicesReport {
    AudioDevicesReport {
        devices: Vec::new(),
        live: false,
        enumeration_error: None,
    }
}

#[cfg(feature = "pipewire-backend")]
fn audio_devices_backend_name() -> &'static str {
    "pipewire"
}

#[cfg(not(feature = "pipewire-backend"))]
fn audio_devices_backend_name() -> &'static str {
    "unavailable"
}

fn config_summary_json(config: &VinputConfig) -> serde_json::Value {
    let summary = config.summary();
    serde_json::json!({
        "ok": true,
        "version": summary.version,
        "active_scene": summary.active_scene,
        "active_provider": summary.active_provider,
        "scene_count": summary.scene_count,
        "provider_count": summary.provider_count,
        "registry_mirror_count": summary.registry_mirror_count,
    })
}

fn print_registry_summary() -> anyhow::Result<()> {
    let config = VinputConfig::bundled_default().context("parse bundled config")?;
    let index_asset = AssetEntry {
        path: "index.json".to_owned(),
        sha256: None,
        size_bytes: None,
    };
    let summary = serde_json::json!({
        "base_url_count": config.registry.base_urls.len(),
        "index_urls": index_asset.resolved_urls(&config.registry),
    });
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

struct ModelListOwnedRequest {
    available: bool,
    installed: bool,
    model_root: Option<PathBuf>,
    registry: Option<PathBuf>,
    i18n: Option<PathBuf>,
    config: Option<PathBuf>,
    locale: String,
    json_output: bool,
}

fn handle_model_list_command(request: &ModelListOwnedRequest) -> anyhow::Result<()> {
    print_model_list(ModelListRequest {
        available: request.available,
        installed: request.installed,
        model_root: request.model_root.as_deref(),
        registry_path: request.registry.as_deref(),
        i18n_path: request.i18n.as_deref(),
        config_path: request.config.as_ref(),
        locale: &request.locale,
        json_output: request.json_output,
    })
}

struct LoadedLiveModelRegistry {
    registry: LiveModelRegistry,
    source_json: serde_json::Value,
    source_label: String,
    remote_base_url: Option<String>,
}

struct LoadedLiveI18n {
    i18n: Option<LiveRegistryI18n>,
    source_json: serde_json::Value,
    source_label: String,
}

struct FetchedText {
    url: String,
    text: String,
}

#[derive(Clone, Copy)]
struct ModelListRequest<'a> {
    available: bool,
    installed: bool,
    model_root: Option<&'a Path>,
    registry_path: Option<&'a Path>,
    i18n_path: Option<&'a Path>,
    config_path: Option<&'a PathBuf>,
    locale: &'a str,
    json_output: bool,
}

fn print_model_list(request: ModelListRequest<'_>) -> anyhow::Result<()> {
    if request.available && request.installed {
        anyhow::bail!("model list cannot combine --available and --installed");
    }
    if request.installed {
        return print_installed_model_list(request.model_root, request.json_output);
    }

    let (loaded, i18n) = load_live_model_catalog(
        request.registry_path,
        request.i18n_path,
        request.config_path,
        request.locale,
    )?;
    let models = loaded
        .registry
        .items
        .iter()
        .map(|model| live_model_list_json(model, i18n.i18n.as_ref()))
        .collect::<Vec<_>>();
    let output = serde_json::json!({
        "ok": true,
        "source": loaded.source_json,
        "i18n": i18n.source_json,
        "model_count": models.len(),
        "models": models,
    });

    if request.json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_model_list_text(&loaded, &i18n);
    }
    Ok(())
}

fn print_installed_model_list(model_root: Option<&Path>, json_output: bool) -> anyhow::Result<()> {
    let model_root = match model_root {
        Some(path) => path.to_path_buf(),
        None => default_model_root()?,
    };
    let models = load_installed_model_list(&model_root)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&installed_model_list_json(&model_root, &models))?
        );
    } else {
        print_installed_model_list_text(&model_root, &models);
    }
    Ok(())
}

fn load_installed_model_list(model_root: &Path) -> anyhow::Result<Vec<InstalledModelInfo>> {
    if !model_root.exists() {
        return Ok(Vec::new());
    }
    if !model_root.is_dir() {
        anyhow::bail!("model root `{}` is not a directory", model_root.display());
    }
    let mut models = Vec::new();
    for entry in fs::read_dir(model_root)
        .with_context(|| format!("read model root `{}`", model_root.display()))?
    {
        let entry =
            entry.with_context(|| format!("read entry under `{}`", model_root.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("inspect model root entry `{}`", path.display()))?;
        if file_type.is_dir() && path.join("vinput-model.json").is_file() {
            models.push(load_installed_model_info(&path)?);
        }
    }
    models.sort_by(|left, right| left.model_dir.cmp(&right.model_dir));
    Ok(models)
}

fn print_model_info(
    id_or_short_id: &str,
    registry_path: Option<&Path>,
    i18n_path: Option<&Path>,
    config_path: Option<&PathBuf>,
    locale: &str,
    json_output: bool,
) -> anyhow::Result<()> {
    if is_model_path_selector(id_or_short_id) {
        let info = load_installed_model_info(Path::new(id_or_short_id))?;
        if json_output {
            println!(
                "{}",
                serde_json::to_string_pretty(&installed_model_info_json(&info)?)?
            );
        } else {
            print_installed_model_info_text(&info);
        }
        return Ok(());
    }

    let (loaded, i18n) = load_live_model_catalog(registry_path, i18n_path, config_path, locale)?;
    let model = loaded
        .registry
        .model_by_id_or_short_id(id_or_short_id)
        .with_context(|| format!("unknown model id or short_id `{id_or_short_id}`"))?;
    let output = live_model_info_json(model, i18n.i18n.as_ref(), &loaded, &i18n)?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_model_info_text(model, i18n.i18n.as_ref(), &loaded, &i18n);
    }
    Ok(())
}

struct InstalledModelInfo {
    model_dir: PathBuf,
    metadata_path: PathBuf,
    metadata: LiveVinputModelMetadata,
    files: Vec<String>,
    file_count: usize,
}

fn is_model_path_selector(selector: &str) -> bool {
    let path = Path::new(selector);
    path.is_absolute() || selector.contains('/')
}

fn load_installed_model_info(model_dir: &Path) -> anyhow::Result<InstalledModelInfo> {
    if !model_dir.exists() {
        anyhow::bail!(
            "installed model directory `{}` does not exist",
            model_dir.display()
        );
    }
    if !model_dir.is_dir() {
        anyhow::bail!(
            "installed model path `{}` is not a directory",
            model_dir.display()
        );
    }
    let metadata_path = model_dir.join("vinput-model.json");
    let metadata_text = fs::read_to_string(&metadata_path).with_context(|| {
        format!(
            "read installed model metadata `{}`",
            metadata_path.display()
        )
    })?;
    let metadata =
        serde_json::from_str::<LiveVinputModelMetadata>(&metadata_text).with_context(|| {
            format!(
                "parse installed model metadata `{}`",
                metadata_path.display()
            )
        })?;
    let files = collect_installed_model_files(model_dir)?;
    let file_count = files.len();
    Ok(InstalledModelInfo {
        model_dir: model_dir.to_path_buf(),
        metadata_path,
        metadata,
        files,
        file_count,
    })
}

fn collect_installed_model_files(model_dir: &Path) -> anyhow::Result<Vec<String>> {
    let mut files = Vec::new();
    collect_installed_model_files_inner(model_dir, model_dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_installed_model_files_inner(
    root: &Path,
    current: &Path,
    files: &mut Vec<String>,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(current)
        .with_context(|| format!("read installed model directory `{}`", current.display()))?
    {
        let entry = entry.with_context(|| format!("read entry under `{}`", current.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("inspect installed model entry `{}`", path.display()))?;
        if file_type.is_dir() {
            collect_installed_model_files_inner(root, &path, files)?;
        } else if file_type.is_file() {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            files.push(relative.to_string_lossy().into_owned());
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct ModelInstallPlanRequest<'a> {
    id_or_short_id: &'a str,
    registry_path: Option<&'a Path>,
    i18n_path: Option<&'a Path>,
    config_path: Option<&'a PathBuf>,
    locale: &'a str,
    model_root: Option<&'a Path>,
    staging_root: Option<&'a Path>,
    dry_run: bool,
    json_output: bool,
}

fn print_model_install_plan(request: ModelInstallPlanRequest<'_>) -> anyhow::Result<()> {
    let (loaded, i18n) = load_live_model_catalog(
        request.registry_path,
        request.i18n_path,
        request.config_path,
        request.locale,
    )?;
    let model = loaded
        .registry
        .model_by_id_or_short_id(request.id_or_short_id)
        .with_context(|| format!("unknown model id or short_id `{}`", request.id_or_short_id))?;
    let model_root = match request.model_root {
        Some(path) => path.to_path_buf(),
        None => default_model_root()?,
    };
    let staging_root = match request.staging_root {
        Some(path) => path.to_path_buf(),
        None => default_model_install_staging_root()?,
    };

    if request.dry_run {
        let output = live_model_install_plan_json(
            model,
            i18n.i18n.as_ref(),
            &loaded,
            &i18n,
            &model_root,
            &staging_root,
        )?;
        if request.json_output {
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            print_model_install_plan_text(model, i18n.i18n.as_ref(), &model_root, &staging_root)?;
        }
        return Ok(());
    }

    let model_dir = model_root.join(managed_model_dir_name(model));
    let staging_dir = staging_root.join(managed_model_dir_name(model));
    let source = ReqwestRegistryAssetSource::with_timeout(Duration::from_secs(300));
    let installed = install_live_model(
        &source,
        &LiveModelInstallRequest {
            model,
            model_dir: model_dir.clone(),
            staging_dir: staging_dir.clone(),
        },
    )
    .with_context(|| format!("install live model `{}`", model.id))?;

    if request.json_output {
        let output =
            live_model_install_result_json(model, i18n.i18n.as_ref(), &loaded, &i18n, &installed)?;
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_model_install_result_text(model, i18n.i18n.as_ref(), &installed);
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct ModelRemoveRequest<'a> {
    selector: &'a str,
    registry_path: Option<&'a Path>,
    i18n_path: Option<&'a Path>,
    config_path: Option<&'a PathBuf>,
    locale: &'a str,
    model_root: Option<&'a Path>,
    dry_run: bool,
    yes: bool,
    json_output: bool,
}

struct ModelRemovePlan {
    selector: String,
    selector_kind: String,
    model_root: PathBuf,
    target_path: PathBuf,
    exists: bool,
    is_dir: bool,
    resolved_model_id: Option<String>,
    resolved_short_id: Option<String>,
    resolved_title: Option<String>,
    removed: bool,
}

fn print_model_remove_plan(request: ModelRemoveRequest<'_>) -> anyhow::Result<()> {
    if request.dry_run && request.yes {
        anyhow::bail!("model remove cannot combine --dry-run and --yes");
    }
    if !request.dry_run && !request.yes {
        anyhow::bail!(
            "model remove requires --yes to delete; rerun with --dry-run to inspect the removal plan"
        );
    }
    let model_root = match request.model_root {
        Some(path) => path.to_path_buf(),
        None => default_model_root()?,
    };
    let mut plan = build_model_remove_plan(request, &model_root)?;
    if request.yes {
        remove_managed_model_dir(&plan, request.config_path)?;
        plan.removed = true;
    }
    if request.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&model_remove_plan_json(&plan))?
        );
    } else {
        print_model_remove_plan_text(&plan);
    }
    Ok(())
}

fn build_model_remove_plan(
    request: ModelRemoveRequest<'_>,
    model_root: &Path,
) -> anyhow::Result<ModelRemovePlan> {
    let resolution = resolve_model_remove_target(
        request.selector,
        request.registry_path,
        request.i18n_path,
        request.config_path,
        request.locale,
        model_root,
    );
    ensure_managed_remove_target(model_root, &resolution.target_path)?;
    let metadata = fs::metadata(&resolution.target_path);
    let (exists, is_dir) = match metadata {
        Ok(metadata) => (true, metadata.is_dir()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (false, false),
        Err(error) => {
            anyhow::bail!(
                "inspect model remove target `{}`: {}",
                resolution.target_path.display(),
                error.kind()
            );
        }
    };
    Ok(ModelRemovePlan {
        selector: request.selector.to_owned(),
        selector_kind: resolution.selector_kind,
        model_root: model_root.to_path_buf(),
        target_path: resolution.target_path,
        exists,
        is_dir,
        resolved_model_id: resolution.resolved_model_id,
        resolved_short_id: resolution.resolved_short_id,
        resolved_title: resolution.resolved_title,
        removed: false,
    })
}

fn remove_managed_model_dir(
    plan: &ModelRemovePlan,
    config_path: Option<&PathBuf>,
) -> anyhow::Result<()> {
    if !plan.exists {
        anyhow::bail!(
            "model remove target `{}` does not exist",
            plan.target_path.display()
        );
    }
    if !plan.is_dir {
        anyhow::bail!(
            "model remove target `{}` is not a directory",
            plan.target_path.display()
        );
    }
    ensure_model_not_active(&plan.target_path, config_path)?;
    fs::remove_dir_all(&plan.target_path)
        .with_context(|| format!("remove model directory `{}`", plan.target_path.display()))
}

fn ensure_model_not_active(
    target_path: &Path,
    config_path: Option<&PathBuf>,
) -> anyhow::Result<()> {
    let config = match config_path {
        Some(config_path) => load_config_file(config_path)?,
        None => VinputConfig::bundled_default().context("parse bundled config")?,
    };
    for provider in &config.asr.providers {
        if provider.kind == AsrProviderKind::Local
            && let Some(model) = &provider.model
            && same_path_text(Path::new(model), target_path)
        {
            anyhow::bail!(
                "refusing to remove active model `{}` used by ASR provider `{}`",
                target_path.display(),
                provider.id
            );
        }
    }
    Ok(())
}

struct ModelRemoveResolution {
    target_path: PathBuf,
    selector_kind: String,
    resolved_model_id: Option<String>,
    resolved_short_id: Option<String>,
    resolved_title: Option<String>,
}

fn resolve_model_remove_target(
    selector: &str,
    registry_path: Option<&Path>,
    i18n_path: Option<&Path>,
    config_path: Option<&PathBuf>,
    locale: &str,
    model_root: &Path,
) -> ModelRemoveResolution {
    let selector_path = Path::new(selector);
    if selector_path.is_absolute() || selector.contains('/') {
        return ModelRemoveResolution {
            target_path: selector_path.to_path_buf(),
            selector_kind: "path".to_owned(),
            resolved_model_id: None,
            resolved_short_id: None,
            resolved_title: None,
        };
    }

    if let Ok((loaded, i18n)) =
        load_live_model_catalog(registry_path, i18n_path, config_path, locale)
        && let Some(model) = loaded.registry.model_by_id_or_short_id(selector)
    {
        return ModelRemoveResolution {
            target_path: model_root.join(managed_model_dir_name(model)),
            selector_kind: "registry".to_owned(),
            resolved_model_id: Some(model.id.clone()),
            resolved_short_id: model.short_id.clone(),
            resolved_title: Some(model.resolved_title(i18n.i18n.as_ref())),
        };
    }

    ModelRemoveResolution {
        target_path: model_root.join(safe_path_component(selector)),
        selector_kind: "managed-dir".to_owned(),
        resolved_model_id: None,
        resolved_short_id: None,
        resolved_title: None,
    }
}

fn ensure_managed_remove_target(model_root: &Path, target_path: &Path) -> anyhow::Result<()> {
    let relative = target_path.strip_prefix(model_root).with_context(|| {
        format!(
            "refusing to remove `{}` because it is outside model root `{}`",
            target_path.display(),
            model_root.display()
        )
    })?;
    if relative.as_os_str().is_empty() {
        anyhow::bail!(
            "refusing to remove model root `{}`; select a managed model directory",
            model_root.display()
        );
    }
    for component in relative.components() {
        if matches!(
            component,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        ) {
            anyhow::bail!(
                "refusing unsafe model remove target `{}`",
                target_path.display()
            );
        }
    }
    Ok(())
}

fn model_remove_plan_json(plan: &ModelRemovePlan) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": !plan.removed,
        "will_remove": plan.removed,
        "removed": plan.removed,
        "selector": {
            "input": plan.selector,
            "kind": plan.selector_kind,
            "resolved_model_id": plan.resolved_model_id,
            "resolved_short_id": plan.resolved_short_id,
            "title": plan.resolved_title,
        },
        "target": {
            "model_root": plan.model_root,
            "path": plan.target_path,
            "exists": plan.exists && !plan.removed,
            "is_dir": plan.is_dir,
            "managed": true,
        },
        "next_steps": [
            "run vinput model use --dry-run to verify the active config does not point at the removed model",
            "restart or reload the daemon after removing an inactive model"
        ],
    })
}

fn print_model_remove_plan_text(plan: &ModelRemovePlan) {
    println!("dry_run: {}", !plan.removed);
    println!("selector: {}", plan.selector);
    println!("selector_kind: {}", plan.selector_kind);
    println!("model_root: {}", plan.model_root.display());
    println!("target_path: {}", plan.target_path.display());
    println!("exists: {}", plan.exists && !plan.removed);
    println!("is_dir: {}", plan.is_dir);
    println!("managed: true");
    println!("will_remove: {}", plan.removed);
    println!("removed: {}", plan.removed);
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct ModelUseRequest<'a> {
    selector: &'a str,
    registry_path: Option<&'a Path>,
    i18n_path: Option<&'a Path>,
    config_path: Option<&'a PathBuf>,
    locale: &'a str,
    provider: Option<&'a str>,
    output_path: Option<&'a Path>,
    in_place: bool,
    model_root: Option<&'a Path>,
    reload_daemon: bool,
    dry_run: bool,
    json_output: bool,
}

#[allow(clippy::pedantic)]
struct ModelUsePreview {
    config_path: Option<PathBuf>,
    provider_id: String,
    provider_kind: AsrProviderKind,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    reload_daemon: bool,
    reloaded_daemon: bool,
    wrote_config: bool,
    before_active_provider: String,
    before_model: Option<String>,
    after_active_provider: String,
    after_model: String,
    selector_kind: String,
    selector: String,
    resolved_model_id: Option<String>,
    resolved_short_id: Option<String>,
    resolved_title: Option<String>,
}

#[derive(Clone)]
enum ModelUseWriteTarget {
    DryRun,
    Output(PathBuf),
    InPlace {
        config_path: PathBuf,
        backup_path: PathBuf,
    },
}

impl ModelUseWriteTarget {
    fn output_path(&self) -> Option<PathBuf> {
        match self {
            Self::DryRun => None,
            Self::Output(path) => Some(path.clone()),
            Self::InPlace { config_path, .. } => Some(config_path.clone()),
        }
    }

    fn backup_path(&self) -> Option<PathBuf> {
        match self {
            Self::InPlace { backup_path, .. } => Some(backup_path.clone()),
            Self::DryRun | Self::Output(_) => None,
        }
    }

    fn in_place(&self) -> bool {
        matches!(self, Self::InPlace { .. })
    }
}

fn model_use_write_target(request: &ModelUseRequest<'_>) -> anyhow::Result<ModelUseWriteTarget> {
    if request.output_path.is_some() && request.in_place {
        anyhow::bail!("model use cannot combine --output and --in-place");
    }
    if request.dry_run {
        return Ok(ModelUseWriteTarget::DryRun);
    }
    if request.in_place {
        let config_path = request
            .config_path
            .with_context(|| "model use --in-place requires --config <path>")?;
        return Ok(ModelUseWriteTarget::InPlace {
            config_path: config_path.clone(),
            backup_path: config_backup_path(config_path),
        });
    }
    let output_path = request.output_path.with_context(|| {
        "model use writes require --output <path> or --in-place; rerun with --dry-run to inspect the config patch"
    })?;
    if let Some(config_path) = request.config_path
        && same_path_text(config_path, output_path)
    {
        anyhow::bail!(
            "refusing to overwrite input config `{}` with --output; use --in-place to create a backup",
            config_path.display()
        );
    }
    Ok(ModelUseWriteTarget::Output(output_path.to_path_buf()))
}

fn config_backup_path(config_path: &Path) -> PathBuf {
    let mut backup = config_path.as_os_str().to_os_string();
    backup.push(".bak");
    PathBuf::from(backup)
}

struct ModelUseResolution {
    model_value: String,
    selector_kind: String,
    resolved_model_id: Option<String>,
    resolved_short_id: Option<String>,
    resolved_title: Option<String>,
}

fn print_model_use_preview(request: ModelUseRequest<'_>) -> anyhow::Result<()> {
    let write_target = model_use_write_target(&request)?;

    let mut config = match request.config_path {
        Some(config_path) => load_config_file(config_path)?,
        None => VinputConfig::bundled_default().context("parse bundled config")?,
    };
    let model_root = match request.model_root {
        Some(path) => path.to_path_buf(),
        None => default_model_root()?,
    };
    let resolution = resolve_model_use_value(
        request.selector,
        request.registry_path,
        request.i18n_path,
        request.config_path,
        request.locale,
        &model_root,
    );
    let provider_id = request
        .provider
        .map_or_else(|| config.asr.active_provider.clone(), str::to_owned);
    let provider_index = config
        .asr
        .providers
        .iter()
        .position(|provider| provider.id == provider_id)
        .with_context(|| format!("ASR provider `{provider_id}` not found in config"))?;
    let provider = &config.asr.providers[provider_index];
    if provider.kind != AsrProviderKind::Local {
        anyhow::bail!("ASR provider `{provider_id}` is not local and cannot use a managed model");
    }

    let mut preview = ModelUsePreview {
        config_path: request.config_path.cloned(),
        provider_id: provider_id.clone(),
        provider_kind: provider.kind.clone(),
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        reload_daemon: request.reload_daemon,
        reloaded_daemon: false,
        wrote_config: false,
        before_active_provider: config.asr.active_provider.clone(),
        before_model: provider.model.clone(),
        after_active_provider: provider_id,
        after_model: resolution.model_value,
        selector_kind: resolution.selector_kind,
        selector: request.selector.to_owned(),
        resolved_model_id: resolution.resolved_model_id,
        resolved_short_id: resolution.resolved_short_id,
        resolved_title: resolution.resolved_title,
    };

    if !request.dry_run {
        config
            .asr
            .active_provider
            .clone_from(&preview.after_active_provider);
        config.asr.providers[provider_index].model = Some(preview.after_model.clone());
        config.validate().context("validate updated config")?;
        write_model_use_config(&config, &write_target)?;
        preview.wrote_config = true;
        if request.reload_daemon {
            reload_asr_backend_via_dbus().context("model use demon update")?;
            preview.reloaded_daemon = true;
        }
    }

    if request.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&model_use_preview_json(&preview))?
        );
    } else {
        print_model_use_preview_text(&preview);
    }
    Ok(())
}

fn resolve_model_use_value(
    selector: &str,
    registry_path: Option<&Path>,
    i18n_path: Option<&Path>,
    config_path: Option<&PathBuf>,
    locale: &str,
    model_root: &Path,
) -> ModelUseResolution {
    let selector_path = Path::new(selector);
    if selector_path.is_absolute() || selector.contains('/') {
        return ModelUseResolution {
            model_value: selector_path.to_string_lossy().into_owned(),
            selector_kind: "path".to_owned(),
            resolved_model_id: None,
            resolved_short_id: None,
            resolved_title: None,
        };
    }

    if let Ok((loaded, i18n)) =
        load_live_model_catalog(registry_path, i18n_path, config_path, locale)
        && let Some(model) = loaded.registry.model_by_id_or_short_id(selector)
    {
        return ModelUseResolution {
            model_value: model_root
                .join(managed_model_dir_name(model))
                .to_string_lossy()
                .into_owned(),
            selector_kind: "registry".to_owned(),
            resolved_model_id: Some(model.id.clone()),
            resolved_short_id: model.short_id.clone(),
            resolved_title: Some(model.resolved_title(i18n.i18n.as_ref())),
        };
    }

    ModelUseResolution {
        model_value: model_root
            .join(safe_path_component(selector))
            .to_string_lossy()
            .into_owned(),
        selector_kind: "managed-dir".to_owned(),
        resolved_model_id: None,
        resolved_short_id: None,
        resolved_title: None,
    }
}

fn model_use_preview_json(preview: &ModelUsePreview) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": !preview.wrote_config,
        "will_write_config": preview.wrote_config,
        "wrote_config": preview.wrote_config,
        "config_path": preview.config_path,
        "output_path": preview.output_path,
        "backup_path": preview.backup_path,
        "in_place": preview.in_place,
        "reload_daemon": {
            "requested": preview.reload_daemon,
            "will_call_dbus": preview.reload_daemon && !preview.reloaded_daemon,
            "called": preview.reloaded_daemon,
            "dbus": {
                "service": dbus::SERVICE_BUS_NAME,
                "object_path": dbus::SERVICE_OBJECT_PATH,
                "interface": dbus::SERVICE_INTERFACE,
                "method": dbus::method::RELOAD_ASR_BACKEND,
            },
        },
        "selector": {
            "input": preview.selector,
            "kind": preview.selector_kind,
            "resolved_model_id": preview.resolved_model_id,
            "resolved_short_id": preview.resolved_short_id,
            "title": preview.resolved_title,
        },
        "patch": {
            "asr.active_provider": {
                "before": preview.before_active_provider,
                "after": preview.after_active_provider,
            },
            "asr.providers[].model": {
                "provider_id": preview.provider_id,
                "provider_type": format!("{:?}", preview.provider_kind).to_lowercase(),
                "before": preview.before_model,
                "after": preview.after_model,
            }
        },
        "next_steps": [
            "use the written config with vinput asr-state --config <path>",
            "restart or reload the daemon with the updated config"
        ],
    })
}

fn print_model_use_preview_text(preview: &ModelUsePreview) {
    println!("dry_run: {}", !preview.wrote_config);
    println!("selector: {}", preview.selector);
    println!("selector_kind: {}", preview.selector_kind);
    println!("provider_id: {}", preview.provider_id);
    println!("active_provider_before: {}", preview.before_active_provider);
    println!("active_provider_after: {}", preview.after_active_provider);
    println!(
        "model_before: {}",
        optional_str(preview.before_model.as_deref())
    );
    println!("model_after: {}", preview.after_model);
    println!("will_write_config: {}", preview.wrote_config);
    println!("wrote_config: {}", preview.wrote_config);
    if let Some(output_path) = &preview.output_path {
        println!("output_path: {}", output_path.display());
    }
    if let Some(backup_path) = &preview.backup_path {
        println!("backup_path: {}", backup_path.display());
    }
    println!("in_place: {}", preview.in_place);
    println!("reload_daemon_requested: {}", preview.reload_daemon);
    println!("daemon_reloaded: {}", preview.reloaded_daemon);
}

fn write_model_use_config(
    config: &VinputConfig,
    target: &ModelUseWriteTarget,
) -> anyhow::Result<()> {
    match target {
        ModelUseWriteTarget::DryRun => Ok(()),
        ModelUseWriteTarget::Output(output_path) => write_config_output(config, output_path),
        ModelUseWriteTarget::InPlace {
            config_path,
            backup_path,
        } => write_config_in_place(config, config_path, backup_path),
    }
}

fn write_config_output(config: &VinputConfig, output_path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = output_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("create config output directory `{}`", parent.display()))?;
    }
    let contents = serde_json::to_string_pretty(config).context("serialize updated config")?;
    write_file_atomically(output_path, &format!("{contents}\n"))
        .with_context(|| format!("write updated config `{}`", output_path.display()))
}

fn write_config_in_place(
    config: &VinputConfig,
    config_path: &Path,
    backup_path: &Path,
) -> anyhow::Result<()> {
    fs::copy(config_path, backup_path).with_context(|| {
        format!(
            "backup config `{}` to `{}`",
            config_path.display(),
            backup_path.display()
        )
    })?;
    write_config_output(config, config_path)
}

fn write_file_atomically(path: &Path, contents: &str) -> anyhow::Result<()> {
    let temp_path = atomic_temp_path(path);
    fs::write(&temp_path, contents)
        .with_context(|| format!("write temporary config `{}`", temp_path.display()))?;
    fs::rename(&temp_path, path).with_context(|| {
        format!(
            "rename temporary config `{}` to `{}`",
            temp_path.display(),
            path.display()
        )
    })
}

fn atomic_temp_path(path: &Path) -> PathBuf {
    let mut temp = path.as_os_str().to_os_string();
    temp.push(format!(".tmp-{}", std::process::id()));
    PathBuf::from(temp)
}

fn same_path_text(left: &Path, right: &Path) -> bool {
    left == right
}

fn load_live_model_catalog(
    registry_path: Option<&Path>,
    i18n_path: Option<&Path>,
    config_path: Option<&PathBuf>,
    locale: &str,
) -> anyhow::Result<(LoadedLiveModelRegistry, LoadedLiveI18n)> {
    let config = match config_path {
        Some(config_path) => load_config_file(config_path)?,
        None => VinputConfig::bundled_default().context("parse bundled config")?,
    };
    let loaded = load_live_model_registry(registry_path, &config.registry)?;
    let i18n = load_live_i18n(i18n_path, loaded.remote_base_url.as_deref(), locale)?;
    Ok((loaded, i18n))
}

fn load_live_model_registry(
    registry_path: Option<&Path>,
    registry_config: &RegistryConfig,
) -> anyhow::Result<LoadedLiveModelRegistry> {
    let registry_urls = live_registry_urls(registry_config, "registry/models.json");
    if let Some(path) = registry_path {
        let input = fs::read_to_string(path)
            .with_context(|| format!("read live model registry `{}`", path.display()))?;
        let registry = LiveModelRegistry::from_json_str(&input)
            .with_context(|| format!("validate live model registry `{}`", path.display()))?;
        return Ok(LoadedLiveModelRegistry {
            registry,
            source_json: serde_json::json!({
                "kind": "file",
                "path": path,
                "mirror_count": registry_config.base_urls.len(),
                "registry_urls": registry_urls,
            }),
            source_label: format!("file:{}", path.display()),
            remote_base_url: None,
        });
    }

    let source = ReqwestRegistryTextSource::with_timeout(Duration::from_secs(30));
    let fetched = fetch_text_from_mirrors(&source, &registry_urls)
        .context("fetch live model registry from configured mirrors")?;
    let registry = LiveModelRegistry::from_json_str(&fetched.text).with_context(|| {
        format!(
            "validate live model registry fetched from `{}`",
            fetched.url
        )
    })?;
    let remote_base_url = fetched
        .url
        .strip_suffix("/registry/models.json")
        .map(str::to_owned);
    let source_label = format!("url:{}", fetched.url);
    Ok(LoadedLiveModelRegistry {
        registry,
        source_json: serde_json::json!({
            "kind": "http",
            "url": fetched.url,
            "mirror_count": registry_config.base_urls.len(),
            "registry_urls": registry_urls,
        }),
        source_label,
        remote_base_url,
    })
}

fn load_live_i18n(
    i18n_path: Option<&Path>,
    remote_base_url: Option<&str>,
    locale: &str,
) -> anyhow::Result<LoadedLiveI18n> {
    if let Some(path) = i18n_path {
        let input = fs::read_to_string(path)
            .with_context(|| format!("read live registry i18n `{}`", path.display()))?;
        let i18n = LiveRegistryI18n::from_json_str(&input)
            .with_context(|| format!("parse live registry i18n `{}`", path.display()))?;
        return Ok(LoadedLiveI18n {
            i18n: Some(i18n),
            source_json: serde_json::json!({
                "kind": "file",
                "path": path,
                "loaded": true,
                "error": null,
            }),
            source_label: format!("file:{}", path.display()),
        });
    }

    let Some(remote_base_url) = remote_base_url else {
        return Ok(LoadedLiveI18n {
            i18n: None,
            source_json: serde_json::json!({
                "kind": "none",
                "loaded": false,
                "error": null,
            }),
            source_label: "none".to_owned(),
        });
    };
    if locale.trim().is_empty() {
        return Ok(LoadedLiveI18n {
            i18n: None,
            source_json: serde_json::json!({
                "kind": "none",
                "loaded": false,
                "error": "empty locale",
            }),
            source_label: "none".to_owned(),
        });
    }

    let url = join_url(remote_base_url, &format!("i18n/{}.json", locale.trim()));
    let source = ReqwestRegistryTextSource::with_timeout(Duration::from_secs(10));
    match source.fetch_registry_text(&url) {
        Ok(input) => match LiveRegistryI18n::from_json_str(&input) {
            Ok(i18n) => Ok(LoadedLiveI18n {
                i18n: Some(i18n),
                source_json: serde_json::json!({
                    "kind": "http",
                    "url": url,
                    "locale": locale.trim(),
                    "loaded": true,
                    "error": null,
                }),
                source_label: format!("url:{url}"),
            }),
            Err(error) => Ok(LoadedLiveI18n {
                i18n: None,
                source_json: serde_json::json!({
                    "kind": "http",
                    "url": url,
                    "locale": locale.trim(),
                    "loaded": false,
                    "error": error.to_string(),
                }),
                source_label: format!("url:{url} (i18n parse failed)"),
            }),
        },
        Err(error) => Ok(LoadedLiveI18n {
            i18n: None,
            source_json: serde_json::json!({
                "kind": "http",
                "url": url,
                "locale": locale.trim(),
                "loaded": false,
                "error": error,
            }),
            source_label: format!("url:{url} (i18n unavailable)"),
        }),
    }
}

fn fetch_text_from_mirrors(
    source: &impl RegistryTextSource,
    urls: &[String],
) -> anyhow::Result<FetchedText> {
    if urls.is_empty() {
        anyhow::bail!("no live registry mirrors configured");
    }

    let mut failures = Vec::new();
    for url in urls {
        match source.fetch_registry_text(url) {
            Ok(text) => {
                return Ok(FetchedText {
                    url: url.clone(),
                    text,
                });
            }
            Err(message) => failures.push(serde_json::json!({
                "url": url,
                "error": message,
            })),
        }
    }

    anyhow::bail!(
        "all live registry mirrors failed: {}",
        serde_json::to_string(&failures)?
    );
}

fn live_model_list_json(
    model: &LiveModelEntry,
    i18n: Option<&LiveRegistryI18n>,
) -> serde_json::Value {
    let support = live_model_support(model);
    serde_json::json!({
        "id": model.id,
        "short_id": model.short_id,
        "title": model.resolved_title(i18n),
        "description": model.resolved_description(i18n),
        "language": model.language,
        "size_bytes": model.size_bytes,
        "backend": model.backend(),
        "family": model.model_family(),
        "runtime": model_runtime(model),
        "supports_hotwords": model.supports_hotwords(),
        "supported": support.supported,
        "support": support.reason,
        "url_count": model.urls.len(),
        "urls": model.urls,
        "sha256": model.sha256,
    })
}

fn installed_model_list_json(
    model_root: &Path,
    models: &[InstalledModelInfo],
) -> serde_json::Value {
    let models = models
        .iter()
        .map(installed_model_list_item_json)
        .collect::<Vec<_>>();
    serde_json::json!({
        "ok": true,
        "source": {
            "kind": "installed",
            "model_root": model_root,
        },
        "model_count": models.len(),
        "models": models,
    })
}

fn installed_model_list_item_json(info: &InstalledModelInfo) -> serde_json::Value {
    serde_json::json!({
        "name": installed_model_dir_name(&info.model_dir),
        "model_dir": info.model_dir,
        "metadata_path": info.metadata_path,
        "backend": info.metadata.backend,
        "family": info.metadata.model_family(),
        "language": info.metadata.language,
        "runtime": info.metadata.runtime,
        "size_bytes": info.metadata.size_bytes,
        "supports_hotwords": info.metadata.supports_hotwords,
        "file_count": info.file_count,
        "files": info.files,
    })
}

fn installed_model_info_json(info: &InstalledModelInfo) -> anyhow::Result<serde_json::Value> {
    Ok(serde_json::json!({
        "ok": true,
        "source": {
            "kind": "installed",
            "path": info.model_dir,
            "metadata_path": info.metadata_path,
        },
        "model": {
            "model_dir": info.model_dir,
            "metadata_path": info.metadata_path,
            "backend": info.metadata.backend,
            "family": info.metadata.model_family(),
            "language": info.metadata.language,
            "runtime": info.metadata.runtime,
            "size_bytes": info.metadata.size_bytes,
            "supports_hotwords": info.metadata.supports_hotwords,
            "file_count": info.file_count,
            "files": info.files,
            "vinput_model": info.metadata.to_raw_value().context("serialize installed model metadata")?,
        },
    }))
}

fn live_model_info_json(
    model: &LiveModelEntry,
    i18n: Option<&LiveRegistryI18n>,
    loaded: &LoadedLiveModelRegistry,
    loaded_i18n: &LoadedLiveI18n,
) -> anyhow::Result<serde_json::Value> {
    let mut model_json = live_model_list_json(model, i18n);
    model_json["vinput_model"] =
        model
            .vinput_model
            .as_ref()
            .map_or(Ok(serde_json::Value::Null), |metadata| {
                metadata
                    .to_raw_value()
                    .context("serialize vinput_model metadata")
            })?;
    Ok(serde_json::json!({
        "ok": true,
        "source": loaded.source_json,
        "i18n": loaded_i18n.source_json,
        "model": model_json,
    }))
}

fn live_model_install_result_json(
    model: &LiveModelEntry,
    i18n: Option<&LiveRegistryI18n>,
    loaded: &LoadedLiveModelRegistry,
    loaded_i18n: &LoadedLiveI18n,
    installed: &LiveModelInstallResult,
) -> anyhow::Result<serde_json::Value> {
    let mut model_json = live_model_list_json(model, i18n);
    model_json["vinput_model"] =
        model
            .vinput_model
            .as_ref()
            .map_or(Ok(serde_json::Value::Null), |metadata| {
                metadata
                    .to_raw_value()
                    .context("serialize vinput_model metadata")
            })?;
    Ok(serde_json::json!({
        "ok": true,
        "dry_run": false,
        "source": loaded.source_json,
        "i18n": loaded_i18n.source_json,
        "model": model_json,
        "install": {
            "model_dir": installed.materialized.target_path,
            "metadata_path": installed.metadata_path,
            "archive_path": installed.staged_asset.path,
            "extract_path": installed.staged_archive.path,
            "materialize_source_path": installed.materialize_source_path,
            "replaced_existing": installed.materialized.replaced_existing,
            "checksum_verified": installed.checksum_verified(),
            "file_count": installed.staged_archive.file_count,
            "directory_count": installed.staged_archive.directory_count,
        },
        "will_write_config": false,
        "next_steps": [
            "run vinput model use to update config",
            "run vinput asr-state to verify native runtime loading"
        ],
    }))
}

fn live_model_install_plan_json(
    model: &LiveModelEntry,
    i18n: Option<&LiveRegistryI18n>,
    loaded: &LoadedLiveModelRegistry,
    loaded_i18n: &LoadedLiveI18n,
    model_root: &Path,
    staging_root: &Path,
) -> anyhow::Result<serde_json::Value> {
    let archive_file_name = model_archive_file_name(model)?;
    let archive_format = archive_format_label(archive_file_name);
    let archive_supported = ArchiveFormat::from_path(archive_file_name).is_some();
    let model_dir_name = managed_model_dir_name(model);
    let model_dir = model_root.join(&model_dir_name);
    let staging_dir = staging_root.join(&model_dir_name);
    let archive_path = staging_dir.join("archives").join(archive_file_name);
    let extract_dir = staging_dir.join("extract");
    let metadata_path = model_dir.join("vinput-model.json");

    let mut model_json = live_model_list_json(model, i18n);
    model_json["vinput_model"] =
        model
            .vinput_model
            .as_ref()
            .map_or(Ok(serde_json::Value::Null), |metadata| {
                metadata
                    .to_raw_value()
                    .context("serialize vinput_model metadata")
            })?;

    Ok(serde_json::json!({
        "ok": true,
        "dry_run": true,
        "source": loaded.source_json,
        "i18n": loaded_i18n.source_json,
        "model": model_json,
        "archive": {
            "file_name": archive_file_name,
            "format": archive_format,
            "supported": archive_supported,
            "supported_formats": ["tar", "tar_zst", "tar_bz2"],
            "urls": model.urls,
            "sha256": model.sha256,
            "size_bytes": model.size_bytes,
        },
        "target": {
            "model_root": model_root,
            "model_dir_name": model_dir_name,
            "model_dir": model_dir,
            "metadata_path": metadata_path,
            "config_model_value": model_dir,
        },
        "staging": {
            "staging_root": staging_root,
            "staging_dir": staging_dir,
            "archive_path": archive_path,
            "extract_dir": extract_dir,
        },
        "will_download": false,
        "will_extract": false,
        "will_write_config": false,
        "next_steps": [
            "download archive with mirror fallback",
            "verify sha256 before extraction",
            "extract with safe archive policy",
            "materialize model directory",
            "write vinput-model.json metadata",
            "run vinput model use to update config"
        ],
    }))
}

fn print_model_list_text(loaded: &LoadedLiveModelRegistry, i18n: &LoadedLiveI18n) {
    println!("registry_source: {}", loaded.source_label);
    println!("i18n_source: {}", i18n.source_label);
    println!("models: {}", loaded.registry.items.len());
    println!("id\tshort_id\tlanguage\tsize\tbackend\tfamily\tsupport\ttitle");
    for model in &loaded.registry.items {
        let support = live_model_support(model);
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            model.id,
            optional_str(model.short_id.as_deref()),
            optional_str(model.language.as_deref()),
            format_size_bytes(model.size_bytes),
            optional_str(model.backend()),
            optional_str(model.model_family()),
            support.reason,
            model.resolved_title(i18n.i18n.as_ref()),
        );
    }
}

fn print_installed_model_list_text(model_root: &Path, models: &[InstalledModelInfo]) {
    println!("model_root: {}", model_root.display());
    println!("models: {}", models.len());
    println!("name	path	language	size	backend	family	runtime	hotwords	files");
    for model in models {
        println!(
            "{}	{}	{}	{}	{}	{}	{}	{}	{}",
            installed_model_dir_name(&model.model_dir),
            model.model_dir.display(),
            optional_str(model.metadata.language.as_deref()),
            format_size_bytes(model.metadata.size_bytes),
            optional_str(model.metadata.backend.as_deref()),
            optional_str(model.metadata.model_family()),
            optional_str(model.metadata.runtime.as_deref()),
            model.metadata.supports_hotwords,
            model.file_count,
        );
    }
}

fn installed_model_dir_name(model_dir: &Path) -> String {
    model_dir.file_name().map_or_else(
        || model_dir.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn print_installed_model_info_text(info: &InstalledModelInfo) {
    println!("source: installed");
    println!("model_dir: {}", info.model_dir.display());
    println!("metadata_path: {}", info.metadata_path.display());
    println!(
        "backend: {}",
        optional_str(info.metadata.backend.as_deref())
    );
    println!("family: {}", optional_str(info.metadata.model_family()));
    println!(
        "language: {}",
        optional_str(info.metadata.language.as_deref())
    );
    println!(
        "runtime: {}",
        optional_str(info.metadata.runtime.as_deref())
    );
    println!("size: {}", format_size_bytes(info.metadata.size_bytes));
    println!("supports_hotwords: {}", info.metadata.supports_hotwords);
    println!("files: {}", info.file_count);
    for file in &info.files {
        println!("  - {file}");
    }
}

fn print_model_info_text(
    model: &LiveModelEntry,
    i18n: Option<&LiveRegistryI18n>,
    loaded: &LoadedLiveModelRegistry,
    loaded_i18n: &LoadedLiveI18n,
) {
    let support = live_model_support(model);
    println!("registry_source: {}", loaded.source_label);
    println!("i18n_source: {}", loaded_i18n.source_label);
    println!("id: {}", model.id);
    println!("short_id: {}", optional_str(model.short_id.as_deref()));
    println!("title: {}", model.resolved_title(i18n));
    println!(
        "description: {}",
        optional_str(model.resolved_description(i18n).as_deref())
    );
    println!("language: {}", optional_str(model.language.as_deref()));
    println!("size: {}", format_size_bytes(model.size_bytes));
    println!("backend: {}", optional_str(model.backend()));
    println!("family: {}", optional_str(model.model_family()));
    println!("runtime: {}", optional_str(model_runtime(model)));
    println!("support: {}", support.reason);
    println!("supported: {}", support.supported);
    println!("supports_hotwords: {}", model.supports_hotwords());
    println!("sha256: {}", optional_str(model.sha256.as_deref()));
    println!("urls:");
    for url in &model.urls {
        println!("  - {url}");
    }
}

fn print_model_install_result_text(
    model: &LiveModelEntry,
    i18n: Option<&LiveRegistryI18n>,
    installed: &LiveModelInstallResult,
) {
    println!("dry_run: false");
    println!("id: {}", model.id);
    println!("short_id: {}", optional_str(model.short_id.as_deref()));
    println!("title: {}", model.resolved_title(i18n));
    println!(
        "target_model_dir: {}",
        installed.materialized.target_path.display()
    );
    println!("metadata_path: {}", installed.metadata_path.display());
    println!("archive_path: {}", installed.staged_asset.path.display());
    println!("extract_path: {}", installed.staged_archive.path.display());
    println!(
        "materialize_source_path: {}",
        installed.materialize_source_path.display()
    );
    println!(
        "replaced_existing: {}",
        installed.materialized.replaced_existing
    );
    println!("checksum_verified: {}", installed.checksum_verified());
    println!("file_count: {}", installed.staged_archive.file_count);
    println!(
        "directory_count: {}",
        installed.staged_archive.directory_count
    );
    println!("will_write_config: false");
    println!(
        "next_step: vinput model use {}",
        optional_str(model.short_id.as_deref().or(Some(model.id.as_str())))
    );
}

fn print_model_install_plan_text(
    model: &LiveModelEntry,
    i18n: Option<&LiveRegistryI18n>,
    model_root: &Path,
    staging_root: &Path,
) -> anyhow::Result<()> {
    let archive_file_name = model_archive_file_name(model)?;
    let archive_format = archive_format_label(archive_file_name);
    let archive_supported = ArchiveFormat::from_path(archive_file_name).is_some();
    let model_dir_name = managed_model_dir_name(model);
    let model_dir = model_root.join(&model_dir_name);
    let staging_dir = staging_root.join(&model_dir_name);
    println!("dry_run: true");
    println!("id: {}", model.id);
    println!("short_id: {}", optional_str(model.short_id.as_deref()));
    println!("title: {}", model.resolved_title(i18n));
    println!("target_model_dir: {}", model_dir.display());
    println!(
        "metadata_path: {}",
        model_dir.join("vinput-model.json").display()
    );
    println!("config_model_value: {}", model_dir.display());
    println!("staging_dir: {}", staging_dir.display());
    println!("archive_file: {archive_file_name}");
    println!("archive_format: {archive_format}");
    println!("archive_supported: {archive_supported}");
    println!("sha256: {}", optional_str(model.sha256.as_deref()));
    println!("size: {}", format_size_bytes(model.size_bytes));
    println!("urls:");
    for url in &model.urls {
        println!("  - {url}");
    }
    println!("will_download: false");
    println!("will_extract: false");
    println!("will_write_config: false");
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct ModelSupport {
    supported: bool,
    reason: &'static str,
}

fn live_model_support(model: &LiveModelEntry) -> ModelSupport {
    match (model.backend(), model.model_family(), model_runtime(model)) {
        (Some("sherpa-offline"), Some("sense_voice"), Some("offline")) => ModelSupport {
            supported: true,
            reason: "supported",
        },
        (Some("sherpa-offline"), Some("sense_voice"), _) => ModelSupport {
            supported: false,
            reason: "unsupported-runtime",
        },
        (Some("sherpa-offline"), Some(_), _) => ModelSupport {
            supported: false,
            reason: "unsupported-family",
        },
        (Some(_), _, _) => ModelSupport {
            supported: false,
            reason: "unsupported-backend",
        },
        (None, _, _) => ModelSupport {
            supported: false,
            reason: "missing-backend",
        },
    }
}

fn model_runtime(model: &LiveModelEntry) -> Option<&str> {
    model
        .vinput_model
        .as_ref()
        .and_then(|metadata| metadata.runtime.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn model_archive_file_name(model: &LiveModelEntry) -> anyhow::Result<&str> {
    let first_url = model
        .urls
        .first()
        .context("live model has no download URLs")?;
    let file_name = first_url
        .rsplit('/')
        .next()
        .unwrap_or(first_url)
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    if file_name.is_empty() {
        anyhow::bail!(
            "live model `{}` has no archive file name in first URL",
            model.id
        );
    }
    Ok(file_name)
}

fn managed_model_dir_name(model: &LiveModelEntry) -> String {
    let preferred = model
        .short_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&model.id);
    safe_path_component(preferred)
}

fn safe_path_component(value: &str) -> String {
    let mut component = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    while component.starts_with('.') {
        component.remove(0);
    }
    while component.ends_with('.') {
        component.pop();
    }
    if component.is_empty() {
        "model".to_owned()
    } else {
        component
    }
}

fn archive_format_label(file_name: &str) -> &'static str {
    if ascii_suffix_eq(file_name, ".tar.zst") {
        "tar_zst"
    } else if ascii_suffix_eq(file_name, ".tar.bz2") || ascii_suffix_eq(file_name, ".tbz2") {
        "tar_bz2"
    } else if ascii_suffix_eq(file_name, ".tar.gz") || ascii_suffix_eq(file_name, ".tgz") {
        "tar_gz"
    } else if ascii_suffix_eq(file_name, ".tar") {
        "tar"
    } else {
        "unsupported"
    }
}

fn ascii_suffix_eq(value: &str, suffix: &str) -> bool {
    value
        .as_bytes()
        .get(value.len().saturating_sub(suffix.len())..)
        .is_some_and(|tail| tail.eq_ignore_ascii_case(suffix.as_bytes()))
}

fn optional_str(value: Option<&str>) -> &str {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("-")
}

fn format_size_bytes(size_bytes: Option<u64>) -> String {
    let Some(size_bytes) = size_bytes else {
        return "unknown".to_owned();
    };
    if size_bytes < 1024 {
        return format!("{size_bytes} B");
    }
    if size_bytes < 1024 * 1024 {
        return format_tenths(size_bytes, 1024, "KiB");
    }
    if size_bytes < 1024 * 1024 * 1024 {
        return format_tenths(size_bytes, 1024 * 1024, "MiB");
    }
    format_tenths(size_bytes, 1024 * 1024 * 1024, "GiB")
}

fn format_tenths(size_bytes: u64, unit: u64, label: &str) -> String {
    let tenths = u128::from(size_bytes) * 10 / u128::from(unit);
    format!("{}.{:01} {label}", tenths / 10, tenths % 10)
}

fn live_registry_urls(registry: &RegistryConfig, path: &str) -> Vec<String> {
    registry
        .base_urls
        .iter()
        .map(|base_url| join_url(base_url, path))
        .collect()
}

fn join_url(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

fn validate_registry_index(path: &PathBuf) -> anyhow::Result<()> {
    let input = fs::read_to_string(path)
        .with_context(|| format!("read registry index `{}`", path.display()))?;
    let index = RegistryIndex::from_json_str(&input)
        .with_context(|| format!("validate registry index `{}`", path.display()))?;
    let index_summary = index.summary();
    let summary = serde_json::json!({
        "ok": true,
        "version": index_summary.version,
        "model_count": index_summary.model_count,
        "adapter_count": index_summary.adapter_count,
        "asset_count": index_summary.asset_count,
    });
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn validate_config_file(path: &PathBuf, _summary_only: bool) -> anyhow::Result<()> {
    let input =
        fs::read_to_string(path).with_context(|| format!("read config `{}`", path.display()))?;
    let config = VinputConfig::from_json_str(&input)
        .with_context(|| format!("parse config `{}`", path.display()))?;
    config
        .validate()
        .with_context(|| format!("validate config `{}`", path.display()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&config_summary_json(&config))?
    );
    Ok(())
}

fn handle_config_example(
    kind: Option<ConfigExample>,
    list: bool,
    output: Option<&Path>,
) -> anyhow::Result<()> {
    if list {
        return list_config_examples();
    }
    let kind = kind.context("config example kind is required unless --list is set")?;
    export_config_example(kind, output)
}

fn list_config_examples() -> anyhow::Result<()> {
    let examples = ConfigExample::value_variants()
        .iter()
        .map(|kind| {
            serde_json::json!({
                "name": kind.to_possible_value().expect("config example has clap value").get_name(),
                "description": config_example_description(*kind),
            })
        })
        .collect::<Vec<_>>();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({"examples": examples}))?
    );
    Ok(())
}

fn export_config_example(kind: ConfigExample, output: Option<&Path>) -> anyhow::Result<()> {
    let contents = config_example_contents(kind);
    let config = VinputConfig::from_json_str(contents).context("parse bundled example config")?;
    config
        .validate()
        .context("validate bundled example config before export")?;

    if let Some(output) = output {
        if let Some(parent) = output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).with_context(|| {
                format!("create config example directory `{}`", parent.display())
            })?;
        }
        fs::write(output, contents)
            .with_context(|| format!("write config example `{}`", output.display()))?;
    } else {
        print!("{contents}");
    }
    Ok(())
}

fn config_example_description(kind: ConfigExample) -> &'static str {
    match kind {
        ConfigExample::Default => "upstream-compatible default config skeleton",
        ConfigExample::CommandDemo => "deterministic command ASR/text adapter demo",
        ConfigExample::ConfiguredPipewireLive => {
            "configured command backends for live PipeWire smoke"
        }
    }
}

fn config_example_contents(kind: ConfigExample) -> &'static str {
    match kind {
        ConfigExample::Default => include_str!("../../../data/default-config.json"),
        ConfigExample::CommandDemo => include_str!("../../../data/e2e-command-demo-config.json"),
        ConfigExample::ConfiguredPipewireLive => {
            include_str!("../../../data/e2e-configured-pipewire-live.json")
        }
    }
}

fn print_registry_plan(
    path: &PathBuf,
    config_path: Option<&PathBuf>,
    model_id: Option<&str>,
    adapter_id: Option<&str>,
    summary_only: bool,
) -> anyhow::Result<()> {
    let input = fs::read_to_string(path)
        .with_context(|| format!("read registry index `{}`", path.display()))?;
    let index = RegistryIndex::from_json_str(&input)
        .with_context(|| format!("validate registry index `{}`", path.display()))?;
    let config = match config_path {
        Some(config_path) => load_config_file(config_path)?,
        None => VinputConfig::bundled_default().context("parse bundled config")?,
    };
    let planned_assets = selected_registry_assets(&index, &config.registry, model_id, adapter_id)?;
    let plan_summary = AssetPlanSummary::from_assets(&planned_assets);
    let summary = if summary_only {
        serde_json::json!({
            "ok": true,
            "asset_count": plan_summary.asset_count,
            "known_size_bytes": plan_summary.known_size_bytes,
            "unknown_size_count": plan_summary.unknown_size_count,
        })
    } else {
        serde_json::json!({
            "ok": true,
            "asset_count": plan_summary.asset_count,
            "known_size_bytes": plan_summary.known_size_bytes,
            "unknown_size_count": plan_summary.unknown_size_count,
            "assets": planned_assets,
        })
    };
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn print_registry_install_plan(
    path: &PathBuf,
    target_root: &Path,
    config_path: Option<&PathBuf>,
    model_id: Option<&str>,
    adapter_id: Option<&str>,
    summary_only: bool,
) -> anyhow::Result<()> {
    let input = fs::read_to_string(path)
        .with_context(|| format!("read registry index `{}`", path.display()))?;
    let index = RegistryIndex::from_json_str(&input)
        .with_context(|| format!("validate registry index `{}`", path.display()))?;
    let config = match config_path {
        Some(config_path) => load_config_file(config_path)?,
        None => VinputConfig::bundled_default().context("parse bundled config")?,
    };
    let target_root = target_root.to_string_lossy();
    let plan = match (model_id, adapter_id) {
        (Some(model_id), None) => {
            index.install_model_plan(model_id, &config.registry, &target_root)?
        }
        (None, Some(adapter_id)) => {
            index.install_adapter_plan(adapter_id, &config.registry, &target_root)?
        }
        (None, None) => index.install_plan(&config.registry, &target_root),
        (Some(_), Some(_)) => unreachable!("clap prevents model and adapter together"),
    };
    let summary = if summary_only {
        serde_json::json!({
            "ok": true,
            "target_root": plan.target_root,
            "asset_count": plan.summary.asset_count,
            "known_size_bytes": plan.summary.known_size_bytes,
            "missing_checksum_count": plan.summary.missing_checksum_count,
        })
    } else {
        serde_json::json!({
            "ok": true,
            "target_root": plan.target_root,
            "asset_count": plan.summary.asset_count,
            "known_size_bytes": plan.summary.known_size_bytes,
            "missing_checksum_count": plan.summary.missing_checksum_count,
            "assets": plan.assets,
        })
    };
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn selected_registry_assets(
    index: &RegistryIndex,
    registry: &RegistryConfig,
    model_id: Option<&str>,
    adapter_id: Option<&str>,
) -> anyhow::Result<Vec<PlannedAsset>> {
    Ok(match (model_id, adapter_id) {
        (Some(model_id), None) => index.planned_model_assets(model_id, registry)?,
        (None, Some(adapter_id)) => index.planned_adapter_assets(adapter_id, registry)?,
        (None, None) => index.planned_assets(registry),
        (Some(_), Some(_)) => unreachable!("clap prevents model and adapter together"),
    })
}

fn load_config_file(path: &PathBuf) -> anyhow::Result<VinputConfig> {
    let config = VinputConfig::from_json_file(path)
        .with_context(|| format!("load config `{}`", path.display()))?;
    config
        .validate()
        .with_context(|| format!("validate config `{}`", path.display()))?;
    Ok(config)
}

//! `vinput` command-line prototype.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use vinput_asr::AsrBackendFactory;
use vinput_audio::CaptureTarget;
use vinput_config::{RegistryConfig, VinputConfig};
use vinput_protocol::{RecognitionPayload, ServiceStatus, dbus};
use vinput_registry::{AssetEntry, AssetPlanSummary, PlannedAsset, RegistryIndex};

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
    /// Generate an org.fcitx.Vinput D-Bus activation service file.
    ActivationService {
        /// Path to the vinput-daemon executable used by D-Bus activation.
        #[arg(long)]
        daemon: PathBuf,
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
            output,
        } => write_activation_service(
            &daemon,
            config.as_deref(),
            configured_backends,
            audio_backend.as_deref(),
            &daemon_args,
            user,
            output.as_deref(),
        ),
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
        Ok(path) => serde_json::json!({
            "user_service_path": path,
            "user_service_exists": path.exists(),
        }),
        Err(error) => serde_json::json!({
            "user_service_path": null,
            "user_service_exists": false,
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
    });
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
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

fn user_activation_service_path() -> anyhow::Result<PathBuf> {
    let data_home = match std::env::var_os("XDG_DATA_HOME") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => {
            let home = std::env::var_os("HOME").context(
                "resolve user activation service path: HOME is unset and XDG_DATA_HOME is unset",
            )?;
            PathBuf::from(home).join(".local/share")
        }
    };
    Ok(data_home
        .join("dbus-1")
        .join("services")
        .join("org.fcitx.Vinput.service"))
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

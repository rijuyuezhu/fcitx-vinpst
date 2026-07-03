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
use vinput_config::{RegistryConfig, VinputConfig};
use vinput_protocol::{RecognitionPayload, ServiceStatus, dbus};
use vinput_registry::{
    AssetEntry, AssetPlanSummary, LiveModelEntry, LiveModelRegistry, LiveRegistryI18n,
    PlannedAsset, RegistryIndex, RegistryTextSource, ReqwestRegistryTextSource,
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

/// Model-related commands backed by the live registry catalog.
#[derive(Debug, Subcommand)]
enum ModelCommand {
    /// List models from live registry/models.json metadata.
    List {
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

fn handle_model_command(command: ModelCommand) -> anyhow::Result<()> {
    match command {
        ModelCommand::List {
            registry,
            i18n,
            config,
            locale,
            json,
        } => print_model_list(
            registry.as_deref(),
            i18n.as_deref(),
            config.as_ref(),
            &locale,
            json,
        ),
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

fn print_model_list(
    registry_path: Option<&Path>,
    i18n_path: Option<&Path>,
    config_path: Option<&PathBuf>,
    locale: &str,
    json_output: bool,
) -> anyhow::Result<()> {
    let config = match config_path {
        Some(config_path) => load_config_file(config_path)?,
        None => VinputConfig::bundled_default().context("parse bundled config")?,
    };
    let loaded = load_live_model_registry(registry_path, &config.registry)?;
    let i18n = load_live_i18n(i18n_path, loaded.remote_base_url.as_deref(), locale)?;
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

    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_model_list_text(&loaded, &i18n);
    }
    Ok(())
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
        "runtime": model.vinput_model.as_ref().and_then(|metadata| metadata.runtime.as_deref()),
        "supports_hotwords": model.supports_hotwords(),
        "supported": support.supported,
        "support": support.reason,
        "url_count": model.urls.len(),
        "urls": model.urls,
        "sha256": model.sha256,
    })
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

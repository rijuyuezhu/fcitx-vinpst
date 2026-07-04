//! `vinput` command-line prototype.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
    time::Duration,
};

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use vinput_asr::AsrBackendFactory;
use vinput_audio::CaptureTarget;
use vinput_config::{
    AsrProviderConfig, AsrProviderKind, RegistryConfig, SceneDefinition, VinputConfig,
};
use vinput_protocol::{RecognitionPayload, ServiceStatus, TextAdapterState, dbus};
use vinput_registry::{
    ArchiveFormat, AssetEntry, AssetPlanSummary, LiveModelEntry, LiveModelInstallRequest,
    LiveModelInstallResult, LiveModelRegistry, LiveRegistryI18n, LiveVinputModelMetadata,
    PlannedAsset, RegistryIndex, RegistryTextSource, ReqwestRegistryAssetSource,
    ReqwestRegistryTextSource, install_live_model,
};
use vinput_text::{
    OpenAiCompatibleTextAdapter, ReqwestOpenAiCompatibleChatTransport, TextAdapter, TextRequest,
    build_openai_compatible_chat_request,
};

/// CLI for inspecting and controlling the vinput daemon.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Force machine-readable JSON output for JSON-capable subcommands.
    #[arg(short = 'j', long, global = true)]
    json: bool,
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
    /// Read a config value by JSON pointer.
    Get {
        /// JSON pointer such as `/global/default_language`. Use an empty string for the whole document.
        pointer: String,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Check whether POINTER exists and do not fail when it is missing.
        #[arg(long)]
        exists: bool,
        /// Print machine-readable JSON instead of the raw value.
        #[arg(long)]
        json: bool,
    },
    /// Set an existing config value by JSON pointer.
    Set {
        /// JSON pointer such as `/global/default_language`. The pointer must already exist.
        pointer: String,
        /// New value. Parsed as JSON when possible, otherwise treated as a string.
        value: String,
        /// Treat VALUE as a literal string without JSON parsing.
        #[arg(long)]
        string: bool,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the validated config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Open a config in an editor, then validate and write it back safely.
    Edit {
        /// Optional config JSON file. Omitted to edit the user config, or create it from the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Editor executable to run. Defaults to `$VINPUT_CONFIG_EDITOR`, `$EDITOR`, then `$VISUAL`.
        #[arg(long)]
        editor: Option<String>,
        /// Print the editor plan without invoking the editor or writing files.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
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
    /// Trigger daemon D-Bus activation by querying status.
    Start {
        /// Print the D-Bus activation plan without contacting the daemon.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
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
    /// Stop the user daemon service.
    Stop {
        /// Print the stop plan without mutating user services.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Restart the user daemon service.
    Restart {
        /// Print the restart plan without mutating user services.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Print daemon logs from the user service journal.
    Log {
        /// Limit journal output to the last N lines.
        #[arg(short = 'n', long)]
        lines: Option<u16>,
        /// Print the log retrieval plan without invoking external tools.
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
    /// Query current daemon recording/status state.
    Status {
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

/// Hotword configuration inspection commands.
#[derive(Debug, Subcommand)]
enum HotwordCommand {
    /// Show the hotwords file configured for the active or selected ASR provider.
    Get {
        /// Optional ASR provider id. Defaults to the active provider.
        #[arg(long)]
        provider: Option<String>,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Set the hotwords file for the active or selected ASR provider.
    Set {
        /// Hotwords file path to write into provider config.
        path: String,
        /// Optional ASR provider id. Defaults to the active provider.
        #[arg(long)]
        provider: Option<String>,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Clear the hotwords file for the active or selected ASR provider.
    Clear {
        /// Optional ASR provider id. Defaults to the active provider.
        #[arg(long)]
        provider: Option<String>,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Open the configured hotwords file in an editor.
    Edit {
        /// Optional ASR provider id. Defaults to the active provider.
        #[arg(long)]
        provider: Option<String>,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Editor executable to run. Defaults to `$VINPUT_HOTWORD_EDITOR`, `$VINPUT_CONFIG_EDITOR`, `$EDITOR`, then `$VISUAL`.
        #[arg(long)]
        editor: Option<String>,
        /// Print the edit plan without launching the editor.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
}

/// Audio device selection commands.
#[derive(Debug, Subcommand)]
enum DeviceCommand {
    /// List configured and live capture devices.
    #[command(alias = "ls")]
    List {
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Select the capture device in config.
    Use {
        /// Capture device value, such as default or a `PipeWire` object/name from device list.
        target: String,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
}

/// LLM provider management commands.
#[derive(Debug, Subcommand)]
enum LlmCommand {
    /// List configured LLM providers.
    #[command(alias = "ls")]
    List {
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Print machine-readable JSON instead of text table output.
        #[arg(long)]
        json: bool,
    },
    /// Add an OpenAI-compatible LLM provider to config.
    Add {
        /// New LLM provider id.
        id: String,
        /// Base URL for OpenAI-compatible chat completions.
        #[arg(short = 'u', long)]
        base_url: String,
        /// API key or environment-reference expression.
        #[arg(short = 'k', long)]
        api_key: Option<String>,
        /// Optional default model name.
        #[arg(long)]
        model: Option<String>,
        /// Extra JSON object merged into provider requests.
        #[arg(short = 'e', long)]
        extra_body: Option<String>,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Edit an existing LLM provider in config.
    #[command(alias = "e")]
    Edit {
        /// Existing LLM provider id to edit.
        id: String,
        /// Set base URL for OpenAI-compatible chat completions.
        #[arg(short = 'u', long)]
        base_url: Option<String>,
        /// Set API key or environment-reference expression.
        #[arg(short = 'k', long)]
        api_key: Option<String>,
        /// Clear API key from this provider.
        #[arg(long)]
        clear_api_key: bool,
        /// Set default model name.
        #[arg(long)]
        model: Option<String>,
        /// Clear default model from this provider.
        #[arg(long)]
        clear_model: bool,
        /// Set extra JSON object merged into provider requests.
        #[arg(short = 'e', long)]
        extra_body: Option<String>,
        /// Clear extra JSON body from this provider.
        #[arg(long)]
        clear_extra_body: bool,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Test an LLM provider with an OpenAI-compatible chat request.
    Test {
        /// Existing LLM provider id to test.
        id: String,
        /// Raw text used in the synthetic connectivity test prompt.
        #[arg(long, default_value = "vinput LLM connectivity test")]
        text: String,
        /// Optional timeout in milliseconds for the test request.
        #[arg(long)]
        timeout_ms: Option<u64>,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Print the request plan without contacting the provider.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Remove an LLM provider from config.
    #[command(alias = "rm")]
    Remove {
        /// Existing LLM provider id to remove.
        id: String,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
}

/// Text adapter management commands.
#[derive(Debug, Subcommand)]
enum AdapterCommand {
    /// List configured text adapters.
    #[command(alias = "ls")]
    List {
        /// Legacy-compatible flag for listing registry-available text adapters.
        #[arg(short = 'a', long)]
        available: bool,
        /// Optional local registry index JSON used by --available.
        #[arg(long)]
        registry: Option<PathBuf>,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Print machine-readable JSON instead of text table output.
        #[arg(long)]
        json: bool,
    },
    /// Add a command text adapter to config.
    Add {
        /// New adapter id.
        id: String,
        /// Adapter executable path or command name.
        #[arg(long)]
        command: String,
        /// Adapter command argument. Repeat for multiple args.
        #[arg(long = "arg")]
        args: Vec<String>,
        /// Adapter environment entry as KEY=VALUE. Repeat for multiple entries.
        #[arg(long = "env")]
        env: Vec<String>,
        /// Optional working directory for the adapter process.
        #[arg(long)]
        working_dir: Option<String>,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Print a dry-run install plan for a registry adapter.
    InstallPlan {
        /// Adapter id from the registry index.
        id: String,
        /// Local registry index JSON containing adapter entries.
        #[arg(long)]
        registry: PathBuf,
        /// Target root directory for planned adapter asset installation.
        #[arg(long)]
        target_root: PathBuf,
        /// Optional config JSON file that provides registry mirrors.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Print only summary fields without per-asset rows.
        #[arg(long)]
        summary_only: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Edit a configured command text adapter.
    Edit {
        /// Existing adapter id to edit.
        id: String,
        /// Set adapter executable path or command name.
        #[arg(long)]
        command: Option<String>,
        /// Replace adapter command arguments. Repeat for multiple args.
        #[arg(long = "arg")]
        args: Vec<String>,
        /// Clear adapter command arguments.
        #[arg(long)]
        clear_args: bool,
        /// Replace adapter environment entries with KEY=VALUE assignments.
        #[arg(long = "env")]
        env: Vec<String>,
        /// Clear adapter environment entries.
        #[arg(long)]
        clear_env: bool,
        /// Set optional working directory for the adapter process.
        #[arg(long)]
        working_dir: Option<String>,
        /// Clear adapter working directory.
        #[arg(long)]
        clear_working_dir: bool,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Start a configured text adapter through daemon D-Bus.
    Start {
        /// Existing adapter id to start.
        id: String,
        /// Print the D-Bus call plan without contacting the daemon.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Stop a configured text adapter through daemon D-Bus.
    Stop {
        /// Existing adapter id to stop.
        id: String,
        /// Print the D-Bus call plan without contacting the daemon.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Inspect daemon text adapter runtime state.
    Status {
        /// Optional adapter id to filter. Omitted to show all adapters.
        id: Option<String>,
        /// Print the D-Bus call plan without contacting the daemon.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Remove a configured text adapter.
    #[command(alias = "rm")]
    Remove {
        /// Existing adapter id to remove.
        id: String,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
}

/// Scene management commands.
#[derive(Debug, Subcommand)]
enum SceneCommand {
    /// List configured recognition scenes.
    #[command(alias = "ls")]
    List {
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Print machine-readable JSON instead of text table output.
        #[arg(long)]
        json: bool,
    },
    /// Add a recognition scene to config.
    Add {
        /// New scene id.
        id: String,
        /// Display label or translation key for the scene.
        #[arg(long)]
        label: String,
        /// Optional prompt template for post-processing.
        #[arg(long)]
        prompt: Option<String>,
        /// Optional LLM provider id for this scene.
        #[arg(long)]
        provider_id: Option<String>,
        /// Optional model override for this scene.
        #[arg(long)]
        model: Option<String>,
        /// Number of result candidates requested from post-processing.
        #[arg(long, default_value_t = 0)]
        candidate_count: u8,
        /// Optional per-scene timeout in milliseconds.
        #[arg(long)]
        timeout_ms: Option<u64>,
        /// Recent input context lines to include.
        #[arg(long, default_value_t = 0)]
        context_lines: u8,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Edit an explicitly configured recognition scene.
    Edit {
        /// Existing scene id to edit.
        id: String,
        /// Set display label or translation key for the scene.
        #[arg(long)]
        label: Option<String>,
        /// Set prompt template for post-processing.
        #[arg(long)]
        prompt: Option<String>,
        /// Clear prompt from this scene.
        #[arg(long)]
        clear_prompt: bool,
        /// Set LLM provider id for this scene.
        #[arg(long)]
        provider_id: Option<String>,
        /// Clear LLM provider id from this scene.
        #[arg(long)]
        clear_provider_id: bool,
        /// Set model override for this scene.
        #[arg(long)]
        model: Option<String>,
        /// Clear model override from this scene.
        #[arg(long)]
        clear_model: bool,
        /// Set candidate count.
        #[arg(long)]
        candidate_count: Option<u8>,
        /// Set per-scene timeout in milliseconds.
        #[arg(long)]
        timeout_ms: Option<u64>,
        /// Clear per-scene timeout.
        #[arg(long)]
        clear_timeout: bool,
        /// Set recent input context lines to include.
        #[arg(long)]
        context_lines: Option<u8>,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Select the active recognition scene in config.
    Use {
        /// Existing scene id to activate.
        id: String,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Remove an inactive explicitly configured recognition scene.
    Remove {
        /// Existing inactive scene id to remove.
        id: String,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
}

/// ASR provider management commands.
#[derive(Debug, Subcommand)]
enum ProviderCommand {
    /// List configured ASR providers.
    #[command(alias = "ls")]
    List {
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Print machine-readable JSON instead of text table output.
        #[arg(long)]
        json: bool,
    },
    /// Select the active ASR provider in config.
    Use {
        /// Existing ASR provider id to activate.
        id: String,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Add an ASR provider to config.
    Add {
        /// New ASR provider id.
        id: String,
        /// Provider type: local, command, or remote.
        #[arg(long = "type", default_value = "local")]
        kind: String,
        /// Optional model id/path for this provider.
        #[arg(long)]
        model: Option<String>,
        /// Optional hotwords file path for local/command providers.
        #[arg(long)]
        hotwords_file: Option<String>,
        /// External command for command providers.
        #[arg(long)]
        command: Option<String>,
        /// Repeated argument for command providers.
        #[arg(long = "arg")]
        args: Vec<String>,
        /// Repeated KEY=VALUE environment assignment for command providers.
        #[arg(long = "env")]
        env: Vec<String>,
        /// Endpoint URL or label for remote providers.
        #[arg(long)]
        endpoint: Option<String>,
        /// Optional provider timeout in milliseconds.
        #[arg(long)]
        timeout_ms: Option<u64>,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Edit an existing ASR provider in config.
    Edit {
        /// Existing ASR provider id to edit.
        id: String,
        /// Provider type: local, command, or remote.
        #[arg(long = "type")]
        kind: Option<String>,
        /// Set model id/path for this provider.
        #[arg(long)]
        model: Option<String>,
        /// Clear model from this provider.
        #[arg(long)]
        clear_model: bool,
        /// Set hotwords file path for local/command providers.
        #[arg(long)]
        hotwords_file: Option<String>,
        /// Clear hotwords file from this provider.
        #[arg(long)]
        clear_hotwords_file: bool,
        /// Set external command for command providers.
        #[arg(long)]
        command: Option<String>,
        /// Clear command from this provider.
        #[arg(long)]
        clear_command: bool,
        /// Replace command arguments. Repeat for multiple args.
        #[arg(long = "arg")]
        args: Vec<String>,
        /// Clear command arguments from this provider.
        #[arg(long)]
        clear_args: bool,
        /// Replace environment entries with repeated KEY=VALUE assignments.
        #[arg(long = "env")]
        env: Vec<String>,
        /// Clear environment entries from this provider.
        #[arg(long)]
        clear_env: bool,
        /// Set endpoint URL or label for remote providers.
        #[arg(long)]
        endpoint: Option<String>,
        /// Clear endpoint from this provider.
        #[arg(long)]
        clear_endpoint: bool,
        /// Set provider timeout in milliseconds.
        #[arg(long)]
        timeout_ms: Option<u64>,
        /// Clear provider timeout.
        #[arg(long)]
        clear_timeout: bool,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Remove an inactive ASR provider from config.
    Remove {
        /// Existing inactive ASR provider id to remove.
        id: String,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
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
        /// Full model id, `short_id`, installed path, or managed model directory name with --installed.
        id: String,
        /// Read installed model metadata from the managed model root instead of the live registry.
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
        /// Treat selector as an installed managed model directory name instead of a live registry id.
        #[arg(long)]
        installed: bool,
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
        /// Treat selector as an installed managed model directory name instead of a live registry id.
        #[arg(long)]
        installed: bool,
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
    /// Initialize per-user config and managed directories.
    Init {
        /// Config path to create. Defaults to $XDG_CONFIG_HOME/fcitx-vinput/config.json.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Managed model root to create. Defaults to $XDG_DATA_HOME/fcitx-vinput/models.
        #[arg(long)]
        model_root: Option<PathBuf>,
        /// Managed cache root to create. Defaults to $XDG_CACHE_HOME/fcitx-vinput.
        #[arg(long)]
        cache_root: Option<PathBuf>,
        /// Overwrite an existing config file with the bundled default config.
        #[arg(long)]
        force: bool,
        /// Print the initialization plan without writing files or creating directories.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
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
    /// List or select capture devices.
    Device {
        /// Device operation.
        #[command(subcommand)]
        command: DeviceCommand,
    },
    /// Inspect ASR hotword configuration.
    Hotword {
        /// Hotword operation.
        #[command(subcommand)]
        command: HotwordCommand,
    },
    /// Inspect or manage LLM providers.
    Llm {
        /// LLM operation.
        #[command(subcommand)]
        command: LlmCommand,
    },
    /// Inspect or manage text adapters.
    Adapter {
        /// Adapter operation.
        #[command(subcommand)]
        command: AdapterCommand,
    },
    /// Inspect or manage recognition scenes.
    Scene {
        /// Scene operation.
        #[command(subcommand)]
        command: SceneCommand,
    },
    /// Inspect or manage ASR providers.
    Provider {
        /// Provider operation.
        #[command(subcommand)]
        command: ProviderCommand,
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

fn parse_args_with_global_json_alias() -> Args {
    match Args::try_parse() {
        Ok(args) => args,
        Err(original_error) => {
            let (filtered_args, saw_json_alias) = strip_global_json_aliases(std::env::args_os());
            if !saw_json_alias {
                original_error.exit();
            }
            match Args::try_parse_from(filtered_args) {
                Ok(mut args) => {
                    args.json = true;
                    args
                }
                Err(_) => original_error.exit(),
            }
        }
    }
}

fn strip_global_json_aliases(
    args: impl IntoIterator<Item = std::ffi::OsString>,
) -> (Vec<std::ffi::OsString>, bool) {
    let mut saw_json_alias = false;
    let mut after_argument_delimiter = false;
    let filtered = args
        .into_iter()
        .filter(|arg| {
            if after_argument_delimiter {
                return true;
            }
            if arg == "--" {
                after_argument_delimiter = true;
                return true;
            }
            if arg == "-j" || arg == "--json" {
                saw_json_alias = true;
                return false;
            }
            true
        })
        .collect::<Vec<_>>();
    (filtered, saw_json_alias)
}

fn force_json_output(command: &mut Command) {
    match command {
        Command::Init { json, .. } => *json = true,
        Command::Config { command } => {
            if let Some(command) = command {
                match command {
                    ConfigCommand::Get { json, .. }
                    | ConfigCommand::Set { json, .. }
                    | ConfigCommand::Edit { json, .. } => *json = true,
                    ConfigCommand::Validate { .. } | ConfigCommand::Example { .. } => {}
                }
            }
        }
        Command::Daemon { command } => match command {
            DaemonCommand::Start { json, .. }
            | DaemonCommand::Status { json, .. }
            | DaemonCommand::ReloadAsr { json, .. }
            | DaemonCommand::Stop { json, .. }
            | DaemonCommand::Restart { json, .. }
            | DaemonCommand::Log { json, .. } => *json = true,
        },
        Command::Recording { command } => match command {
            RecordingCommand::Start { json, .. }
            | RecordingCommand::Stop { json, .. }
            | RecordingCommand::Status { json, .. }
            | RecordingCommand::Toggle { json, .. } => *json = true,
        },
        Command::Device { command } => match command {
            DeviceCommand::List { json, .. } | DeviceCommand::Use { json, .. } => *json = true,
        },
        Command::Hotword { command } => match command {
            HotwordCommand::Get { json, .. }
            | HotwordCommand::Set { json, .. }
            | HotwordCommand::Clear { json, .. }
            | HotwordCommand::Edit { json, .. } => *json = true,
        },
        Command::Llm { command } => match command {
            LlmCommand::List { json, .. }
            | LlmCommand::Add { json, .. }
            | LlmCommand::Edit { json, .. }
            | LlmCommand::Test { json, .. }
            | LlmCommand::Remove { json, .. } => *json = true,
        },
        Command::Adapter { command } => match command {
            AdapterCommand::List { json, .. }
            | AdapterCommand::Add { json, .. }
            | AdapterCommand::InstallPlan { json, .. }
            | AdapterCommand::Edit { json, .. }
            | AdapterCommand::Start { json, .. }
            | AdapterCommand::Stop { json, .. }
            | AdapterCommand::Status { json, .. }
            | AdapterCommand::Remove { json, .. } => *json = true,
        },
        Command::Scene { command } => match command {
            SceneCommand::List { json, .. }
            | SceneCommand::Add { json, .. }
            | SceneCommand::Edit { json, .. }
            | SceneCommand::Use { json, .. }
            | SceneCommand::Remove { json, .. } => *json = true,
        },
        Command::Provider { command } => match command {
            ProviderCommand::List { json, .. }
            | ProviderCommand::Add { json, .. }
            | ProviderCommand::Use { json, .. }
            | ProviderCommand::Edit { json, .. }
            | ProviderCommand::Remove { json, .. } => *json = true,
        },
        Command::Model { command } => match command {
            ModelCommand::List { json, .. }
            | ModelCommand::Info { json, .. }
            | ModelCommand::Install { json, .. }
            | ModelCommand::Use { json, .. }
            | ModelCommand::Remove { json, .. } => *json = true,
        },
        Command::Registry { .. }
        | Command::Protocol
        | Command::AsrState { .. }
        | Command::AudioDevices { .. }
        | Command::Doctor { .. }
        | Command::ActivationService { .. }
        | Command::MockResult { .. }
        | Command::Status { .. } => {}
    }
}

#[allow(clippy::too_many_lines)]
fn main() -> anyhow::Result<()> {
    let mut args = parse_args_with_global_json_alias();
    if args.json {
        force_json_output(&mut args.command);
    }

    match args.command {
        Command::Init {
            config,
            model_root,
            cache_root,
            force,
            dry_run,
            json,
        } => handle_init(InitRequest {
            config_path: config.as_deref(),
            model_root: model_root.as_deref(),
            cache_root: cache_root.as_deref(),
            force,
            dry_run,
            json_output: json,
        }),
        Command::Protocol => print_protocol(),
        Command::Config { command } => match command {
            Some(ConfigCommand::Validate { path, summary_only }) => {
                validate_config_file(&path, summary_only)
            }
            Some(ConfigCommand::Get {
                pointer,
                config,
                exists,
                json,
            }) => handle_config_get(&pointer, config.as_ref(), exists, json),
            Some(ConfigCommand::Set {
                pointer,
                value,
                string,
                config,
                output,
                in_place,
                dry_run,
                json,
            }) => handle_config_set(ConfigSetRequest {
                pointer: &pointer,
                raw_value: &value,
                force_string: string,
                config_path: config.as_ref(),
                output_path: output.as_deref(),
                in_place,
                dry_run,
                json_output: json,
            }),
            Some(ConfigCommand::Edit {
                config,
                editor,
                dry_run,
                json,
            }) => handle_config_edit(ConfigEditRequest {
                config_path: config.as_ref(),
                editor: editor.as_deref(),
                dry_run,
                json_output: json,
            }),
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
        Command::Device { command } => handle_device_command(command),
        Command::Hotword { command } => handle_hotword_command(command),
        Command::Llm { command } => handle_llm_command(command),
        Command::Adapter { command } => handle_adapter_command(command),
        Command::Scene { command } => handle_scene_command(command),
        Command::Provider { command } => handle_provider_command(command),
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
        } => print_recording_action("start", selected_text.as_deref(), None, dry_run, json),
        RecordingCommand::Stop {
            scene,
            dry_run,
            json,
        } => print_recording_action("stop", None, scene.as_deref(), dry_run, json),
        RecordingCommand::Status { dry_run, json } => print_recording_status(dry_run, json),
        RecordingCommand::Toggle {
            selected_text,
            scene,
            dry_run,
            json,
        } => print_recording_action(
            "toggle",
            selected_text.as_deref(),
            scene.as_deref(),
            dry_run,
            json,
        ),
    }
}

fn print_recording_status(dry_run: bool, json_output: bool) -> anyhow::Result<()> {
    let output = if dry_run {
        recording_status_plan_json()
    } else {
        recording_status_via_dbus()?
    };
    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_recording_status_text(&output);
    }
    Ok(())
}

fn recording_status_via_dbus() -> anyhow::Result<serde_json::Value> {
    let connection = zbus::blocking::Connection::session().context("connect to session bus")?;
    let proxy = daemon_service_proxy(&connection)?;
    let status: String = proxy
        .call(dbus::method::GET_STATUS, &())
        .context("call GetStatus on daemon D-Bus service")?;
    Ok(recording_status_result_json(&status))
}

fn recording_status_plan_json() -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": true,
        "action": "status",
        "will_call_dbus": false,
        "called": false,
        "dbus": {
            "service": dbus::SERVICE_BUS_NAME,
            "object_path": dbus::SERVICE_OBJECT_PATH,
            "interface": dbus::SERVICE_INTERFACE,
            "method": dbus::method::GET_STATUS,
        },
        "next_steps": [
            "run vinput daemon status --json for full daemon diagnostics",
            "run vinput recording start --dry-run --json to inspect start calls",
            "run vinput doctor to inspect full local diagnostics"
        ],
    })
}

fn recording_status_result_json(status: &str) -> serde_json::Value {
    let parsed = ServiceStatus::parse_wire(status).ok();
    serde_json::json!({
        "ok": true,
        "dry_run": false,
        "action": "status",
        "will_call_dbus": true,
        "called": true,
        "dbus": {
            "service": dbus::SERVICE_BUS_NAME,
            "object_path": dbus::SERVICE_OBJECT_PATH,
            "interface": dbus::SERVICE_INTERFACE,
            "method": dbus::method::GET_STATUS,
        },
        "status": status,
        "known_status": parsed.is_some(),
        "is_recording": parsed == Some(ServiceStatus::Recording),
        "is_busy": matches!(parsed, Some(ServiceStatus::Recording | ServiceStatus::Inferring | ServiceStatus::Postprocessing)),
    })
}

fn print_recording_status_text(output: &serde_json::Value) {
    println!("dry_run: {}", output["dry_run"]);
    println!("action: status");
    println!("will_call_dbus: {}", output["will_call_dbus"]);
    println!("called: {}", output["called"]);
    println!("service: {}", dbus::SERVICE_BUS_NAME);
    println!("object_path: {}", dbus::SERVICE_OBJECT_PATH);
    println!("interface: {}", dbus::SERVICE_INTERFACE);
    println!("method: {}", dbus::method::GET_STATUS);
    if let Some(status) = output["status"].as_str() {
        println!("status: {status}");
        println!("known_status: {}", output["known_status"]);
        println!("is_recording: {}", output["is_recording"]);
        println!("is_busy: {}", output["is_busy"]);
    }
}

fn print_recording_action(
    action: &str,
    selected_text: Option<&str>,
    scene: Option<&str>,
    dry_run: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    if dry_run {
        let output = recording_plan_json(action, selected_text, scene);
        if json_output {
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            print_recording_plan_text(action, selected_text, scene);
        }
        return Ok(());
    }
    let result = recording_action_via_dbus(action, selected_text, scene)?;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        print_recording_result_text(&result);
    }
    Ok(())
}

fn recording_action_via_dbus(
    action: &str,
    selected_text: Option<&str>,
    scene: Option<&str>,
) -> anyhow::Result<serde_json::Value> {
    let connection = zbus::blocking::Connection::session().context("connect to session bus")?;
    let proxy = daemon_service_proxy(&connection)?;
    let method = match (action, selected_text) {
        ("start", Some(text)) => {
            let _: () = proxy
                .call(dbus::method::START_COMMAND_RECORDING, &(text))
                .context("call StartCommandRecording on daemon D-Bus service")?;
            dbus::method::START_COMMAND_RECORDING
        }
        ("start", None) => {
            let _: () = proxy
                .call(dbus::method::START_RECORDING, &())
                .context("call StartRecording on daemon D-Bus service")?;
            dbus::method::START_RECORDING
        }
        ("stop", _) => {
            let payload: String = proxy
                .call(dbus::method::STOP_RECORDING, &(scene.unwrap_or("")))
                .context("call StopRecording on daemon D-Bus service")?;
            return Ok(recording_result_json(
                action,
                dbus::method::STOP_RECORDING,
                scene,
                Some(payload.as_str()),
            ));
        }
        ("toggle", _) => return recording_toggle_via_dbus(&proxy, selected_text, scene),
        _ => anyhow::bail!("unsupported recording action `{action}`"),
    };
    Ok(recording_result_json(action, method, scene, None))
}
fn recording_toggle_via_dbus(
    proxy: &zbus::blocking::Proxy<'_>,
    selected_text: Option<&str>,
    scene: Option<&str>,
) -> anyhow::Result<serde_json::Value> {
    let status: String = proxy
        .call(dbus::method::GET_STATUS, &())
        .context("call GetStatus on daemon D-Bus service")?;
    if status == "recording" {
        let payload: String = proxy
            .call(dbus::method::STOP_RECORDING, &(scene.unwrap_or("")))
            .context("call StopRecording on daemon D-Bus service")?;
        let mut output = recording_result_json(
            "toggle",
            dbus::method::STOP_RECORDING,
            scene,
            Some(payload.as_str()),
        );
        output["status_before"] = serde_json::json!(status);
        return Ok(output);
    }
    let method = if let Some(text) = selected_text {
        let _: () = proxy
            .call(dbus::method::START_COMMAND_RECORDING, &(text))
            .context("call StartCommandRecording on daemon D-Bus service")?;
        dbus::method::START_COMMAND_RECORDING
    } else {
        let _: () = proxy
            .call(dbus::method::START_RECORDING, &())
            .context("call StartRecording on daemon D-Bus service")?;
        dbus::method::START_RECORDING
    };
    let mut output = recording_result_json("toggle", method, scene, None);
    output["status_before"] = serde_json::json!(status);
    Ok(output)
}

fn recording_result_json(
    action: &str,
    method: &str,
    scene: Option<&str>,
    payload_json: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": false,
        "action": action,
        "will_call_dbus": true,
        "called": true,
        "dbus": {
            "service": dbus::SERVICE_BUS_NAME,
            "object_path": dbus::SERVICE_OBJECT_PATH,
            "interface": dbus::SERVICE_INTERFACE,
            "method": method,
        },
        "args": {
            "scene": scene.unwrap_or(""),
        },
        "payload_json": payload_json,
    })
}

fn print_recording_result_text(result: &serde_json::Value) {
    println!("dry_run: false");
    println!("action: {}", optional_json_str(&result["action"]));
    println!("will_call_dbus: true");
    println!("called: true");
    println!("service: {}", dbus::SERVICE_BUS_NAME);
    println!("object_path: {}", dbus::SERVICE_OBJECT_PATH);
    println!("interface: {}", dbus::SERVICE_INTERFACE);
    println!("method: {}", optional_json_str(&result["dbus"]["method"]));
    if let Some(payload_json) = result["payload_json"].as_str() {
        println!("payload_json: {payload_json}");
    }
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
        DaemonCommand::Start { dry_run, json } => print_daemon_start(*dry_run, *json),
        DaemonCommand::Status { dry_run, json } => print_daemon_status(*dry_run, *json),
        DaemonCommand::ReloadAsr { dry_run, json } => print_daemon_reload_asr_plan(*dry_run, *json),
        DaemonCommand::Stop { dry_run, json } => {
            print_daemon_user_service_plan("stop", None, *dry_run, *json)
        }
        DaemonCommand::Restart { dry_run, json } => {
            print_daemon_user_service_plan("restart", None, *dry_run, *json)
        }
        DaemonCommand::Log {
            lines,
            dry_run,
            json,
        } => print_daemon_user_service_plan("log", *lines, *dry_run, *json),
    }
}

fn print_daemon_user_service_plan(
    action: &str,
    log_lines: Option<u16>,
    dry_run: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    let command = daemon_user_service_command(action, log_lines)?;
    if dry_run {
        let output = daemon_user_service_dry_run_json(action, &command);
        if json_output {
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            print_daemon_user_service_dry_run_text(action, &command);
        }
        return Ok(());
    }

    let output = run_daemon_user_service_command(action, &command);
    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_daemon_user_service_result_text(&output);
    }
    Ok(())
}

struct UserServiceCommand {
    program: String,
    args: Vec<String>,
}

impl UserServiceCommand {
    fn argv(&self) -> Vec<String> {
        std::iter::once(self.program.clone())
            .chain(self.args.iter().cloned())
            .collect()
    }

    fn display(&self) -> String {
        self.argv().join(" ")
    }
}

fn daemon_user_service_command(
    action: &str,
    log_lines: Option<u16>,
) -> anyhow::Result<UserServiceCommand> {
    const SERVICE_NAME: &str = "fcitx-vinput.service";
    if log_lines == Some(0) {
        anyhow::bail!("daemon log --lines must be greater than 0");
    }
    match action {
        "stop" => Ok(UserServiceCommand {
            program: std::env::var("VINPUT_DAEMON_SYSTEMCTL")
                .unwrap_or_else(|_| "systemctl".to_owned()),
            args: ["--user", "stop", SERVICE_NAME]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        }),
        "restart" => Ok(UserServiceCommand {
            program: std::env::var("VINPUT_DAEMON_SYSTEMCTL")
                .unwrap_or_else(|_| "systemctl".to_owned()),
            args: ["--user", "restart", SERVICE_NAME]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        }),
        "log" => {
            let mut args = ["--user", "-u", SERVICE_NAME]
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if let Some(lines) = log_lines {
                args.extend(["-n".to_owned(), lines.to_string()]);
            }
            Ok(UserServiceCommand {
                program: std::env::var("VINPUT_DAEMON_JOURNALCTL")
                    .unwrap_or_else(|_| "journalctl".to_owned()),
                args,
            })
        }
        _ => anyhow::bail!("unsupported daemon user service action `{action}`"),
    }
}

fn daemon_user_service_dry_run_json(
    action: &str,
    command: &UserServiceCommand,
) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": true,
        "action": action,
        "will_mutate_user_service": false,
        "strategy": "systemd-user-service",
        "command": command.display(),
        "command_argv": command.argv(),
        "fallback": daemon_user_service_fallback(),
        "next_steps": daemon_user_service_next_steps(action),
    })
}

fn print_daemon_user_service_dry_run_text(action: &str, command: &UserServiceCommand) {
    println!("dry_run: true");
    println!("action: {action}");
    println!("will_mutate_user_service: false");
    println!("strategy: systemd-user-service");
    println!("command: {}", command.display());
    println!("fallback: {}", daemon_user_service_fallback());
    println!("next_step: {}", daemon_user_service_next_steps(action)[0]);
}

fn run_daemon_user_service_command(
    action: &str,
    command: &UserServiceCommand,
) -> serde_json::Value {
    match ProcessCommand::new(&command.program)
        .args(&command.args)
        .output()
    {
        Ok(output) => {
            let exit_status = output.status.code();
            serde_json::json!({
                "ok": output.status.success(),
                "dry_run": false,
                "action": action,
                "will_mutate_user_service": action != "log",
                "strategy": "systemd-user-service",
                "command": command.display(),
                "command_argv": command.argv(),
                "exit_status": exit_status,
                "stdout": String::from_utf8_lossy(&output.stdout),
                "stderr": String::from_utf8_lossy(&output.stderr),
                "fallback": daemon_user_service_fallback(),
                "next_steps": daemon_user_service_next_steps(action),
            })
        }
        Err(error) => serde_json::json!({
            "ok": false,
            "dry_run": false,
            "action": action,
            "will_mutate_user_service": action != "log",
            "strategy": "systemd-user-service",
            "command": command.display(),
            "command_argv": command.argv(),
            "exit_status": null,
            "stdout": "",
            "stderr": "",
            "error": error.to_string(),
            "fallback": daemon_user_service_fallback(),
            "next_steps": daemon_user_service_next_steps(action),
        }),
    }
}

fn print_daemon_user_service_result_text(output: &serde_json::Value) {
    println!("dry_run: false");
    println!("action: {}", optional_json_str(&output["action"]));
    println!(
        "will_mutate_user_service: {}",
        output["will_mutate_user_service"]
            .as_bool()
            .unwrap_or(false)
    );
    println!("strategy: systemd-user-service");
    println!("command: {}", optional_json_str(&output["command"]));
    println!("ok: {}", output["ok"].as_bool().unwrap_or(false));
    match output["exit_status"].as_i64() {
        Some(status) => println!("exit_status: {status}"),
        None => println!("exit_status: -"),
    }
    if let Some(stdout) = output["stdout"].as_str().filter(|value| !value.is_empty()) {
        print!("stdout: {stdout}");
        if !stdout.ends_with('\n') {
            println!();
        }
    }
    if let Some(stderr) = output["stderr"].as_str().filter(|value| !value.is_empty()) {
        print!("stderr: {stderr}");
        if !stderr.ends_with('\n') {
            println!();
        }
    }
    if let Some(error) = output["error"].as_str().filter(|value| !value.is_empty()) {
        println!("error: {error}");
    }
    if output["ok"].as_bool() != Some(true) {
        println!("fallback: {}", daemon_user_service_fallback());
    }
    if let Some(next_step) = output["next_steps"]
        .as_array()
        .and_then(|steps| steps.first())
        .and_then(serde_json::Value::as_str)
    {
        println!("next_step: {next_step}");
    }
}

fn daemon_user_service_next_steps(action: &str) -> Vec<&'static str> {
    match action {
        "log" => vec![
            "adjust --lines to inspect more or fewer journal entries",
            "run vinput daemon status to inspect live D-Bus/runtime state",
        ],
        "restart" => vec![
            "run vinput daemon status to verify the restarted daemon",
            "run vinput daemon log --lines 100 if restart failed",
        ],
        _ => vec![
            "run vinput daemon status to verify daemon availability",
            "run vinput daemon log --lines 100 if service control failed",
        ],
    }
}

const fn daemon_user_service_fallback() -> &'static str {
    "inspect the per-user D-Bus activation service and daemon process manually"
}

fn print_daemon_start(dry_run: bool, json_output: bool) -> anyhow::Result<()> {
    if dry_run {
        let output = serde_json::json!({
            "ok": true,
            "dry_run": true,
            "action": "start",
            "will_call_dbus": false,
            "activation": {
                "strategy": "dbus-service-activation",
                "trigger_method": dbus::method::GET_STATUS,
            },
            "dbus": {
                "service": dbus::SERVICE_BUS_NAME,
                "object_path": dbus::SERVICE_OBJECT_PATH,
                "interface": dbus::SERVICE_INTERFACE,
                "method": dbus::method::GET_STATUS,
            },
            "next_steps": daemon_start_next_steps(),
        });
        if json_output {
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!("dry_run: true");
            println!("action: start");
            println!("will_call_dbus: false");
            println!("strategy: dbus-service-activation");
            println!("method: {}", dbus::method::GET_STATUS);
            println!("service: {}", dbus::SERVICE_BUS_NAME);
            println!("object_path: {}", dbus::SERVICE_OBJECT_PATH);
            println!("interface: {}", dbus::SERVICE_INTERFACE);
            println!("next_step: {}", daemon_start_next_steps()[0]);
        }
        return Ok(());
    }

    let status = daemon_status_via_dbus()?;
    let output = serde_json::json!({
        "ok": true,
        "dry_run": false,
        "action": "start",
        "will_call_dbus": true,
        "called": true,
        "activation": {
            "strategy": "dbus-service-activation",
            "trigger_method": dbus::method::GET_STATUS,
        },
        "daemon": status,
        "next_steps": daemon_start_next_steps(),
    });
    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("dry_run: false");
        println!("action: start");
        println!("will_call_dbus: true");
        println!("called: true");
        println!("status: {}", optional_json_str(&output["daemon"]["status"]));
    }
    Ok(())
}

fn daemon_start_next_steps() -> Vec<&'static str> {
    vec![
        "run vinput daemon status to inspect live D-Bus/runtime state",
        "run vinput daemon log --lines 100 if activation failed",
    ]
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
        "reports": [
            "service_status",
            "asr_backend",
            "runtime_status",
            "text_adapters"
        ],
        "next_steps": [
            "run vinput daemon status without --dry-run to query live daemon diagnostics",
            "run vinput adapter status to inspect text adapter PID/running state",
            "run vinput doctor to inspect local setup and activation readiness"
        ],
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
    println!("reports: service_status, asr_backend, runtime_status, text_adapters");
    println!("next_step: run vinput daemon status without --dry-run");
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
        "target_model_id: {}",
        optional_json_str(&snapshot["asr_backend"]["target_model_id"])
    );
    println!(
        "effective_provider_id: {}",
        optional_json_str(&snapshot["asr_backend"]["effective_provider_id"])
    );
    println!(
        "effective_model_id: {}",
        optional_json_str(&snapshot["asr_backend"]["effective_model_id"])
    );
    println!(
        "last_error: {}",
        optional_json_str(&snapshot["asr_backend"]["last_error"])
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
    println!(
        "remote_endpoints: {}",
        json_string_array_summary(&snapshot["asr_backend"]["remote_endpoints"])
    );
    println!(
        "runtime_status: {}",
        optional_json_str(&snapshot["runtime_status"]["status"])
    );
    println!(
        "runtime_uptime_ms: {}",
        snapshot["runtime_status"]["uptime_ms"]
            .as_u64()
            .unwrap_or(0)
    );
    println!(
        "active_session: {}",
        snapshot["runtime_status"]["active_session"]
            .as_bool()
            .unwrap_or(false)
    );
    println!(
        "text_adapter_count: {}",
        snapshot["runtime_status"]["text_adapters"]["adapter_count"]
            .as_u64()
            .unwrap_or(0)
    );
}

fn optional_json_str(value: &serde_json::Value) -> &str {
    value.as_str().unwrap_or("-")
}

fn json_string_array_summary(value: &serde_json::Value) -> String {
    let values = value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if values.is_empty() {
        "-".to_owned()
    } else {
        values.join(", ")
    }
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

fn handle_device_command(command: DeviceCommand) -> anyhow::Result<()> {
    match command {
        DeviceCommand::List { config, json } => print_device_list(config.as_ref(), json),
        DeviceCommand::Use {
            target,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_device_use(DeviceUseRequest {
            target: &target,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
    }
}

fn handle_hotword_command(command: HotwordCommand) -> anyhow::Result<()> {
    match command {
        HotwordCommand::Get {
            provider,
            config,
            json,
        } => print_hotword_get(provider.as_deref(), config.as_ref(), json),
        HotwordCommand::Set {
            path,
            provider,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_hotword_mutation(HotwordMutationRequest {
            provider_id: provider.as_deref(),
            hotwords_file: Some(&path),
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        HotwordCommand::Clear {
            provider,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_hotword_mutation(HotwordMutationRequest {
            provider_id: provider.as_deref(),
            hotwords_file: None,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        HotwordCommand::Edit {
            provider,
            config,
            editor,
            dry_run,
            json,
        } => print_hotword_edit(HotwordEditRequest {
            provider_id: provider.as_deref(),
            config_path: config.as_ref(),
            editor: editor.as_deref(),
            dry_run,
            json_output: json,
        }),
    }
}

#[derive(Clone, Copy)]
struct HotwordEditRequest<'a> {
    provider_id: Option<&'a str>,
    config_path: Option<&'a PathBuf>,
    editor: Option<&'a str>,
    dry_run: bool,
    json_output: bool,
}

struct HotwordEditOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    active_provider: String,
    provider_id: String,
    provider_type: &'static str,
    hotwords_file: PathBuf,
    editor_argv: Vec<String>,
    dry_run: bool,
    edited: bool,
    exit_status: Option<i32>,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct HotwordMutationRequest<'a> {
    provider_id: Option<&'a str>,
    hotwords_file: Option<&'a str>,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct HotwordMutationOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    active_provider: String,
    provider_id: String,
    provider_type: &'static str,
    before: Option<String>,
    after: Option<String>,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

struct HotwordGetContext {
    config_path: Option<PathBuf>,
    source: &'static str,
    active_provider: String,
    provider: AsrProviderConfig,
}

fn print_hotword_get(
    provider_id: Option<&str>,
    config_path: Option<&PathBuf>,
    json_output: bool,
) -> anyhow::Result<()> {
    let context = load_hotword_get_context(provider_id, config_path)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&hotword_get_json(&context))?
        );
    } else {
        print_hotword_get_text(&context);
    }
    Ok(())
}

fn load_hotword_get_context(
    provider_id: Option<&str>,
    config_path: Option<&PathBuf>,
) -> anyhow::Result<HotwordGetContext> {
    let loaded = load_config_json(config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for hotword get")?;
    let config = VinputConfig::from_json_str(&contents).context("parse config for hotword get")?;
    config
        .validate()
        .context("validate config for hotword get")?;
    let selected_provider_id = provider_id
        .map(normalize_provider_id)
        .transpose()?
        .unwrap_or_else(|| config.asr.active_provider.clone());
    let provider = config
        .asr
        .providers
        .iter()
        .find(|provider| provider.id == selected_provider_id)
        .with_context(|| format!("ASR provider `{selected_provider_id}` not found"))?
        .clone();
    Ok(HotwordGetContext {
        config_path: loaded.path,
        source: loaded.source,
        active_provider: config.asr.active_provider,
        provider,
    })
}

fn hotword_get_json(context: &HotwordGetContext) -> serde_json::Value {
    let hotwords_file = context.provider.hotwords_file.as_deref();
    serde_json::json!({
        "ok": true,
        "config_path": context.config_path.as_ref(),
        "source": context.source,
        "active_provider": context.active_provider.as_str(),
        "provider_id": context.provider.id.as_str(),
        "provider_type": asr_provider_kind_label(&context.provider.kind),
        "active": context.provider.id == context.active_provider,
        "supported": hotword_supported(&context.provider.kind),
        "configured": hotwords_file.is_some_and(|value| !value.trim().is_empty()),
        "hotwords_file": hotwords_file,
        "next_steps": [
            "run vinput provider list to inspect configured ASR providers",
            "run vinput hotword set <path> once hotword mutation support is available",
            "run vinput asr-state to inspect the selected provider runtime readiness"
        ],
    })
}

fn print_hotword_get_text(context: &HotwordGetContext) {
    println!("source: {}", context.source);
    if let Some(path) = &context.config_path {
        println!("config_path: {}", path.display());
    }
    println!("active_provider: {}", context.active_provider);
    println!("provider_id: {}", context.provider.id);
    println!(
        "provider_type: {}",
        asr_provider_kind_label(&context.provider.kind)
    );
    println!("active: {}", context.provider.id == context.active_provider);
    println!("supported: {}", hotword_supported(&context.provider.kind));
    println!(
        "configured: {}",
        configured_label(context.provider.hotwords_file.as_deref())
    );
    println!(
        "hotwords_file: {}",
        context.provider.hotwords_file.as_deref().unwrap_or("-")
    );
}

fn print_hotword_edit(request: HotwordEditRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_hotword_edit(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&hotword_edit_json(&outcome))?
        );
    } else {
        print_hotword_edit_text(&outcome);
    }
    Ok(())
}

fn run_hotword_edit(request: &HotwordEditRequest<'_>) -> anyhow::Result<HotwordEditOutcome> {
    let context = load_hotword_get_context(request.provider_id, request.config_path)?;
    if !hotword_supported(&context.provider.kind) {
        anyhow::bail!(
            "ASR provider `{}` does not support hotwords",
            context.provider.id
        );
    }
    let hotwords_file = context
        .provider
        .hotwords_file
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .with_context(|| "No hotwords file configured. Use 'hotword set <path>' first.")?;
    let editor_argv = resolve_hotword_editor(request.editor)?;
    let mut edited = false;
    let mut exit_status = None;
    if !request.dry_run {
        let status = run_hotword_editor(&editor_argv, Path::new(hotwords_file))?;
        if !status.success() {
            anyhow::bail!("hotword editor exited with status {status}");
        }
        exit_status = status.code();
        edited = true;
    }
    Ok(HotwordEditOutcome {
        config_path: context.config_path,
        source: context.source,
        active_provider: context.active_provider,
        provider_id: context.provider.id,
        provider_type: asr_provider_kind_label(&context.provider.kind),
        hotwords_file: PathBuf::from(hotwords_file),
        editor_argv,
        dry_run: request.dry_run,
        edited,
        exit_status,
    })
}

fn hotword_edit_json(outcome: &HotwordEditOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "active_provider": outcome.active_provider,
        "provider_id": outcome.provider_id,
        "provider_type": outcome.provider_type,
        "hotwords_file": outcome.hotwords_file,
        "editor": outcome.editor_argv.join(" "),
        "editor_argv": outcome.editor_argv,
        "edited": outcome.edited,
        "exit_status": outcome.exit_status,
        "next_steps": [
            "run vinput hotword get to verify the configured hotwords file",
            "run vinput asr-state to inspect the selected provider runtime readiness"
        ],
    })
}

fn print_hotword_edit_text(outcome: &HotwordEditOutcome) {
    println!("dry_run: {}", outcome.dry_run);
    println!("source: {}", outcome.source);
    if let Some(config_path) = &outcome.config_path {
        println!("config_path: {}", config_path.display());
    }
    println!("active_provider: {}", outcome.active_provider);
    println!("provider_id: {}", outcome.provider_id);
    println!("provider_type: {}", outcome.provider_type);
    println!("hotwords_file: {}", outcome.hotwords_file.display());
    println!("editor: {}", outcome.editor_argv.join(" "));
    println!("edited: {}", outcome.edited);
    if let Some(exit_status) = outcome.exit_status {
        println!("exit_status: {exit_status}");
    }
}

fn resolve_hotword_editor(editor: Option<&str>) -> anyhow::Result<Vec<String>> {
    let editor = editor
        .map(str::to_owned)
        .or_else(|| std::env::var("VINPUT_HOTWORD_EDITOR").ok())
        .or_else(|| std::env::var("VINPUT_CONFIG_EDITOR").ok())
        .or_else(|| std::env::var("EDITOR").ok())
        .or_else(|| std::env::var("VISUAL").ok())
        .with_context(
            || "hotword edit requires --editor or $VINPUT_HOTWORD_EDITOR/$VINPUT_CONFIG_EDITOR/$EDITOR/$VISUAL",
        )?;
    let argv = split_editor_argv(&editor);
    if argv.is_empty() {
        anyhow::bail!("hotword editor command is empty");
    }
    Ok(argv)
}

fn run_hotword_editor(
    editor_argv: &[String],
    path: &Path,
) -> anyhow::Result<std::process::ExitStatus> {
    let (program, args) = editor_argv
        .split_first()
        .with_context(|| "hotword editor command is empty")?;
    ProcessCommand::new(program)
        .args(args)
        .arg(path)
        .status()
        .with_context(|| format!("run hotword editor `{}`", editor_argv.join(" ")))
}

fn print_hotword_mutation(request: HotwordMutationRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_hotword_mutation(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&hotword_mutation_json(&outcome))?
        );
    } else {
        print_hotword_mutation_text(&outcome);
    }
    Ok(())
}

fn run_hotword_mutation(
    request: &HotwordMutationRequest<'_>,
) -> anyhow::Result<HotwordMutationOutcome> {
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for hotword mutation")?;
    let config =
        VinputConfig::from_json_str(&contents).context("parse config for hotword mutation")?;
    let provider_id = request
        .provider_id
        .map(normalize_provider_id)
        .transpose()?
        .unwrap_or_else(|| config.asr.active_provider.clone());
    let provider_index = config
        .asr
        .providers
        .iter()
        .position(|provider| provider.id == provider_id)
        .with_context(|| format!("ASR provider `{provider_id}` not found"))?;
    let provider = &config.asr.providers[provider_index];
    if !hotword_supported(&provider.kind) {
        anyhow::bail!("ASR provider `{provider_id}` does not support hotwords");
    }
    let provider_type = asr_provider_kind_label(&provider.kind);
    let before = provider.hotwords_file.clone();
    let after = request
        .hotwords_file
        .map(normalize_hotwords_file)
        .transpose()?;

    let providers = loaded
        .document
        .pointer_mut("/asr/providers")
        .and_then(serde_json::Value::as_array_mut)
        .with_context(|| "config pointer `/asr/providers` not found or not an array")?;
    let provider_object = providers
        .get_mut(provider_index)
        .and_then(serde_json::Value::as_object_mut)
        .with_context(|| format!("ASR provider `{provider_id}` is not a JSON object"))?;
    if let Some(after) = &after {
        provider_object.insert(
            "hotwords_file".to_owned(),
            serde_json::Value::String(after.clone()),
        );
    } else {
        provider_object.remove("hotwords_file");
    }
    validate_config_json_value(&loaded.document, "validate updated hotword config")?;

    let write_target = config_set_write_target(
        request.output_path,
        request.in_place,
        request.dry_run,
        loaded.path.as_ref(),
        &default_path,
    )?;

    let mut wrote_config = false;
    if !request.dry_run {
        write_config_set_document(&loaded.document, &write_target)?;
        wrote_config = true;
    }

    Ok(HotwordMutationOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        active_provider: config.asr.active_provider,
        provider_id,
        provider_type,
        before,
        after,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn normalize_hotwords_file(input: &str) -> anyhow::Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        anyhow::bail!("hotwords file cannot be empty");
    }
    Ok(trimmed.to_owned())
}

fn hotword_mutation_json(outcome: &HotwordMutationOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "active_provider": outcome.active_provider,
        "provider_id": outcome.provider_id,
        "provider_type": outcome.provider_type,
        "before": outcome.before,
        "after": outcome.after,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinput hotword get to verify the configured hotwords file",
            "run vinput asr-state to inspect the selected provider runtime readiness",
            "run vinput doctor to inspect full local diagnostics"
        ],
    })
}

fn print_hotword_mutation_text(outcome: &HotwordMutationOutcome) {
    println!("dry_run: {}", outcome.dry_run);
    println!("source: {}", outcome.source);
    if let Some(config_path) = &outcome.config_path {
        println!("config_path: {}", config_path.display());
    }
    println!("active_provider: {}", outcome.active_provider);
    println!("provider_id: {}", outcome.provider_id);
    println!("provider_type: {}", outcome.provider_type);
    println!("before: {}", outcome.before.as_deref().unwrap_or("-"));
    println!("after: {}", outcome.after.as_deref().unwrap_or("-"));
    println!("in_place: {}", outcome.in_place);
    if let Some(output_path) = &outcome.output_path {
        println!("output_path: {}", output_path.display());
    }
    if let Some(backup_path) = &outcome.backup_path {
        println!("backup_path: {}", backup_path.display());
    }
    println!("will_write_config: {}", !outcome.dry_run);
    println!("wrote_config: {}", outcome.wrote_config);
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct LlmAddRequest<'a> {
    id: &'a str,
    base_url: &'a str,
    api_key: Option<&'a str>,
    model: Option<&'a str>,
    extra_body: Option<&'a str>,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct LlmEditRequest<'a> {
    id: &'a str,
    base_url: Option<&'a str>,
    api_key: Option<&'a str>,
    clear_api_key: bool,
    model: Option<&'a str>,
    clear_model: bool,
    extra_body: Option<&'a str>,
    clear_extra_body: bool,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct LlmEditOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    provider_id: String,
    changed_fields: Vec<String>,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct LlmRemoveRequest<'a> {
    id: &'a str,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct LlmAddOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    provider_id: String,
    before_provider_count: usize,
    after_provider_count: usize,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

struct LlmRemoveOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    removed_provider_id: String,
    before_provider_count: usize,
    after_provider_count: usize,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

struct LlmListContext {
    config_path: Option<PathBuf>,
    source: &'static str,
    config: VinputConfig,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct AdapterAddRequest<'a> {
    id: &'a str,
    command: &'a str,
    args: &'a [String],
    env: &'a [String],
    working_dir: Option<&'a str>,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct AdapterEditRequest<'a> {
    id: &'a str,
    command: Option<&'a str>,
    args: &'a [String],
    clear_args: bool,
    env: &'a [String],
    clear_env: bool,
    working_dir: Option<&'a str>,
    clear_working_dir: bool,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct AdapterEditOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    adapter_id: String,
    changed_fields: Vec<String>,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct AdapterRemoveRequest<'a> {
    id: &'a str,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct AdapterAddOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    adapter_id: String,
    before_adapter_count: usize,
    after_adapter_count: usize,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

struct AdapterRemoveOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    removed_adapter_id: String,
    before_adapter_count: usize,
    after_adapter_count: usize,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

struct AdapterListContext {
    config_path: Option<PathBuf>,
    source: &'static str,
    config: VinputConfig,
}

fn handle_llm_command(command: LlmCommand) -> anyhow::Result<()> {
    match command {
        LlmCommand::List { config, json } => print_llm_list(config.as_ref(), json),
        LlmCommand::Add {
            id,
            base_url,
            api_key,
            model,
            extra_body,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_llm_add(LlmAddRequest {
            id: &id,
            base_url: &base_url,
            api_key: api_key.as_deref(),
            model: model.as_deref(),
            extra_body: extra_body.as_deref(),
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        LlmCommand::Edit {
            id,
            base_url,
            api_key,
            clear_api_key,
            model,
            clear_model,
            extra_body,
            clear_extra_body,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_llm_edit(LlmEditRequest {
            id: &id,
            base_url: base_url.as_deref(),
            api_key: api_key.as_deref(),
            clear_api_key,
            model: model.as_deref(),
            clear_model,
            extra_body: extra_body.as_deref(),
            clear_extra_body,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        LlmCommand::Test {
            id,
            text,
            timeout_ms,
            config,
            dry_run,
            json,
        } => print_llm_test(&id, &text, timeout_ms, config.as_ref(), dry_run, json),
        LlmCommand::Remove {
            id,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_llm_remove(LlmRemoveRequest {
            id: &id,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
    }
}

fn handle_adapter_command(command: AdapterCommand) -> anyhow::Result<()> {
    match command {
        AdapterCommand::List {
            available,
            registry,
            config,
            json,
        } => print_adapter_list(config.as_ref(), available, registry.as_deref(), json),
        AdapterCommand::Add {
            id,
            command,
            args,
            env,
            working_dir,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_adapter_add(AdapterAddRequest {
            id: &id,
            command: &command,
            args: &args,
            env: &env,
            working_dir: working_dir.as_deref(),
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        AdapterCommand::InstallPlan {
            id,
            registry,
            target_root,
            config,
            summary_only,
            json,
        } => print_adapter_install_plan(
            &id,
            &registry,
            &target_root,
            config.as_ref(),
            summary_only,
            json,
        ),
        AdapterCommand::Start { id, dry_run, json } => {
            print_adapter_lifecycle("start", &id, dbus::method::START_ADAPTER, dry_run, json)
        }
        AdapterCommand::Stop { id, dry_run, json } => {
            print_adapter_lifecycle("stop", &id, dbus::method::STOP_ADAPTER, dry_run, json)
        }
        AdapterCommand::Status { id, dry_run, json } => {
            print_adapter_status(id.as_deref(), dry_run, json)
        }
        AdapterCommand::Edit {
            id,
            command,
            args,
            clear_args,
            env,
            clear_env,
            working_dir,
            clear_working_dir,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_adapter_edit(AdapterEditRequest {
            id: &id,
            command: command.as_deref(),
            args: &args,
            clear_args,
            env: &env,
            clear_env,
            working_dir: working_dir.as_deref(),
            clear_working_dir,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        AdapterCommand::Remove {
            id,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_adapter_remove(AdapterRemoveRequest {
            id: &id,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
    }
}

fn print_llm_test(
    id: &str,
    text: &str,
    timeout_ms: Option<u64>,
    config_path: Option<&PathBuf>,
    dry_run: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    let outcome = run_llm_test(id, text, timeout_ms, config_path, dry_run)?;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&outcome)?);
    } else {
        print_llm_test_text(&outcome);
    }
    Ok(())
}

fn run_llm_test(
    id: &str,
    text: &str,
    timeout_ms: Option<u64>,
    config_path: Option<&PathBuf>,
    dry_run: bool,
) -> anyhow::Result<serde_json::Value> {
    let id = normalize_llm_provider_id(id)?;
    let loaded = load_config_json(config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for llm test")?;
    let config = VinputConfig::from_json_str(&contents).context("parse config for llm test")?;
    config.validate().context("validate config for llm test")?;
    let provider = config
        .llm
        .providers
        .iter()
        .find(|provider| provider.id == id)
        .with_context(|| format!("LLM provider `{id}` not found"))?;
    let scene = llm_test_scene(provider, timeout_ms);
    let request = TextRequest {
        raw_text: text,
        scene: &scene,
        selected_text: None,
    };
    let built =
        build_openai_compatible_chat_request(&request, provider, "")?.with_context(|| {
            format!("LLM provider `{id}` cannot build an OpenAI-compatible request")
        })?;
    if dry_run {
        return Ok(llm_test_output(
            loaded.path.as_ref(),
            loaded.source,
            &id,
            timeout_ms,
            true,
            &built,
            None,
            None,
        ));
    }

    let adapter = OpenAiCompatibleTextAdapter::new(
        provider.clone(),
        ReqwestOpenAiCompatibleChatTransport::new(),
    );
    let payload = adapter
        .finish(&request)
        .with_context(|| format!("test LLM provider `{id}`"))?;
    Ok(llm_test_output(
        loaded.path.as_ref(),
        loaded.source,
        &id,
        timeout_ms,
        false,
        &built,
        Some(&payload),
        Some(payload.candidates.len()),
    ))
}

fn llm_test_scene(
    provider: &vinput_config::LlmProviderConfig,
    timeout_ms: Option<u64>,
) -> SceneDefinition {
    SceneDefinition {
        id: "__llm_test__".to_owned(),
        label: "LLM Test".to_owned(),
        prompt: Some(
            "Return a JSON object with a candidates array containing one short connectivity result."
                .to_owned(),
        ),
        provider_id: Some(provider.id.clone()),
        model: provider.model.clone(),
        candidate_count: 1,
        timeout_ms,
        context_lines: 0,
    }
}

#[allow(clippy::too_many_arguments)]
fn llm_test_output(
    config_path: Option<&PathBuf>,
    source: &'static str,
    provider_id: &str,
    timeout_ms: Option<u64>,
    dry_run: bool,
    request: &vinput_text::OpenAiCompatibleChatRequest,
    payload: Option<&RecognitionPayload>,
    candidate_count: Option<usize>,
) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": dry_run,
        "config_path": config_path,
        "source": source,
        "provider_id": provider_id,
        "timeout_ms": timeout_ms,
        "will_call_http": !dry_run,
        "called": !dry_run,
        "request": {
            "url": request.url,
            "headers": request.redacted_headers(),
            "body": request.body,
            "ignored_extra_body_keys": request.ignored_extra_body_keys,
        },
        "result": payload.map(|payload| serde_json::json!({
            "commit_text": payload.commit_text,
            "candidate_count": candidate_count.unwrap_or(0),
        })),
        "next_steps": [
            "run vinput llm list to verify configured LLM providers",
            "run vinput scene list to inspect scene/provider bindings",
            "run vinput doctor to inspect full local diagnostics"
        ],
    })
}

fn print_llm_test_text(outcome: &serde_json::Value) {
    println!("dry_run: {}", outcome["dry_run"]);
    println!("source: {}", outcome["source"].as_str().unwrap_or("-"));
    if let Some(config_path) = outcome["config_path"].as_str() {
        println!("config_path: {config_path}");
    }
    println!(
        "provider_id: {}",
        outcome["provider_id"].as_str().unwrap_or("-")
    );
    println!("timeout_ms: {}", value_or_dash(&outcome["timeout_ms"]));
    println!("will_call_http: {}", outcome["will_call_http"]);
    println!("called: {}", outcome["called"]);
    if let Some(url) = outcome["request"]["url"].as_str() {
        println!("url: {url}");
    }
    if let Some(result) = outcome.get("result").filter(|value| !value.is_null()) {
        println!(
            "commit_text: {}",
            result["commit_text"].as_str().unwrap_or("-")
        );
        println!("candidate_count: {}", result["candidate_count"]);
    }
}

fn value_or_dash(value: &serde_json::Value) -> String {
    if value.is_null() {
        "-".to_owned()
    } else {
        value.to_string()
    }
}

fn print_llm_add(request: LlmAddRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_llm_add(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&llm_add_outcome_json(&outcome))?
        );
    } else {
        print_llm_add_text(&outcome);
    }
    Ok(())
}

fn run_llm_add(request: &LlmAddRequest<'_>) -> anyhow::Result<LlmAddOutcome> {
    let id = normalize_llm_provider_id(request.id)?;
    let base_url = normalize_llm_base_url(request.base_url)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for llm add")?;
    let config = VinputConfig::from_json_str(&contents).context("parse config for llm add")?;
    config.validate().context("validate config for llm add")?;
    if config
        .llm
        .providers
        .iter()
        .any(|provider| provider.id == id)
    {
        anyhow::bail!("LLM provider `{id}` already exists");
    }
    let before_provider_count = config.llm.providers.len();
    let provider = llm_add_json_object(&id, &base_url, request)?;
    llm_providers_array_mut(&mut loaded.document)?.push(serde_json::Value::Object(provider));
    validate_config_json_value(&loaded.document, "validate updated LLM config")?;

    let write_target = config_set_write_target(
        request.output_path,
        request.in_place,
        request.dry_run,
        loaded.path.as_ref(),
        &default_path,
    )?;
    let mut wrote_config = false;
    if !request.dry_run {
        write_config_set_document(&loaded.document, &write_target)?;
        wrote_config = true;
    }
    Ok(LlmAddOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        provider_id: id,
        before_provider_count,
        after_provider_count: before_provider_count + 1,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn print_llm_edit(request: LlmEditRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_llm_edit(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&llm_edit_outcome_json(&outcome))?
        );
    } else {
        print_llm_edit_text(&outcome);
    }
    Ok(())
}

fn run_llm_edit(request: &LlmEditRequest<'_>) -> anyhow::Result<LlmEditOutcome> {
    let id = normalize_llm_provider_id(request.id)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for llm edit")?;
    let config = VinputConfig::from_json_str(&contents).context("parse config for llm edit")?;
    config.validate().context("validate config for llm edit")?;
    if !config
        .llm
        .providers
        .iter()
        .any(|provider| provider.id == id)
    {
        anyhow::bail!("LLM provider `{id}` not found");
    }
    let provider_index = explicit_llm_provider_index(&loaded.document, &id)?;
    let provider_object = llm_providers_array_mut(&mut loaded.document)?
        .get_mut(provider_index)
        .and_then(serde_json::Value::as_object_mut)
        .with_context(|| format!("LLM provider `{id}` is not a JSON object"))?;
    let changed_fields = apply_llm_edit(provider_object, request)?;
    if changed_fields.is_empty() {
        anyhow::bail!("LLM provider edit requires at least one field change");
    }
    validate_config_json_value(&loaded.document, "validate updated LLM config")?;

    let write_target = config_set_write_target(
        request.output_path,
        request.in_place,
        request.dry_run,
        loaded.path.as_ref(),
        &default_path,
    )?;
    let mut wrote_config = false;
    if !request.dry_run {
        write_config_set_document(&loaded.document, &write_target)?;
        wrote_config = true;
    }
    Ok(LlmEditOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        provider_id: id,
        changed_fields,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn apply_llm_edit(
    provider_object: &mut serde_json::Map<String, serde_json::Value>,
    request: &LlmEditRequest<'_>,
) -> anyhow::Result<Vec<String>> {
    let mut changed = Vec::new();
    if let Some(base_url) = request.base_url {
        provider_object.insert(
            "base_url".to_owned(),
            serde_json::Value::String(normalize_llm_base_url(base_url)?),
        );
        changed.push("base_url".to_owned());
    }
    apply_optional_llm_string_edit(
        provider_object,
        "api_key",
        "api-key",
        request.api_key,
        request.clear_api_key,
        &mut changed,
    )?;
    apply_optional_llm_string_edit(
        provider_object,
        "model",
        "model",
        request.model,
        request.clear_model,
        &mut changed,
    )?;
    if request.extra_body.is_some() && request.clear_extra_body {
        anyhow::bail!("LLM provider edit cannot combine --extra-body and --clear-extra-body");
    }
    if let Some(extra_body) = request.extra_body {
        provider_object.insert("extra_body".to_owned(), parse_llm_extra_body(extra_body)?);
        changed.push("extra_body".to_owned());
    } else if request.clear_extra_body {
        provider_object.remove("extra_body");
        changed.push("extra_body".to_owned());
    }
    Ok(changed)
}

fn apply_optional_llm_string_edit(
    provider_object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    option_name: &str,
    value: Option<&str>,
    clear: bool,
    changed: &mut Vec<String>,
) -> anyhow::Result<()> {
    if value.is_some() && clear {
        anyhow::bail!("LLM provider edit cannot combine --{option_name} and --clear-{option_name}");
    }
    if let Some(value) = value {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            anyhow::bail!("LLM provider field `{key}` cannot be empty");
        }
        provider_object.insert(
            key.to_owned(),
            serde_json::Value::String(trimmed.to_owned()),
        );
        changed.push(key.to_owned());
    } else if clear {
        provider_object.remove(key);
        changed.push(key.to_owned());
    }
    Ok(())
}

fn llm_edit_outcome_json(outcome: &LlmEditOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "provider_id": outcome.provider_id,
        "changed_fields": outcome.changed_fields,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinput llm list to verify configured LLM providers",
            "run vinput scene list to inspect scene/provider bindings",
            "run vinput doctor to inspect full local diagnostics"
        ],
    })
}

fn print_llm_edit_text(outcome: &LlmEditOutcome) {
    println!("dry_run: {}", outcome.dry_run);
    println!("source: {}", outcome.source);
    if let Some(config_path) = &outcome.config_path {
        println!("config_path: {}", config_path.display());
    }
    println!("provider_id: {}", outcome.provider_id);
    println!("changed_fields: {}", outcome.changed_fields.join(","));
    println!("in_place: {}", outcome.in_place);
    if let Some(output_path) = &outcome.output_path {
        println!("output_path: {}", output_path.display());
    }
    if let Some(backup_path) = &outcome.backup_path {
        println!("backup_path: {}", backup_path.display());
    }
    println!("will_write_config: {}", !outcome.dry_run);
    println!("wrote_config: {}", outcome.wrote_config);
}

fn print_llm_remove(request: LlmRemoveRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_llm_remove(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&llm_remove_outcome_json(&outcome))?
        );
    } else {
        print_llm_remove_text(&outcome);
    }
    Ok(())
}

fn run_llm_remove(request: &LlmRemoveRequest<'_>) -> anyhow::Result<LlmRemoveOutcome> {
    let id = normalize_llm_provider_id(request.id)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for llm remove")?;
    let config = VinputConfig::from_json_str(&contents).context("parse config for llm remove")?;
    config
        .validate()
        .context("validate config for llm remove")?;
    if !config
        .llm
        .providers
        .iter()
        .any(|provider| provider.id == id)
    {
        anyhow::bail!("LLM provider `{id}` not found");
    }
    let before_provider_count = config.llm.providers.len();
    let provider_index = explicit_llm_provider_index(&loaded.document, &id)?;
    llm_providers_array_mut(&mut loaded.document)?.remove(provider_index);
    validate_config_json_value(&loaded.document, "validate updated LLM config")?;

    let write_target = config_set_write_target(
        request.output_path,
        request.in_place,
        request.dry_run,
        loaded.path.as_ref(),
        &default_path,
    )?;
    let mut wrote_config = false;
    if !request.dry_run {
        write_config_set_document(&loaded.document, &write_target)?;
        wrote_config = true;
    }
    Ok(LlmRemoveOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        removed_provider_id: id,
        before_provider_count,
        after_provider_count: before_provider_count - 1,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn llm_add_json_object(
    id: &str,
    base_url: &str,
    request: &LlmAddRequest<'_>,
) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
    let mut object = serde_json::Map::new();
    object.insert("id".to_owned(), serde_json::Value::String(id.to_owned()));
    object.insert(
        "base_url".to_owned(),
        serde_json::Value::String(base_url.to_owned()),
    );
    insert_optional_llm_string(&mut object, "api_key", request.api_key)?;
    insert_optional_llm_string(&mut object, "model", request.model)?;
    if let Some(extra_body) = request.extra_body {
        object.insert("extra_body".to_owned(), parse_llm_extra_body(extra_body)?);
    }
    Ok(object)
}

fn insert_optional_llm_string(
    object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<&str>,
) -> anyhow::Result<()> {
    if let Some(value) = value {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            anyhow::bail!("LLM provider field `{key}` cannot be empty");
        }
        object.insert(
            key.to_owned(),
            serde_json::Value::String(trimmed.to_owned()),
        );
    }
    Ok(())
}

fn parse_llm_extra_body(extra_body: &str) -> anyhow::Result<serde_json::Value> {
    let value: serde_json::Value =
        serde_json::from_str(extra_body).with_context(|| "parse --extra-body as JSON object")?;
    if !value.is_object() {
        anyhow::bail!("LLM provider --extra-body must be a JSON object");
    }
    Ok(value)
}

fn llm_providers_array_mut(
    document: &mut serde_json::Value,
) -> anyhow::Result<&mut Vec<serde_json::Value>> {
    document
        .pointer_mut("/llm/providers")
        .and_then(serde_json::Value::as_array_mut)
        .with_context(|| "config pointer `/llm/providers` not found or not an array")
}

fn explicit_llm_provider_index(document: &serde_json::Value, id: &str) -> anyhow::Result<usize> {
    document
        .pointer("/llm/providers")
        .and_then(serde_json::Value::as_array)
        .with_context(|| "config pointer `/llm/providers` not found or not an array")?
        .iter()
        .position(|provider| provider.get("id").and_then(serde_json::Value::as_str) == Some(id))
        .with_context(|| format!("LLM provider `{id}` is not explicitly configured"))
}

fn normalize_llm_provider_id(id: &str) -> anyhow::Result<String> {
    let id = id.trim();
    if id.is_empty() {
        anyhow::bail!("LLM provider id cannot be empty");
    }
    Ok(id.to_owned())
}

fn normalize_llm_base_url(base_url: &str) -> anyhow::Result<String> {
    let base_url = base_url.trim();
    if base_url.is_empty() {
        anyhow::bail!("LLM provider base URL cannot be empty");
    }
    Ok(base_url.to_owned())
}

fn llm_add_outcome_json(outcome: &LlmAddOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "provider_id": outcome.provider_id,
        "before_provider_count": outcome.before_provider_count,
        "after_provider_count": outcome.after_provider_count,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinput llm list to verify configured LLM providers",
            "run vinput scene list to inspect scene/provider bindings",
            "run vinput doctor to inspect full local diagnostics"
        ],
    })
}

fn llm_remove_outcome_json(outcome: &LlmRemoveOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "removed_provider_id": outcome.removed_provider_id,
        "before_provider_count": outcome.before_provider_count,
        "after_provider_count": outcome.after_provider_count,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinput llm list to verify configured LLM providers",
            "run vinput scene list to inspect scene/provider bindings",
            "run vinput doctor to inspect full local diagnostics"
        ],
    })
}

fn print_llm_add_text(outcome: &LlmAddOutcome) {
    println!("dry_run: {}", outcome.dry_run);
    println!("source: {}", outcome.source);
    if let Some(config_path) = &outcome.config_path {
        println!("config_path: {}", config_path.display());
    }
    println!("provider_id: {}", outcome.provider_id);
    println!("before_provider_count: {}", outcome.before_provider_count);
    println!("after_provider_count: {}", outcome.after_provider_count);
    println!("in_place: {}", outcome.in_place);
    if let Some(output_path) = &outcome.output_path {
        println!("output_path: {}", output_path.display());
    }
    if let Some(backup_path) = &outcome.backup_path {
        println!("backup_path: {}", backup_path.display());
    }
    println!("will_write_config: {}", !outcome.dry_run);
    println!("wrote_config: {}", outcome.wrote_config);
}

fn print_llm_remove_text(outcome: &LlmRemoveOutcome) {
    println!("dry_run: {}", outcome.dry_run);
    println!("source: {}", outcome.source);
    if let Some(config_path) = &outcome.config_path {
        println!("config_path: {}", config_path.display());
    }
    println!("removed_provider_id: {}", outcome.removed_provider_id);
    println!("before_provider_count: {}", outcome.before_provider_count);
    println!("after_provider_count: {}", outcome.after_provider_count);
    println!("in_place: {}", outcome.in_place);
    if let Some(output_path) = &outcome.output_path {
        println!("output_path: {}", output_path.display());
    }
    if let Some(backup_path) = &outcome.backup_path {
        println!("backup_path: {}", backup_path.display());
    }
    println!("will_write_config: {}", !outcome.dry_run);
    println!("wrote_config: {}", outcome.wrote_config);
}

fn print_llm_list(config_path: Option<&PathBuf>, json_output: bool) -> anyhow::Result<()> {
    let context = load_llm_list_context(config_path)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&llm_list_json(&context))?
        );
    } else {
        print_llm_list_text(&context);
    }
    Ok(())
}

fn load_llm_list_context(config_path: Option<&PathBuf>) -> anyhow::Result<LlmListContext> {
    let loaded = load_config_json(config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for llm list")?;
    let config = VinputConfig::from_json_str(&contents).context("parse config for llm list")?;
    config.validate().context("validate config for llm list")?;
    Ok(LlmListContext {
        config_path: loaded.path,
        source: loaded.source,
        config,
    })
}

fn llm_list_json(context: &LlmListContext) -> serde_json::Value {
    let providers = context
        .config
        .llm
        .providers
        .iter()
        .map(llm_provider_summary_json)
        .collect::<Vec<_>>();
    serde_json::json!({
        "ok": true,
        "config_path": context.config_path.as_ref(),
        "source": context.source,
        "provider_count": providers.len(),
        "providers": providers,
        "next_steps": [
            "run vinput scene list to inspect scene/provider bindings",
            "run vinput adapter list to inspect command text adapters",
            "run vinput doctor to inspect full local diagnostics"
        ],
    })
}

fn llm_provider_summary_json(provider: &vinput_config::LlmProviderConfig) -> serde_json::Value {
    serde_json::json!({
        "id": provider.id.as_str(),
        "base_url_configured": !provider.base_url.trim().is_empty(),
        "api_key_configured": !provider.api_key.trim().is_empty(),
        "model": provider.model.as_deref(),
        "extra_body_configured": provider.extra_body.as_object().is_some_and(|object| !object.is_empty()),
        "extra_field_count": provider.extra.len(),
    })
}

fn print_llm_list_text(context: &LlmListContext) {
    println!("source: {}", context.source);
    if let Some(path) = &context.config_path {
        println!("config_path: {}", path.display());
    }
    println!("provider_count: {}", context.config.llm.providers.len());
    println!("id	base_url	api_key	model	extra_body	extra_fields");
    for provider in &context.config.llm.providers {
        println!(
            "{}	{}	{}	{}	{}	{}",
            provider.id,
            bool_label(!provider.base_url.trim().is_empty()),
            bool_label(!provider.api_key.trim().is_empty()),
            provider.model.as_deref().unwrap_or("-"),
            bool_label(
                provider
                    .extra_body
                    .as_object()
                    .is_some_and(|object| !object.is_empty())
            ),
            provider.extra.len(),
        );
    }
}

fn print_adapter_install_plan(
    id: &str,
    registry_path: &Path,
    target_root: &Path,
    config_path: Option<&PathBuf>,
    summary_only: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    let id = normalize_adapter_id(id)?;
    let input = fs::read_to_string(registry_path)
        .with_context(|| format!("read registry index `{}`", registry_path.display()))?;
    let index = RegistryIndex::from_json_str(&input)
        .with_context(|| format!("validate registry index `{}`", registry_path.display()))?;
    let config = match config_path {
        Some(config_path) => load_config_file(config_path)?,
        None => VinputConfig::bundled_default().context("parse bundled config")?,
    };
    let target_root_string = target_root.to_string_lossy();
    let plan = index.install_adapter_plan(&id, &config.registry, &target_root_string)?;
    if json_output {
        let output = if summary_only {
            serde_json::json!({
                "ok": true,
                "adapter_id": id,
                "registry_path": registry_path,
                "target_root": plan.target_root,
                "asset_count": plan.summary.asset_count,
                "known_size_bytes": plan.summary.known_size_bytes,
                "missing_checksum_count": plan.summary.missing_checksum_count,
            })
        } else {
            serde_json::json!({
                "ok": true,
                "adapter_id": id,
                "registry_path": registry_path,
                "target_root": plan.target_root,
                "asset_count": plan.summary.asset_count,
                "known_size_bytes": plan.summary.known_size_bytes,
                "missing_checksum_count": plan.summary.missing_checksum_count,
                "assets": plan.assets,
            })
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_adapter_install_plan_text(&id, registry_path, &plan, summary_only);
    }
    Ok(())
}

fn print_adapter_install_plan_text(
    id: &str,
    registry_path: &Path,
    plan: &vinput_registry::InstallPlan,
    summary_only: bool,
) {
    println!("adapter_id: {id}");
    println!("registry_path: {}", registry_path.display());
    println!("target_root: {}", plan.target_root);
    println!("asset_count: {}", plan.summary.asset_count);
    println!("known_size_bytes: {}", plan.summary.known_size_bytes);
    println!(
        "missing_checksum_count: {}",
        plan.summary.missing_checksum_count
    );
    if summary_only {
        return;
    }
    println!("source_path	target_path	urls	checksum_policy	size_bytes");
    for asset in &plan.assets {
        println!(
            "{}	{}	{}	{:?}	{}",
            asset.source_path,
            asset.target_path,
            asset.urls.len(),
            asset.checksum_policy,
            asset
                .size_bytes
                .map_or_else(|| "-".to_owned(), |size| size.to_string()),
        );
    }
}

fn print_adapter_lifecycle(
    action: &str,
    adapter_id: &str,
    method: &str,
    dry_run: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    let adapter_id = normalize_adapter_id(adapter_id)?;
    if !dry_run {
        call_adapter_lifecycle_via_dbus(method, &adapter_id)?;
    }
    let output = adapter_lifecycle_output(action, &adapter_id, method, dry_run);
    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("dry_run: {dry_run}");
        println!("adapter_id: {adapter_id}");
        println!("action: {action}");
        println!("will_call_dbus: {}", !dry_run);
        println!("called: {}", !dry_run);
        println!("service: {}", dbus::SERVICE_BUS_NAME);
        println!("object_path: {}", dbus::SERVICE_OBJECT_PATH);
        println!("interface: {}", dbus::SERVICE_INTERFACE);
        println!("method: {method}");
    }
    Ok(())
}

fn adapter_lifecycle_output(
    action: &str,
    adapter_id: &str,
    method: &str,
    dry_run: bool,
) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": dry_run,
        "action": action,
        "adapter_id": adapter_id,
        "will_call_dbus": !dry_run,
        "called": !dry_run,
        "dbus": {
            "service": dbus::SERVICE_BUS_NAME,
            "object_path": dbus::SERVICE_OBJECT_PATH,
            "interface": dbus::SERVICE_INTERFACE,
            "method": method,
        },
        "next_steps": [
            "run vinput daemon status --json to inspect text adapter runtime state",
            "run vinput adapter list to verify configured text adapters",
            "run vinput doctor to inspect full local diagnostics"
        ],
    })
}

fn call_adapter_lifecycle_via_dbus(method: &str, adapter_id: &str) -> anyhow::Result<()> {
    let connection = zbus::blocking::Connection::session().context("connect to session bus")?;
    let proxy = daemon_service_proxy(&connection)?;
    let _: () = proxy
        .call(method, &(adapter_id))
        .with_context(|| format!("call {method} on daemon D-Bus service"))?;
    Ok(())
}

fn print_adapter_status(
    adapter_id: Option<&str>,
    dry_run: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    let adapter_id = adapter_id.map(normalize_adapter_id).transpose()?;
    let output = if dry_run {
        adapter_status_plan_json(adapter_id.as_deref())
    } else {
        let state = call_text_adapter_state_via_dbus()?;
        adapter_status_state_json(adapter_id.as_deref(), &state)?
    };
    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_adapter_status_text(&output);
    }
    Ok(())
}

fn call_text_adapter_state_via_dbus() -> anyhow::Result<TextAdapterState> {
    let connection = zbus::blocking::Connection::session().context("connect to session bus")?;
    let proxy = daemon_service_proxy(&connection)?;
    let raw: String = proxy
        .call(dbus::method::GET_TEXT_ADAPTER_STATE, &())
        .context("call GetTextAdapterState on daemon D-Bus service")?;
    serde_json::from_str::<TextAdapterState>(&raw).context("parse GetTextAdapterState response")
}

fn adapter_status_plan_json(adapter_id: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": true,
        "action": "status",
        "adapter_id": adapter_id,
        "will_call_dbus": false,
        "called": false,
        "dbus": {
            "service": dbus::SERVICE_BUS_NAME,
            "object_path": dbus::SERVICE_OBJECT_PATH,
            "interface": dbus::SERVICE_INTERFACE,
            "method": dbus::method::GET_TEXT_ADAPTER_STATE,
        },
        "next_steps": [
            "run vinput adapter status without --dry-run to query daemon runtime state",
            "run vinput adapter start or stop to change adapter runtime state"
        ],
    })
}

fn adapter_status_state_json(
    adapter_id: Option<&str>,
    state: &TextAdapterState,
) -> anyhow::Result<serde_json::Value> {
    let state_json = serde_json::json!({
        "adapter_count": state.adapter_count,
        "adapter_ids": state.adapter_ids,
        "single_adapter_id": state.single_adapter_id,
    });
    if let Some(adapter_id) = adapter_id {
        let adapter = state
            .adapters
            .iter()
            .find(|adapter| adapter.id == adapter_id)
            .with_context(|| format!("text adapter `{adapter_id}` not found in daemon state"))?;
        return Ok(serde_json::json!({
            "ok": true,
            "dry_run": false,
            "action": "status",
            "adapter_id": adapter_id,
            "state": state_json,
            "adapter": adapter,
        }));
    }
    Ok(serde_json::json!({
        "ok": true,
        "dry_run": false,
        "action": "status",
        "adapter_id": serde_json::Value::Null,
        "state": state_json,
        "adapters": state.adapters,
    }))
}

fn print_adapter_status_text(output: &serde_json::Value) {
    println!("dry_run: {}", output["dry_run"].as_bool().unwrap_or(false));
    println!("action: status");
    println!(
        "adapter_id: {}",
        output["adapter_id"].as_str().unwrap_or("-")
    );
    if output["dry_run"].as_bool().unwrap_or(false) {
        println!(
            "will_call_dbus: {}",
            output["will_call_dbus"].as_bool().unwrap_or(false)
        );
        println!("called: {}", output["called"].as_bool().unwrap_or(false));
        println!("service: {}", dbus::SERVICE_BUS_NAME);
        println!("object_path: {}", dbus::SERVICE_OBJECT_PATH);
        println!("interface: {}", dbus::SERVICE_INTERFACE);
        println!("method: {}", dbus::method::GET_TEXT_ADAPTER_STATE);
        return;
    }
    println!(
        "adapter_count: {}",
        output["state"]["adapter_count"].as_u64().unwrap_or(0)
    );
    if let Some(adapter) = output.get("adapter") {
        print_adapter_status_row(adapter);
        return;
    }
    println!("id	kind	running	pid	args	env	working_dir");
    if let Some(adapters) = output["adapters"].as_array() {
        for adapter in adapters {
            print_adapter_status_row(adapter);
        }
    }
}

fn print_adapter_status_row(adapter: &serde_json::Value) {
    println!(
        "{}	{}	{}	{}	{}	{}	{}",
        adapter["id"].as_str().unwrap_or("-"),
        adapter["kind"].as_str().unwrap_or("-"),
        adapter["is_running"].as_bool().unwrap_or(false),
        adapter["pid"]
            .as_u64()
            .map_or_else(|| "-".to_owned(), |pid| pid.to_string()),
        adapter["args_count"].as_u64().unwrap_or(0),
        adapter["env_count"].as_u64().unwrap_or(0),
        adapter["has_working_dir"].as_bool().unwrap_or(false),
    );
}

fn print_adapter_edit(request: AdapterEditRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_adapter_edit(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&adapter_edit_outcome_json(&outcome))?
        );
    } else {
        print_adapter_edit_text(&outcome);
    }
    Ok(())
}

fn run_adapter_edit(request: &AdapterEditRequest<'_>) -> anyhow::Result<AdapterEditOutcome> {
    let id = normalize_adapter_id(request.id)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for adapter edit")?;
    let config = VinputConfig::from_json_str(&contents).context("parse config for adapter edit")?;
    config
        .validate()
        .context("validate config for adapter edit")?;
    if !config.llm.adapters.iter().any(|adapter| adapter.id == id) {
        anyhow::bail!("text adapter `{id}` not found");
    }
    let adapter_index = explicit_adapter_index(&loaded.document, &id)?;
    let adapter_object = llm_adapters_array_mut(&mut loaded.document)?
        .get_mut(adapter_index)
        .and_then(serde_json::Value::as_object_mut)
        .with_context(|| format!("text adapter `{id}` is not a JSON object"))?;
    let changed_fields = apply_adapter_edit(adapter_object, request)?;
    if changed_fields.is_empty() {
        anyhow::bail!("text adapter edit requires at least one field change");
    }
    validate_config_json_value(&loaded.document, "validate updated adapter config")?;

    let write_target = config_set_write_target(
        request.output_path,
        request.in_place,
        request.dry_run,
        loaded.path.as_ref(),
        &default_path,
    )?;
    let mut wrote_config = false;
    if !request.dry_run {
        write_config_set_document(&loaded.document, &write_target)?;
        wrote_config = true;
    }
    Ok(AdapterEditOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        adapter_id: id,
        changed_fields,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn apply_adapter_edit(
    adapter_object: &mut serde_json::Map<String, serde_json::Value>,
    request: &AdapterEditRequest<'_>,
) -> anyhow::Result<Vec<String>> {
    let mut changed = Vec::new();
    if let Some(command) = request.command {
        adapter_object.insert(
            "command".to_owned(),
            serde_json::Value::String(normalize_adapter_command(command)?),
        );
        changed.push("command".to_owned());
    }
    if !request.args.is_empty() && request.clear_args {
        anyhow::bail!("text adapter edit cannot combine --arg and --clear-args");
    }
    if !request.args.is_empty() {
        adapter_object.insert("args".to_owned(), serde_json::json!(request.args));
        changed.push("args".to_owned());
    } else if request.clear_args {
        adapter_object.remove("args");
        changed.push("args".to_owned());
    }
    if !request.env.is_empty() && request.clear_env {
        anyhow::bail!("text adapter edit cannot combine --env and --clear-env");
    }
    if !request.env.is_empty() {
        adapter_object.insert(
            "env".to_owned(),
            serde_json::json!(parse_adapter_env(request.env)?),
        );
        changed.push("env".to_owned());
    } else if request.clear_env {
        adapter_object.remove("env");
        changed.push("env".to_owned());
    }
    if request.working_dir.is_some() && request.clear_working_dir {
        anyhow::bail!("text adapter edit cannot combine --working-dir and --clear-working-dir");
    }
    if let Some(working_dir) = request.working_dir {
        let working_dir = working_dir.trim();
        if working_dir.is_empty() {
            anyhow::bail!("text adapter field `working_dir` cannot be empty");
        }
        adapter_object.insert(
            "working_dir".to_owned(),
            serde_json::Value::String(working_dir.to_owned()),
        );
        changed.push("working_dir".to_owned());
    } else if request.clear_working_dir {
        adapter_object.remove("working_dir");
        changed.push("working_dir".to_owned());
    }
    Ok(changed)
}

fn adapter_edit_outcome_json(outcome: &AdapterEditOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "adapter_id": outcome.adapter_id,
        "changed_fields": outcome.changed_fields,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinput adapter list to verify configured text adapters",
            "run vinput daemon status --json to inspect text adapter runtime state",
            "run vinput doctor to inspect full local diagnostics"
        ],
    })
}

fn print_adapter_edit_text(outcome: &AdapterEditOutcome) {
    println!("dry_run: {}", outcome.dry_run);
    println!("source: {}", outcome.source);
    if let Some(config_path) = &outcome.config_path {
        println!("config_path: {}", config_path.display());
    }
    println!("adapter_id: {}", outcome.adapter_id);
    println!("changed_fields: {}", outcome.changed_fields.join(","));
    println!("in_place: {}", outcome.in_place);
    if let Some(output_path) = &outcome.output_path {
        println!("output_path: {}", output_path.display());
    }
    if let Some(backup_path) = &outcome.backup_path {
        println!("backup_path: {}", backup_path.display());
    }
    println!("will_write_config: {}", !outcome.dry_run);
    println!("wrote_config: {}", outcome.wrote_config);
}

fn print_adapter_add(request: AdapterAddRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_adapter_add(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&adapter_add_outcome_json(&outcome))?
        );
    } else {
        print_adapter_add_text(&outcome);
    }
    Ok(())
}

fn run_adapter_add(request: &AdapterAddRequest<'_>) -> anyhow::Result<AdapterAddOutcome> {
    let id = normalize_adapter_id(request.id)?;
    let command = normalize_adapter_command(request.command)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for adapter add")?;
    let config = VinputConfig::from_json_str(&contents).context("parse config for adapter add")?;
    config
        .validate()
        .context("validate config for adapter add")?;
    if config.llm.adapters.iter().any(|adapter| adapter.id == id) {
        anyhow::bail!("text adapter `{id}` already exists");
    }
    let before_adapter_count = config.llm.adapters.len();
    let adapter = adapter_add_json_object(&id, &command, request)?;
    llm_adapters_array_mut(&mut loaded.document)?.push(serde_json::Value::Object(adapter));
    validate_config_json_value(&loaded.document, "validate updated adapter config")?;

    let write_target = config_set_write_target(
        request.output_path,
        request.in_place,
        request.dry_run,
        loaded.path.as_ref(),
        &default_path,
    )?;
    let mut wrote_config = false;
    if !request.dry_run {
        write_config_set_document(&loaded.document, &write_target)?;
        wrote_config = true;
    }
    Ok(AdapterAddOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        adapter_id: id,
        before_adapter_count,
        after_adapter_count: before_adapter_count + 1,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn print_adapter_remove(request: AdapterRemoveRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_adapter_remove(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&adapter_remove_outcome_json(&outcome))?
        );
    } else {
        print_adapter_remove_text(&outcome);
    }
    Ok(())
}

fn run_adapter_remove(request: &AdapterRemoveRequest<'_>) -> anyhow::Result<AdapterRemoveOutcome> {
    let id = normalize_adapter_id(request.id)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for adapter remove")?;
    let config =
        VinputConfig::from_json_str(&contents).context("parse config for adapter remove")?;
    config
        .validate()
        .context("validate config for adapter remove")?;
    if !config.llm.adapters.iter().any(|adapter| adapter.id == id) {
        anyhow::bail!("text adapter `{id}` not found");
    }
    let before_adapter_count = config.llm.adapters.len();
    let adapter_index = explicit_adapter_index(&loaded.document, &id)?;
    llm_adapters_array_mut(&mut loaded.document)?.remove(adapter_index);
    validate_config_json_value(&loaded.document, "validate updated adapter config")?;

    let write_target = config_set_write_target(
        request.output_path,
        request.in_place,
        request.dry_run,
        loaded.path.as_ref(),
        &default_path,
    )?;
    let mut wrote_config = false;
    if !request.dry_run {
        write_config_set_document(&loaded.document, &write_target)?;
        wrote_config = true;
    }
    Ok(AdapterRemoveOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        removed_adapter_id: id,
        before_adapter_count,
        after_adapter_count: before_adapter_count - 1,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn adapter_add_json_object(
    id: &str,
    command: &str,
    request: &AdapterAddRequest<'_>,
) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
    let mut object = serde_json::Map::new();
    object.insert("id".to_owned(), serde_json::Value::String(id.to_owned()));
    object.insert(
        "command".to_owned(),
        serde_json::Value::String(command.to_owned()),
    );
    if !request.args.is_empty() {
        object.insert("args".to_owned(), serde_json::json!(request.args));
    }
    if !request.env.is_empty() {
        object.insert(
            "env".to_owned(),
            serde_json::json!(parse_adapter_env(request.env)?),
        );
    }
    if let Some(working_dir) = request.working_dir {
        let trimmed = working_dir.trim();
        if trimmed.is_empty() {
            anyhow::bail!("text adapter field `working_dir` cannot be empty");
        }
        object.insert(
            "working_dir".to_owned(),
            serde_json::Value::String(trimmed.to_owned()),
        );
    }
    Ok(object)
}

fn parse_adapter_env(entries: &[String]) -> anyhow::Result<BTreeMap<String, String>> {
    let mut env = BTreeMap::new();
    for entry in entries {
        let (key, value) = entry
            .split_once('=')
            .with_context(|| format!("text adapter env `{entry}` is not KEY=VALUE"))?;
        let key = key.trim();
        if key.is_empty() {
            anyhow::bail!("text adapter env `{entry}` has an empty key");
        }
        env.insert(key.to_owned(), value.to_owned());
    }
    Ok(env)
}

fn llm_adapters_array_mut(
    document: &mut serde_json::Value,
) -> anyhow::Result<&mut Vec<serde_json::Value>> {
    document
        .pointer_mut("/llm/adapters")
        .and_then(serde_json::Value::as_array_mut)
        .with_context(|| "config pointer `/llm/adapters` not found or not an array")
}

fn explicit_adapter_index(document: &serde_json::Value, id: &str) -> anyhow::Result<usize> {
    document
        .pointer("/llm/adapters")
        .and_then(serde_json::Value::as_array)
        .with_context(|| "config pointer `/llm/adapters` not found or not an array")?
        .iter()
        .position(|adapter| adapter.get("id").and_then(serde_json::Value::as_str) == Some(id))
        .with_context(|| format!("text adapter `{id}` is not explicitly configured"))
}

fn normalize_adapter_id(id: &str) -> anyhow::Result<String> {
    let id = id.trim();
    if id.is_empty() {
        anyhow::bail!("text adapter id cannot be empty");
    }
    Ok(id.to_owned())
}

fn normalize_adapter_command(command: &str) -> anyhow::Result<String> {
    let command = command.trim();
    if command.is_empty() {
        anyhow::bail!("text adapter command cannot be empty");
    }
    Ok(command.to_owned())
}

fn adapter_add_outcome_json(outcome: &AdapterAddOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "adapter_id": outcome.adapter_id,
        "before_adapter_count": outcome.before_adapter_count,
        "after_adapter_count": outcome.after_adapter_count,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinput adapter list to verify configured text adapters",
            "run vinput scene list to inspect scenes that need adapters",
            "run vinput doctor to inspect full local diagnostics"
        ],
    })
}

fn adapter_remove_outcome_json(outcome: &AdapterRemoveOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "removed_adapter_id": outcome.removed_adapter_id,
        "before_adapter_count": outcome.before_adapter_count,
        "after_adapter_count": outcome.after_adapter_count,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinput adapter list to verify configured text adapters",
            "run vinput scene list to inspect scenes that need adapters",
            "run vinput doctor to inspect full local diagnostics"
        ],
    })
}

fn print_adapter_add_text(outcome: &AdapterAddOutcome) {
    println!("dry_run: {}", outcome.dry_run);
    println!("source: {}", outcome.source);
    if let Some(config_path) = &outcome.config_path {
        println!("config_path: {}", config_path.display());
    }
    println!("adapter_id: {}", outcome.adapter_id);
    println!("before_adapter_count: {}", outcome.before_adapter_count);
    println!("after_adapter_count: {}", outcome.after_adapter_count);
    println!("in_place: {}", outcome.in_place);
    if let Some(output_path) = &outcome.output_path {
        println!("output_path: {}", output_path.display());
    }
    if let Some(backup_path) = &outcome.backup_path {
        println!("backup_path: {}", backup_path.display());
    }
    println!("will_write_config: {}", !outcome.dry_run);
    println!("wrote_config: {}", outcome.wrote_config);
}

fn print_adapter_remove_text(outcome: &AdapterRemoveOutcome) {
    println!("dry_run: {}", outcome.dry_run);
    println!("source: {}", outcome.source);
    if let Some(config_path) = &outcome.config_path {
        println!("config_path: {}", config_path.display());
    }
    println!("removed_adapter_id: {}", outcome.removed_adapter_id);
    println!("before_adapter_count: {}", outcome.before_adapter_count);
    println!("after_adapter_count: {}", outcome.after_adapter_count);
    println!("in_place: {}", outcome.in_place);
    if let Some(output_path) = &outcome.output_path {
        println!("output_path: {}", output_path.display());
    }
    if let Some(backup_path) = &outcome.backup_path {
        println!("backup_path: {}", backup_path.display());
    }
    println!("will_write_config: {}", !outcome.dry_run);
    println!("wrote_config: {}", outcome.wrote_config);
}

fn print_adapter_list(
    config_path: Option<&PathBuf>,
    available: bool,
    registry_path: Option<&Path>,
    json_output: bool,
) -> anyhow::Result<()> {
    if available {
        return print_available_adapter_list(registry_path, config_path, json_output);
    }
    let context = load_adapter_list_context(config_path)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&adapter_list_json(&context))?
        );
    } else {
        print_adapter_list_text(&context);
    }
    Ok(())
}

fn print_available_adapter_list(
    registry_path: Option<&Path>,
    config_path: Option<&PathBuf>,
    json_output: bool,
) -> anyhow::Result<()> {
    let registry_path =
        registry_path.with_context(|| "adapter list --available requires --registry <path>")?;
    let input = fs::read_to_string(registry_path)
        .with_context(|| format!("read registry index `{}`", registry_path.display()))?;
    let index = RegistryIndex::from_json_str(&input)
        .with_context(|| format!("validate registry index `{}`", registry_path.display()))?;
    let context = load_adapter_list_context(config_path)?;
    let configured_ids = context
        .config
        .llm
        .adapters
        .iter()
        .map(|adapter| adapter.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let adapters = index
        .adapters
        .iter()
        .map(|adapter| available_adapter_json(adapter, &configured_ids))
        .collect::<Vec<_>>();
    let output = serde_json::json!({
        "ok": true,
        "registry_path": registry_path,
        "config_path": context.config_path.as_ref(),
        "config_source": context.source,
        "adapter_count": adapters.len(),
        "adapters": adapters,
        "next_steps": [
            "run vinput registry plan <path> --adapter <id> to inspect assets",
            "run vinput adapter add <id> --command <path> --dry-run --json to preview config",
            "run vinput doctor to inspect full local diagnostics"
        ],
    });
    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_available_adapter_list_text(registry_path, &context, &index, &configured_ids);
    }
    Ok(())
}

fn available_adapter_json(
    adapter: &vinput_registry::AdapterEntry,
    configured_ids: &std::collections::BTreeSet<&str>,
) -> serde_json::Value {
    let known_size_bytes = adapter
        .assets
        .iter()
        .filter_map(|asset| asset.size_bytes)
        .sum::<u64>();
    let unknown_size_count = adapter
        .assets
        .iter()
        .filter(|asset| asset.size_bytes.is_none())
        .count();
    serde_json::json!({
        "id": adapter.id,
        "label": adapter.label,
        "kind": adapter.kind,
        "configured": configured_ids.contains(adapter.id.as_str()),
        "asset_count": adapter.assets.len(),
        "known_size_bytes": known_size_bytes,
        "unknown_size_count": unknown_size_count,
        "assets": adapter.assets,
    })
}

fn print_available_adapter_list_text(
    registry_path: &Path,
    context: &AdapterListContext,
    index: &RegistryIndex,
    configured_ids: &std::collections::BTreeSet<&str>,
) {
    println!("registry_path: {}", registry_path.display());
    println!("config_source: {}", context.source);
    if let Some(path) = &context.config_path {
        println!("config_path: {}", path.display());
    }
    println!("adapter_count: {}", index.adapters.len());
    println!("id	kind	configured	assets	known_size_bytes	unknown_size_count	label");
    for adapter in &index.adapters {
        let known_size_bytes = adapter
            .assets
            .iter()
            .filter_map(|asset| asset.size_bytes)
            .sum::<u64>();
        let unknown_size_count = adapter
            .assets
            .iter()
            .filter(|asset| asset.size_bytes.is_none())
            .count();
        println!(
            "{}	{}	{}	{}	{}	{}	{}",
            adapter.id,
            adapter.kind,
            bool_label(configured_ids.contains(adapter.id.as_str())),
            adapter.assets.len(),
            known_size_bytes,
            unknown_size_count,
            adapter.label,
        );
    }
}

fn load_adapter_list_context(config_path: Option<&PathBuf>) -> anyhow::Result<AdapterListContext> {
    let loaded = load_config_json(config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for adapter list")?;
    let config = VinputConfig::from_json_str(&contents).context("parse config for adapter list")?;
    config
        .validate()
        .context("validate config for adapter list")?;
    Ok(AdapterListContext {
        config_path: loaded.path,
        source: loaded.source,
        config,
    })
}

fn adapter_list_json(context: &AdapterListContext) -> serde_json::Value {
    let adapters = context
        .config
        .llm
        .adapters
        .iter()
        .map(adapter_summary_json)
        .collect::<Vec<_>>();
    serde_json::json!({
        "ok": true,
        "config_path": context.config_path.as_ref(),
        "source": context.source,
        "adapter_count": adapters.len(),
        "adapters": adapters,
        "next_steps": [
            "run vinput scene list to inspect scenes that need adapters",
            "run vinput daemon status --dry-run --json to inspect daemon D-Bus calls",
            "run vinput doctor to inspect full local diagnostics"
        ],
    })
}

fn adapter_summary_json(adapter: &vinput_config::LlmAdapterConfig) -> serde_json::Value {
    serde_json::json!({
        "id": adapter.id.as_str(),
        "command_configured": !adapter.command.trim().is_empty(),
        "args_count": adapter.args.len(),
        "env_count": adapter.env.len(),
        "working_dir_configured": adapter.working_dir.as_ref().is_some_and(|value| !value.trim().is_empty()),
        "extra_field_count": adapter.extra.len(),
    })
}

fn print_adapter_list_text(context: &AdapterListContext) {
    println!("source: {}", context.source);
    if let Some(path) = &context.config_path {
        println!("config_path: {}", path.display());
    }
    println!("adapter_count: {}", context.config.llm.adapters.len());
    println!("id	command	args	env	working_dir	extra_fields");
    for adapter in &context.config.llm.adapters {
        println!(
            "{}	{}	{}	{}	{}	{}",
            adapter.id,
            bool_label(!adapter.command.trim().is_empty()),
            adapter.args.len(),
            adapter.env.len(),
            bool_label(
                adapter
                    .working_dir
                    .as_ref()
                    .is_some_and(|value| !value.trim().is_empty())
            ),
            adapter.extra.len(),
        );
    }
}

fn bool_label(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

struct SceneListContext {
    config_path: Option<PathBuf>,
    source: &'static str,
    config: VinputConfig,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct SceneAddRequest<'a> {
    id: &'a str,
    label: &'a str,
    prompt: Option<&'a str>,
    provider_id: Option<&'a str>,
    model: Option<&'a str>,
    candidate_count: u8,
    timeout_ms: Option<u64>,
    context_lines: u8,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct SceneEditRequest<'a> {
    id: &'a str,
    label: Option<&'a str>,
    prompt: Option<&'a str>,
    clear_prompt: bool,
    provider_id: Option<&'a str>,
    clear_provider_id: bool,
    model: Option<&'a str>,
    clear_model: bool,
    candidate_count: Option<u8>,
    timeout_ms: Option<u64>,
    clear_timeout: bool,
    context_lines: Option<u8>,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct SceneRemoveRequest<'a> {
    id: &'a str,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct SceneAddOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    scene_id: String,
    active_scene: String,
    before_scene_count: usize,
    after_scene_count: usize,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

struct SceneEditOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    scene_id: String,
    active_scene: String,
    changed_fields: Vec<String>,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

struct SceneRemoveOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    removed_scene_id: String,
    active_scene: String,
    before_scene_count: usize,
    after_scene_count: usize,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct SceneUseRequest<'a> {
    id: &'a str,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct SceneUseOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    before: String,
    after: String,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

fn handle_scene_command(command: SceneCommand) -> anyhow::Result<()> {
    match command {
        SceneCommand::List { config, json } => print_scene_list(config.as_ref(), json),
        SceneCommand::Add {
            id,
            label,
            prompt,
            provider_id,
            model,
            candidate_count,
            timeout_ms,
            context_lines,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_scene_add(SceneAddRequest {
            id: &id,
            label: &label,
            prompt: prompt.as_deref(),
            provider_id: provider_id.as_deref(),
            model: model.as_deref(),
            candidate_count,
            timeout_ms,
            context_lines,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        SceneCommand::Edit {
            id,
            label,
            prompt,
            clear_prompt,
            provider_id,
            clear_provider_id,
            model,
            clear_model,
            candidate_count,
            timeout_ms,
            clear_timeout,
            context_lines,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_scene_edit(SceneEditRequest {
            id: &id,
            label: label.as_deref(),
            prompt: prompt.as_deref(),
            clear_prompt,
            provider_id: provider_id.as_deref(),
            clear_provider_id,
            model: model.as_deref(),
            clear_model,
            candidate_count,
            timeout_ms,
            clear_timeout,
            context_lines,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        SceneCommand::Use {
            id,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_scene_use(SceneUseRequest {
            id: &id,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        SceneCommand::Remove {
            id,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_scene_remove(SceneRemoveRequest {
            id: &id,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
    }
}

fn print_scene_list(config_path: Option<&PathBuf>, json_output: bool) -> anyhow::Result<()> {
    let context = load_scene_list_context(config_path)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&scene_list_json(&context))?
        );
    } else {
        print_scene_list_text(&context);
    }
    Ok(())
}

fn load_scene_list_context(config_path: Option<&PathBuf>) -> anyhow::Result<SceneListContext> {
    let loaded = load_config_json(config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for scene list")?;
    let config = VinputConfig::from_json_str(&contents).context("parse config for scene list")?;
    config
        .validate()
        .context("validate config for scene list")?;
    Ok(SceneListContext {
        config_path: loaded.path,
        source: loaded.source,
        config,
    })
}

fn scene_list_json(context: &SceneListContext) -> serde_json::Value {
    let active_scene = context.config.scenes.active_scene.as_str();
    let scenes = context
        .config
        .scenes
        .definitions
        .iter()
        .map(|scene| scene_summary_json(scene, active_scene))
        .collect::<Vec<_>>();
    serde_json::json!({
        "ok": true,
        "config_path": context.config_path.as_ref(),
        "source": context.source,
        "active_scene": active_scene,
        "scene_count": scenes.len(),
        "scenes": scenes,
        "next_steps": [
            "run vinput scene use <id> --dry-run --json to preview scene selection",
            "run vinput recording start --dry-run --json to inspect recording D-Bus calls",
            "run vinput doctor to inspect full local diagnostics"
        ],
    })
}

fn scene_summary_json(
    scene: &vinput_config::SceneDefinition,
    active_scene: &str,
) -> serde_json::Value {
    serde_json::json!({
        "id": scene.id.as_str(),
        "label": scene.label.as_str(),
        "active": scene.id.as_str() == active_scene,
        "prompt_configured": scene.prompt.as_ref().is_some_and(|value| !value.trim().is_empty()),
        "provider_id": scene.provider_id.as_deref(),
        "model": scene.model.as_deref(),
        "candidate_count": scene.candidate_count,
        "timeout_ms": scene.timeout_ms,
        "context_lines": scene.context_lines,
    })
}

fn print_scene_list_text(context: &SceneListContext) {
    println!("source: {}", context.source);
    if let Some(path) = &context.config_path {
        println!("config_path: {}", path.display());
    }
    println!("active_scene: {}", context.config.scenes.active_scene);
    println!("scene_count: {}", context.config.scenes.definitions.len());
    println!("active	id	label	prompt	provider	model	candidates	timeout_ms	context_lines");
    for scene in &context.config.scenes.definitions {
        println!(
            "{}	{}	{}	{}	{}	{}	{}	{}	{}",
            if scene.id == context.config.scenes.active_scene {
                "*"
            } else {
                ""
            },
            scene.id,
            scene.label,
            configured_label(scene.prompt.as_deref()),
            scene.provider_id.as_deref().unwrap_or("-"),
            scene.model.as_deref().unwrap_or("-"),
            scene.candidate_count,
            scene
                .timeout_ms
                .map_or_else(|| "-".to_owned(), |value| value.to_string()),
            scene.context_lines
        );
    }
}

fn print_scene_add(request: SceneAddRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_scene_add(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&scene_add_outcome_json(&outcome))?
        );
    } else {
        print_scene_add_text(&outcome);
    }
    Ok(())
}

fn run_scene_add(request: &SceneAddRequest<'_>) -> anyhow::Result<SceneAddOutcome> {
    let id = normalize_scene_id(request.id)?;
    let label = normalize_scene_label(request.label)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for scene add")?;
    let config = VinputConfig::from_json_str(&contents).context("parse config for scene add")?;
    config.validate().context("validate config for scene add")?;
    if config.scenes.definitions.iter().any(|scene| scene.id == id) {
        anyhow::bail!("scene `{id}` already exists");
    }
    let before_scene_count = config.scenes.definitions.len();
    let scene = scene_add_json_object(&id, &label, request)?;
    scene_definitions_array_mut(&mut loaded.document)?.push(serde_json::Value::Object(scene));
    validate_config_json_value(&loaded.document, "validate updated scene config")?;

    let write_target = config_set_write_target(
        request.output_path,
        request.in_place,
        request.dry_run,
        loaded.path.as_ref(),
        &default_path,
    )?;
    let mut wrote_config = false;
    if !request.dry_run {
        write_config_set_document(&loaded.document, &write_target)?;
        wrote_config = true;
    }
    Ok(SceneAddOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        scene_id: id,
        active_scene: config.scenes.active_scene,
        before_scene_count,
        after_scene_count: before_scene_count + 1,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn print_scene_edit(request: SceneEditRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_scene_edit(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&scene_edit_outcome_json(&outcome))?
        );
    } else {
        print_scene_edit_text(&outcome);
    }
    Ok(())
}

fn run_scene_edit(request: &SceneEditRequest<'_>) -> anyhow::Result<SceneEditOutcome> {
    let id = normalize_scene_id(request.id)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for scene edit")?;
    let config = VinputConfig::from_json_str(&contents).context("parse config for scene edit")?;
    config
        .validate()
        .context("validate config for scene edit")?;
    if !config.scenes.definitions.iter().any(|scene| scene.id == id) {
        anyhow::bail!("scene `{id}` not found");
    }
    let scene_index = explicit_scene_index(&loaded.document, &id)?;
    let scene_object = scene_definitions_array_mut(&mut loaded.document)?
        .get_mut(scene_index)
        .and_then(serde_json::Value::as_object_mut)
        .with_context(|| format!("scene `{id}` is not a JSON object"))?;
    let changed_fields = apply_scene_edit(scene_object, request)?;
    if changed_fields.is_empty() {
        anyhow::bail!("scene edit requires at least one field change");
    }
    validate_config_json_value(&loaded.document, "validate updated scene config")?;

    let write_target = config_set_write_target(
        request.output_path,
        request.in_place,
        request.dry_run,
        loaded.path.as_ref(),
        &default_path,
    )?;
    let mut wrote_config = false;
    if !request.dry_run {
        write_config_set_document(&loaded.document, &write_target)?;
        wrote_config = true;
    }
    Ok(SceneEditOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        scene_id: id,
        active_scene: config.scenes.active_scene,
        changed_fields,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn print_scene_remove(request: SceneRemoveRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_scene_remove(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&scene_remove_outcome_json(&outcome))?
        );
    } else {
        print_scene_remove_text(&outcome);
    }
    Ok(())
}

fn run_scene_remove(request: &SceneRemoveRequest<'_>) -> anyhow::Result<SceneRemoveOutcome> {
    let id = normalize_scene_id(request.id)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for scene remove")?;
    let config = VinputConfig::from_json_str(&contents).context("parse config for scene remove")?;
    config
        .validate()
        .context("validate config for scene remove")?;
    if !config.scenes.definitions.iter().any(|scene| scene.id == id) {
        anyhow::bail!("scene `{id}` not found");
    }
    if id == config.scenes.active_scene {
        anyhow::bail!("refusing to remove active scene `{id}`; run vinput scene use <id> first");
    }
    let before_scene_count = config.scenes.definitions.len();
    let scene_index = explicit_scene_index(&loaded.document, &id)?;
    scene_definitions_array_mut(&mut loaded.document)?.remove(scene_index);
    validate_config_json_value(&loaded.document, "validate updated scene config")?;

    let write_target = config_set_write_target(
        request.output_path,
        request.in_place,
        request.dry_run,
        loaded.path.as_ref(),
        &default_path,
    )?;
    let mut wrote_config = false;
    if !request.dry_run {
        write_config_set_document(&loaded.document, &write_target)?;
        wrote_config = true;
    }
    Ok(SceneRemoveOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        removed_scene_id: id,
        active_scene: config.scenes.active_scene,
        before_scene_count,
        after_scene_count: before_scene_count - 1,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn scene_add_json_object(
    id: &str,
    label: &str,
    request: &SceneAddRequest<'_>,
) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
    let mut object = serde_json::Map::new();
    object.insert("id".to_owned(), serde_json::Value::String(id.to_owned()));
    object.insert(
        "label".to_owned(),
        serde_json::Value::String(label.to_owned()),
    );
    insert_optional_scene_string(&mut object, "prompt", request.prompt)?;
    insert_optional_scene_string(&mut object, "provider_id", request.provider_id)?;
    insert_optional_scene_string(&mut object, "model", request.model)?;
    object.insert(
        "candidate_count".to_owned(),
        serde_json::json!(request.candidate_count),
    );
    if let Some(timeout_ms) = request.timeout_ms {
        object.insert("timeout_ms".to_owned(), serde_json::json!(timeout_ms));
    }
    object.insert(
        "context_lines".to_owned(),
        serde_json::json!(request.context_lines),
    );
    Ok(object)
}

fn insert_optional_scene_string(
    object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<&str>,
) -> anyhow::Result<()> {
    if let Some(value) = value {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            anyhow::bail!("scene field `{key}` cannot be empty");
        }
        object.insert(
            key.to_owned(),
            serde_json::Value::String(trimmed.to_owned()),
        );
    }
    Ok(())
}

fn apply_scene_edit(
    scene_object: &mut serde_json::Map<String, serde_json::Value>,
    request: &SceneEditRequest<'_>,
) -> anyhow::Result<Vec<String>> {
    let mut changed = Vec::new();
    if let Some(label) = request.label {
        scene_object.insert(
            "label".to_owned(),
            serde_json::Value::String(normalize_scene_label(label)?),
        );
        changed.push("label".to_owned());
    }
    apply_optional_scene_string_edit(
        scene_object,
        "prompt",
        "prompt",
        request.prompt,
        request.clear_prompt,
        &mut changed,
    )?;
    apply_optional_scene_string_edit(
        scene_object,
        "provider_id",
        "provider-id",
        request.provider_id,
        request.clear_provider_id,
        &mut changed,
    )?;
    apply_optional_scene_string_edit(
        scene_object,
        "model",
        "model",
        request.model,
        request.clear_model,
        &mut changed,
    )?;
    if let Some(candidate_count) = request.candidate_count {
        scene_object.insert(
            "candidate_count".to_owned(),
            serde_json::json!(candidate_count),
        );
        changed.push("candidate_count".to_owned());
    }
    if request.timeout_ms.is_some() && request.clear_timeout {
        anyhow::bail!("scene edit cannot combine --timeout-ms and --clear-timeout");
    }
    if let Some(timeout_ms) = request.timeout_ms {
        scene_object.insert("timeout_ms".to_owned(), serde_json::json!(timeout_ms));
        changed.push("timeout_ms".to_owned());
    } else if request.clear_timeout {
        scene_object.remove("timeout_ms");
        changed.push("timeout_ms".to_owned());
    }
    if let Some(context_lines) = request.context_lines {
        scene_object.insert("context_lines".to_owned(), serde_json::json!(context_lines));
        changed.push("context_lines".to_owned());
    }
    Ok(changed)
}

fn apply_optional_scene_string_edit(
    scene_object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    option_name: &str,
    value: Option<&str>,
    clear: bool,
    changed: &mut Vec<String>,
) -> anyhow::Result<()> {
    if value.is_some() && clear {
        anyhow::bail!("scene edit cannot combine --{option_name} and --clear-{option_name}");
    }
    if let Some(value) = value {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            anyhow::bail!("scene field `{key}` cannot be empty");
        }
        scene_object.insert(
            key.to_owned(),
            serde_json::Value::String(trimmed.to_owned()),
        );
        changed.push(key.to_owned());
    } else if clear {
        scene_object.remove(key);
        changed.push(key.to_owned());
    }
    Ok(())
}

fn explicit_scene_index(document: &serde_json::Value, id: &str) -> anyhow::Result<usize> {
    document
        .pointer("/scenes/definitions")
        .and_then(serde_json::Value::as_array)
        .with_context(|| "config pointer `/scenes/definitions` not found or not an array")?
        .iter()
        .position(|scene| scene.get("id").and_then(serde_json::Value::as_str) == Some(id))
        .with_context(|| format!("scene `{id}` is not explicitly configured"))
}

fn scene_definitions_array_mut(
    document: &mut serde_json::Value,
) -> anyhow::Result<&mut Vec<serde_json::Value>> {
    document
        .pointer_mut("/scenes/definitions")
        .and_then(serde_json::Value::as_array_mut)
        .with_context(|| "config pointer `/scenes/definitions` not found or not an array")
}

fn normalize_scene_label(label: &str) -> anyhow::Result<String> {
    let label = label.trim();
    if label.is_empty() {
        anyhow::bail!("scene label cannot be empty");
    }
    Ok(label.to_owned())
}

fn scene_add_outcome_json(outcome: &SceneAddOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "scene_id": outcome.scene_id,
        "active_scene": outcome.active_scene,
        "before_scene_count": outcome.before_scene_count,
        "after_scene_count": outcome.after_scene_count,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinput scene list to verify configured scenes",
            "run vinput scene use <id> --dry-run --json to preview scene selection",
            "run vinput doctor to inspect full local diagnostics"
        ],
    })
}

fn scene_edit_outcome_json(outcome: &SceneEditOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "scene_id": outcome.scene_id,
        "active_scene": outcome.active_scene,
        "changed_fields": outcome.changed_fields,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinput scene list to verify configured scenes",
            "run vinput recording start --dry-run --json to inspect recording D-Bus calls",
            "run vinput doctor to inspect full local diagnostics"
        ],
    })
}

fn scene_remove_outcome_json(outcome: &SceneRemoveOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "removed_scene_id": outcome.removed_scene_id,
        "active_scene": outcome.active_scene,
        "before_scene_count": outcome.before_scene_count,
        "after_scene_count": outcome.after_scene_count,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinput scene list to verify configured scenes",
            "run vinput recording start --dry-run --json to inspect recording D-Bus calls",
            "run vinput doctor to inspect full local diagnostics"
        ],
    })
}

fn print_scene_add_text(outcome: &SceneAddOutcome) {
    println!("dry_run: {}", outcome.dry_run);
    println!("source: {}", outcome.source);
    if let Some(config_path) = &outcome.config_path {
        println!("config_path: {}", config_path.display());
    }
    println!("scene_id: {}", outcome.scene_id);
    println!("active_scene: {}", outcome.active_scene);
    println!("before_scene_count: {}", outcome.before_scene_count);
    println!("after_scene_count: {}", outcome.after_scene_count);
    println!("in_place: {}", outcome.in_place);
    if let Some(output_path) = &outcome.output_path {
        println!("output_path: {}", output_path.display());
    }
    if let Some(backup_path) = &outcome.backup_path {
        println!("backup_path: {}", backup_path.display());
    }
    println!("will_write_config: {}", !outcome.dry_run);
    println!("wrote_config: {}", outcome.wrote_config);
}

fn print_scene_edit_text(outcome: &SceneEditOutcome) {
    println!("dry_run: {}", outcome.dry_run);
    println!("source: {}", outcome.source);
    if let Some(config_path) = &outcome.config_path {
        println!("config_path: {}", config_path.display());
    }
    println!("scene_id: {}", outcome.scene_id);
    println!("active_scene: {}", outcome.active_scene);
    println!("changed_fields: {}", outcome.changed_fields.join(","));
    println!("in_place: {}", outcome.in_place);
    if let Some(output_path) = &outcome.output_path {
        println!("output_path: {}", output_path.display());
    }
    if let Some(backup_path) = &outcome.backup_path {
        println!("backup_path: {}", backup_path.display());
    }
    println!("will_write_config: {}", !outcome.dry_run);
    println!("wrote_config: {}", outcome.wrote_config);
}

fn print_scene_remove_text(outcome: &SceneRemoveOutcome) {
    println!("dry_run: {}", outcome.dry_run);
    println!("source: {}", outcome.source);
    if let Some(config_path) = &outcome.config_path {
        println!("config_path: {}", config_path.display());
    }
    println!("removed_scene_id: {}", outcome.removed_scene_id);
    println!("active_scene: {}", outcome.active_scene);
    println!("before_scene_count: {}", outcome.before_scene_count);
    println!("after_scene_count: {}", outcome.after_scene_count);
    println!("in_place: {}", outcome.in_place);
    if let Some(output_path) = &outcome.output_path {
        println!("output_path: {}", output_path.display());
    }
    if let Some(backup_path) = &outcome.backup_path {
        println!("backup_path: {}", backup_path.display());
    }
    println!("will_write_config: {}", !outcome.dry_run);
    println!("wrote_config: {}", outcome.wrote_config);
}

fn print_scene_use(request: SceneUseRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_scene_use(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&scene_use_outcome_json(&outcome))?
        );
    } else {
        print_scene_use_text(&outcome);
    }
    Ok(())
}

fn run_scene_use(request: &SceneUseRequest<'_>) -> anyhow::Result<SceneUseOutcome> {
    let id = normalize_scene_id(request.id)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for scene use")?;
    let config = VinputConfig::from_json_str(&contents).context("parse config for scene use")?;
    config.validate().context("validate config for scene use")?;
    if !config.scenes.definitions.iter().any(|scene| scene.id == id) {
        anyhow::bail!("scene `{id}` not found");
    }
    let before = config.scenes.active_scene;
    *loaded
        .document
        .pointer_mut("/scenes/active_scene")
        .with_context(|| "config pointer `/scenes/active_scene` not found")? =
        serde_json::Value::String(id.clone());
    validate_config_json_value(&loaded.document, "validate updated scene config")?;

    let write_target = config_set_write_target(
        request.output_path,
        request.in_place,
        request.dry_run,
        loaded.path.as_ref(),
        &default_path,
    )?;

    let mut wrote_config = false;
    if !request.dry_run {
        write_config_set_document(&loaded.document, &write_target)?;
        wrote_config = true;
    }

    Ok(SceneUseOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        before,
        after: id,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn scene_use_outcome_json(outcome: &SceneUseOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "before": outcome.before,
        "after": outcome.after,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinput scene list to verify the active scene",
            "run vinput recording start --dry-run --json to inspect recording D-Bus calls",
            "run vinput doctor to inspect full local diagnostics"
        ],
    })
}

fn print_scene_use_text(outcome: &SceneUseOutcome) {
    println!("dry_run: {}", outcome.dry_run);
    println!("source: {}", outcome.source);
    if let Some(config_path) = &outcome.config_path {
        println!("config_path: {}", config_path.display());
    }
    println!("before: {}", outcome.before);
    println!("after: {}", outcome.after);
    println!("in_place: {}", outcome.in_place);
    if let Some(output_path) = &outcome.output_path {
        println!("output_path: {}", output_path.display());
    }
    if let Some(backup_path) = &outcome.backup_path {
        println!("backup_path: {}", backup_path.display());
    }
    println!("will_write_config: {}", !outcome.dry_run);
    println!("wrote_config: {}", outcome.wrote_config);
}

fn normalize_scene_id(id: &str) -> anyhow::Result<String> {
    let id = id.trim();
    if id.is_empty() {
        anyhow::bail!("scene id cannot be empty");
    }
    Ok(id.to_owned())
}

fn hotword_supported(kind: &AsrProviderKind) -> bool {
    matches!(kind, AsrProviderKind::Local | AsrProviderKind::Command)
}

#[allow(clippy::too_many_lines)]
fn handle_provider_command(command: ProviderCommand) -> anyhow::Result<()> {
    match command {
        ProviderCommand::List { config, json } => print_provider_list(config.as_ref(), json),
        ProviderCommand::Use {
            id,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_provider_use(ProviderUseRequest {
            id: &id,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        ProviderCommand::Add {
            id,
            kind,
            model,
            hotwords_file,
            command,
            args,
            env,
            endpoint,
            timeout_ms,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_provider_add(ProviderAddRequest {
            id: &id,
            kind: &kind,
            model: model.as_deref(),
            hotwords_file: hotwords_file.as_deref(),
            command: command.as_deref(),
            args: &args,
            env: &env,
            endpoint: endpoint.as_deref(),
            timeout_ms,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        ProviderCommand::Edit {
            id,
            kind,
            model,
            clear_model,
            hotwords_file,
            clear_hotwords_file,
            command,
            clear_command,
            args,
            clear_args,
            env,
            clear_env,
            endpoint,
            clear_endpoint,
            timeout_ms,
            clear_timeout,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_provider_edit(ProviderEditRequest {
            id: &id,
            kind: kind.as_deref(),
            model: model.as_deref(),
            clear_model,
            hotwords_file: hotwords_file.as_deref(),
            clear_hotwords_file,
            command: command.as_deref(),
            clear_command,
            args: &args,
            clear_args,
            env: &env,
            clear_env,
            endpoint: endpoint.as_deref(),
            clear_endpoint,
            timeout_ms,
            clear_timeout,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        ProviderCommand::Remove {
            id,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_provider_remove(ProviderRemoveRequest {
            id: &id,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
    }
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct ProviderAddRequest<'a> {
    id: &'a str,
    kind: &'a str,
    model: Option<&'a str>,
    hotwords_file: Option<&'a str>,
    command: Option<&'a str>,
    args: &'a [String],
    env: &'a [String],
    endpoint: Option<&'a str>,
    timeout_ms: Option<u64>,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct ProviderAddOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    provider_id: String,
    provider_type: &'static str,
    active_provider: String,
    before_provider_count: usize,
    after_provider_count: usize,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct ProviderEditRequest<'a> {
    id: &'a str,
    kind: Option<&'a str>,
    model: Option<&'a str>,
    clear_model: bool,
    hotwords_file: Option<&'a str>,
    clear_hotwords_file: bool,
    command: Option<&'a str>,
    clear_command: bool,
    args: &'a [String],
    clear_args: bool,
    env: &'a [String],
    clear_env: bool,
    endpoint: Option<&'a str>,
    clear_endpoint: bool,
    timeout_ms: Option<u64>,
    clear_timeout: bool,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct ProviderEditOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    provider_id: String,
    before_provider_type: &'static str,
    after_provider_type: &'static str,
    active_provider: String,
    changed_fields: Vec<String>,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct ProviderRemoveRequest<'a> {
    id: &'a str,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct ProviderRemoveOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    removed_provider_id: String,
    removed_provider_type: &'static str,
    active_provider: String,
    before_provider_count: usize,
    after_provider_count: usize,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct ProviderUseRequest<'a> {
    id: &'a str,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct ProviderUseOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    before: String,
    after: String,
    provider_type: &'static str,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

struct ProviderListContext {
    config_path: Option<PathBuf>,
    source: &'static str,
    config: VinputConfig,
}

fn print_provider_list(config_path: Option<&PathBuf>, json_output: bool) -> anyhow::Result<()> {
    let context = load_provider_list_context(config_path)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&provider_list_json(&context))?
        );
    } else {
        print_provider_list_text(&context);
    }
    Ok(())
}

fn load_provider_list_context(
    config_path: Option<&PathBuf>,
) -> anyhow::Result<ProviderListContext> {
    let loaded = load_config_json(config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for provider list")?;
    let config =
        VinputConfig::from_json_str(&contents).context("parse config for provider list")?;
    config
        .validate()
        .context("validate config for provider list")?;
    Ok(ProviderListContext {
        config_path: loaded.path,
        source: loaded.source,
        config,
    })
}

fn provider_list_json(context: &ProviderListContext) -> serde_json::Value {
    let active_provider = context.config.asr.active_provider.as_str();
    let providers = context
        .config
        .asr
        .providers
        .iter()
        .map(|provider| provider_summary_json(provider, active_provider))
        .collect::<Vec<_>>();
    serde_json::json!({
        "ok": true,
        "config_path": context.config_path.as_ref(),
        "source": context.source,
        "active_provider": active_provider,
        "provider_count": providers.len(),
        "providers": providers,
        "next_steps": [
            "run vinput provider use <id> once provider mutation support is available",
            "run vinput asr-state to inspect the selected provider runtime readiness",
            "run vinput doctor to inspect full local diagnostics"
        ],
    })
}

fn provider_summary_json(provider: &AsrProviderConfig, active_provider: &str) -> serde_json::Value {
    serde_json::json!({
        "id": provider.id.as_str(),
        "type": asr_provider_kind_label(&provider.kind),
        "active": provider.id.as_str() == active_provider,
        "model": provider.model.as_deref(),
        "hotwords_file_configured": provider.hotwords_file.as_ref().is_some_and(|value| !value.trim().is_empty()),
        "command_configured": provider.command.as_ref().is_some_and(|value| !value.trim().is_empty()),
        "args_count": provider.args.len(),
        "env_count": provider.env.len(),
        "endpoint_configured": provider.endpoint.as_ref().is_some_and(|value| !value.trim().is_empty()),
        "timeout_ms": provider.timeout_ms,
    })
}

fn print_provider_list_text(context: &ProviderListContext) {
    println!("source: {}", context.source);
    if let Some(path) = &context.config_path {
        println!("config_path: {}", path.display());
    }
    println!("active_provider: {}", context.config.asr.active_provider);
    println!("provider_count: {}", context.config.asr.providers.len());
    println!("active\tid\ttype\tmodel\thotwords\tcommand\tendpoint\ttimeout_ms");
    for provider in &context.config.asr.providers {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            if provider.id == context.config.asr.active_provider {
                "*"
            } else {
                ""
            },
            provider.id,
            asr_provider_kind_label(&provider.kind),
            provider.model.as_deref().unwrap_or("-"),
            configured_label(provider.hotwords_file.as_deref()),
            configured_label(provider.command.as_deref()),
            configured_label(provider.endpoint.as_deref()),
            provider
                .timeout_ms
                .map_or_else(|| "-".to_owned(), |value| value.to_string())
        );
    }
}

fn print_provider_edit(request: ProviderEditRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_provider_edit(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&provider_edit_outcome_json(&outcome))?
        );
    } else {
        print_provider_edit_text(&outcome);
    }
    Ok(())
}

fn run_provider_edit(request: &ProviderEditRequest<'_>) -> anyhow::Result<ProviderEditOutcome> {
    let id = normalize_provider_id(request.id)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for provider edit")?;
    let config =
        VinputConfig::from_json_str(&contents).context("parse config for provider edit")?;
    let provider_index = config
        .asr
        .providers
        .iter()
        .position(|provider| provider.id == id)
        .with_context(|| format!("ASR provider `{id}` not found"))?;
    let before_provider = &config.asr.providers[provider_index];
    let before_provider_type = asr_provider_kind_label(&before_provider.kind);

    let providers = loaded
        .document
        .pointer_mut("/asr/providers")
        .and_then(serde_json::Value::as_array_mut)
        .with_context(|| "config pointer `/asr/providers` not found or not an array")?;
    let provider_object = providers
        .get_mut(provider_index)
        .and_then(serde_json::Value::as_object_mut)
        .with_context(|| format!("ASR provider `{id}` is not a JSON object"))?;
    let changed_fields = apply_provider_edit(provider_object, request)?;
    if changed_fields.is_empty() {
        anyhow::bail!("provider edit requires at least one field change");
    }

    validate_config_json_value(&loaded.document, "validate updated provider config")?;
    let updated_contents =
        serde_json::to_string(&loaded.document).context("serialize updated provider config")?;
    let updated_config =
        VinputConfig::from_json_str(&updated_contents).context("parse updated provider config")?;
    let after_provider = &updated_config.asr.providers[provider_index];
    let after_provider_type = asr_provider_kind_label(&after_provider.kind);

    let write_target = config_set_write_target(
        request.output_path,
        request.in_place,
        request.dry_run,
        loaded.path.as_ref(),
        &default_path,
    )?;

    let mut wrote_config = false;
    if !request.dry_run {
        write_config_set_document(&loaded.document, &write_target)?;
        wrote_config = true;
    }

    Ok(ProviderEditOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        provider_id: id,
        before_provider_type,
        after_provider_type,
        active_provider: config.asr.active_provider,
        changed_fields,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn provider_edit_outcome_json(outcome: &ProviderEditOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "provider_id": outcome.provider_id,
        "before_provider_type": outcome.before_provider_type,
        "after_provider_type": outcome.after_provider_type,
        "active_provider": outcome.active_provider,
        "changed_fields": outcome.changed_fields,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinput provider list to verify configured ASR providers",
            "run vinput asr-state to inspect provider runtime readiness",
            "run vinput doctor to inspect full local diagnostics"
        ],
    })
}

fn print_provider_edit_text(outcome: &ProviderEditOutcome) {
    println!("dry_run: {}", outcome.dry_run);
    println!("source: {}", outcome.source);
    if let Some(config_path) = &outcome.config_path {
        println!("config_path: {}", config_path.display());
    }
    println!("provider_id: {}", outcome.provider_id);
    println!("before_provider_type: {}", outcome.before_provider_type);
    println!("after_provider_type: {}", outcome.after_provider_type);
    println!("active_provider: {}", outcome.active_provider);
    println!("changed_fields: {}", outcome.changed_fields.join(","));
    println!("in_place: {}", outcome.in_place);
    if let Some(output_path) = &outcome.output_path {
        println!("output_path: {}", output_path.display());
    }
    if let Some(backup_path) = &outcome.backup_path {
        println!("backup_path: {}", backup_path.display());
    }
    println!("will_write_config: {}", !outcome.dry_run);
    println!("wrote_config: {}", outcome.wrote_config);
}

fn print_provider_add(request: ProviderAddRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_provider_add(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&provider_add_outcome_json(&outcome))?
        );
    } else {
        print_provider_add_text(&outcome);
    }
    Ok(())
}

fn run_provider_add(request: &ProviderAddRequest<'_>) -> anyhow::Result<ProviderAddOutcome> {
    let id = normalize_provider_id(request.id)?;
    let provider_type = normalize_provider_kind(request.kind)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for provider add")?;
    let config = VinputConfig::from_json_str(&contents).context("parse config for provider add")?;
    if config
        .asr
        .providers
        .iter()
        .any(|provider| provider.id == id)
    {
        anyhow::bail!("ASR provider `{id}` already exists");
    }
    let before_provider_count = config.asr.providers.len();
    let provider = provider_add_json_object(&id, provider_type, request)?;

    let providers = loaded
        .document
        .pointer_mut("/asr/providers")
        .and_then(serde_json::Value::as_array_mut)
        .with_context(|| "config pointer `/asr/providers` not found or not an array")?;
    providers.push(serde_json::Value::Object(provider));
    validate_config_json_value(&loaded.document, "validate updated provider config")?;

    let write_target = config_set_write_target(
        request.output_path,
        request.in_place,
        request.dry_run,
        loaded.path.as_ref(),
        &default_path,
    )?;

    let mut wrote_config = false;
    if !request.dry_run {
        write_config_set_document(&loaded.document, &write_target)?;
        wrote_config = true;
    }

    Ok(ProviderAddOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        provider_id: id,
        provider_type,
        active_provider: config.asr.active_provider,
        before_provider_count,
        after_provider_count: before_provider_count + 1,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn apply_provider_edit(
    provider_object: &mut serde_json::Map<String, serde_json::Value>,
    request: &ProviderEditRequest<'_>,
) -> anyhow::Result<Vec<String>> {
    let mut changed = Vec::new();
    if let Some(kind) = request.kind {
        provider_object.insert(
            "type".to_owned(),
            serde_json::Value::String(normalize_provider_kind(kind)?.to_owned()),
        );
        changed.push("type".to_owned());
    }
    apply_optional_provider_string_edit(
        provider_object,
        "model",
        "model",
        request.model,
        request.clear_model,
        &mut changed,
    )?;
    apply_optional_provider_string_edit(
        provider_object,
        "hotwords_file",
        "hotwords-file",
        request.hotwords_file,
        request.clear_hotwords_file,
        &mut changed,
    )?;
    apply_optional_provider_string_edit(
        provider_object,
        "command",
        "command",
        request.command,
        request.clear_command,
        &mut changed,
    )?;
    if !request.args.is_empty() && request.clear_args {
        anyhow::bail!("provider edit cannot combine --arg and --clear-args");
    }
    if !request.args.is_empty() {
        provider_object.insert("args".to_owned(), serde_json::json!(request.args));
        changed.push("args".to_owned());
    } else if request.clear_args {
        provider_object.remove("args");
        changed.push("args".to_owned());
    }
    if !request.env.is_empty() && request.clear_env {
        anyhow::bail!("provider edit cannot combine --env and --clear-env");
    }
    if !request.env.is_empty() {
        provider_object.insert(
            "env".to_owned(),
            serde_json::json!(parse_provider_env(request.env)?),
        );
        changed.push("env".to_owned());
    } else if request.clear_env {
        provider_object.remove("env");
        changed.push("env".to_owned());
    }
    apply_optional_provider_string_edit(
        provider_object,
        "endpoint",
        "endpoint",
        request.endpoint,
        request.clear_endpoint,
        &mut changed,
    )?;
    if request.timeout_ms.is_some() && request.clear_timeout {
        anyhow::bail!("provider edit cannot combine --timeout-ms and --clear-timeout");
    }
    if let Some(timeout_ms) = request.timeout_ms {
        provider_object.insert("timeout_ms".to_owned(), serde_json::json!(timeout_ms));
        changed.push("timeout_ms".to_owned());
    } else if request.clear_timeout {
        provider_object.remove("timeout_ms");
        changed.push("timeout_ms".to_owned());
    }
    Ok(changed)
}

fn apply_optional_provider_string_edit(
    provider_object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    option_name: &str,
    value: Option<&str>,
    clear: bool,
    changed: &mut Vec<String>,
) -> anyhow::Result<()> {
    if value.is_some() && clear {
        anyhow::bail!("provider edit cannot combine --{option_name} and --clear-{option_name}");
    }
    if let Some(value) = value {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            anyhow::bail!("provider field `{key}` cannot be empty");
        }
        provider_object.insert(
            key.to_owned(),
            serde_json::Value::String(trimmed.to_owned()),
        );
        changed.push(key.to_owned());
    } else if clear {
        provider_object.remove(key);
        changed.push(key.to_owned());
    }
    Ok(())
}

fn provider_add_json_object(
    id: &str,
    provider_type: &'static str,
    request: &ProviderAddRequest<'_>,
) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
    let mut object = serde_json::Map::new();
    object.insert("id".to_owned(), serde_json::Value::String(id.to_owned()));
    object.insert(
        "type".to_owned(),
        serde_json::Value::String(provider_type.to_owned()),
    );
    if let Some(timeout_ms) = request.timeout_ms {
        object.insert("timeout_ms".to_owned(), serde_json::json!(timeout_ms));
    }
    insert_optional_string(&mut object, "model", request.model)?;
    insert_optional_string(&mut object, "hotwords_file", request.hotwords_file)?;
    insert_optional_string(&mut object, "command", request.command)?;
    if !request.args.is_empty() {
        object.insert("args".to_owned(), serde_json::json!(request.args));
    }
    let env = parse_provider_env(request.env)?;
    if !env.is_empty() {
        object.insert("env".to_owned(), serde_json::json!(env));
    }
    insert_optional_string(&mut object, "endpoint", request.endpoint)?;
    Ok(object)
}

fn insert_optional_string(
    object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<&str>,
) -> anyhow::Result<()> {
    if let Some(value) = value {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            anyhow::bail!("provider field `{key}` cannot be empty");
        }
        object.insert(
            key.to_owned(),
            serde_json::Value::String(trimmed.to_owned()),
        );
    }
    Ok(())
}

fn parse_provider_env(entries: &[String]) -> anyhow::Result<BTreeMap<String, String>> {
    let mut env = BTreeMap::new();
    for entry in entries {
        let (key, value) = entry
            .split_once('=')
            .with_context(|| format!("provider env `{entry}` is not KEY=VALUE"))?;
        let key = key.trim();
        if key.is_empty() {
            anyhow::bail!("provider env `{entry}` has an empty key");
        }
        env.insert(key.to_owned(), value.to_owned());
    }
    Ok(env)
}

fn normalize_provider_kind(kind: &str) -> anyhow::Result<&'static str> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "local" => Ok("local"),
        "command" => Ok("command"),
        "remote" => Ok("remote"),
        other => {
            anyhow::bail!("unsupported ASR provider type `{other}`; use local, command, or remote")
        }
    }
}

fn provider_add_outcome_json(outcome: &ProviderAddOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "provider_id": outcome.provider_id,
        "provider_type": outcome.provider_type,
        "active_provider": outcome.active_provider,
        "before_provider_count": outcome.before_provider_count,
        "after_provider_count": outcome.after_provider_count,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinput provider list to verify configured ASR providers",
            "run vinput provider use <id> to activate the new provider",
            "run vinput asr-state to inspect provider runtime readiness"
        ],
    })
}

fn print_provider_add_text(outcome: &ProviderAddOutcome) {
    println!("dry_run: {}", outcome.dry_run);
    println!("source: {}", outcome.source);
    if let Some(config_path) = &outcome.config_path {
        println!("config_path: {}", config_path.display());
    }
    println!("provider_id: {}", outcome.provider_id);
    println!("provider_type: {}", outcome.provider_type);
    println!("active_provider: {}", outcome.active_provider);
    println!("before_provider_count: {}", outcome.before_provider_count);
    println!("after_provider_count: {}", outcome.after_provider_count);
    println!("in_place: {}", outcome.in_place);
    if let Some(output_path) = &outcome.output_path {
        println!("output_path: {}", output_path.display());
    }
    if let Some(backup_path) = &outcome.backup_path {
        println!("backup_path: {}", backup_path.display());
    }
    println!("will_write_config: {}", !outcome.dry_run);
    println!("wrote_config: {}", outcome.wrote_config);
}

fn print_provider_remove(request: ProviderRemoveRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_provider_remove(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&provider_remove_outcome_json(&outcome))?
        );
    } else {
        print_provider_remove_text(&outcome);
    }
    Ok(())
}

fn run_provider_remove(
    request: &ProviderRemoveRequest<'_>,
) -> anyhow::Result<ProviderRemoveOutcome> {
    let id = normalize_provider_id(request.id)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for provider remove")?;
    let config =
        VinputConfig::from_json_str(&contents).context("parse config for provider remove")?;
    let provider_index = config
        .asr
        .providers
        .iter()
        .position(|provider| provider.id == id)
        .with_context(|| format!("ASR provider `{id}` not found"))?;
    let provider = &config.asr.providers[provider_index];
    if provider.id == config.asr.active_provider {
        anyhow::bail!(
            "refusing to remove active ASR provider `{}`; run vinput provider use <id> first",
            provider.id
        );
    }
    let removed_provider_type = asr_provider_kind_label(&provider.kind);
    let before_provider_count = config.asr.providers.len();

    let providers = loaded
        .document
        .pointer_mut("/asr/providers")
        .and_then(serde_json::Value::as_array_mut)
        .with_context(|| "config pointer `/asr/providers` not found or not an array")?;
    providers.remove(provider_index);
    validate_config_json_value(&loaded.document, "validate updated provider config")?;

    let write_target = config_set_write_target(
        request.output_path,
        request.in_place,
        request.dry_run,
        loaded.path.as_ref(),
        &default_path,
    )?;

    let mut wrote_config = false;
    if !request.dry_run {
        write_config_set_document(&loaded.document, &write_target)?;
        wrote_config = true;
    }

    Ok(ProviderRemoveOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        removed_provider_id: id,
        removed_provider_type,
        active_provider: config.asr.active_provider,
        before_provider_count,
        after_provider_count: before_provider_count - 1,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn provider_remove_outcome_json(outcome: &ProviderRemoveOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "removed_provider_id": outcome.removed_provider_id,
        "removed_provider_type": outcome.removed_provider_type,
        "active_provider": outcome.active_provider,
        "before_provider_count": outcome.before_provider_count,
        "after_provider_count": outcome.after_provider_count,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinput provider list to verify configured ASR providers",
            "run vinput asr-state to inspect the active provider runtime readiness",
            "run vinput doctor to inspect full local diagnostics"
        ],
    })
}

fn print_provider_remove_text(outcome: &ProviderRemoveOutcome) {
    println!("dry_run: {}", outcome.dry_run);
    println!("source: {}", outcome.source);
    if let Some(config_path) = &outcome.config_path {
        println!("config_path: {}", config_path.display());
    }
    println!("removed_provider_id: {}", outcome.removed_provider_id);
    println!("removed_provider_type: {}", outcome.removed_provider_type);
    println!("active_provider: {}", outcome.active_provider);
    println!("before_provider_count: {}", outcome.before_provider_count);
    println!("after_provider_count: {}", outcome.after_provider_count);
    println!("in_place: {}", outcome.in_place);
    if let Some(output_path) = &outcome.output_path {
        println!("output_path: {}", output_path.display());
    }
    if let Some(backup_path) = &outcome.backup_path {
        println!("backup_path: {}", backup_path.display());
    }
    println!("will_write_config: {}", !outcome.dry_run);
    println!("wrote_config: {}", outcome.wrote_config);
}

fn print_provider_use(request: ProviderUseRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_provider_use(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&provider_use_outcome_json(&outcome))?
        );
    } else {
        print_provider_use_text(&outcome);
    }
    Ok(())
}

fn run_provider_use(request: &ProviderUseRequest<'_>) -> anyhow::Result<ProviderUseOutcome> {
    let after = normalize_provider_id(request.id)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for provider use")?;
    let config = VinputConfig::from_json_str(&contents).context("parse config for provider use")?;
    let provider = config
        .asr
        .providers
        .iter()
        .find(|provider| provider.id == after)
        .with_context(|| format!("ASR provider `{after}` not found"))?;
    let provider_type = asr_provider_kind_label(&provider.kind);
    let before = config.asr.active_provider;
    *loaded
        .document
        .pointer_mut("/asr/active_provider")
        .with_context(|| "config pointer `/asr/active_provider` not found")? =
        serde_json::Value::String(after.clone());
    validate_config_json_value(&loaded.document, "validate updated provider config")?;

    let write_target = config_set_write_target(
        request.output_path,
        request.in_place,
        request.dry_run,
        loaded.path.as_ref(),
        &default_path,
    )?;

    let mut wrote_config = false;
    if !request.dry_run {
        write_config_set_document(&loaded.document, &write_target)?;
        wrote_config = true;
    }

    Ok(ProviderUseOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        before,
        after,
        provider_type,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn normalize_provider_id(input: &str) -> anyhow::Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        anyhow::bail!("ASR provider id cannot be empty");
    }
    Ok(trimmed.to_owned())
}

fn provider_use_outcome_json(outcome: &ProviderUseOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "before": outcome.before,
        "after": outcome.after,
        "provider_type": outcome.provider_type,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinput provider list to verify the active provider",
            "run vinput asr-state to inspect the selected provider runtime readiness",
            "run vinput doctor to inspect full local diagnostics"
        ],
    })
}

fn print_provider_use_text(outcome: &ProviderUseOutcome) {
    println!("dry_run: {}", outcome.dry_run);
    println!("source: {}", outcome.source);
    if let Some(config_path) = &outcome.config_path {
        println!("config_path: {}", config_path.display());
    }
    println!("before: {}", outcome.before);
    println!("after: {}", outcome.after);
    println!("provider_type: {}", outcome.provider_type);
    println!("in_place: {}", outcome.in_place);
    if let Some(output_path) = &outcome.output_path {
        println!("output_path: {}", output_path.display());
    }
    if let Some(backup_path) = &outcome.backup_path {
        println!("backup_path: {}", backup_path.display());
    }
    println!("will_write_config: {}", !outcome.dry_run);
    println!("wrote_config: {}", outcome.wrote_config);
}

fn asr_provider_kind_label(kind: &AsrProviderKind) -> &'static str {
    match kind {
        AsrProviderKind::Local => "local",
        AsrProviderKind::Remote => "remote",
        AsrProviderKind::Command => "command",
    }
}

fn configured_label(value: Option<&str>) -> &'static str {
    if value.is_some_and(|value| !value.trim().is_empty()) {
        "yes"
    } else {
        "no"
    }
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct DeviceUseRequest<'a> {
    target: &'a str,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct DeviceListContext {
    config_path: Option<PathBuf>,
    source: &'static str,
    config: VinputConfig,
}

struct DeviceUseOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    before: String,
    after: String,
    capture_target: CaptureTarget,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

fn print_device_list(config_path: Option<&PathBuf>, json_output: bool) -> anyhow::Result<()> {
    let context = load_device_list_context(config_path)?;
    let audio = audio_devices_json(&context.config)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "config_path": context.config_path.as_ref(),
                "source": context.source,
                "audio": audio,
            }))?
        );
    } else {
        print_device_list_text(context.config_path.as_ref(), context.source, &audio);
    }
    Ok(())
}

fn load_device_list_context(config_path: Option<&PathBuf>) -> anyhow::Result<DeviceListContext> {
    let loaded = load_config_json(config_path)?;
    let document = loaded.document;
    let contents = serde_json::to_string(&document).context("serialize config for device list")?;
    let config = VinputConfig::from_json_str(&contents).context("parse config for device list")?;
    config
        .validate()
        .context("validate config for device list")?;
    Ok(DeviceListContext {
        config_path: loaded.path,
        source: loaded.source,
        config,
    })
}

fn print_device_list_text(config_path: Option<&PathBuf>, source: &str, audio: &serde_json::Value) {
    println!("source: {source}");
    if let Some(path) = config_path {
        println!("config_path: {}", path.display());
    }
    println!(
        "capture_device: {}",
        audio["capture_device"].as_str().unwrap_or("")
    );
    println!(
        "backend: {}",
        audio["backend"].as_str().unwrap_or("unknown")
    );
    println!("live: {}", audio["live"].as_bool().unwrap_or(false));
    if let Some(error) = audio["enumeration_error"].as_str() {
        println!("enumeration_error: {error}");
    }
    println!("target\tid\tname\tdescription");
    println!("default\t-\tdefault\tDefault capture source");
    if let Some(devices) = audio["devices"].as_array() {
        for device in devices {
            let id = device["id"]
                .as_u64()
                .map_or_else(|| "-".to_owned(), |id| id.to_string());
            let name = device["name"].as_str().unwrap_or("");
            let description = device["description"].as_str().unwrap_or("");
            println!("{name}\t{id}\t{name}\t{description}");
        }
    }
}

fn print_device_use(request: DeviceUseRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_device_use(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&device_use_outcome_json(&outcome))?
        );
    } else {
        print_device_use_text(&outcome);
    }
    Ok(())
}

fn run_device_use(request: &DeviceUseRequest<'_>) -> anyhow::Result<DeviceUseOutcome> {
    let after = normalize_capture_device_value(request.target)?;
    let capture_target = CaptureTarget::from_config_value(&after)
        .with_context(|| format!("parse capture device `{}`", request.target))?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let before = loaded
        .document
        .pointer("/global/capture_device")
        .and_then(serde_json::Value::as_str)
        .with_context(|| "config pointer `/global/capture_device` not found or not a string")?
        .to_owned();
    *loaded
        .document
        .pointer_mut("/global/capture_device")
        .with_context(|| "config pointer `/global/capture_device` not found")? =
        serde_json::Value::String(after.clone());
    validate_config_json_value(&loaded.document, "validate updated device config")?;

    let write_target = config_set_write_target(
        request.output_path,
        request.in_place,
        request.dry_run,
        loaded.path.as_ref(),
        &default_path,
    )?;

    let mut wrote_config = false;
    if !request.dry_run {
        write_config_set_document(&loaded.document, &write_target)?;
        wrote_config = true;
    }

    Ok(DeviceUseOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        before,
        after,
        capture_target,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn normalize_capture_device_value(input: &str) -> anyhow::Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        anyhow::bail!("capture device cannot be empty");
    }
    Ok(trimmed.to_owned())
}

fn device_use_outcome_json(outcome: &DeviceUseOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path,
        "source": outcome.source,
        "before": outcome.before,
        "after": outcome.after,
        "capture_target": capture_target_json(&outcome.capture_target),
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinput device list to verify the configured capture target",
            "run vinput doctor to inspect audio and config diagnostics"
        ],
    })
}

fn print_device_use_text(outcome: &DeviceUseOutcome) {
    println!("dry_run: {}", outcome.dry_run);
    println!("source: {}", outcome.source);
    if let Some(config_path) = &outcome.config_path {
        println!("config_path: {}", config_path.display());
    }
    println!("before: {}", outcome.before);
    println!("after: {}", outcome.after);
    println!(
        "capture_target: {}",
        capture_target_label(&outcome.capture_target)
    );
    println!("in_place: {}", outcome.in_place);
    if let Some(output_path) = &outcome.output_path {
        println!("output_path: {}", output_path.display());
    }
    if let Some(backup_path) = &outcome.backup_path {
        println!("backup_path: {}", backup_path.display());
    }
    println!("will_write_config: {}", !outcome.dry_run);
    println!("wrote_config: {}", outcome.wrote_config);
}

fn capture_target_label(target: &CaptureTarget) -> String {
    match target {
        CaptureTarget::Default => "default".to_owned(),
        CaptureTarget::Object(value) => format!("object:{value}"),
    }
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
            installed,
            model_root,
            registry,
            i18n,
            config,
            locale,
            json,
        } => print_model_info(ModelInfoRequest {
            id_or_short_id: &id,
            installed,
            model_root: model_root.as_deref(),
            registry_path: registry.as_deref(),
            i18n_path: i18n.as_deref(),
            config_path: config.as_ref(),
            locale: &locale,
            json_output: json,
        }),
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
            installed,
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
            installed,
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
            installed,
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
            installed,
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

#[derive(Clone, Copy)]
struct InitRequest<'a> {
    config_path: Option<&'a Path>,
    model_root: Option<&'a Path>,
    cache_root: Option<&'a Path>,
    force: bool,
    dry_run: bool,
    json_output: bool,
}

#[allow(clippy::struct_excessive_bools)]
struct InitOutcome {
    dry_run: bool,
    force: bool,
    config_path: PathBuf,
    config_existed: bool,
    wrote_config: bool,
    model_root: PathBuf,
    model_root_existed: bool,
    created_model_root: bool,
    cache_root: PathBuf,
    cache_root_existed: bool,
    created_cache_root: bool,
    activation_service_path: Option<PathBuf>,
    activation_command_argv: Vec<String>,
}

fn handle_init(request: InitRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_init(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&init_outcome_json(&outcome))?
        );
    } else {
        print_init_outcome_text(&outcome);
    }
    Ok(())
}

fn run_init(request: &InitRequest<'_>) -> anyhow::Result<InitOutcome> {
    let config_path = match request.config_path {
        Some(path) => path.to_path_buf(),
        None => default_config_path()?,
    };
    let model_root = match request.model_root {
        Some(path) => path.to_path_buf(),
        None => default_model_root()?,
    };
    let cache_root = match request.cache_root {
        Some(path) => path.to_path_buf(),
        None => default_cache_root()?,
    };
    let activation_service_path = user_activation_service_path().ok();
    let activation_command_argv = init_activation_command_argv(&config_path);

    let config_existed = config_path.exists();
    let model_root_existed = model_root.exists();
    let cache_root_existed = cache_root.exists();
    let mut wrote_config = false;
    let mut created_model_root = false;
    let mut created_cache_root = false;

    let bundled_config = VinputConfig::bundled_default().context("parse bundled init config")?;
    bundled_config
        .validate()
        .context("validate bundled init config")?;

    if !request.dry_run {
        if !config_existed || request.force {
            if let Some(parent) = config_path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create config directory `{}`", parent.display()))?;
            }
            let contents = config_example_contents(ConfigExample::Default);
            write_file_atomically(&config_path, contents)
                .with_context(|| format!("write default config `{}`", config_path.display()))?;
            wrote_config = true;
        }
        if !model_root_existed {
            fs::create_dir_all(&model_root)
                .with_context(|| format!("create model root `{}`", model_root.display()))?;
            created_model_root = true;
        }
        if !cache_root_existed {
            fs::create_dir_all(&cache_root)
                .with_context(|| format!("create cache root `{}`", cache_root.display()))?;
            created_cache_root = true;
        }
    }

    Ok(InitOutcome {
        dry_run: request.dry_run,
        force: request.force,
        config_path,
        config_existed,
        wrote_config,
        model_root,
        model_root_existed,
        created_model_root,
        cache_root,
        cache_root_existed,
        created_cache_root,
        activation_service_path,
        activation_command_argv,
    })
}

fn init_outcome_json(outcome: &InitOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "force": outcome.force,
        "config": {
            "path": outcome.config_path,
            "existed": outcome.config_existed,
            "will_write": outcome.dry_run && (!outcome.config_existed || outcome.force),
            "wrote": outcome.wrote_config,
        },
        "directories": {
            "model_root": {
                "path": outcome.model_root,
                "existed": outcome.model_root_existed,
                "will_create": outcome.dry_run && !outcome.model_root_existed,
                "created": outcome.created_model_root,
            },
            "cache_root": {
                "path": outcome.cache_root,
                "existed": outcome.cache_root_existed,
                "will_create": outcome.dry_run && !outcome.cache_root_existed,
                "created": outcome.created_cache_root,
            },
        },
        "activation_service": {
            "user_service_path": outcome.activation_service_path,
            "command": outcome.activation_command_argv.join(" "),
            "command_argv": outcome.activation_command_argv,
        },
        "next_steps": [
            "install a model with vinput model install <id-or-short-id>",
            "select it with vinput model use <id-or-short-id> --config <path> --in-place",
            "install D-Bus activation with the suggested activation-service command"
        ],
    })
}

fn print_init_outcome_text(outcome: &InitOutcome) {
    println!("dry_run: {}", outcome.dry_run);
    println!("force: {}", outcome.force);
    println!("config_path: {}", outcome.config_path.display());
    println!("config_existed: {}", outcome.config_existed);
    println!(
        "config_will_write: {}",
        outcome.dry_run && (!outcome.config_existed || outcome.force)
    );
    println!("config_wrote: {}", outcome.wrote_config);
    println!("model_root: {}", outcome.model_root.display());
    println!("model_root_existed: {}", outcome.model_root_existed);
    println!(
        "model_root_will_create: {}",
        outcome.dry_run && !outcome.model_root_existed
    );
    println!("model_root_created: {}", outcome.created_model_root);
    println!("cache_root: {}", outcome.cache_root.display());
    println!("cache_root_existed: {}", outcome.cache_root_existed);
    println!(
        "cache_root_will_create: {}",
        outcome.dry_run && !outcome.cache_root_existed
    );
    println!("cache_root_created: {}", outcome.created_cache_root);
    if let Some(path) = &outcome.activation_service_path {
        println!("activation_service_path: {}", path.display());
    }
    println!(
        "activation_service_command: {}",
        outcome.activation_command_argv.join(" ")
    );
    println!("next: vinput model install <id-or-short-id>");
}

fn init_activation_command_argv(config_path: &Path) -> Vec<String> {
    vec![
        "vinput".to_owned(),
        "activation-service".to_owned(),
        "--daemon".to_owned(),
        default_daemon_path_hint().to_string_lossy().into_owned(),
        "--config".to_owned(),
        config_path.to_string_lossy().into_owned(),
        "--configured-backends".to_owned(),
        "--user".to_owned(),
    ]
}

fn default_daemon_path_hint() -> PathBuf {
    if let Ok(current_exe) = std::env::current_exe()
        && let Some(parent) = current_exe.parent()
    {
        let sibling = parent.join("vinput-daemon");
        if sibling.exists() {
            return sibling;
        }
    }
    PathBuf::from("vinput-daemon")
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
        "next_steps": doctor_next_steps(&config),
    });
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn doctor_next_steps(config: &VinputConfig) -> Vec<String> {
    vec![
        "run vinput provider list to inspect configured ASR providers".to_owned(),
        format!(
            "run vinput provider use {} --dry-run --json to preview provider selection",
            config.asr.active_provider
        ),
        "run vinput hotword get --json to inspect hotword configuration".to_owned(),
        "run vinput device list --json to inspect capture devices".to_owned(),
        "run vinput device use <target> --dry-run --json to preview capture-device selection"
            .to_owned(),
        "run vinput daemon status --dry-run --json to inspect daemon D-Bus calls".to_owned(),
    ]
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

fn default_config_path() -> anyhow::Result<PathBuf> {
    Ok(user_config_home()?.join("fcitx-vinput").join("config.json"))
}

fn user_config_home() -> anyhow::Result<PathBuf> {
    match std::env::var_os("XDG_CONFIG_HOME") {
        Some(value) if !value.is_empty() => Ok(PathBuf::from(value)),
        _ => Ok(user_home()?.join(".config")),
    }
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

fn default_cache_root() -> anyhow::Result<PathBuf> {
    Ok(user_cache_home()?.join("fcitx-vinput"))
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

#[derive(Clone, Copy)]
struct ModelInfoRequest<'a> {
    id_or_short_id: &'a str,
    installed: bool,
    model_root: Option<&'a Path>,
    registry_path: Option<&'a Path>,
    i18n_path: Option<&'a Path>,
    config_path: Option<&'a PathBuf>,
    locale: &'a str,
    json_output: bool,
}

fn print_model_info(request: ModelInfoRequest<'_>) -> anyhow::Result<()> {
    if request.installed || is_model_path_selector(request.id_or_short_id) {
        let model_dir = resolve_installed_model_info_selector(
            request.id_or_short_id,
            request.installed,
            request.model_root,
        )?;
        let info = load_installed_model_info(&model_dir)?;
        if request.json_output {
            println!(
                "{}",
                serde_json::to_string_pretty(&installed_model_info_json(&info)?)?
            );
        } else {
            print_installed_model_info_text(&info);
        }
        return Ok(());
    }

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
    let output = live_model_info_json(model, i18n.i18n.as_ref(), &loaded, &i18n)?;

    if request.json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_model_info_text(model, i18n.i18n.as_ref(), &loaded, &i18n);
    }
    Ok(())
}

fn resolve_installed_model_info_selector(
    selector: &str,
    installed: bool,
    model_root: Option<&Path>,
) -> anyhow::Result<PathBuf> {
    if is_model_path_selector(selector) {
        if installed {
            anyhow::bail!(
                "model info --installed expects a managed model directory name, not a path"
            );
        }
        return Ok(PathBuf::from(selector));
    }
    let model_root = match model_root {
        Some(path) => path.to_path_buf(),
        None => default_model_root()?,
    };
    Ok(model_root.join(safe_path_component(selector)))
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
#[allow(clippy::struct_excessive_bools)]
struct ModelRemoveRequest<'a> {
    selector: &'a str,
    installed: bool,
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
        request.installed,
        request.registry_path,
        request.i18n_path,
        request.config_path,
        request.locale,
        model_root,
    )?;
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
    installed: bool,
    registry_path: Option<&Path>,
    i18n_path: Option<&Path>,
    config_path: Option<&PathBuf>,
    locale: &str,
    model_root: &Path,
) -> anyhow::Result<ModelRemoveResolution> {
    let selector_path = Path::new(selector);
    if selector_path.is_absolute() || selector.contains('/') {
        if installed {
            anyhow::bail!(
                "model remove --installed expects a managed model directory name, not a path"
            );
        }
        return Ok(ModelRemoveResolution {
            target_path: selector_path.to_path_buf(),
            selector_kind: "path".to_owned(),
            resolved_model_id: None,
            resolved_short_id: None,
            resolved_title: None,
        });
    }

    if !installed
        && let Ok((loaded, i18n)) =
            load_live_model_catalog(registry_path, i18n_path, config_path, locale)
        && let Some(model) = loaded.registry.model_by_id_or_short_id(selector)
    {
        return Ok(ModelRemoveResolution {
            target_path: model_root.join(managed_model_dir_name(model)),
            selector_kind: "registry".to_owned(),
            resolved_model_id: Some(model.id.clone()),
            resolved_short_id: model.short_id.clone(),
            resolved_title: Some(model.resolved_title(i18n.i18n.as_ref())),
        });
    }

    Ok(ModelRemoveResolution {
        target_path: model_root.join(safe_path_component(selector)),
        selector_kind: "managed-dir".to_owned(),
        resolved_model_id: None,
        resolved_short_id: None,
        resolved_title: None,
    })
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
    installed: bool,
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
        request.installed,
        request.registry_path,
        request.i18n_path,
        request.config_path,
        request.locale,
        &model_root,
    )?;
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
    installed: bool,
    registry_path: Option<&Path>,
    i18n_path: Option<&Path>,
    config_path: Option<&PathBuf>,
    locale: &str,
    model_root: &Path,
) -> anyhow::Result<ModelUseResolution> {
    let selector_path = Path::new(selector);
    if selector_path.is_absolute() || selector.contains('/') {
        if installed {
            anyhow::bail!(
                "model use --installed expects a managed model directory name, not a path"
            );
        }
        return Ok(ModelUseResolution {
            model_value: selector_path.to_string_lossy().into_owned(),
            selector_kind: "path".to_owned(),
            resolved_model_id: None,
            resolved_short_id: None,
            resolved_title: None,
        });
    }

    if !installed
        && let Ok((loaded, i18n)) =
            load_live_model_catalog(registry_path, i18n_path, config_path, locale)
        && let Some(model) = loaded.registry.model_by_id_or_short_id(selector)
    {
        return Ok(ModelUseResolution {
            model_value: model_root
                .join(managed_model_dir_name(model))
                .to_string_lossy()
                .into_owned(),
            selector_kind: "registry".to_owned(),
            resolved_model_id: Some(model.id.clone()),
            resolved_short_id: model.short_id.clone(),
            resolved_title: Some(model.resolved_title(i18n.i18n.as_ref())),
        });
    }

    Ok(ModelUseResolution {
        model_value: model_root
            .join(safe_path_component(selector))
            .to_string_lossy()
            .into_owned(),
        selector_kind: "managed-dir".to_owned(),
        resolved_model_id: None,
        resolved_short_id: None,
        resolved_title: None,
    })
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

fn handle_config_get(
    pointer: &str,
    config_path: Option<&PathBuf>,
    exists_only: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    ensure_json_pointer(pointer)?;
    let loaded = load_config_json(config_path)?;
    let value = loaded.document.pointer(pointer);
    if exists_only {
        print_config_get_exists(&loaded, pointer, value, json_output)?;
        return Ok(());
    }
    let value = value.with_context(|| format!("config pointer `{pointer}` not found"))?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "config_path": loaded.path,
                "source": loaded.source,
                "pointer": pointer,
                "exists": true,
                "value": value,
            }))?
        );
    } else {
        print_config_value(value)?;
    }
    Ok(())
}

fn print_config_get_exists(
    loaded: &LoadedConfigJson,
    pointer: &str,
    value: Option<&serde_json::Value>,
    json_output: bool,
) -> anyhow::Result<()> {
    let exists = value.is_some();
    if json_output {
        let mut payload = serde_json::json!({
            "ok": true,
            "config_path": loaded.path.clone(),
            "source": loaded.source,
            "pointer": pointer,
            "exists": exists,
        });
        if let Some(value) = value {
            payload["value"] = value.clone();
        }
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("{exists}");
    }
    Ok(())
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct ConfigSetRequest<'a> {
    pointer: &'a str,
    raw_value: &'a str,
    force_string: bool,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct LoadedConfigJson {
    path: Option<PathBuf>,
    source: &'static str,
    document: serde_json::Value,
}

#[derive(Clone)]
enum ConfigSetWriteTarget {
    DryRun,
    Output(PathBuf),
    InPlace {
        config_path: PathBuf,
        backup_path: Option<PathBuf>,
    },
}

impl ConfigSetWriteTarget {
    fn output_path(&self) -> Option<PathBuf> {
        match self {
            Self::DryRun => None,
            Self::Output(path) => Some(path.clone()),
            Self::InPlace { config_path, .. } => Some(config_path.clone()),
        }
    }

    fn backup_path(&self) -> Option<PathBuf> {
        match self {
            Self::InPlace { backup_path, .. } => backup_path.clone(),
            Self::DryRun | Self::Output(_) => None,
        }
    }

    fn in_place(&self) -> bool {
        matches!(self, Self::InPlace { .. })
    }
}

#[allow(clippy::struct_excessive_bools)]
struct ConfigSetOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    pointer: String,
    raw_value: String,
    force_string: bool,
    parsed_value_kind: &'static str,
    before: serde_json::Value,
    after: serde_json::Value,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

fn handle_config_set(request: ConfigSetRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_config_set(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&config_set_outcome_json(&outcome))?
        );
    } else {
        print_config_set_outcome_text(&outcome)?;
    }
    Ok(())
}

fn run_config_set(request: &ConfigSetRequest<'_>) -> anyhow::Result<ConfigSetOutcome> {
    ensure_json_pointer(request.pointer)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let before = loaded
        .document
        .pointer(request.pointer)
        .with_context(|| format!("config pointer `{}` not found", request.pointer))?
        .clone();
    let (after, parsed_value_kind) =
        parse_config_set_value(request.raw_value, request.force_string);
    *loaded
        .document
        .pointer_mut(request.pointer)
        .with_context(|| format!("config pointer `{}` not found", request.pointer))? =
        after.clone();

    validate_config_json_value(&loaded.document, "validate updated config")?;

    let write_target = config_set_write_target(
        request.output_path,
        request.in_place,
        request.dry_run,
        loaded.path.as_ref(),
        &default_path,
    )?;

    let mut wrote_config = false;
    if !request.dry_run {
        write_config_set_document(&loaded.document, &write_target)?;
        wrote_config = true;
    }

    Ok(ConfigSetOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        pointer: request.pointer.to_owned(),
        raw_value: request.raw_value.to_owned(),
        force_string: request.force_string,
        parsed_value_kind,
        before,
        after,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn config_set_write_target(
    output_path: Option<&Path>,
    in_place: bool,
    dry_run: bool,
    input_path: Option<&PathBuf>,
    default_path: &Path,
) -> anyhow::Result<ConfigSetWriteTarget> {
    if output_path.is_some() && in_place {
        anyhow::bail!("config set cannot combine --output and --in-place");
    }
    if dry_run {
        return Ok(ConfigSetWriteTarget::DryRun);
    }
    if in_place {
        let target = input_path
            .cloned()
            .unwrap_or_else(|| default_path.to_path_buf());
        let backup_path = target.exists().then(|| config_backup_path(&target));
        return Ok(ConfigSetWriteTarget::InPlace {
            config_path: target,
            backup_path,
        });
    }
    let output_path = output_path.with_context(|| {
        "config set writes require --output <path> or --in-place; rerun with --dry-run to inspect the config patch"
    })?;
    if let Some(input_path) = input_path
        && same_path_text(input_path, output_path)
    {
        anyhow::bail!(
            "refusing to overwrite input config `{}` with --output; use --in-place to create a backup",
            input_path.display()
        );
    }
    Ok(ConfigSetWriteTarget::Output(output_path.to_path_buf()))
}

fn write_config_set_document(
    document: &serde_json::Value,
    target: &ConfigSetWriteTarget,
) -> anyhow::Result<()> {
    match target {
        ConfigSetWriteTarget::DryRun => Ok(()),
        ConfigSetWriteTarget::Output(output_path) => write_config_json_value(document, output_path),
        ConfigSetWriteTarget::InPlace {
            config_path,
            backup_path,
        } => {
            if let Some(backup_path) = backup_path {
                fs::copy(config_path, backup_path).with_context(|| {
                    format!(
                        "backup config `{}` to `{}`",
                        config_path.display(),
                        backup_path.display()
                    )
                })?;
            }
            write_config_json_value(document, config_path)
        }
    }
}

fn write_config_json_value(document: &serde_json::Value, output_path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = output_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("create config output directory `{}`", parent.display()))?;
    }
    let contents = serde_json::to_string_pretty(document).context("serialize updated config")?;
    write_file_atomically(output_path, &format!("{contents}\n"))
        .with_context(|| format!("write updated config `{}`", output_path.display()))
}

fn config_set_outcome_json(outcome: &ConfigSetOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path,
        "source": outcome.source,
        "pointer": outcome.pointer,
        "raw_value": outcome.raw_value,
        "force_string": outcome.force_string,
        "parsed_value_kind": outcome.parsed_value_kind,
        "before": outcome.before,
        "after": outcome.after,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
    })
}

fn print_config_set_outcome_text(outcome: &ConfigSetOutcome) -> anyhow::Result<()> {
    println!("dry_run: {}", outcome.dry_run);
    println!("source: {}", outcome.source);
    if let Some(config_path) = &outcome.config_path {
        println!("config_path: {}", config_path.display());
    }
    println!("pointer: {}", outcome.pointer);
    println!("force_string: {}", outcome.force_string);
    println!("parsed_value_kind: {}", outcome.parsed_value_kind);
    print!("before: ");
    print_config_value_inline(&outcome.before)?;
    print!("after: ");
    print_config_value_inline(&outcome.after)?;
    println!("in_place: {}", outcome.in_place);
    if let Some(output_path) = &outcome.output_path {
        println!("output_path: {}", output_path.display());
    }
    if let Some(backup_path) = &outcome.backup_path {
        println!("backup_path: {}", backup_path.display());
    }
    println!("will_write_config: {}", !outcome.dry_run);
    println!("wrote_config: {}", outcome.wrote_config);
    Ok(())
}

fn load_config_json(config_path: Option<&PathBuf>) -> anyhow::Result<LoadedConfigJson> {
    let path = if let Some(path) = config_path {
        Some(path.clone())
    } else {
        let default_path = default_config_path()?;
        default_path.exists().then_some(default_path)
    };
    let (source, contents) = match &path {
        Some(path) => (
            "file",
            fs::read_to_string(path)
                .with_context(|| format!("read config `{}`", path.display()))?,
        ),
        None => (
            "bundled-default",
            config_example_contents(ConfigExample::Default).to_owned(),
        ),
    };
    let document = serde_json::from_str::<serde_json::Value>(&contents)
        .with_context(|| format!("parse {source} config as JSON"))?;
    validate_config_json_value(&document, "validate config")?;
    Ok(LoadedConfigJson {
        path,
        source,
        document,
    })
}

fn validate_config_json_value(document: &serde_json::Value, context: &str) -> anyhow::Result<()> {
    let contents = serde_json::to_string(document).context("serialize config for validation")?;
    let config = VinputConfig::from_json_str(&contents).context("parse config")?;
    config.validate().with_context(|| context.to_owned())
}

fn ensure_json_pointer(pointer: &str) -> anyhow::Result<()> {
    if pointer.is_empty() || pointer.starts_with('/') {
        Ok(())
    } else {
        anyhow::bail!("config pointer `{pointer}` is not a JSON pointer; use /section/key")
    }
}

fn parse_config_set_value(
    raw_value: &str,
    force_string: bool,
) -> (serde_json::Value, &'static str) {
    if force_string {
        return (serde_json::Value::String(raw_value.to_owned()), "string");
    }
    match serde_json::from_str::<serde_json::Value>(raw_value) {
        Ok(value) => {
            let kind = match &value {
                serde_json::Value::Null => "null",
                serde_json::Value::Bool(_) => "bool",
                serde_json::Value::Number(_) => "number",
                serde_json::Value::String(_) => "string",
                serde_json::Value::Array(_) => "array",
                serde_json::Value::Object(_) => "object",
            };
            (value, kind)
        }
        Err(_) => (serde_json::Value::String(raw_value.to_owned()), "string"),
    }
}

fn print_config_value(value: &serde_json::Value) -> anyhow::Result<()> {
    if let Some(value) = value.as_str() {
        println!("{value}");
    } else {
        println!("{}", serde_json::to_string_pretty(value)?);
    }
    Ok(())
}

fn print_config_value_inline(value: &serde_json::Value) -> anyhow::Result<()> {
    if let Some(value) = value.as_str() {
        println!("{value}");
    } else {
        println!("{}", serde_json::to_string(value)?);
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct ConfigEditRequest<'a> {
    config_path: Option<&'a PathBuf>,
    editor: Option<&'a str>,
    dry_run: bool,
    json_output: bool,
}

struct ConfigEditPlan {
    config_path: PathBuf,
    source: &'static str,
    editor_argv: Vec<String>,
    backup_path: Option<PathBuf>,
    existed: bool,
    dry_run: bool,
}

struct ConfigEditOutcome {
    plan: ConfigEditPlan,
    temp_path: Option<PathBuf>,
    changed: bool,
    wrote_config: bool,
    exit_status: Option<i32>,
}

fn handle_config_edit(request: ConfigEditRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_config_edit(request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&config_edit_outcome_json(&outcome))?
        );
    } else {
        print_config_edit_outcome_text(&outcome);
    }
    Ok(())
}

fn run_config_edit(request: ConfigEditRequest<'_>) -> anyhow::Result<ConfigEditOutcome> {
    let default_path = default_config_path()?;
    let target_path = request
        .config_path
        .cloned()
        .unwrap_or_else(|| default_path.clone());
    let loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string_pretty(&loaded.document).context("serialize editable config")?;
    let contents = format!("{contents}\n");
    let editor_argv = resolve_config_editor(request.editor)?;
    let existed = target_path.exists();
    let backup_path = existed.then(|| config_backup_path(&target_path));
    let plan = ConfigEditPlan {
        config_path: target_path.clone(),
        source: loaded.source,
        editor_argv,
        backup_path,
        existed,
        dry_run: request.dry_run,
    };

    if request.dry_run {
        return Ok(ConfigEditOutcome {
            plan,
            temp_path: None,
            changed: false,
            wrote_config: false,
            exit_status: None,
        });
    }

    let temp_path = config_edit_temp_path(&target_path);
    write_config_edit_temp_file(&temp_path, &contents)?;
    let status = run_config_editor(&plan.editor_argv, &temp_path)?;
    let exit_status = status.code();
    if !status.success() {
        let _ = fs::remove_file(&temp_path);
        anyhow::bail!(
            "config editor `{}` exited with status {}",
            plan.editor_argv.join(" "),
            exit_status.map_or_else(|| "signal".to_owned(), |code| code.to_string())
        );
    }

    let edited_contents = fs::read_to_string(&temp_path)
        .with_context(|| format!("read edited config `{}`", temp_path.display()))?;
    let edited_document = serde_json::from_str::<serde_json::Value>(&edited_contents)
        .with_context(|| format!("parse edited config `{}` as JSON", temp_path.display()))?;
    validate_config_json_value(&edited_document, "validate edited config")?;
    let normalized = format!(
        "{}\n",
        serde_json::to_string_pretty(&edited_document).context("serialize edited config")?
    );
    let changed = normalized != contents || !target_path.exists();
    if changed {
        if let Some(backup_path) = &plan.backup_path {
            fs::copy(&target_path, backup_path).with_context(|| {
                format!(
                    "backup config `{}` to `{}`",
                    target_path.display(),
                    backup_path.display()
                )
            })?;
        }
        if let Some(parent) = target_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("create config directory `{}`", parent.display()))?;
        }
        write_file_atomically(&target_path, &normalized)
            .with_context(|| format!("write edited config `{}`", target_path.display()))?;
    }
    fs::remove_file(&temp_path)
        .with_context(|| format!("remove temporary edit file `{}`", temp_path.display()))?;

    Ok(ConfigEditOutcome {
        plan,
        temp_path: Some(temp_path),
        changed,
        wrote_config: changed,
        exit_status,
    })
}

fn config_edit_outcome_json(outcome: &ConfigEditOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.plan.dry_run,
        "config_path": outcome.plan.config_path,
        "source": outcome.plan.source,
        "existed": outcome.plan.existed,
        "editor": outcome.plan.editor_argv.join(" "),
        "editor_argv": outcome.plan.editor_argv,
        "backup_path": outcome.plan.backup_path,
        "temp_path": outcome.temp_path,
        "changed": outcome.changed,
        "will_write_config": !outcome.plan.dry_run,
        "wrote_config": outcome.wrote_config,
        "exit_status": outcome.exit_status,
    })
}

fn print_config_edit_outcome_text(outcome: &ConfigEditOutcome) {
    println!("dry_run: {}", outcome.plan.dry_run);
    println!("source: {}", outcome.plan.source);
    println!("config_path: {}", outcome.plan.config_path.display());
    println!("existed: {}", outcome.plan.existed);
    println!("editor: {}", outcome.plan.editor_argv.join(" "));
    if let Some(backup_path) = &outcome.plan.backup_path {
        println!("backup_path: {}", backup_path.display());
    }
    if let Some(temp_path) = &outcome.temp_path {
        println!("temp_path: {}", temp_path.display());
    }
    println!("changed: {}", outcome.changed);
    println!("will_write_config: {}", !outcome.plan.dry_run);
    println!("wrote_config: {}", outcome.wrote_config);
    if let Some(exit_status) = outcome.exit_status {
        println!("exit_status: {exit_status}");
    }
}

fn resolve_config_editor(editor: Option<&str>) -> anyhow::Result<Vec<String>> {
    let editor = editor
        .map(str::to_owned)
        .or_else(|| std::env::var("VINPUT_CONFIG_EDITOR").ok())
        .or_else(|| std::env::var("EDITOR").ok())
        .or_else(|| std::env::var("VISUAL").ok())
        .with_context(
            || "config edit requires --editor or $VINPUT_CONFIG_EDITOR/$EDITOR/$VISUAL",
        )?;
    let argv = split_editor_argv(&editor);
    if argv.is_empty() {
        anyhow::bail!("config editor command is empty");
    }
    Ok(argv)
}

fn split_editor_argv(editor: &str) -> Vec<String> {
    editor
        .split_whitespace()
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn run_config_editor(
    editor_argv: &[String],
    path: &Path,
) -> anyhow::Result<std::process::ExitStatus> {
    let (program, args) = editor_argv
        .split_first()
        .with_context(|| "config editor command is empty")?;
    ProcessCommand::new(program)
        .args(args)
        .arg(path)
        .status()
        .with_context(|| format!("run config editor `{}`", editor_argv.join(" ")))
}

fn config_edit_temp_path(target_path: &Path) -> PathBuf {
    let mut path = std::env::temp_dir();
    let target_name = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.json");
    path.push(format!(
        "vinput-config-edit-{}-{}-{target_name}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos())
    ));
    path
}

fn write_config_edit_temp_file(path: &Path, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("create temporary edit directory `{}`", parent.display()))?;
    }
    fs::write(path, contents)
        .with_context(|| format!("write temporary edit file `{}`", path.display()))
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

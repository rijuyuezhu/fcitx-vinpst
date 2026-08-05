//! Command-backed text adapter protocol, process runner, registry, and processor.

use std::{
    io::Write,
    process::{Command, Output},
};

use serde::{Deserialize, Serialize};
use vinpst_config::{COMMAND_SCENE_ID, LlmAdapterConfig, RAW_SCENE_ID, SceneDefinition};
use vinpst_process::{PipedCommandError, run_piped_command};
use vinpst_protocol::RecognitionPayload;

use crate::{
    TextAdapter, TextError, TextProcessor, TextRequest, command_mode_payload,
    scene_needs_postprocessing,
};

/// JSON request passed to command-backed text adapter helpers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandTextRequest {
    /// Stable adapter id from config.
    pub adapter_id: String,
    /// Raw ASR text before post-processing.
    pub raw_text: String,
    /// Optional selected text for command-mode transforms.
    #[serde(default)]
    pub selected_text: Option<String>,
    /// Scene metadata that selected this adapter.
    pub scene: CommandTextScene,
}

impl CommandTextRequest {
    /// Builds a command-helper request from adapter id and runtime text request.
    #[must_use]
    pub fn from_text_request(adapter_id: impl Into<String>, request: &TextRequest<'_>) -> Self {
        Self {
            adapter_id: adapter_id.into(),
            raw_text: request.raw_text.to_owned(),
            selected_text: request.selected_text.map(ToOwned::to_owned),
            scene: CommandTextScene::from_definition(request.scene),
        }
    }
}

/// Scene metadata serialized into command text adapter requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandTextScene {
    /// Scene id.
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// Optional prompt configured for the scene.
    #[serde(default)]
    pub prompt: Option<String>,
    /// Optional provider id configured for the scene.
    #[serde(default)]
    pub provider_id: Option<String>,
    /// Optional model id configured for the scene.
    #[serde(default)]
    pub model: Option<String>,
    /// Number of candidates requested by the scene.
    pub candidate_count: u8,
    /// Effective scene timeout in milliseconds.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Previous context lines requested by the scene.
    pub context_lines: u8,
}

impl CommandTextScene {
    /// Copies command-helper scene metadata from typed config.
    #[must_use]
    pub fn from_definition(scene: &SceneDefinition) -> Self {
        Self {
            id: scene.id.clone(),
            label: scene.label.clone(),
            prompt: scene.prompt.clone(),
            provider_id: scene.provider_id.clone(),
            model: scene.model.clone(),
            candidate_count: scene.candidate_count,
            timeout_ms: Some(scene.effective_timeout_ms()),
            context_lines: scene.context_lines,
        }
    }
}

/// JSON response returned by command-backed text adapter helpers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandTextResponse {
    /// Full recognition payload returned by the helper.
    #[serde(default)]
    pub payload: Option<RecognitionPayload>,
    /// Final text after post-processing.
    #[serde(default)]
    pub text: Option<String>,
    /// Error message returned by the helper.
    #[serde(default, alias = "failure")]
    pub error: Option<String>,
}

impl CommandTextResponse {
    /// Converts a helper response into the daemon recognition payload.
    pub fn into_payload(self) -> Result<RecognitionPayload, TextError> {
        if let Some(message) = self.error.filter(|message| !message.trim().is_empty()) {
            return Err(TextError::AdapterFailed(message));
        }
        if let Some(payload) = self.payload {
            return Ok(payload.normalized());
        }
        let Some(text) = self.text.filter(|text| !text.trim().is_empty()) else {
            return Err(TextError::AdapterFailed(
                "command text response missing final text".to_owned(),
            ));
        };
        Ok(RecognitionPayload::raw(text))
    }
}

/// Runner seam for command-backed text adapters.
pub trait CommandTextRunner: Send + Sync {
    /// Executes the configured command adapter for one post-processing request.
    fn run(
        &self,
        adapter_id: &str,
        command: &str,
        args: &[String],
        env: &std::collections::HashMap<String, String>,
        working_dir: Option<&str>,
        request: &TextRequest<'_>,
    ) -> Result<RecognitionPayload, TextError>;
}

/// Runner placeholder used until process execution is ported.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UnsupportedCommandTextRunner;

impl CommandTextRunner for UnsupportedCommandTextRunner {
    fn run(
        &self,
        _adapter_id: &str,
        _command: &str,
        _args: &[String],
        _env: &std::collections::HashMap<String, String>,
        _working_dir: Option<&str>,
        request: &TextRequest<'_>,
    ) -> Result<RecognitionPayload, TextError> {
        Err(TextError::UnsupportedAdapter(request.scene.id.clone()))
    }
}

/// Process runner for command-backed text adapter providers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProcessCommandTextRunner;

impl CommandTextRunner for ProcessCommandTextRunner {
    fn run(
        &self,
        adapter_id: &str,
        command: &str,
        args: &[String],
        env: &std::collections::HashMap<String, String>,
        working_dir: Option<&str>,
        request: &TextRequest<'_>,
    ) -> Result<RecognitionPayload, TextError> {
        let helper_request = CommandTextRequest::from_text_request(adapter_id, request);
        let mut request_bytes = serde_json::to_vec(&helper_request).map_err(|error| {
            TextError::AdapterFailed(format!(
                "failed to encode text adapter request for `{adapter_id}`: {error}"
            ))
        })?;
        request_bytes.push(b'\n');

        let mut command_process = Command::new(command);
        command_process.args(args).envs(env);
        if let Some(working_dir) = working_dir {
            command_process.current_dir(working_dir);
        }
        let result = run_piped_command(
            &mut command_process,
            Some(request.scene.effective_timeout_ms()),
            |stdin| stdin.write_all(&request_bytes),
        )
        .map_err(|error| text_adapter_process_error(adapter_id, error))?;
        let output = result.output;
        if !output.status.success() {
            return text_adapter_exit_error(adapter_id, &output);
        }
        if let Some(error) = result.stdin_error {
            return Err(TextError::AdapterFailed(format!(
                "failed to write text adapter request for `{adapter_id}`: {error}"
            )));
        }
        let response: CommandTextResponse =
            serde_json::from_slice(&output.stdout).map_err(|error| {
                TextError::AdapterFailed(format!(
                    "failed to decode text adapter response for `{adapter_id}`: {error}"
                ))
            })?;
        response.into_payload()
    }
}

fn text_adapter_process_error(adapter_id: &str, error: PipedCommandError) -> TextError {
    let message = match error {
        PipedCommandError::Spawn(error) => {
            format!("failed to spawn text adapter `{adapter_id}`: {error}")
        }
        PipedCommandError::WatchdogStart(error) => {
            format!("failed to start text adapter `{adapter_id}` timeout watchdog: {error}")
        }
        PipedCommandError::OutputCaptureStart(error) => {
            format!("failed to capture text adapter `{adapter_id}` output: {error}")
        }
        PipedCommandError::StdinUnavailable => {
            format!("text adapter `{adapter_id}` did not expose stdin")
        }
        PipedCommandError::TimedOut { timeout_ms } => {
            format!("text adapter `{adapter_id}` timed out after {timeout_ms} ms")
        }
        PipedCommandError::StdoutTooLarge { limit } => {
            format!("text adapter `{adapter_id}` stdout exceeds {limit}-byte limit")
        }
        PipedCommandError::StderrTooLarge { limit } => {
            format!("text adapter `{adapter_id}` stderr exceeds {limit}-byte limit")
        }
        PipedCommandError::StdoutRead => {
            format!("failed to read text adapter `{adapter_id}` stdout")
        }
        PipedCommandError::StderrRead => {
            format!("failed to read text adapter `{adapter_id}` stderr")
        }
        PipedCommandError::Wait(error) => {
            format!("failed to wait for text adapter `{adapter_id}`: {error}")
        }
        PipedCommandError::WatchdogPanicked => {
            format!("text adapter `{adapter_id}` timeout watchdog panicked")
        }
        PipedCommandError::StdoutReaderPanicked => {
            format!("text adapter `{adapter_id}` stdout reader panicked")
        }
        PipedCommandError::StderrReaderPanicked => {
            format!("text adapter `{adapter_id}` stderr reader panicked")
        }
    };
    TextError::AdapterFailed(message)
}

fn text_adapter_exit_error(
    adapter_id: &str,
    output: &Output,
) -> Result<RecognitionPayload, TextError> {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    if stderr.is_empty() {
        return Err(TextError::AdapterFailed(format!(
            "text adapter `{adapter_id}` exited with {}",
            output.status
        )));
    }
    Err(TextError::AdapterFailed(format!(
        "text adapter `{adapter_id}` exited with {}: {stderr}",
        output.status
    )))
}

/// Command-backed text adapter skeleton.
///
/// It owns the command configuration shape and delegates execution to a runner
/// seam so real process spawning can be added without making tests flaky.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandTextAdapter<R = UnsupportedCommandTextRunner> {
    id: String,
    command: String,
    args: Vec<String>,
    env: std::collections::HashMap<String, String>,
    working_dir: Option<String>,
    runner: R,
}

impl CommandTextAdapter<UnsupportedCommandTextRunner> {
    /// Creates a command adapter skeleton from executable and arguments.
    #[must_use]
    pub fn new(command: impl Into<String>, args: Vec<String>) -> Self {
        Self::with_runner(command, args, UnsupportedCommandTextRunner)
    }

    /// Creates a command adapter skeleton from typed config.
    #[must_use]
    pub fn from_config(config: &LlmAdapterConfig) -> Self {
        Self::with_adapter_config(config, UnsupportedCommandTextRunner)
    }
}

impl<R> CommandTextAdapter<R> {
    /// Creates a command adapter with an injected runner.
    #[must_use]
    pub fn with_runner(command: impl Into<String>, args: Vec<String>, runner: R) -> Self {
        Self::with_config(
            String::new(),
            command,
            args,
            std::collections::HashMap::default(),
            None,
            runner,
        )
    }

    /// Creates a command adapter with full typed command config and runner.
    #[must_use]
    pub fn with_config(
        id: impl Into<String>,
        command: impl Into<String>,
        args: Vec<String>,
        env: std::collections::HashMap<String, String>,
        working_dir: Option<String>,
        runner: R,
    ) -> Self {
        Self {
            id: id.into(),
            command: command.into(),
            args,
            env,
            working_dir,
            runner,
        }
    }

    /// Creates a command adapter from typed config with a supplied runner.
    #[must_use]
    pub fn with_adapter_config(config: &LlmAdapterConfig, runner: R) -> Self {
        Self::with_config(
            config.id.clone(),
            config.command.clone(),
            config.args.clone(),
            config.env.clone(),
            config.working_dir.clone(),
            runner,
        )
    }

    /// Returns the configured adapter id, if known.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the configured command path or name.
    #[must_use]
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Returns configured command arguments.
    #[must_use]
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// Returns configured command environment variables.
    #[must_use]
    pub fn env(&self) -> &std::collections::HashMap<String, String> {
        &self.env
    }

    /// Returns configured command working directory.
    #[must_use]
    pub fn working_dir(&self) -> Option<&str> {
        self.working_dir.as_deref()
    }

    /// Returns the configured command runner.
    #[must_use]
    pub const fn runner(&self) -> &R {
        &self.runner
    }
}

impl<R: CommandTextRunner> TextAdapter for CommandTextAdapter<R> {
    fn finish(&self, request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError> {
        self.runner.run(
            &self.id,
            &self.command,
            &self.args,
            &self.env,
            self.working_dir.as_deref(),
            request,
        )
    }
}

/// Registry of configured text adapters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdapterRegistry {
    command_adapters: std::collections::HashMap<String, CommandTextAdapter>,
}

impl AdapterRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a registry from typed command adapter config entries.
    #[must_use]
    pub fn from_configs(adapters: &[LlmAdapterConfig]) -> Self {
        Self {
            command_adapters: adapters
                .iter()
                .map(|adapter| (adapter.id.clone(), CommandTextAdapter::from_config(adapter)))
                .collect(),
        }
    }

    /// Returns the number of registered adapters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.command_adapters.len()
    }

    /// Returns whether the registry has no adapters.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.command_adapters.is_empty()
    }

    /// Returns whether a command adapter id is registered.
    #[must_use]
    pub fn contains_command_adapter(&self, id: &str) -> bool {
        self.command_adapters.contains_key(id)
    }

    /// Looks up a command adapter by id.
    #[must_use]
    pub fn command_adapter(&self, id: &str) -> Option<&CommandTextAdapter> {
        self.command_adapters.get(id)
    }

    /// Returns the only configured command adapter when exactly one exists.
    #[must_use]
    pub fn single_command_adapter(&self) -> Option<&CommandTextAdapter> {
        if self.command_adapters.len() == 1 {
            self.command_adapters.values().next()
        } else {
            None
        }
    }
}

/// Text processor that dispatches post-processing scenes to configured command adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandTextProcessor<R = UnsupportedCommandTextRunner> {
    adapters: Vec<CommandTextAdapter<R>>,
}

impl CommandTextProcessor<UnsupportedCommandTextRunner> {
    /// Builds a processor from typed command adapter config entries.
    #[must_use]
    pub fn from_configs(adapters: &[LlmAdapterConfig]) -> Self {
        Self {
            adapters: adapters
                .iter()
                .map(CommandTextAdapter::from_config)
                .collect(),
        }
    }
}

impl<R> CommandTextProcessor<R> {
    /// Builds a processor from already-constructed command adapters.
    #[must_use]
    pub fn from_adapters(adapters: Vec<CommandTextAdapter<R>>) -> Self {
        Self { adapters }
    }

    /// Returns the number of configured command adapters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.adapters.len()
    }

    /// Returns whether no command adapters are configured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }
}

impl<R: Clone> CommandTextProcessor<R> {
    /// Builds a processor from typed command adapter config entries and one reusable runner.
    #[must_use]
    pub fn from_configs_with_runner(adapters: &[LlmAdapterConfig], runner: R) -> Self {
        Self {
            adapters: adapters
                .iter()
                .map(|adapter| CommandTextAdapter::with_adapter_config(adapter, runner.clone()))
                .collect(),
        }
    }
}

impl<R: CommandTextRunner> TextProcessor for CommandTextProcessor<R> {
    fn finish(&self, request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError> {
        if request.scene.id == RAW_SCENE_ID || !scene_needs_postprocessing(request.scene) {
            return Ok(RecognitionPayload::raw(request.raw_text));
        }
        let [adapter] = self.adapters.as_slice() else {
            if self.adapters.is_empty() {
                if request.scene.id == COMMAND_SCENE_ID {
                    return Ok(command_mode_payload(
                        request.selected_text.unwrap_or_default(),
                        request.raw_text,
                        std::iter::empty(),
                    ));
                }
                return Err(TextError::AdapterRequired(request.scene.id.clone()));
            }
            return Err(TextError::AmbiguousAdapter(request.scene.id.clone()));
        };
        adapter.finish(request)
    }
}

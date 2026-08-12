//! Command-backed ASR protocol, process runners, and backend implementation.

use std::{
    io::Write,
    process::{Command, Output},
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vinpst_audio::{PcmBuffer, PcmSpec, i16_samples_to_le_bytes};
use vinpst_config::{AsrProviderConfig, AsrProviderKind};
use vinpst_process::{PipedCommandError, PipedCommandOutput, run_piped_command};

use crate::{
    AsrBackend, AsrError, BackendCapabilities, BackendDescriptor, RecognitionContext,
    RecognitionEvent, RecognitionSession,
};

/// Parsed external command ASR provider specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandAsrSpec {
    /// Provider id from config.
    pub provider_id: String,
    /// Executable path or command name.
    pub command: String,
    /// Arguments passed to the command.
    pub args: Vec<String>,
    /// Environment variables passed to the command.
    pub env: std::collections::HashMap<String, String>,
    /// Optional model id selected for this provider.
    pub model_id: Option<String>,
    /// Optional hotwords file configured for this provider.
    pub hotwords_file: Option<String>,
    /// Optional timeout in milliseconds.
    pub timeout_ms: Option<u64>,
}

impl TryFrom<&AsrProviderConfig> for CommandAsrSpec {
    type Error = AsrError;

    fn try_from(provider: &AsrProviderConfig) -> Result<Self, Self::Error> {
        if provider.kind != AsrProviderKind::Command {
            return Err(AsrError::Backend(format!(
                "provider `{}` is not a command ASR provider",
                provider.id
            )));
        }
        let command = provider
            .command
            .as_deref()
            .map(str::trim)
            .filter(|command| !command.is_empty())
            .ok_or_else(|| {
                AsrError::Backend(format!(
                    "command ASR provider `{}` must configure a command",
                    provider.id
                ))
            })?;
        Ok(Self {
            provider_id: provider.id.clone(),
            command: command.to_owned(),
            args: provider.args.clone(),
            env: provider.env.clone(),
            model_id: provider.model.clone(),
            hotwords_file: provider.hotwords_file.clone(),
            timeout_ms: provider.timeout_ms,
        })
    }
}

/// Buffered request passed to command-backed ASR runners.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CommandAsrRequest {
    /// Provider id selected for this request.
    pub provider_id: String,
    /// Optional model id selected for this provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// Optional hotwords file configured for this provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotwords_file: Option<String>,
    /// Optional request timeout in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// Recognition context from the active scene or command mode.
    pub context: RecognitionContext,
    /// PCM layout metadata for the buffered signed 16-bit samples.
    #[serde(default)]
    pub pcm: PcmSpec,
    /// Buffered signed 16-bit PCM samples, interleaved when channel count is greater than one.
    pub samples: Vec<i16>,
}

impl CommandAsrRequest {
    /// Creates a buffered request from parsed provider metadata and runtime context.
    #[must_use]
    pub fn from_spec(
        spec: &CommandAsrSpec,
        context: RecognitionContext,
        samples: Vec<i16>,
    ) -> Self {
        Self::from_spec_with_pcm(spec, context, PcmSpec::default(), samples)
    }

    /// Creates a buffered request with explicit PCM metadata.
    #[must_use]
    pub fn from_spec_with_pcm(
        spec: &CommandAsrSpec,
        context: RecognitionContext,
        pcm: PcmSpec,
        samples: Vec<i16>,
    ) -> Self {
        Self {
            provider_id: spec.provider_id.clone(),
            model_id: spec.model_id.clone(),
            hotwords_file: spec.hotwords_file.clone(),
            timeout_ms: spec.timeout_ms,
            context,
            pcm,
            samples,
        }
    }
}

/// Response returned by a command-backed ASR helper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct CommandAsrResponse {
    /// Optional streaming partial text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial_text: Option<String>,
    /// Final recognized text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Backend error message produced by the helper.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl CommandAsrResponse {
    /// Converts a helper response into recognition events.
    pub fn into_events(self) -> Result<Vec<RecognitionEvent>, AsrError> {
        let mut events = Vec::new();
        if let Some(partial_text) = self.partial_text.filter(|text| !text.trim().is_empty()) {
            events.push(RecognitionEvent::PartialText { text: partial_text });
        }
        if let Some(message) = self.error.filter(|message| !message.trim().is_empty()) {
            events.push(RecognitionEvent::Error { message });
            events.push(RecognitionEvent::Completed);
            return Ok(events);
        }
        let Some(text) = self.text.filter(|text| !text.trim().is_empty()) else {
            return Err(AsrError::Backend(
                "command ASR response missing final text".to_owned(),
            ));
        };
        events.push(RecognitionEvent::FinalText { text });
        events.push(RecognitionEvent::Completed);
        Ok(events)
    }
}

/// Runner seam for command-backed ASR providers.
pub trait CommandAsrRunner: Send + Sync {
    /// Recognizes one buffered command ASR request.
    fn recognize(
        &self,
        spec: &CommandAsrSpec,
        request: &CommandAsrRequest,
    ) -> Result<Vec<RecognitionEvent>, AsrError>;
}

/// Runner placeholder used until process execution is ported.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnsupportedCommandAsrRunner;

impl CommandAsrRunner for UnsupportedCommandAsrRunner {
    fn recognize(
        &self,
        spec: &CommandAsrSpec,
        _request: &CommandAsrRequest,
    ) -> Result<Vec<RecognitionEvent>, AsrError> {
        Err(AsrError::Backend(format!(
            "command ASR provider `{}` runner is not implemented yet",
            spec.provider_id
        )))
    }
}

/// Builds a legacy command-streaming audio JSON line.
///
/// The line contains raw signed 16-bit little-endian PCM bytes encoded as
/// base64 and a `commit` flag indicating whether the chunk finalizes audio.
#[must_use]
pub fn legacy_command_streaming_audio_line(samples: &[i16], commit: bool) -> String {
    serde_json::json!({
        "type": "audio",
        "audio_base64": encode_base64(&i16_le_pcm_bytes(samples)),
        "commit": commit,
    })
    .to_string()
}

/// Builds a legacy command-streaming finish control JSON line.
#[must_use]
pub fn legacy_command_streaming_finish_line() -> String {
    serde_json::json!({"type": "finish"}).to_string()
}

fn i16_le_pcm_bytes(samples: &[i16]) -> Vec<u8> {
    i16_samples_to_le_bytes(samples)
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        output.push(TABLE[(b0 >> 2) as usize] as char);
        output.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}

/// Parses one legacy command-streaming JSON line into recognition events.
///
/// Supported legacy event types are `session_started`, `partial`, `final`,
/// `final_timestamps`, `error`, and `closed`. Unknown event types are ignored.
pub fn parse_legacy_command_streaming_line(line: &str) -> Result<Vec<RecognitionEvent>, AsrError> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(Vec::new());
    }
    let payload = serde_json::from_str::<serde_json::Value>(line)
        .map_err(|error| AsrError::Backend(format!("invalid streaming provider JSON: {error}")))?;
    let event_type = payload
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    match event_type {
        "partial" => Ok(payload
            .get("text")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| {
                vec![RecognitionEvent::PartialText {
                    text: text.to_owned(),
                }]
            })
            .unwrap_or_default()),
        "final" | "final_timestamps" => Ok(payload
            .get("text")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| {
                vec![RecognitionEvent::FinalText {
                    text: text.to_owned(),
                }]
            })
            .unwrap_or_default()),
        "error" => Ok(vec![RecognitionEvent::Error {
            message: payload
                .get("message")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|message| !message.is_empty())
                .unwrap_or("failed.")
                .to_owned(),
        }]),
        "closed" => Ok(vec![RecognitionEvent::Completed]),
        _ => Ok(Vec::new()),
    }
}

/// Legacy process runner for command ASR providers.
///
/// The original C++ batch command backend writes raw signed 16-bit little-endian
/// PCM bytes to stdin and treats trimmed stdout as the final recognized text.
#[derive(Debug, Clone, Copy, Default)]
pub struct LegacyCommandBatchRunner;

impl CommandAsrRunner for LegacyCommandBatchRunner {
    fn recognize(
        &self,
        spec: &CommandAsrSpec,
        request: &CommandAsrRequest,
    ) -> Result<Vec<RecognitionEvent>, AsrError> {
        let mut command = Command::new(&spec.command);
        command.args(&spec.args).envs(&spec.env);
        let result =
            run_command_asr_process(spec, "legacy command ASR provider", &mut command, |stdin| {
                write_i16_le_pcm(stdin, &request.samples)
            })?;
        let output = result.output;
        if !output.status.success() {
            return command_exit_error(spec, &output);
        }
        if let Some(error) = result.stdin_error {
            return Err(AsrError::Backend(format!(
                "failed to write legacy command ASR PCM for `{}`: {error}",
                spec.provider_id
            )));
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if text.is_empty() {
            return Err(AsrError::Backend(format!(
                "legacy command ASR provider `{}` returned no text",
                spec.provider_id
            )));
        }
        Ok(vec![
            RecognitionEvent::FinalText { text },
            RecognitionEvent::Completed,
        ])
    }
}

fn write_i16_le_pcm(mut writer: impl Write, samples: &[i16]) -> std::io::Result<()> {
    for sample in samples {
        writer.write_all(&sample.to_le_bytes())?;
    }
    Ok(())
}

/// One-shot JSON-lines compatibility runner used by focused protocol tests.
///
/// Production `.streaming` providers use `LegacyCommandStreamingBackend`,
/// which owns a long-lived helper process across audio pushes.
#[cfg(test)]
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LegacyCommandStreamingRunner;

#[cfg(test)]
impl CommandAsrRunner for LegacyCommandStreamingRunner {
    fn recognize(
        &self,
        spec: &CommandAsrSpec,
        request: &CommandAsrRequest,
    ) -> Result<Vec<RecognitionEvent>, AsrError> {
        let audio_line = legacy_command_streaming_audio_line(&request.samples, true);
        let finish_line = legacy_command_streaming_finish_line();
        let mut command = Command::new(&spec.command);
        command.args(&spec.args).envs(&spec.env);
        let result = run_command_asr_process(
            spec,
            "legacy command streaming ASR provider",
            &mut command,
            |stdin| {
                stdin.write_all(audio_line.as_bytes())?;
                stdin.write_all(b"\n")?;
                stdin.write_all(finish_line.as_bytes())?;
                stdin.write_all(b"\n")
            },
        )?;
        let output = result.output;
        if !output.status.success() {
            return command_exit_error(spec, &output);
        }
        if let Some(error) = result.stdin_error {
            return Err(AsrError::Backend(format!(
                "failed to write legacy command streaming events for `{}`: {error}",
                spec.provider_id
            )));
        }
        parse_legacy_command_streaming_stdout(&output.stdout)
    }
}

#[cfg(test)]
fn parse_legacy_command_streaming_stdout(stdout: &[u8]) -> Result<Vec<RecognitionEvent>, AsrError> {
    let stdout = String::from_utf8_lossy(stdout);
    let mut events = Vec::new();
    let mut last_partial_text = String::new();
    for line in stdout.lines() {
        for event in parse_legacy_command_streaming_line(line)? {
            match &event {
                RecognitionEvent::PartialText { text } if text == &last_partial_text => {}
                RecognitionEvent::PartialText { text } => {
                    last_partial_text.clone_from(text);
                    events.push(event);
                }
                RecognitionEvent::FinalText { .. } => {
                    last_partial_text.clear();
                    events.push(event);
                }
                _ => events.push(event),
            }
        }
    }
    if events.is_empty() {
        return Err(AsrError::Backend(
            "legacy command streaming provider returned no events".to_owned(),
        ));
    }
    if !matches!(events.last(), Some(RecognitionEvent::Completed)) {
        events.push(RecognitionEvent::Completed);
    }
    Ok(events)
}

/// Process runner for command-backed ASR providers.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessCommandAsrRunner;

impl CommandAsrRunner for ProcessCommandAsrRunner {
    fn recognize(
        &self,
        spec: &CommandAsrSpec,
        request: &CommandAsrRequest,
    ) -> Result<Vec<RecognitionEvent>, AsrError> {
        let mut request_bytes = serde_json::to_vec(request).map_err(|error| {
            AsrError::Backend(format!(
                "failed to encode command ASR request for `{}`: {error}",
                spec.provider_id
            ))
        })?;
        request_bytes.push(b'\n');
        let mut command = Command::new(&spec.command);
        command.args(&spec.args).envs(&spec.env);
        let result =
            run_command_asr_process(spec, "command ASR provider", &mut command, |stdin| {
                stdin.write_all(&request_bytes)
            })?;
        let output = result.output;
        if !output.status.success() {
            return command_exit_error(spec, &output);
        }
        if let Some(error) = result.stdin_error {
            return Err(AsrError::Backend(format!(
                "failed to write command ASR request for `{}`: {error}",
                spec.provider_id
            )));
        }
        let response: CommandAsrResponse =
            serde_json::from_slice(&output.stdout).map_err(|error| {
                AsrError::Backend(format!(
                    "failed to decode command ASR response for `{}`: {error}",
                    spec.provider_id
                ))
            })?;
        response.into_events()
    }
}

fn run_command_asr_process(
    spec: &CommandAsrSpec,
    spawn_label: &str,
    command: &mut Command,
    write_stdin: impl FnOnce(&mut std::process::ChildStdin) -> std::io::Result<()>,
) -> Result<PipedCommandOutput, AsrError> {
    run_piped_command(command, spec.timeout_ms, write_stdin)
        .map_err(|error| command_asr_process_error(spec, spawn_label, error))
}

fn command_asr_process_error(
    spec: &CommandAsrSpec,
    spawn_label: &str,
    error: PipedCommandError,
) -> AsrError {
    let provider_id = &spec.provider_id;
    let message = match error {
        PipedCommandError::Spawn(error) => {
            format!("failed to spawn {spawn_label} `{provider_id}`: {error}")
        }
        PipedCommandError::WatchdogStart(error) => format!(
            "failed to start command ASR provider `{provider_id}` timeout watchdog: {error}"
        ),
        PipedCommandError::OutputCaptureStart(error) => {
            format!("failed to capture command ASR provider `{provider_id}` output: {error}")
        }
        PipedCommandError::StdinUnavailable => {
            format!("{spawn_label} `{provider_id}` did not expose stdin")
        }
        PipedCommandError::TimedOut { timeout_ms } => {
            format!("command ASR provider `{provider_id}` timed out after {timeout_ms} ms")
        }
        PipedCommandError::StdoutTooLarge { limit } => {
            format!("command ASR provider `{provider_id}` stdout exceeds {limit}-byte limit")
        }
        PipedCommandError::StderrTooLarge { limit } => {
            format!("command ASR provider `{provider_id}` stderr exceeds {limit}-byte limit")
        }
        PipedCommandError::StdoutRead => {
            format!("failed to read command ASR provider `{provider_id}` stdout")
        }
        PipedCommandError::StderrRead => {
            format!("failed to read command ASR provider `{provider_id}` stderr")
        }
        PipedCommandError::Wait(error) => {
            format!("failed to wait for command ASR provider `{provider_id}`: {error}")
        }
        PipedCommandError::WatchdogPanicked => {
            format!("command ASR provider `{provider_id}` timeout watchdog panicked")
        }
        PipedCommandError::StdoutReaderPanicked => {
            format!("command ASR provider `{provider_id}` stdout reader panicked")
        }
        PipedCommandError::StderrReaderPanicked => {
            format!("command ASR provider `{provider_id}` stderr reader panicked")
        }
    };
    AsrError::Backend(message)
}

fn command_exit_error(
    spec: &CommandAsrSpec,
    output: &Output,
) -> Result<Vec<RecognitionEvent>, AsrError> {
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(AsrError::Backend(format!(
        "command ASR provider `{}` exited with {}: {}",
        spec.provider_id,
        output.status,
        stderr.trim()
    )))
}

/// Command-backed ASR backend skeleton.
#[derive(Debug, Clone)]
pub struct CommandAsrBackend<R = UnsupportedCommandAsrRunner> {
    spec: CommandAsrSpec,
    descriptor: BackendDescriptor,
    runner: R,
}

impl CommandAsrBackend<UnsupportedCommandAsrRunner> {
    /// Creates a command ASR backend skeleton from a parsed spec.
    #[must_use]
    pub fn new(spec: CommandAsrSpec) -> Self {
        Self::with_runner(spec, UnsupportedCommandAsrRunner)
    }
}

impl<R> CommandAsrBackend<R> {
    /// Creates a command ASR backend with an injected buffered runner.
    #[must_use]
    pub fn with_runner(spec: CommandAsrSpec, runner: R) -> Self {
        Self::with_runner_and_capabilities(spec, runner, BackendCapabilities::buffered())
    }

    /// Creates a command ASR backend with an injected runner and explicit capabilities.
    #[must_use]
    pub fn with_runner_and_capabilities(
        spec: CommandAsrSpec,
        runner: R,
        capabilities: BackendCapabilities,
    ) -> Self {
        let descriptor = BackendDescriptor::new(
            spec.provider_id.clone(),
            spec.model_id.clone().unwrap_or_default(),
            "Command ASR",
            capabilities,
        );
        Self {
            spec,
            descriptor,
            runner,
        }
    }

    /// Creates a command ASR backend from typed provider config with an injected runner.
    pub fn with_config(provider: &AsrProviderConfig, runner: R) -> Result<Self, AsrError> {
        Self::with_config_and_capabilities(provider, runner, BackendCapabilities::buffered())
    }

    /// Creates a command ASR backend from typed provider config with explicit capabilities.
    pub fn with_config_and_capabilities(
        provider: &AsrProviderConfig,
        runner: R,
        capabilities: BackendCapabilities,
    ) -> Result<Self, AsrError> {
        Ok(Self::with_runner_and_capabilities(
            CommandAsrSpec::try_from(provider)?,
            runner,
            capabilities,
        ))
    }

    /// Returns the parsed command provider spec.
    #[must_use]
    pub const fn spec(&self) -> &CommandAsrSpec {
        &self.spec
    }

    /// Returns the configured command runner.
    #[must_use]
    pub const fn runner(&self) -> &R {
        &self.runner
    }
}

impl<R: CommandAsrRunner + Clone + 'static> AsrBackend for CommandAsrBackend<R> {
    fn describe(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn create_session(
        &self,
        context: RecognitionContext,
    ) -> Result<Box<dyn RecognitionSession>, AsrError> {
        Ok(Box::new(CommandRecognitionSession {
            spec: self.spec.clone(),
            context,
            runner: self.runner.clone(),
            pcm: PcmSpec::default(),
            samples: Vec::new(),
            finished: false,
            cancelled: false,
            events: Vec::new(),
        }))
    }
}

#[derive(Debug)]
struct CommandRecognitionSession<R> {
    spec: CommandAsrSpec,
    context: RecognitionContext,
    runner: R,
    pcm: PcmSpec,
    samples: Vec<i16>,
    finished: bool,
    cancelled: bool,
    events: Vec<RecognitionEvent>,
}

impl<R: CommandAsrRunner> RecognitionSession for CommandRecognitionSession<R> {
    fn push_pcm(&mut self, pcm: &PcmBuffer) -> Result<(), AsrError> {
        if self.cancelled {
            return Err(AsrError::Cancelled);
        }
        if self.finished {
            return Err(AsrError::AlreadyFinished);
        }
        let next_pcm = pcm.spec();
        if !self.samples.is_empty() && self.pcm != next_pcm {
            return Err(AsrError::Backend(format!(
                "command ASR PCM spec changed from {} Hz/{} channel(s) to {} Hz/{} channel(s)",
                self.pcm.sample_rate_hz,
                self.pcm.channels,
                next_pcm.sample_rate_hz,
                next_pcm.channels
            )));
        }
        self.pcm = next_pcm;
        self.samples.extend_from_slice(pcm.samples());
        Ok(())
    }

    fn push_audio(&mut self, samples: &[i16]) -> Result<(), AsrError> {
        if self.cancelled {
            return Err(AsrError::Cancelled);
        }
        if self.finished {
            return Err(AsrError::AlreadyFinished);
        }
        self.samples.extend_from_slice(samples);
        Ok(())
    }

    fn finish(&mut self) -> Result<(), AsrError> {
        if self.cancelled {
            return Err(AsrError::Cancelled);
        }
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        let request = CommandAsrRequest::from_spec_with_pcm(
            &self.spec,
            self.context.clone(),
            self.pcm,
            self.samples.clone(),
        );
        self.events = self.runner.recognize(&self.spec, &request)?;
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), AsrError> {
        self.cancelled = true;
        self.finished = true;
        self.samples.clear();
        self.events.clear();
        self.events.push(RecognitionEvent::Completed);
        Ok(())
    }

    fn poll_events(&mut self) -> Result<Vec<RecognitionEvent>, AsrError> {
        Ok(std::mem::take(&mut self.events))
    }
}

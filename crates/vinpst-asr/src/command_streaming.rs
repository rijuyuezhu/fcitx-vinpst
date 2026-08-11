//! Incremental legacy JSON-lines command ASR backend.

use std::{
    io::{Read, Write},
    os::fd::AsFd,
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use nix::fcntl::{FcntlArg, OFlag, fcntl};
use vinpst_process::{
    MAX_COMMAND_OUTPUT_BYTES, configure_process_group, terminate_child_process_group,
    try_wait_child_and_cleanup,
};

use crate::{
    AsrBackend, AsrError, BackendCapabilities, BackendDescriptor, CommandAsrSpec,
    RecognitionContext, RecognitionEvent, RecognitionSession, legacy_command_streaming_audio_line,
    legacy_command_streaming_finish_line, parse_legacy_command_streaming_line,
};

const DEFAULT_FINAL_TIMEOUT: Duration = Duration::from_secs(10);
const FINAL_GRACE: Duration = Duration::from_millis(200);
const IO_RETRY_DELAY: Duration = Duration::from_millis(5);
const CLEANUP_GRACE: Duration = Duration::from_millis(100);
const CLEANUP_FORCE: Duration = Duration::from_secs(1);

/// Real incremental backend for legacy `.streaming` command providers.
#[derive(Debug, Clone)]
pub struct LegacyCommandStreamingBackend {
    spec: CommandAsrSpec,
    descriptor: BackendDescriptor,
}

impl LegacyCommandStreamingBackend {
    /// Builds one streaming command backend from a parsed provider spec.
    #[must_use]
    pub fn new(spec: CommandAsrSpec) -> Self {
        let descriptor = BackendDescriptor::new(
            spec.provider_id.clone(),
            spec.model_id.clone().unwrap_or_default(),
            "Command ASR",
            BackendCapabilities::streaming(),
        );
        Self { spec, descriptor }
    }

    /// Builds one streaming command backend from typed provider config.
    pub fn with_config(provider: &vinpst_config::AsrProviderConfig) -> Result<Self, AsrError> {
        Ok(Self::new(CommandAsrSpec::try_from(provider)?))
    }
}

impl AsrBackend for LegacyCommandStreamingBackend {
    fn describe(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn create_session(
        &self,
        _context: RecognitionContext,
    ) -> Result<Box<dyn RecognitionSession>, AsrError> {
        Ok(Box::new(LegacyCommandStreamingSession::new(
            self.spec.clone(),
        )))
    }
}

const EVENT_FINAL: u8 = 1 << 0;
const EVENT_ERROR: u8 = 1 << 1;
const EVENT_COMPLETED: u8 = 1 << 2;
const EVENT_COMPLETED_EMITTED: u8 = 1 << 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionLifecycle {
    Active,
    Finished,
    Cancelled,
}

#[derive(Debug)]
struct LegacyCommandStreamingSession {
    spec: CommandAsrSpec,
    process: Option<StreamingProcess>,
    pending_samples: Vec<i16>,
    events: Vec<RecognitionEvent>,
    stdout_buffer: String,
    stderr_buffer: String,
    stderr_tail: String,
    stdout_bytes: usize,
    stderr_bytes: usize,
    last_partial_text: String,
    event_flags: u8,
    lifecycle: SessionLifecycle,
}

impl LegacyCommandStreamingSession {
    fn new(spec: CommandAsrSpec) -> Self {
        Self {
            spec,
            process: None,
            pending_samples: Vec::new(),
            events: Vec::new(),
            stdout_buffer: String::new(),
            stderr_buffer: String::new(),
            stderr_tail: String::new(),
            stdout_bytes: 0,
            stderr_bytes: 0,
            last_partial_text: String::new(),
            event_flags: 0,
            lifecycle: SessionLifecycle::Active,
        }
    }

    fn ensure_started(&mut self) -> Result<(), AsrError> {
        if self.process.is_some() {
            return Ok(());
        }
        let process = StreamingProcess::spawn(&self.spec)?;
        self.process = Some(process);
        Ok(())
    }

    fn operation_timeout(&self) -> Duration {
        self.spec
            .timeout_ms
            .map_or(DEFAULT_FINAL_TIMEOUT, Duration::from_millis)
    }

    fn flush_pending_chunk(&mut self, commit: bool) -> Result<(), AsrError> {
        if self.pending_samples.is_empty() {
            return Ok(());
        }
        let line = legacy_command_streaming_audio_line(&self.pending_samples, commit);
        self.pending_samples.clear();
        self.write_json_line(&line)
    }

    fn write_json_line(&mut self, line: &str) -> Result<(), AsrError> {
        self.ensure_started()?;
        let deadline = Instant::now() + self.operation_timeout();
        let mut bytes = Vec::with_capacity(line.len() + 1);
        bytes.extend_from_slice(line.as_bytes());
        bytes.push(b'\n');
        let mut offset = 0;

        while offset < bytes.len() {
            self.pump_io()?;
            let process = self.process.as_mut().ok_or_else(|| {
                AsrError::Backend(format!(
                    "command streaming provider `{}` is not running",
                    self.spec.provider_id
                ))
            })?;
            let Some(stdin) = process.stdin.as_mut() else {
                return Err(self.provider_error("provider stdin is already closed."));
            };
            match stdin.write(&bytes[offset..]) {
                Ok(0) => return Err(self.provider_error("provider closed stdin.")),
                Ok(written) => offset += written,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(self.timeout_error());
                    }
                    thread::sleep(IO_RETRY_DELAY);
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => {
                    return Err(self.provider_error(&format!("write failed: {error}")));
                }
            }
            if Instant::now() >= deadline && offset < bytes.len() {
                return Err(self.timeout_error());
            }
        }
        self.pump_io()
    }

    fn pump_io(&mut self) -> Result<(), AsrError> {
        let mut stdout_chunks = Vec::new();
        let mut stderr_chunks = Vec::new();
        let mut stdout_closed = false;
        let mut stderr_closed = false;
        let mut exited = None;

        if let Some(process) = self.process.as_mut() {
            if let Some(stdout) = process.stdout.as_mut() {
                stdout_closed = read_nonblocking(stdout, &mut stdout_chunks)?;
            }
            if let Some(stderr) = process.stderr.as_mut() {
                stderr_closed = read_nonblocking(stderr, &mut stderr_chunks)?;
            }
            if process.exit_status.is_none() {
                exited = try_wait_child_and_cleanup(&mut process.child).map_err(|error| {
                    self.provider_error(&format!("failed to wait for provider: {error}"))
                })?;
            }
        }

        for chunk in stdout_chunks {
            self.consume_stdout_bytes(&chunk)?;
        }
        for chunk in stderr_chunks {
            self.consume_stderr_bytes(&chunk)?;
        }
        if stdout_closed && let Some(process) = self.process.as_mut() {
            process.stdout = None;
        }
        if stderr_closed {
            self.flush_stderr_tail();
            if let Some(process) = self.process.as_mut() {
                process.stderr = None;
            }
        }
        if let Some(status) = exited {
            if let Some(process) = self.process.as_mut() {
                process.exit_status = Some(status);
                process.stdin = None;
                process.stdout = None;
                process.stderr = None;
            }
            self.flush_stderr_tail();
            if !status.success() && !self.saw_error() && !self.saw_final_text() {
                let detail = self.stderr_or("failed.");
                self.queue_error(&detail);
            }
        }
        Ok(())
    }

    fn consume_stdout_bytes(&mut self, chunk: &[u8]) -> Result<(), AsrError> {
        self.stdout_bytes = self.stdout_bytes.saturating_add(chunk.len());
        if self.stdout_bytes > MAX_COMMAND_OUTPUT_BYTES {
            return Err(self.provider_error(&format!(
                "stdout exceeds {MAX_COMMAND_OUTPUT_BYTES}-byte limit"
            )));
        }
        self.stdout_buffer.push_str(&String::from_utf8_lossy(chunk));
        while let Some(newline) = self.stdout_buffer.find('\n') {
            let line = self.stdout_buffer[..newline].trim().to_owned();
            self.stdout_buffer.drain(..=newline);
            if line.is_empty() {
                continue;
            }
            self.handle_output_line(&line)?;
        }
        Ok(())
    }

    fn consume_stderr_bytes(&mut self, chunk: &[u8]) -> Result<(), AsrError> {
        self.stderr_bytes = self.stderr_bytes.saturating_add(chunk.len());
        if self.stderr_bytes > MAX_COMMAND_OUTPUT_BYTES {
            return Err(self.provider_error(&format!(
                "stderr exceeds {MAX_COMMAND_OUTPUT_BYTES}-byte limit"
            )));
        }
        self.stderr_buffer.push_str(&String::from_utf8_lossy(chunk));
        while let Some(newline) = self.stderr_buffer.find('\n') {
            let line = self.stderr_buffer[..newline].trim().to_owned();
            self.stderr_buffer.drain(..=newline);
            if !line.is_empty() {
                self.stderr_tail = line;
            }
        }
        Ok(())
    }

    fn flush_stderr_tail(&mut self) {
        let tail = self.stderr_buffer.trim();
        if !tail.is_empty() {
            self.stderr_tail = tail.to_owned();
        }
        self.stderr_buffer.clear();
    }

    fn handle_output_line(&mut self, line: &str) -> Result<(), AsrError> {
        for event in parse_legacy_command_streaming_line(line)? {
            match event {
                RecognitionEvent::PartialText { text } if text == self.last_partial_text => {}
                RecognitionEvent::PartialText { text } => {
                    self.last_partial_text.clone_from(&text);
                    self.events.push(RecognitionEvent::PartialText { text });
                }
                RecognitionEvent::FinalText { text } => {
                    self.last_partial_text.clear();
                    self.event_flags |= EVENT_FINAL;
                    self.events.push(RecognitionEvent::FinalText { text });
                }
                RecognitionEvent::Error { message } => self.queue_error(&message),
                RecognitionEvent::Completed => {
                    self.event_flags |= EVENT_COMPLETED;
                }
            }
        }
        Ok(())
    }

    fn queue_error(&mut self, message: &str) {
        let message = if message.trim().is_empty() {
            "failed.".to_owned()
        } else {
            message.trim().to_owned()
        };
        self.event_flags |= EVENT_ERROR;
        self.events.push(RecognitionEvent::Error { message });
    }

    fn queue_completed(&mut self) {
        if self.completed_emitted() {
            return;
        }
        self.event_flags |= EVENT_COMPLETED;
        self.event_flags |= EVENT_COMPLETED_EMITTED;
        self.events.push(RecognitionEvent::Completed);
    }

    fn wait_for_finish(&mut self) -> Result<(), AsrError> {
        let deadline = Instant::now() + self.operation_timeout();
        while !self.saw_final_text() && !self.completed() && !self.child_exited() {
            self.pump_io()?;
            if Instant::now() >= deadline {
                let message = self.stderr_or("timed out.");
                self.queue_error(&message);
                self.queue_completed();
                self.cleanup_process();
                return Err(AsrError::Backend(message));
            }
            thread::sleep(IO_RETRY_DELAY);
        }

        if self.saw_final_text() {
            let grace_deadline = Instant::now() + FINAL_GRACE;
            while !self.completed() && !self.child_exited() && Instant::now() < grace_deadline {
                self.pump_io()?;
                thread::sleep(IO_RETRY_DELAY);
            }
            self.pump_io()?;
            self.queue_completed();
            self.cleanup_process();
            return Ok(());
        }

        self.pump_io()?;
        if !self.saw_final_text() && !self.saw_error() {
            let message = self.stderr_or("returned no text.");
            self.queue_error(&message);
        }
        self.queue_completed();
        let error = self.first_error_message();
        self.cleanup_process();
        match error {
            Some(message) => Err(AsrError::Backend(message)),
            None => Ok(()),
        }
    }

    const fn saw_final_text(&self) -> bool {
        self.event_flags & EVENT_FINAL != 0
    }

    const fn saw_error(&self) -> bool {
        self.event_flags & EVENT_ERROR != 0
    }

    const fn completed(&self) -> bool {
        self.event_flags & EVENT_COMPLETED != 0
    }

    const fn completed_emitted(&self) -> bool {
        self.event_flags & EVENT_COMPLETED_EMITTED != 0
    }

    fn child_exited(&self) -> bool {
        self.process
            .as_ref()
            .is_some_and(|process| process.exit_status.is_some())
    }

    fn first_error_message(&self) -> Option<String> {
        self.events.iter().find_map(|event| match event {
            RecognitionEvent::Error { message } => Some(message.clone()),
            RecognitionEvent::PartialText { .. }
            | RecognitionEvent::FinalText { .. }
            | RecognitionEvent::Completed => None,
        })
    }

    fn stderr_or(&self, fallback: &str) -> String {
        let detail = self.stderr_tail.trim();
        if detail.is_empty() {
            fallback.to_owned()
        } else {
            detail.to_owned()
        }
    }

    fn timeout_error(&self) -> AsrError {
        self.provider_error(&format!(
            "timed out after {} ms",
            self.operation_timeout().as_millis()
        ))
    }

    fn provider_error(&self, detail: &str) -> AsrError {
        AsrError::Backend(format!(
            "command streaming provider `{}` {detail}",
            self.spec.provider_id
        ))
    }

    fn close_stdin(&mut self) {
        if let Some(process) = self.process.as_mut() {
            process.stdin = None;
        }
    }

    fn cleanup_process(&mut self) {
        let Some(mut process) = self.process.take() else {
            return;
        };
        process.stdin = None;
        process.stdout = None;
        process.stderr = None;
        if process.exit_status.is_none() {
            let _ = terminate_child_process_group(&mut process.child, CLEANUP_GRACE, CLEANUP_FORCE);
        }
    }
}

impl RecognitionSession for LegacyCommandStreamingSession {
    fn push_audio(&mut self, samples: &[i16]) -> Result<(), AsrError> {
        if self.lifecycle == SessionLifecycle::Cancelled {
            return Err(AsrError::Cancelled);
        }
        if self.lifecycle == SessionLifecycle::Finished {
            return Err(AsrError::AlreadyFinished);
        }
        self.ensure_started()?;
        self.flush_pending_chunk(false)?;
        self.pending_samples.clear();
        self.pending_samples.extend_from_slice(samples);
        self.pump_io()
    }

    fn finish(&mut self) -> Result<(), AsrError> {
        if self.lifecycle == SessionLifecycle::Cancelled {
            return Err(AsrError::Cancelled);
        }
        if self.lifecycle == SessionLifecycle::Finished {
            return Ok(());
        }
        self.ensure_started()?;
        self.lifecycle = SessionLifecycle::Finished;
        self.flush_pending_chunk(true)?;
        self.write_json_line(&legacy_command_streaming_finish_line())?;
        self.close_stdin();
        self.wait_for_finish()
    }

    fn cancel(&mut self) -> Result<(), AsrError> {
        if self.lifecycle == SessionLifecycle::Cancelled {
            return Ok(());
        }
        self.lifecycle = SessionLifecycle::Cancelled;
        if self.ensure_started().is_ok() {
            let _ = self.write_json_line(&serde_json::json!({"type":"cancel"}).to_string());
        }
        self.pending_samples.clear();
        self.events.clear();
        self.queue_completed();
        self.cleanup_process();
        Ok(())
    }

    fn poll_events(&mut self) -> Result<Vec<RecognitionEvent>, AsrError> {
        self.pump_io()?;
        Ok(std::mem::take(&mut self.events))
    }
}

impl Drop for LegacyCommandStreamingSession {
    fn drop(&mut self) {
        self.cleanup_process();
    }
}

#[derive(Debug)]
struct StreamingProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
    exit_status: Option<ExitStatus>,
}

impl StreamingProcess {
    fn spawn(spec: &CommandAsrSpec) -> Result<Self, AsrError> {
        let mut command = Command::new(&spec.command);
        command
            .args(&spec.args)
            .envs(&spec.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_process_group(&mut command);
        let mut child = command.spawn().map_err(|error| {
            AsrError::Backend(format!(
                "failed to launch command streaming provider `{}`: {error}",
                spec.provider_id
            ))
        })?;
        let stdin = child.stdin.take().ok_or_else(|| {
            AsrError::Backend(format!(
                "command streaming provider `{}` did not expose stdin",
                spec.provider_id
            ))
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            AsrError::Backend(format!(
                "command streaming provider `{}` did not expose stdout",
                spec.provider_id
            ))
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            AsrError::Backend(format!(
                "command streaming provider `{}` did not expose stderr",
                spec.provider_id
            ))
        })?;

        if let Err(error) = set_nonblocking(&stdin)
            .and_then(|()| set_nonblocking(&stdout))
            .and_then(|()| set_nonblocking(&stderr))
        {
            let _ = terminate_child_process_group(&mut child, CLEANUP_GRACE, CLEANUP_FORCE);
            return Err(AsrError::Backend(format!(
                "failed to configure command streaming provider `{}` pipes: {error}",
                spec.provider_id
            )));
        }

        Ok(Self {
            child,
            stdin: Some(stdin),
            stdout: Some(stdout),
            stderr: Some(stderr),
            exit_status: None,
        })
    }
}

fn set_nonblocking(fd: &impl AsFd) -> std::io::Result<()> {
    let raw_flags = fcntl(fd, FcntlArg::F_GETFL).map_err(std::io::Error::from)?;
    let flags = OFlag::from_bits_truncate(raw_flags);
    fcntl(fd, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK))
        .map(|_| ())
        .map_err(std::io::Error::from)
}

fn read_nonblocking(reader: &mut impl Read, chunks: &mut Vec<Vec<u8>>) -> Result<bool, AsrError> {
    loop {
        let mut buffer = vec![0_u8; 4096];
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(true),
            Ok(read) => {
                buffer.truncate(read);
                chunks.push(buffer);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(false),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => {
                return Err(AsrError::Backend(format!(
                    "failed to read command streaming provider output: {error}"
                )));
            }
        }
    }
}

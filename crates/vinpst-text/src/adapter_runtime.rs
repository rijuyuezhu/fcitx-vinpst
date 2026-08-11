//! Runtime filesystem paths and process supervision for command text adapters.

use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{Child, ChildStderr, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use vinpst_config::LlmAdapterConfig;
use vinpst_process::{
    MAX_COMMAND_OUTPUT_BYTES, ProcessGroupSignal, configure_process_group,
    process_group_has_live_members, set_nonblocking, signal_process_group_and_child,
    terminate_child_process_group, try_wait_child_and_cleanup,
};

use crate::TextError;

const ADAPTER_PID_RECORD_VERSION: u32 = 1;
const ADAPTER_GRACEFUL_STOP_TIMEOUT: Duration = Duration::from_secs(2);
const ADAPTER_FORCE_STOP_TIMEOUT: Duration = Duration::from_secs(3);
const ADAPTER_STOP_POLL_INTERVAL: Duration = Duration::from_millis(25);
const ADAPTER_STARTUP_PROBE: Duration = Duration::from_millis(250);

/// Returns the default text adapter runtime directory.
///
/// On Linux desktop sessions this should be rooted under `XDG_RUNTIME_DIR`.
/// Tests can pass an explicit value to keep the path deterministic; production
/// callers can use [`AdapterRuntimePaths::for_current_user`].
#[must_use]
pub fn default_adapter_runtime_dir(xdg_runtime_dir: Option<&Path>) -> PathBuf {
    let base = xdg_runtime_dir
        .filter(|path| !path.as_os_str().is_empty())
        .map_or_else(std::env::temp_dir, Path::to_path_buf);
    base.join("vinpst").join("adapters")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct AdapterPidRecord {
    version: u32,
    pid: u32,
    start_time_ticks: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StoredAdapterPid {
    Legacy(u32),
    Fingerprinted(AdapterPidRecord),
}

/// Filesystem layout helper for supervised text adapter runtime state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterRuntimePaths {
    runtime_dir: PathBuf,
}

impl AdapterRuntimePaths {
    /// Creates runtime paths rooted at `runtime_dir`.
    #[must_use]
    pub fn new(runtime_dir: impl Into<PathBuf>) -> Self {
        Self {
            runtime_dir: runtime_dir.into(),
        }
    }

    /// Creates runtime paths for the current user session.
    #[must_use]
    pub fn for_current_user() -> Self {
        let xdg_runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
        Self::new(default_adapter_runtime_dir(xdg_runtime_dir.as_deref()))
    }

    /// Returns the runtime directory.
    #[must_use]
    pub fn runtime_dir(&self) -> &Path {
        &self.runtime_dir
    }

    /// Builds a path for an adapter pid file using a safe adapter id.
    pub fn pid_path(&self, adapter_id: &str) -> Result<PathBuf, TextError> {
        Ok(self.runtime_dir.join(adapter_pid_file_name(adapter_id)?))
    }

    /// Writes a legacy PID-only file and returns its path.
    ///
    /// New supervised starts use a fingerprinted record instead. This method is
    /// retained for compatibility tests and migration tooling.
    pub fn write_pid(&self, adapter_id: &str, pid: u32) -> Result<PathBuf, TextError> {
        let path = self.pid_path(adapter_id)?;
        self.ensure_runtime_dir()?;
        fs::write(&path, pid.to_string()).map_err(|error| {
            TextError::AdapterRuntimeIo(format!(
                "failed to write adapter pid file `{}`: {error}",
                path.display()
            ))
        })?;
        Ok(path)
    }

    /// Reads either a fingerprinted or legacy PID file. Missing files are `None`.
    pub fn read_pid(&self, adapter_id: &str) -> Result<Option<u32>, TextError> {
        Ok(self
            .read_stored_pid(adapter_id)?
            .map(|stored| match stored {
                StoredAdapterPid::Legacy(pid) => pid,
                StoredAdapterPid::Fingerprinted(record) => record.pid,
            }))
    }

    /// Removes an adapter pid file. Missing files return `Ok(false)`.
    pub fn remove_pid(&self, adapter_id: &str) -> Result<bool, TextError> {
        let path = self.pid_path(adapter_id)?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
            Err(error) => Err(TextError::AdapterRuntimeIo(format!(
                "failed to remove adapter pid file `{}`: {error}",
                path.display()
            ))),
        }
    }

    fn ensure_runtime_dir(&self) -> Result<(), TextError> {
        fs::create_dir_all(&self.runtime_dir).map_err(|error| {
            TextError::AdapterRuntimeIo(format!(
                "failed to create adapter runtime directory `{}`: {error}",
                self.runtime_dir.display()
            ))
        })
    }

    fn write_pid_record(
        &self,
        adapter_id: &str,
        record: AdapterPidRecord,
    ) -> Result<PathBuf, TextError> {
        let path = self.pid_path(adapter_id)?;
        self.ensure_runtime_dir()?;
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                TextError::AdapterRuntimeIo("adapter pid file name is not valid UTF-8".to_owned())
            })?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let temporary_path = self
            .runtime_dir
            .join(format!(".{file_name}.tmp-{}-{nonce}", std::process::id()));
        let bytes = serde_json::to_vec(&record).map_err(|error| {
            TextError::AdapterRuntimeIo(format!(
                "failed to encode adapter pid record for `{adapter_id}`: {error}"
            ))
        })?;
        let write_result = (|| -> Result<(), std::io::Error> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary_path)?;
            file.write_all(&bytes)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            fs::rename(&temporary_path, &path)
        })();
        if let Err(error) = write_result {
            let _ = fs::remove_file(&temporary_path);
            return Err(TextError::AdapterRuntimeIo(format!(
                "failed to write adapter pid file `{}`: {error}",
                path.display()
            )));
        }
        Ok(path)
    }

    fn read_stored_pid(&self, adapter_id: &str) -> Result<Option<StoredAdapterPid>, TextError> {
        let path = self.pid_path(adapter_id)?;
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(TextError::AdapterRuntimeIo(format!(
                    "failed to read adapter pid file `{}`: {error}",
                    path.display()
                )));
            }
        };
        let trimmed = content.trim();
        if trimmed.starts_with('{') {
            let record = serde_json::from_str::<AdapterPidRecord>(trimmed).map_err(|error| {
                TextError::InvalidAdapterPid(format!(
                    "invalid fingerprinted pid record in `{}`: {error}",
                    path.display()
                ))
            })?;
            if record.version != ADAPTER_PID_RECORD_VERSION
                || record.pid == 0
                || record.start_time_ticks == 0
            {
                return Err(TextError::InvalidAdapterPid(format!(
                    "unsupported or incomplete pid record in `{}`",
                    path.display()
                )));
            }
            return Ok(Some(StoredAdapterPid::Fingerprinted(record)));
        }
        trimmed
            .parse::<u32>()
            .map(StoredAdapterPid::Legacy)
            .map(Some)
            .map_err(|error| {
                TextError::InvalidAdapterPid(format!(
                    "invalid pid in `{}`: {error}",
                    path.display()
                ))
            })
    }
}

/// Validates that an adapter id can be used safely for runtime files.
pub fn validate_adapter_id(adapter_id: &str) -> Result<(), TextError> {
    if adapter_id.is_empty()
        || adapter_id == "."
        || adapter_id == ".."
        || adapter_id.contains('/')
        || adapter_id.contains('\\')
    {
        return Err(TextError::InvalidAdapterId(adapter_id.to_owned()));
    }
    Ok(())
}

fn adapter_pid_file_name(adapter_id: &str) -> Result<String, TextError> {
    validate_adapter_id(adapter_id)?;
    Ok(format!("{adapter_id}.pid"))
}

/// Command specification for a supervised text adapter process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterProcessSpec {
    /// Stable adapter id.
    pub id: String,
    /// Executable path or program name.
    pub command: String,
    /// Command-line arguments.
    pub args: Vec<String>,
    /// Environment variables added to the child process.
    pub env: std::collections::HashMap<String, String>,
    /// Optional child working directory.
    pub working_dir: Option<String>,
}

impl AdapterProcessSpec {
    /// Builds a process spec from typed adapter config.
    #[must_use]
    pub fn from_config(config: &LlmAdapterConfig) -> Self {
        Self {
            id: config.id.clone(),
            command: config.command.clone(),
            args: config.args.clone(),
            env: config.env.clone(),
            working_dir: config.working_dir.clone(),
        }
    }
}

/// A started adapter child process whose fingerprinted pid file has been written.
#[derive(Debug)]
pub struct StartedAdapterProcess {
    /// Stable adapter id.
    pub id: String,
    /// Child process id and process-group id.
    pub pid: u32,
    /// Linux process start-time fingerprint from `/proc/<pid>/stat`.
    pub start_time_ticks: u64,
    /// Path to the written pid file.
    pub pid_path: PathBuf,
    /// Running child process handle.
    pub child: Child,
    stderr: Option<ChildStderr>,
    stderr_buffer: Vec<u8>,
    stderr_line_truncated: bool,
}

impl StartedAdapterProcess {
    /// Drains currently available stderr into trimmed notification lines.
    pub fn drain_stderr_lines(&mut self, flush_partial: bool) -> Result<Vec<String>, TextError> {
        drain_adapter_stderr(
            &self.id,
            &mut self.stderr,
            &mut self.stderr_buffer,
            &mut self.stderr_line_truncated,
            flush_partial,
        )
    }

    /// Reaps an exited direct child after cleaning any remaining descendants.
    pub fn try_wait_and_cleanup(&mut self) -> Result<Option<ExitStatus>, TextError> {
        try_wait_child_and_cleanup(&mut self.child).map_err(|error| {
            TextError::AdapterRuntimeIo(format!(
                "failed to inspect text adapter `{}` pid {}: {error}",
                self.id, self.pid
            ))
        })
    }
}

/// Result of asking the supervisor to stop an adapter process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterStopOutcome {
    /// No safe matching process existed, so no process was targeted.
    NotRunning,
    /// The matching process group was terminated and the pid file removed.
    Stopped {
        /// Process id read from the pid file or tracked child.
        pid: u32,
    },
}

/// Stops a text adapter from a fingerprinted pid file.
///
/// Legacy PID-only files are removed without signaling because they cannot
/// distinguish the original process from an unrelated process that reused the
/// same PID.
pub fn stop_adapter_process(
    adapter_id: &str,
    paths: &AdapterRuntimePaths,
) -> Result<AdapterStopOutcome, TextError> {
    let Some(stored) = paths.read_stored_pid(adapter_id)? else {
        return Ok(AdapterStopOutcome::NotRunning);
    };
    let StoredAdapterPid::Fingerprinted(record) = stored else {
        paths.remove_pid(adapter_id)?;
        return Ok(AdapterStopOutcome::NotRunning);
    };
    match process_record_state(record)? {
        ProcessRecordState::Running => {}
        ProcessRecordState::Missing
        | ProcessRecordState::Exited
        | ProcessRecordState::Mismatched => {
            paths.remove_pid(adapter_id)?;
            return Ok(AdapterStopOutcome::NotRunning);
        }
    }

    signal_adapter_group(adapter_id, record.pid, ProcessGroupSignal::Terminate)?;
    match wait_for_record_cleanup(record, ADAPTER_GRACEFUL_STOP_TIMEOUT)? {
        RecordWaitOutcome::Cleaned => {
            paths.remove_pid(adapter_id)?;
            return Ok(AdapterStopOutcome::Stopped { pid: record.pid });
        }
        RecordWaitOutcome::Mismatched => {
            paths.remove_pid(adapter_id)?;
            return Ok(AdapterStopOutcome::NotRunning);
        }
        RecordWaitOutcome::TimedOut => {}
    }

    signal_adapter_group(adapter_id, record.pid, ProcessGroupSignal::Kill)?;
    match wait_for_record_cleanup(record, ADAPTER_FORCE_STOP_TIMEOUT)? {
        RecordWaitOutcome::Cleaned => {
            paths.remove_pid(adapter_id)?;
            Ok(AdapterStopOutcome::Stopped { pid: record.pid })
        }
        RecordWaitOutcome::Mismatched => {
            paths.remove_pid(adapter_id)?;
            Ok(AdapterStopOutcome::NotRunning)
        }
        RecordWaitOutcome::TimedOut => Err(TextError::AdapterRuntimeIo(format!(
            "text adapter `{adapter_id}` pid {} did not exit after force kill",
            record.pid
        ))),
    }
}

/// Stops a child currently owned by the running daemon.
pub fn stop_started_adapter_process(
    process: &mut StartedAdapterProcess,
    paths: &AdapterRuntimePaths,
) -> Result<AdapterStopOutcome, TextError> {
    terminate_child_process_group(
        &mut process.child,
        ADAPTER_GRACEFUL_STOP_TIMEOUT,
        ADAPTER_FORCE_STOP_TIMEOUT,
    )
    .map_err(|error| {
        TextError::AdapterRuntimeIo(format!(
            "failed to stop text adapter `{}` pid {}: {error}",
            process.id, process.pid
        ))
    })?;
    paths.remove_pid(&process.id)?;
    Ok(AdapterStopOutcome::Stopped { pid: process.pid })
}

fn expand_adapter_candidate_path(
    candidate: &str,
    current_dir: &Path,
    home_dir: Option<&Path>,
) -> Option<PathBuf> {
    if candidate.is_empty() {
        return None;
    }
    let mut path = if let Some(suffix) = candidate.strip_prefix('~') {
        let home_dir = home_dir?;
        home_dir.join(suffix.strip_prefix('/').unwrap_or(suffix))
    } else {
        PathBuf::from(candidate)
    };
    if path.is_relative() {
        path = current_dir.join(path);
    }
    Some(path)
}

pub(crate) fn infer_adapter_working_dir(
    spec: &AdapterProcessSpec,
    current_dir: &Path,
    home_dir: Option<&Path>,
) -> PathBuf {
    for candidate in spec.args.iter().chain(std::iter::once(&spec.command)) {
        let Some(path) = expand_adapter_candidate_path(candidate, current_dir, home_dir) else {
            continue;
        };
        if path.is_file()
            && let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            return parent.to_path_buf();
        }
    }
    current_dir.to_path_buf()
}

/// Starts a text adapter process and writes a fingerprinted pid file.
pub fn start_adapter_process(
    spec: &AdapterProcessSpec,
    paths: &AdapterRuntimePaths,
) -> Result<StartedAdapterProcess, TextError> {
    prepare_adapter_pid_slot(&spec.id, paths)?;
    paths.ensure_runtime_dir()?;
    let mut command = Command::new(&spec.command);
    configure_process_group(&mut command);
    command
        .args(&spec.args)
        .envs(&spec.env)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let inferred_working_dir;
    if let Some(working_dir) = &spec.working_dir {
        command.current_dir(working_dir);
    } else if let Ok(current_dir) = std::env::current_dir() {
        inferred_working_dir = infer_adapter_working_dir(
            spec,
            &current_dir,
            std::env::var_os("HOME").as_deref().map(Path::new),
        );
        command.current_dir(&inferred_working_dir);
    }

    let mut child = command.spawn().map_err(|error| {
        TextError::AdapterFailed(format!(
            "failed to spawn text adapter `{}`: {error}",
            spec.id
        ))
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        TextError::AdapterRuntimeIo(format!("text adapter `{}` did not expose stderr", spec.id))
    })?;
    if let Err(error) = set_nonblocking(&stderr) {
        let _ = terminate_child_process_group(
            &mut child,
            ADAPTER_GRACEFUL_STOP_TIMEOUT,
            ADAPTER_FORCE_STOP_TIMEOUT,
        );
        return Err(TextError::AdapterRuntimeIo(format!(
            "failed to monitor text adapter `{}` stderr: {error}",
            spec.id
        )));
    }
    let pid = child.id();
    let mut stderr = Some(stderr);
    let mut stderr_buffer = Vec::new();
    let mut stderr_line_truncated = false;
    probe_adapter_startup(
        &spec.id,
        &mut child,
        &mut stderr,
        &mut stderr_buffer,
        &mut stderr_line_truncated,
    )?;
    let Some(identity) = read_process_identity(pid)? else {
        let lines = drain_adapter_stderr(
            &spec.id,
            &mut stderr,
            &mut stderr_buffer,
            &mut stderr_line_truncated,
            true,
        )?;
        let _ = signal_process_group_and_child(pid, ProcessGroupSignal::Kill);
        let _ = child.wait();
        if !lines.is_empty() {
            return Err(TextError::AdapterFailed(lines.join("\n")));
        }
        return Err(TextError::AdapterRuntimeIo(format!(
            "text adapter `{}` pid {pid} disappeared before supervision",
            spec.id
        )));
    };

    let record = AdapterPidRecord {
        version: ADAPTER_PID_RECORD_VERSION,
        pid,
        start_time_ticks: identity.start_time_ticks,
    };
    let pid_path = match paths.write_pid_record(&spec.id, record) {
        Ok(path) => path,
        Err(error) => {
            let _ = signal_process_group_and_child(pid, ProcessGroupSignal::Kill);
            let _ = child.wait();
            return Err(error);
        }
    };

    Ok(StartedAdapterProcess {
        id: spec.id.clone(),
        pid,
        start_time_ticks: identity.start_time_ticks,
        pid_path,
        child,
        stderr,
        stderr_buffer,
        stderr_line_truncated,
    })
}

fn probe_adapter_startup(
    adapter_id: &str,
    child: &mut Child,
    stderr: &mut Option<ChildStderr>,
    stderr_buffer: &mut Vec<u8>,
    stderr_line_truncated: &mut bool,
) -> Result<(), TextError> {
    let startup_deadline = Instant::now() + ADAPTER_STARTUP_PROBE;
    loop {
        match try_wait_child_and_cleanup(child) {
            Ok(Some(_status)) => {
                let lines = drain_adapter_stderr(
                    adapter_id,
                    stderr,
                    stderr_buffer,
                    stderr_line_truncated,
                    true,
                )?;
                let message = if lines.is_empty() {
                    "adapter exited immediately".to_owned()
                } else {
                    lines.join("\n")
                };
                return Err(TextError::AdapterFailed(message));
            }
            Ok(None) => {}
            Err(error) => {
                let _ = terminate_child_process_group(
                    child,
                    ADAPTER_GRACEFUL_STOP_TIMEOUT,
                    ADAPTER_FORCE_STOP_TIMEOUT,
                );
                return Err(TextError::AdapterRuntimeIo(format!(
                    "failed to probe text adapter `{adapter_id}` startup: {error}"
                )));
            }
        }
        if Instant::now() >= startup_deadline {
            return Ok(());
        }
        thread::sleep(ADAPTER_STOP_POLL_INTERVAL);
    }
}

fn drain_adapter_stderr(
    adapter_id: &str,
    stderr: &mut Option<ChildStderr>,
    buffer: &mut Vec<u8>,
    line_truncated: &mut bool,
    flush_partial: bool,
) -> Result<Vec<String>, TextError> {
    let mut lines = Vec::new();
    let mut reached_eof = false;
    if let Some(stderr) = stderr.as_mut() {
        loop {
            let mut chunk = [0_u8; 4096];
            match stderr.read(&mut chunk) {
                Ok(0) => {
                    reached_eof = true;
                    break;
                }
                Ok(read) => append_stderr_chunk(&chunk[..read], buffer, line_truncated, &mut lines),
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => {
                    return Err(TextError::AdapterRuntimeIo(format!(
                        "failed to read text adapter `{adapter_id}` stderr: {error}"
                    )));
                }
            }
        }
    }
    if reached_eof {
        *stderr = None;
    }
    if flush_partial || reached_eof {
        push_stderr_line(buffer, line_truncated, &mut lines);
    }
    Ok(lines)
}

fn append_stderr_chunk(
    chunk: &[u8],
    buffer: &mut Vec<u8>,
    line_truncated: &mut bool,
    lines: &mut Vec<String>,
) {
    for byte in chunk {
        if *byte == b'\n' {
            push_stderr_line(buffer, line_truncated, lines);
        } else if buffer.len() < MAX_COMMAND_OUTPUT_BYTES {
            buffer.push(*byte);
        } else {
            *line_truncated = true;
        }
    }
}

fn push_stderr_line(buffer: &mut Vec<u8>, line_truncated: &mut bool, lines: &mut Vec<String>) {
    let mut line = String::from_utf8_lossy(buffer)
        .trim_matches(|ch: char| ch.is_ascii_whitespace())
        .to_owned();
    if *line_truncated {
        line.push('…');
    }
    if !line.is_empty() {
        lines.push(line);
    }
    buffer.clear();
    *line_truncated = false;
}

fn prepare_adapter_pid_slot(
    adapter_id: &str,
    paths: &AdapterRuntimePaths,
) -> Result<(), TextError> {
    let Some(stored) = paths.read_stored_pid(adapter_id)? else {
        return Ok(());
    };
    match stored {
        StoredAdapterPid::Legacy(pid) => Err(TextError::InvalidAdapterPid(format!(
            "legacy PID-only record for `{adapter_id}` (pid {pid}) must be cleared with stop before start"
        ))),
        StoredAdapterPid::Fingerprinted(record) => match process_record_state(record)? {
            ProcessRecordState::Running => {
                Err(TextError::AdapterAlreadyRunning(adapter_id.to_owned()))
            }
            ProcessRecordState::Exited
            | ProcessRecordState::Missing
            | ProcessRecordState::Mismatched => {
                paths.remove_pid(adapter_id)?;
                Ok(())
            }
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProcessIdentity {
    state: char,
    start_time_ticks: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessRecordState {
    Running,
    Exited,
    Missing,
    Mismatched,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecordWaitOutcome {
    Cleaned,
    Mismatched,
    TimedOut,
}

fn read_process_identity(pid: u32) -> Result<Option<ProcessIdentity>, TextError> {
    let path = PathBuf::from(format!("/proc/{pid}/stat"));
    let stat = match fs::read_to_string(&path) {
        Ok(stat) => stat,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(TextError::AdapterRuntimeIo(format!(
                "failed to inspect adapter process pid {pid}: {error}"
            )));
        }
    };
    let closing_paren = stat.rfind(')').ok_or_else(|| {
        TextError::AdapterRuntimeIo(format!("adapter process pid {pid} has malformed proc stat"))
    })?;
    let fields: Vec<_> = stat[closing_paren + 1..].split_whitespace().collect();
    let state = fields
        .first()
        .and_then(|value| value.chars().next())
        .ok_or_else(|| {
            TextError::AdapterRuntimeIo(format!("adapter process pid {pid} has missing proc state"))
        })?;
    let start_time_ticks = fields
        .get(19)
        .ok_or_else(|| {
            TextError::AdapterRuntimeIo(format!(
                "adapter process pid {pid} has missing proc start time"
            ))
        })?
        .parse::<u64>()
        .map_err(|error| {
            TextError::AdapterRuntimeIo(format!(
                "adapter process pid {pid} has invalid proc start time: {error}"
            ))
        })?;
    Ok(Some(ProcessIdentity {
        state,
        start_time_ticks,
    }))
}

fn process_record_state(record: AdapterPidRecord) -> Result<ProcessRecordState, TextError> {
    let Some(identity) = read_process_identity(record.pid)? else {
        return Ok(ProcessRecordState::Missing);
    };
    if identity.start_time_ticks != record.start_time_ticks {
        return Ok(ProcessRecordState::Mismatched);
    }
    if matches!(identity.state, 'Z' | 'X') {
        Ok(ProcessRecordState::Exited)
    } else {
        Ok(ProcessRecordState::Running)
    }
}

fn wait_for_record_cleanup(
    record: AdapterPidRecord,
    timeout: Duration,
) -> Result<RecordWaitOutcome, TextError> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match process_record_state(record)? {
            ProcessRecordState::Missing | ProcessRecordState::Exited => {
                if !process_group_has_live_members(record.pid).map_err(|error| {
                    TextError::AdapterRuntimeIo(format!(
                        "failed to inspect text adapter process group {}: {error}",
                        record.pid
                    ))
                })? {
                    return Ok(RecordWaitOutcome::Cleaned);
                }
            }
            ProcessRecordState::Mismatched => return Ok(RecordWaitOutcome::Mismatched),
            ProcessRecordState::Running => {}
        }
        if std::time::Instant::now() >= deadline {
            return Ok(RecordWaitOutcome::TimedOut);
        }
        thread::sleep(ADAPTER_STOP_POLL_INTERVAL);
    }
}

fn signal_adapter_group(
    adapter_id: &str,
    pid: u32,
    signal: ProcessGroupSignal,
) -> Result<(), TextError> {
    match signal_process_group_and_child(pid, signal) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(TextError::AdapterRuntimeIo(format!(
            "failed to signal text adapter `{adapter_id}` pid {pid}: {error}"
        ))),
    }
}

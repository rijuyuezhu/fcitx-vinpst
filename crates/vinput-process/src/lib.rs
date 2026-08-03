//! Shared supervision for bounded command helper processes.

#![cfg(unix)]

#[cfg(target_os = "linux")]
use std::fs;
use std::{
    io::{self, Read},
    os::unix::process::CommandExt,
    process::{Child, ChildStdin, Command, Output, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

#[cfg(target_os = "linux")]
use nix::{
    errno::Errno,
    sys::wait::{Id, WaitPidFlag, WaitStatus, waitid},
};
use nix::{
    sys::signal::{Signal, kill, killpg},
    unistd::Pid,
};

/// Maximum bytes retained independently from command stdout and stderr.
pub const MAX_COMMAND_OUTPUT_BYTES: usize = 1024 * 1024;

/// Signal used for supervised helper process groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessGroupSignal {
    /// Request graceful termination.
    Terminate,
    /// Force immediate termination.
    Kill,
}

const OUTPUT_OK: u8 = 0;
const STDOUT_TOO_LARGE: u8 = 1;
const STDERR_TOO_LARGE: u8 = 2;
const STDOUT_READ_FAILED: u8 = 3;
const STDERR_READ_FAILED: u8 = 4;

/// Completed command output plus any error encountered while writing stdin.
#[derive(Debug)]
pub struct PipedCommandOutput {
    /// Exit status and bounded stdout/stderr bytes.
    pub output: Output,
    /// Stdin write failure, retained so callers can prefer a non-zero exit error.
    pub stdin_error: Option<io::Error>,
}

/// Failures produced while supervising a command helper.
#[derive(Debug, thiserror::Error)]
pub enum PipedCommandError {
    /// The helper process could not be spawned.
    #[error("failed to spawn process")]
    Spawn(#[source] io::Error),
    /// The timeout watchdog thread could not be started.
    #[error("failed to start timeout watchdog")]
    WatchdogStart(#[source] io::Error),
    /// The stdout/stderr reader threads could not be started.
    #[error("failed to capture process output")]
    OutputCaptureStart(#[source] io::Error),
    /// The configured stdin pipe was unexpectedly unavailable.
    #[error("stdin pipe is unavailable")]
    StdinUnavailable,
    /// The helper exceeded its configured deadline.
    #[error("process timed out after {timeout_ms} ms")]
    TimedOut {
        /// Effective timeout in milliseconds.
        timeout_ms: u64,
    },
    /// Stdout exceeded the per-stream safety limit.
    #[error("stdout exceeds {limit}-byte limit")]
    StdoutTooLarge {
        /// Maximum retained stdout bytes.
        limit: usize,
    },
    /// Stderr exceeded the per-stream safety limit.
    #[error("stderr exceeds {limit}-byte limit")]
    StderrTooLarge {
        /// Maximum retained stderr bytes.
        limit: usize,
    },
    /// Reading stdout failed before completion.
    #[error("failed to read stdout")]
    StdoutRead,
    /// Reading stderr failed before completion.
    #[error("failed to read stderr")]
    StderrRead,
    /// Waiting for the direct child failed.
    #[error("failed to wait for process")]
    Wait(#[source] io::Error),
    /// The watchdog thread panicked.
    #[error("timeout watchdog panicked")]
    WatchdogPanicked,
    /// The stdout reader thread panicked.
    #[error("stdout reader panicked")]
    StdoutReaderPanicked,
    /// The stderr reader thread panicked.
    #[error("stderr reader panicked")]
    StderrReaderPanicked,
}

/// Configures a command so its child becomes leader of a new process group.
pub fn configure_process_group(command: &mut Command) {
    command.process_group(0);
}

/// Returns whether a process group currently exists.
pub fn process_group_exists(process_group_id: u32) -> io::Result<bool> {
    let raw_id = i32::try_from(process_group_id)
        .map_err(|_| io::Error::other("process group id exceeds signed range"))?;
    match killpg(Pid::from_raw(raw_id), None) {
        Ok(()) | Err(nix::errno::Errno::EPERM) => Ok(true),
        Err(nix::errno::Errno::ESRCH) => Ok(false),
        Err(error) => Err(io::Error::from_raw_os_error(error as i32)),
    }
}

/// Returns whether a process group still has at least one non-zombie member.
///
/// Linux keeps an exited process in its process group until its parent reaps
/// the zombie. A signal-zero group probe therefore cannot distinguish useful
/// work from a group containing only unreaped zombies. On Linux this helper
/// scans `/proc` after the group probe and treats zombie-only groups as clean.
/// Other Unix targets conservatively fall back to group existence.
pub fn process_group_has_live_members(process_group_id: u32) -> io::Result<bool> {
    if !process_group_exists(process_group_id)? {
        return Ok(false);
    }

    #[cfg(not(target_os = "linux"))]
    return Ok(true);

    #[cfg(target_os = "linux")]
    {
        let mut saw_member = false;
        for entry in fs::read_dir("/proc")? {
            let Ok(entry) = entry else {
                continue;
            };
            let Some(file_name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if file_name.parse::<u32>().is_err() {
                continue;
            }
            let stat = match fs::read_to_string(entry.path().join("stat")) {
                Ok(stat) => stat,
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
                    ) =>
                {
                    continue;
                }
                Err(error) => return Err(error),
            };
            let Some(closing_paren) = stat.rfind(')') else {
                continue;
            };
            let mut fields = stat[closing_paren + 1..].split_whitespace();
            let Some(state) = fields.next().and_then(|value| value.chars().next()) else {
                continue;
            };
            let Some(_parent_pid) = fields.next() else {
                continue;
            };
            let Some(group_id) = fields.next().and_then(|value| value.parse::<u32>().ok()) else {
                continue;
            };
            if group_id != process_group_id {
                continue;
            }
            saw_member = true;
            if !matches!(state, 'Z' | 'X') {
                return Ok(true);
            }
        }

        if saw_member {
            Ok(false)
        } else {
            process_group_exists(process_group_id)
        }
    }
}

/// Sends a signal to a supervised process group, falling back to its direct child.
///
/// The group is authoritative for commands created by [`configure_process_group`].
/// The direct-child fallback covers a missing group without sending a second
/// signal after a successful group operation. `NotFound` means both targets are
/// absent.
pub fn signal_process_group_and_child(
    process_group_id: u32,
    signal: ProcessGroupSignal,
) -> io::Result<()> {
    let raw_id = i32::try_from(process_group_id)
        .map_err(|_| io::Error::other("process group id exceeds signed range"))?;
    let process_group = Pid::from_raw(raw_id);
    let signal = match signal {
        ProcessGroupSignal::Terminate => Signal::SIGTERM,
        ProcessGroupSignal::Kill => Signal::SIGKILL,
    };
    let group_error = match killpg(process_group, signal) {
        Ok(()) => return Ok(()),
        Err(error) => error,
    };
    let child_error = match kill(process_group, signal) {
        Ok(()) => return Ok(()),
        Err(error) => error,
    };
    let error = if group_error == nix::errno::Errno::ESRCH {
        child_error
    } else {
        group_error
    };
    Err(io::Error::from_raw_os_error(error as i32))
}

/// Returns whether the direct child has exited, cleaning descendants before reap.
///
/// On Linux, `waitid(WNOWAIT | WNOHANG)` reserves the direct child's PID/PGID
/// until descendants are killed and `Child::wait` performs the final reap.
pub fn try_wait_child_and_cleanup(
    child: &mut Child,
) -> io::Result<Option<std::process::ExitStatus>> {
    try_wait_direct_child_and_cleanup(child, child.id())
}

/// Gracefully terminates a tracked child process group, then force-kills it.
pub fn terminate_child_process_group(
    child: &mut Child,
    graceful_timeout: Duration,
    force_timeout: Duration,
) -> io::Result<()> {
    let process_group_id = child.id();
    match signal_process_group_and_child(process_group_id, ProcessGroupSignal::Terminate) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    if wait_for_child_cleanup(child, graceful_timeout)? {
        return Ok(());
    }
    match signal_process_group_and_child(process_group_id, ProcessGroupSignal::Kill) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    if wait_for_child_cleanup(child, force_timeout)? {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "process group did not exit after force kill",
        ))
    }
}

fn wait_for_child_cleanup(child: &mut Child, timeout: Duration) -> io::Result<bool> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if try_wait_child_and_cleanup(child)?.is_some() {
            return Ok(true);
        }
        if std::time::Instant::now() >= deadline {
            return Ok(false);
        }
        thread::sleep(Duration::from_millis(25));
    }
}

/// Runs one piped helper with process-group cleanup and bounded output capture.
///
/// The watchdog, when configured, starts immediately after spawn, so the
/// deadline covers stdin writing, helper execution, and output recovery. The
/// direct child and any descendants are terminated on timeout, output failure,
/// or after the direct child exits. Stdout and stderr are retained independently
/// up to [`MAX_COMMAND_OUTPUT_BYTES`].
pub fn run_piped_command(
    command: &mut Command,
    timeout_ms: Option<u64>,
    write_stdin: impl FnOnce(&mut ChildStdin) -> io::Result<()>,
) -> Result<PipedCommandOutput, PipedCommandError> {
    configure_process_group(command);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(PipedCommandError::Spawn)?;
    let process_group_id = child.id();
    let watchdog = match timeout_ms
        .map(|timeout_ms| start_watchdog(process_group_id, timeout_ms))
        .transpose()
    {
        Ok(watchdog) => watchdog,
        Err(error) => {
            kill_process_group(process_group_id);
            let _ = child.wait();
            return Err(PipedCommandError::WatchdogStart(error));
        }
    };
    let output_readers = match start_output_readers(&mut child, process_group_id) {
        Ok(readers) => readers,
        Err(error) => {
            kill_process_group(process_group_id);
            let _ = child.wait();
            let _ = finish_watchdog(watchdog);
            return Err(PipedCommandError::OutputCaptureStart(error));
        }
    };
    let Some(mut stdin) = child.stdin.take() else {
        kill_process_group(process_group_id);
        let _ = wait_for_output(child, watchdog, output_readers);
        return Err(PipedCommandError::StdinUnavailable);
    };
    let stdin_error = write_stdin(&mut stdin).err();
    drop(stdin);

    let output = wait_for_output(child, watchdog, output_readers)?;
    Ok(PipedCommandOutput {
        output,
        stdin_error,
    })
}

struct Watchdog {
    completed_sender: mpsc::Sender<()>,
    timed_out: Arc<AtomicBool>,
    thread: thread::JoinHandle<()>,
    timeout_ms: u64,
}

struct OutputReaders {
    stdout: thread::JoinHandle<Vec<u8>>,
    stderr: thread::JoinHandle<Vec<u8>>,
    failure: Arc<AtomicU8>,
}

fn start_watchdog(process_group_id: u32, timeout_ms: u64) -> io::Result<Watchdog> {
    let (completed_sender, completed_receiver) = mpsc::channel();
    let timed_out = Arc::new(AtomicBool::new(false));
    let watchdog_timed_out = Arc::clone(&timed_out);
    let thread = thread::Builder::new()
        .name("vinput-command-watchdog".to_owned())
        .spawn(move || {
            match completed_receiver.recv_timeout(Duration::from_millis(timeout_ms)) {
                Ok(()) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    watchdog_timed_out.store(true, Ordering::Release);
                    kill_process_group(process_group_id);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    kill_process_group(process_group_id);
                }
            }
        })?;
    Ok(Watchdog {
        completed_sender,
        timed_out,
        thread,
        timeout_ms,
    })
}

fn start_output_readers(child: &mut Child, process_group_id: u32) -> io::Result<OutputReaders> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("stdout pipe is unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("stderr pipe is unavailable"))?;
    let failure = Arc::new(AtomicU8::new(OUTPUT_OK));
    let stdout_failure = Arc::clone(&failure);
    let stdout = thread::Builder::new()
        .name("vinput-command-stdout".to_owned())
        .spawn(move || {
            read_output(
                stdout,
                process_group_id,
                stdout_failure.as_ref(),
                STDOUT_TOO_LARGE,
                STDOUT_READ_FAILED,
            )
        })?;
    let stderr_failure = Arc::clone(&failure);
    let stderr = match thread::Builder::new()
        .name("vinput-command-stderr".to_owned())
        .spawn(move || {
            read_output(
                stderr,
                process_group_id,
                stderr_failure.as_ref(),
                STDERR_TOO_LARGE,
                STDERR_READ_FAILED,
            )
        }) {
        Ok(stderr) => stderr,
        Err(error) => {
            kill_process_group(process_group_id);
            let _ = stdout.join();
            return Err(error);
        }
    };
    Ok(OutputReaders {
        stdout,
        stderr,
        failure,
    })
}

fn read_output(
    reader: impl Read,
    process_group_id: u32,
    failure: &AtomicU8,
    too_large_code: u8,
    read_failed_code: u8,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    match reader
        .take(u64::try_from(MAX_COMMAND_OUTPUT_BYTES).expect("1 MiB fits u64") + 1)
        .read_to_end(&mut bytes)
    {
        Ok(_) if bytes.len() > MAX_COMMAND_OUTPUT_BYTES => {
            record_output_failure(failure, too_large_code, process_group_id);
            bytes.truncate(MAX_COMMAND_OUTPUT_BYTES);
        }
        Ok(_) => {}
        Err(_) => record_output_failure(failure, read_failed_code, process_group_id),
    }
    bytes
}

fn record_output_failure(failure: &AtomicU8, code: u8, process_group_id: u32) {
    if failure
        .compare_exchange(OUTPUT_OK, code, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        kill_process_group(process_group_id);
    }
}

fn wait_for_output(
    mut child: Child,
    watchdog: Option<Watchdog>,
    output_readers: OutputReaders,
) -> Result<Output, PipedCommandError> {
    let process_group_id = child.id();
    let status = loop {
        match try_wait_direct_child_and_cleanup(&mut child, process_group_id) {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(error) => break Err(error),
        }
    };
    let stdout = output_readers.stdout.join();
    let stderr = output_readers.stderr.join();
    let timed_out = finish_watchdog(watchdog)?;

    match output_readers.failure.load(Ordering::Acquire) {
        OUTPUT_OK => {}
        STDOUT_TOO_LARGE => {
            return Err(PipedCommandError::StdoutTooLarge {
                limit: MAX_COMMAND_OUTPUT_BYTES,
            });
        }
        STDERR_TOO_LARGE => {
            return Err(PipedCommandError::StderrTooLarge {
                limit: MAX_COMMAND_OUTPUT_BYTES,
            });
        }
        STDOUT_READ_FAILED => return Err(PipedCommandError::StdoutRead),
        STDERR_READ_FAILED => return Err(PipedCommandError::StderrRead),
        code => unreachable!("unknown output failure code: {code}"),
    }
    if let Some(timeout_ms) = timed_out {
        return Err(PipedCommandError::TimedOut { timeout_ms });
    }
    let stdout = stdout.map_err(|_| PipedCommandError::StdoutReaderPanicked)?;
    let stderr = stderr.map_err(|_| PipedCommandError::StderrReaderPanicked)?;
    let status = status.map_err(PipedCommandError::Wait)?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

#[cfg(target_os = "linux")]
fn try_wait_direct_child_and_cleanup(
    child: &mut Child,
    process_group_id: u32,
) -> io::Result<Option<std::process::ExitStatus>> {
    let raw_pid = i32::try_from(process_group_id)
        .map_err(|_| io::Error::other("child process id exceeds signed range"))?;
    let child_pid = Pid::from_raw(raw_pid);
    loop {
        match waitid(
            Id::Pid(child_pid),
            WaitPidFlag::WEXITED | WaitPidFlag::WNOWAIT | WaitPidFlag::WNOHANG,
        ) {
            Ok(WaitStatus::Exited(..) | WaitStatus::Signaled(..)) => break,
            Ok(WaitStatus::StillAlive) => return Ok(None),
            Err(error) if error != Errno::EINTR => {
                kill_process_group(process_group_id);
                let _ = child.wait();
                return Err(io::Error::from_raw_os_error(error as i32));
            }
            _ => {}
        }
    }
    // WNOWAIT keeps the direct child as a zombie, reserving its PID/PGID while
    // remaining descendants are terminated. Child::wait then performs the reap.
    kill_process_group(process_group_id);
    child.wait().map(Some)
}

#[cfg(not(target_os = "linux"))]
fn try_wait_direct_child_and_cleanup(
    child: &mut Child,
    process_group_id: u32,
) -> io::Result<Option<std::process::ExitStatus>> {
    let status = child.try_wait()?;
    if status.is_some() {
        kill_process_group(process_group_id);
    }
    Ok(status)
}

fn finish_watchdog(watchdog: Option<Watchdog>) -> Result<Option<u64>, PipedCommandError> {
    let Some(watchdog) = watchdog else {
        return Ok(None);
    };
    let _ = watchdog.completed_sender.send(());
    watchdog
        .thread
        .join()
        .map_err(|_| PipedCommandError::WatchdogPanicked)?;
    Ok(watchdog
        .timed_out
        .load(Ordering::Acquire)
        .then_some(watchdog.timeout_ms))
}

fn kill_process_group(process_group_id: u32) {
    let _ = signal_process_group_and_child(process_group_id, ProcessGroupSignal::Kill);
}

#[cfg(test)]
mod tests {
    use std::{io::Write, process::Command, time::Instant};
    #[cfg(target_os = "linux")]
    use std::{process::Stdio, thread, time::Duration};

    use super::{MAX_COMMAND_OUTPUT_BYTES, PipedCommandError, run_piped_command};
    #[cfg(target_os = "linux")]
    use super::{
        ProcessGroupSignal, configure_process_group, process_group_exists,
        process_group_has_live_members, signal_process_group_and_child,
    };

    #[test]
    fn captures_bounded_output_and_stdin() {
        let mut command = Command::new("sh");
        command.args(["-c", "cat; printf err >&2"]);
        let result =
            run_piped_command(&mut command, Some(1_000), |stdin| stdin.write_all(b"input"))
                .unwrap();

        assert!(result.stdin_error.is_none());
        assert!(result.output.status.success());
        assert_eq!(result.output.stdout, b"input");
        assert_eq!(result.output.stderr, b"err");
    }

    #[test]
    fn timeout_covers_helper_execution() {
        let mut command = Command::new("sh");
        command.args(["-c", "cat >/dev/null; sleep 30"]);
        let started = Instant::now();
        let error = run_piped_command(&mut command, Some(25), |_| Ok(())).unwrap_err();

        assert!(started.elapsed().as_secs() < 2);
        assert!(matches!(
            error,
            PipedCommandError::TimedOut { timeout_ms: 25 }
        ));
    }

    #[test]
    fn rejects_oversized_stdout_without_waiting_for_exit() {
        let mut command = Command::new("sh");
        command.args(["-c", "head -c 1100000 /dev/zero | tr '\\0' x; sleep 30"]);
        let error = run_piped_command(&mut command, Some(5_000), |_| Ok(())).unwrap_err();

        assert!(matches!(
            error,
            PipedCommandError::StdoutTooLarge { limit }
                if limit == MAX_COMMAND_OUTPUT_BYTES
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn zombie_only_group_has_no_live_members() {
        let mut command = Command::new("sh");
        configure_process_group(&mut command);
        command.args(["-c", "exit 0"]);
        let mut child = command.spawn().unwrap();
        let process_group_id = child.id();
        let deadline = Instant::now() + Duration::from_secs(2);

        while process_group_has_live_members(process_group_id).unwrap() && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(10));
        }
        let group_exists = process_group_exists(process_group_id).unwrap();
        let has_live_members = process_group_has_live_members(process_group_id).unwrap();
        child.wait().unwrap();

        assert!(
            group_exists,
            "unreaped zombie should keep the group visible"
        );
        assert!(!has_live_members, "zombie-only group should count as clean");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn reaped_leader_group_stays_live_while_descendant_runs() {
        let mut command = Command::new("sh");
        configure_process_group(&mut command);
        command
            .args(["-c", "sleep 30 </dev/null >/dev/null 2>&1 & echo $!"])
            .stdout(Stdio::piped());
        let child = command.spawn().unwrap();
        let process_group_id = child.id();
        let output = child.wait_with_output().unwrap();
        let descendant_pid = String::from_utf8(output.stdout)
            .unwrap()
            .trim()
            .parse::<u32>()
            .unwrap();

        let live_result = process_group_has_live_members(process_group_id);
        let _ = signal_process_group_and_child(process_group_id, ProcessGroupSignal::Kill);
        let deadline = Instant::now() + Duration::from_secs(2);
        while process_group_has_live_members(process_group_id).unwrap_or(false)
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(10));
        }

        assert!(
            live_result.unwrap(),
            "group should remain live while descendant {descendant_pid} runs"
        );
        assert!(
            !process_group_has_live_members(process_group_id).unwrap(),
            "killed descendant group should become zombie-only or disappear"
        );
    }
}

//! Shared supervision for bounded command helper processes.

#![cfg(unix)]

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
    command
        .process_group(0)
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
    let status = wait_for_direct_child_and_cleanup(&mut child, process_group_id);
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
fn wait_for_direct_child_and_cleanup(
    child: &mut Child,
    process_group_id: u32,
) -> io::Result<std::process::ExitStatus> {
    let raw_pid = i32::try_from(process_group_id)
        .map_err(|_| io::Error::other("child process id exceeds signed range"))?;
    let child_pid = Pid::from_raw(raw_pid);
    loop {
        match waitid(
            Id::Pid(child_pid),
            WaitPidFlag::WEXITED | WaitPidFlag::WNOWAIT,
        ) {
            Ok(WaitStatus::Exited(..) | WaitStatus::Signaled(..)) => break,
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
    child.wait()
}

#[cfg(not(target_os = "linux"))]
fn wait_for_direct_child_and_cleanup(
    child: &mut Child,
    process_group_id: u32,
) -> io::Result<std::process::ExitStatus> {
    let status = child.wait();
    kill_process_group(process_group_id);
    status
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
    if let Ok(process_group_id) = i32::try_from(process_group_id) {
        let process_group = Pid::from_raw(process_group_id);
        let _ = killpg(process_group, Signal::SIGKILL);
        let _ = kill(process_group, Signal::SIGKILL);
    }
}

#[cfg(test)]
mod tests {
    use std::{io::Write, process::Command, time::Instant};

    use super::{MAX_COMMAND_OUTPUT_BYTES, PipedCommandError, run_piped_command};

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
}

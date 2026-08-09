//! Best-effort default-sink output ducking through `WirePlumber`'s `wpctl`.

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

const DEFAULT_SINK: &str = "@DEFAULT_AUDIO_SINK@";
const WPCTL_TIMEOUT: Duration = Duration::from_secs(2);

/// Abstracts default-sink volume control for deterministic runtime tests.
pub(super) trait OutputVolumeControl: Send {
    /// Reads the current default-sink linear volume.
    fn read_default_sink_volume(&mut self) -> Option<f64>;

    /// Sets the current default-sink linear volume.
    fn set_default_sink_volume(&mut self, volume: f64) -> bool;
}

/// Idempotently lowers and restores the default output sink volume.
pub(super) struct OutputDucker {
    control: Box<dyn OutputVolumeControl>,
    saved_volume: Option<f64>,
}

impl OutputDucker {
    /// Creates a ducker with an injected volume-control implementation.
    pub(super) fn with_control(control: Box<dyn OutputVolumeControl>) -> Self {
        Self {
            control,
            saved_volume: None,
        }
    }

    /// Lowers the default sink to `current * scale` without blocking recording on failure.
    pub(super) fn duck(&mut self, scale: f32) {
        if self.saved_volume.is_some() {
            return;
        }
        let scale = f64::from(scale.clamp(0.0, 1.0));
        let Some(current) = self.control.read_default_sink_volume() else {
            tracing::debug!(
                "output ducking skipped because wpctl or the default sink is unavailable"
            );
            return;
        };
        let target = current * scale;
        if !self.control.set_default_sink_volume(target) {
            tracing::debug!(
                current_volume = current,
                target_volume = target,
                "output ducking skipped because setting the default sink volume failed"
            );
            return;
        }
        self.saved_volume = Some(current);
        tracing::debug!(
            current_volume = current,
            target_volume = target,
            scale,
            "ducked default output sink"
        );
    }

    /// Restores the volume saved by the most recent successful `duck` call.
    pub(super) fn restore(&mut self) {
        let Some(saved_volume) = self.saved_volume.take() else {
            return;
        };
        if self.control.set_default_sink_volume(saved_volume) {
            tracing::debug!(saved_volume, "restored default output sink");
        } else {
            tracing::warn!(saved_volume, "failed to restore default output sink");
        }
    }

    #[cfg(test)]
    pub(super) fn is_ducked(&self) -> bool {
        self.saved_volume.is_some()
    }
}

impl Default for OutputDucker {
    fn default() -> Self {
        Self::with_control(Box::new(WpctlOutputVolumeControl::default()))
    }
}

struct WpctlOutputVolumeControl {
    command: PathBuf,
    timeout: Duration,
    runner: Box<dyn WpctlCommandRunner>,
}

impl WpctlOutputVolumeControl {
    fn new(command: impl Into<PathBuf>, timeout: Duration) -> Self {
        Self::with_runner(command, timeout, Box::new(SystemWpctlCommandRunner))
    }

    fn with_runner(
        command: impl Into<PathBuf>,
        timeout: Duration,
        runner: Box<dyn WpctlCommandRunner>,
    ) -> Self {
        Self {
            command: command.into(),
            timeout,
            runner,
        }
    }

    fn run(&mut self, args: &[&str]) -> Option<String> {
        self.runner.run(&self.command, args, self.timeout)
    }
}

trait WpctlCommandRunner: Send {
    fn run(&mut self, command: &Path, args: &[&str], timeout: Duration) -> Option<String>;
}

struct SystemWpctlCommandRunner;

impl WpctlCommandRunner for SystemWpctlCommandRunner {
    fn run(&mut self, command: &Path, args: &[&str], timeout: Duration) -> Option<String> {
        let timeout_ms = u64::try_from(timeout.as_millis()).ok()?;
        let mut command = Command::new(command);
        command.args(args);
        let result =
            vinpst_process::run_piped_command(&mut command, Some(timeout_ms), |_| Ok(())).ok()?;
        if !result.output.status.success() {
            return None;
        }
        String::from_utf8(result.output.stdout).ok()
    }
}

impl Default for WpctlOutputVolumeControl {
    fn default() -> Self {
        Self::new("wpctl", WPCTL_TIMEOUT)
    }
}

impl OutputVolumeControl for WpctlOutputVolumeControl {
    fn read_default_sink_volume(&mut self) -> Option<f64> {
        let output = self.run(&["get-volume", DEFAULT_SINK])?;
        parse_wpctl_volume(&output)
    }

    fn set_default_sink_volume(&mut self, volume: f64) -> bool {
        if !volume.is_finite() || volume < 0.0 {
            return false;
        }
        let formatted = format!("{volume:.4}");
        self.run(&["set-volume", DEFAULT_SINK, &formatted])
            .is_some()
    }
}

fn parse_wpctl_volume(text: &str) -> Option<f64> {
    let (_, suffix) = text.split_once("Volume:")?;
    let token = suffix.split_whitespace().next()?;
    let value = token.parse::<f64>().ok()?;
    (value.is_finite() && value >= 0.0).then_some(value)
}

#[cfg(test)]
mod tests {
    use std::{
        os::unix::fs::PermissionsExt,
        sync::{Arc, Mutex},
        time::Instant,
    };

    use super::*;

    #[derive(Default)]
    struct FakeState {
        current: Option<f64>,
        writes: Vec<f64>,
        set_succeeds: bool,
    }

    struct FakeControl {
        state: Arc<Mutex<FakeState>>,
    }

    impl OutputVolumeControl for FakeControl {
        fn read_default_sink_volume(&mut self) -> Option<f64> {
            self.state
                .lock()
                .expect("fake volume lock poisoned")
                .current
        }

        fn set_default_sink_volume(&mut self, volume: f64) -> bool {
            let mut state = self.state.lock().expect("fake volume lock poisoned");
            state.writes.push(volume);
            state.set_succeeds
        }
    }

    fn fake_ducker(state: Arc<Mutex<FakeState>>) -> OutputDucker {
        OutputDucker::with_control(Box::new(FakeControl { state }))
    }

    #[derive(Debug, PartialEq)]
    struct FakeCommandInvocation {
        command: PathBuf,
        args: Vec<String>,
        timeout: Duration,
    }

    #[derive(Default)]
    struct FakeCommandState {
        responses: Vec<Option<String>>,
        invocations: Vec<FakeCommandInvocation>,
    }

    struct FakeWpctlCommandRunner {
        state: Arc<Mutex<FakeCommandState>>,
    }

    impl WpctlCommandRunner for FakeWpctlCommandRunner {
        fn run(&mut self, command: &Path, args: &[&str], timeout: Duration) -> Option<String> {
            let mut state = self.state.lock().expect("fake command lock poisoned");
            state.invocations.push(FakeCommandInvocation {
                command: command.to_path_buf(),
                args: args.iter().map(|value| (*value).to_owned()).collect(),
                timeout,
            });
            if state.responses.is_empty() {
                return None;
            }
            state.responses.remove(0)
        }
    }

    #[test]
    fn wpctl_volume_parser_accepts_plain_and_muted_output() {
        assert_eq!(parse_wpctl_volume("Volume: 1.00\n"), Some(1.0));
        assert_eq!(parse_wpctl_volume("Volume: 0.15 [MUTED]\n"), Some(0.15));
        assert_eq!(parse_wpctl_volume("no volume"), None);
        assert_eq!(parse_wpctl_volume("Volume: NaN"), None);
        assert_eq!(parse_wpctl_volume("Volume: -0.1"), None);
    }

    #[test]
    fn duck_and_restore_are_idempotent_and_preserve_original_volume() {
        let state = Arc::new(Mutex::new(FakeState {
            current: Some(0.8),
            set_succeeds: true,
            ..FakeState::default()
        }));
        let mut ducker = fake_ducker(Arc::clone(&state));

        ducker.duck(0.25);
        ducker.duck(0.5);
        assert!(ducker.is_ducked());
        ducker.restore();
        ducker.restore();

        let state = state.lock().expect("fake volume lock poisoned");
        assert_eq!(state.writes.len(), 2);
        assert!((state.writes[0] - 0.2).abs() < f64::EPSILON);
        assert!((state.writes[1] - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn duck_clamps_scale_and_failures_never_enter_ducked_state() {
        let state = Arc::new(Mutex::new(FakeState {
            current: Some(0.5),
            set_succeeds: true,
            ..FakeState::default()
        }));
        let mut ducker = fake_ducker(Arc::clone(&state));
        ducker.duck(-1.0);
        assert!(ducker.is_ducked());
        ducker.restore();

        {
            let mut state = state.lock().expect("fake volume lock poisoned");
            assert!(state.writes[0].abs() < f64::EPSILON);
            state.current = None;
            state.writes.clear();
        }
        ducker.duck(0.25);
        assert!(!ducker.is_ducked());
        assert!(
            state
                .lock()
                .expect("fake volume lock poisoned")
                .writes
                .is_empty()
        );

        {
            let mut state = state.lock().expect("fake volume lock poisoned");
            state.current = Some(0.5);
            state.set_succeeds = false;
        }
        ducker.duck(0.25);
        assert!(!ducker.is_ducked());
    }

    #[test]
    fn wpctl_control_uses_expected_arguments_and_fixed_volume_format() {
        let command = PathBuf::from("/test/bin/wpctl");
        let timeout = Duration::from_secs(7);
        let state = Arc::new(Mutex::new(FakeCommandState {
            responses: vec![
                Some("Volume: 0.80 [MUTED]\n".to_owned()),
                Some(String::new()),
            ],
            ..FakeCommandState::default()
        }));
        let runner = FakeWpctlCommandRunner {
            state: Arc::clone(&state),
        };
        let mut control =
            WpctlOutputVolumeControl::with_runner(&command, timeout, Box::new(runner));

        assert_eq!(control.read_default_sink_volume(), Some(0.8));
        assert!(control.set_default_sink_volume(0.2));

        let state = state.lock().expect("fake command lock poisoned");
        assert_eq!(
            state.invocations,
            [
                FakeCommandInvocation {
                    command: command.clone(),
                    args: vec!["get-volume".to_owned(), DEFAULT_SINK.to_owned()],
                    timeout,
                },
                FakeCommandInvocation {
                    command,
                    args: vec![
                        "set-volume".to_owned(),
                        DEFAULT_SINK.to_owned(),
                        "0.2000".to_owned(),
                    ],
                    timeout,
                },
            ]
        );
    }

    #[test]
    fn wpctl_control_kills_commands_after_the_deadline() {
        let directory = tempfile::tempdir().expect("create slow wpctl directory");
        let command = directory.path().join("wpctl");
        std::fs::write(&command, "#!/bin/sh\nsleep 1\necho 'Volume: 0.8'\n")
            .expect("write slow wpctl");
        let mut permissions = std::fs::metadata(&command)
            .expect("stat slow wpctl")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&command, permissions).expect("make slow wpctl executable");

        let started = Instant::now();
        let mut control = WpctlOutputVolumeControl::new(&command, Duration::from_millis(20));
        assert_eq!(control.read_default_sink_volume(), None);
        assert!(started.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn wpctl_control_reaps_descendants_that_inherit_output_pipes() {
        let directory = tempfile::tempdir().expect("create descendant wpctl directory");
        let command = directory.path().join("wpctl");
        std::fs::write(&command, "#!/bin/sh\n(sleep 5) &\necho 'Volume: 0.8'\n")
            .expect("write descendant wpctl");
        let mut permissions = std::fs::metadata(&command)
            .expect("stat descendant wpctl")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&command, permissions).expect("make descendant wpctl executable");

        let started = Instant::now();
        let mut control = WpctlOutputVolumeControl::new(&command, Duration::from_secs(2));
        assert_eq!(control.read_default_sink_volume(), Some(0.8));
        assert!(started.elapsed() < Duration::from_millis(500));
    }
}

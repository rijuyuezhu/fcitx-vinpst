//! Feature-gated `PipeWire` backend scaffolding.
//!
//! Device enumeration is live when a user `PipeWire` session is available.
//! The recorder owns a live worker thread that keeps an inactive connected
//! stream across normal recordings, toggles it with `set_active`, captures
//! pinned `S16LE` PCM chunks, and returns the accumulated buffer when stopped.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use crate::{
    AudioChunkCallback, AudioDeviceEnumerator, AudioDeviceInfo, AudioError, AudioRecorder,
    CaptureTarget, CapturedAudio, DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE_HZ, PcmBuffer, PcmSpec,
};

const MEDIA_CLASS_AUDIO_SOURCE: &str = "Audio/Source";
const PW_KEY_MEDIA_CLASS: &str = "media.class";
const PW_KEY_NODE_NAME: &str = "node.name";
const PW_KEY_NODE_DESCRIPTION: &str = "node.description";
const CAPTURE_REUSE_ENV: &str = "VINPST_CAPTURE_REUSE";
const CAPTURE_IDLE_DESTROY_ENV: &str = "VINPST_CAPTURE_IDLE_DESTROY_MS";
const DEFAULT_CAPTURE_IDLE_DESTROY_MS: u64 = 15_000;
const MAX_CAPTURE_IDLE_DESTROY_MS: u64 = 600_000;
const UNKNOWN_TIMING_MS: u64 = u64::MAX;

/// `PipeWire` stream sample format requested by the future live recorder.
pub const RECORDING_FORMAT: &str = "S16LE";

/// `PipeWire` stream sample rate requested by the future live recorder.
pub const RECORDING_SAMPLE_RATE_HZ: u32 = DEFAULT_SAMPLE_RATE_HZ;

/// `PipeWire` stream channel count requested by the future live recorder.
pub const RECORDING_CHANNELS: u16 = DEFAULT_CHANNELS;

/// Returns the PCM spec that future `PipeWire` capture must deliver to ASR.
#[must_use]
pub const fn recording_pcm_spec() -> PcmSpec {
    PcmSpec {
        sample_rate_hz: RECORDING_SAMPLE_RATE_HZ,
        channels: RECORDING_CHANNELS,
    }
}

/// Planned `PipeWire` stream settings for a capture target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipeWireStreamConfig {
    /// Capture target selected by config or UI.
    pub target: CaptureTarget,
    /// Signed PCM format requested from `PipeWire`.
    pub format: &'static str,
    /// PCM layout delivered to ASR.
    pub pcm_spec: PcmSpec,
}

impl PipeWireStreamConfig {
    /// Builds the default live stream configuration for a target.
    #[must_use]
    pub fn for_target(target: CaptureTarget) -> Self {
        Self {
            target,
            format: RECORDING_FORMAT,
            pcm_spec: recording_pcm_spec(),
        }
    }
}

/// Timing recorded for the most recent successful `PipeWire` capture start.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PipeWireStartTiming {
    /// Time since the previous stream deactivation or destruction.
    pub idle_gap_ms: Option<u64>,
    /// Time spent creating and connecting a fresh stream.
    pub create_stream_ms: u64,
    /// Time spent activating the selected stream.
    pub set_active_ms: u64,
    /// Whether the successful start reused the existing connected stream.
    pub stream_reused: bool,
    /// Whether the successful start created a new stream.
    pub created_new_stream: bool,
    /// Total wall-clock time spent in `begin_recording`.
    pub start_total_ms: u64,
}

/// Enables live `PipeWire` source enumeration tests when set in the environment.
pub const TEST_PIPEWIRE_ENUMERATE_ENV: &str = "VINPST_TEST_PIPEWIRE_ENUMERATE";

/// Enables live `PipeWire` client context tests when set in the environment.
pub const TEST_PIPEWIRE_CONTEXT_ENV: &str = "VINPST_TEST_PIPEWIRE_CONTEXT";

/// Enables live `PipeWire` recorder tests when set in the environment.
pub const TEST_PIPEWIRE_RECORD_ENV: &str = "VINPST_TEST_PIPEWIRE_RECORD";
/// Optional live-test recording duration in milliseconds.
pub const TEST_PIPEWIRE_RECORD_MS_ENV: &str = "VINPST_TEST_PIPEWIRE_RECORD_MS";
/// Optional minimum absolute PCM peak required by the live recorder test.
pub const TEST_PIPEWIRE_MIN_PEAK_ENV: &str = "VINPST_TEST_PIPEWIRE_MIN_PEAK";
/// First explicit source used by the live target-switch test.
pub const TEST_PIPEWIRE_SWITCH_SOURCE_A_ENV: &str = "VINPST_TEST_PIPEWIRE_SWITCH_SOURCE_A";
/// Second explicit source used by the live target-switch test.
pub const TEST_PIPEWIRE_SWITCH_SOURCE_B_ENV: &str = "VINPST_TEST_PIPEWIRE_SWITCH_SOURCE_B";
/// JSON evidence path written by the live target-switch test.
pub const TEST_PIPEWIRE_SWITCH_SUMMARY_ENV: &str = "VINPST_TEST_PIPEWIRE_SWITCH_SUMMARY";

/// Returns whether a `PipeWire` live integration test gate is explicitly enabled.
#[must_use]
pub fn live_test_enabled(env_name: &str) -> bool {
    std::env::var_os(env_name).is_some()
}

/// Initialize the `PipeWire` client library.
pub fn initialize() {
    pipewire::init();
}

/// Probe that the optional `PipeWire` bindings link and initialize.
pub fn probe_client_linkage() {
    initialize();
}

/// Create the minimal `PipeWire` main loop and context objects.
///
/// This requires a usable `PipeWire` client configuration and is therefore
/// intended for explicit local integration checks, not default CI.
pub fn probe_client_context() -> Result<(), AudioError> {
    probe_client_linkage();
    let mainloop = pipewire::main_loop::MainLoopBox::new(None).map_err(pipewire_error)?;
    let _context =
        pipewire::context::ContextBox::new(mainloop.loop_(), None).map_err(pipewire_error)?;
    Ok(())
}

/// Convert a `PipeWire` registry global into audio-source metadata.
pub fn audio_device_from_global<P>(
    global: &pipewire::registry::GlobalObject<P>,
) -> Option<AudioDeviceInfo>
where
    P: AsRef<pipewire::spa::utils::dict::DictRef>,
{
    if global.type_ != pipewire::types::ObjectType::Node {
        return None;
    }
    let props = global.props.as_ref()?.as_ref();
    if props.get(PW_KEY_MEDIA_CLASS) != Some(MEDIA_CLASS_AUDIO_SOURCE) {
        return None;
    }
    let name = props.get(PW_KEY_NODE_NAME).unwrap_or_default();
    let description = props.get(PW_KEY_NODE_DESCRIPTION).unwrap_or_default();
    Some(AudioDeviceInfo::new(global.id, name, description))
}

/// Feature-gated `PipeWire` device enumerator.
#[derive(Debug, Clone, Copy, Default)]
pub struct PipeWireDeviceEnumerator;

impl AudioDeviceEnumerator for PipeWireDeviceEnumerator {
    fn enumerate_audio_sources(&mut self) -> Result<Vec<AudioDeviceInfo>, AudioError> {
        enumerate_audio_sources()
    }
}

/// Feature-gated reusable `PipeWire` recorder.
pub struct PipeWireAudioRecorder {
    stream_config: PipeWireStreamConfig,
    chunk_callback: Arc<Mutex<Option<AudioChunkCallback>>>,
    worker: Option<PipeWireRecordingWorker>,
    recording: bool,
    last_start_timing: PipeWireStartTiming,
    first_buffer_latency_ms: Arc<AtomicU64>,
}

struct PipeWireRecordingWorker {
    command_tx: pipewire::channel::Sender<WorkerCommand>,
    idle_timer_tx: mpsc::Sender<IdleTimerCommand>,
    join: thread::JoinHandle<Result<(), AudioError>>,
    idle_timer_join: thread::JoinHandle<()>,
}

enum WorkerCommand {
    Begin {
        config: PipeWireStreamConfig,
        response: mpsc::SyncSender<Result<PipeWireStartTiming, AudioError>>,
    },
    Finish {
        response: mpsc::SyncSender<Result<(CapturedAudio, u64), AudioError>>,
    },
    ExpireIdle {
        generation: u64,
    },
    Shutdown,
}

enum IdleTimerCommand {
    Schedule { delay: Duration, generation: u64 },
    Shutdown,
}

impl PipeWireAudioRecorder {
    /// Creates a recorder placeholder for future live `PipeWire` capture.
    #[must_use]
    pub fn new() -> Self {
        Self {
            stream_config: PipeWireStreamConfig::for_target(CaptureTarget::default()),
            chunk_callback: Arc::new(Mutex::new(None)),
            worker: None,
            recording: false,
            last_start_timing: PipeWireStartTiming::default(),
            first_buffer_latency_ms: Arc::new(AtomicU64::new(UNKNOWN_TIMING_MS)),
        }
    }

    /// Returns the last target passed to `begin_recording`.
    #[must_use]
    pub fn target(&self) -> &CaptureTarget {
        &self.stream_config.target
    }

    /// Returns the planned stream configuration for the next live capture.
    #[must_use]
    pub fn stream_config(&self) -> &PipeWireStreamConfig {
        &self.stream_config
    }

    /// Returns timing for the most recent successful capture start.
    #[must_use]
    pub const fn last_start_timing(&self) -> PipeWireStartTiming {
        self.last_start_timing
    }

    /// Returns latency from capture arming to the first non-empty `PipeWire` buffer.
    #[must_use]
    pub fn first_buffer_latency_ms(&self) -> Option<u64> {
        let value = self.first_buffer_latency_ms.load(Ordering::Relaxed);
        (value != UNKNOWN_TIMING_MS).then_some(value)
    }
}

impl Default for PipeWireAudioRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PipeWireAudioRecorder {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = shutdown_recording_worker(worker, &self.stream_config);
        }
    }
}

impl AudioRecorder for PipeWireAudioRecorder {
    fn begin_recording(&mut self, target: CaptureTarget) -> Result<(), AudioError> {
        if self.recording {
            return Err(AudioError::RecorderAlreadyRecording);
        }
        let start_at = Instant::now();
        self.last_start_timing = PipeWireStartTiming::default();
        self.first_buffer_latency_ms
            .store(UNKNOWN_TIMING_MS, Ordering::Relaxed);
        self.stream_config = PipeWireStreamConfig::for_target(target);
        if !capture_stream_reuse_enabled()
            && let Some(worker) = self.worker.take()
        {
            shutdown_recording_worker(worker, &self.stream_config)?;
        }
        if self.worker.is_none() {
            self.worker = Some(spawn_recording_worker(
                Arc::clone(&self.chunk_callback),
                Arc::clone(&self.first_buffer_latency_ms),
                &self.stream_config,
            )?);
        }
        let worker = self.worker.as_mut().ok_or_else(|| {
            pipewire_recording_error(&self.stream_config, "PipeWire worker is unavailable")
        })?;
        let (response_tx, response_rx) = mpsc::sync_channel(1);
        send_worker_command(
            worker,
            WorkerCommand::Begin {
                config: self.stream_config.clone(),
                response: response_tx,
            },
            &self.stream_config,
        )?;
        let mut timing = response_rx.recv().map_err(|error| {
            pipewire_recording_error(
                &self.stream_config,
                format!("PipeWire worker dropped begin response: {error}"),
            )
        })??;
        timing.start_total_ms = duration_millis(start_at.elapsed());
        self.last_start_timing = timing;
        tracing::debug!(
            target = %pipewire_capture_source_name(&self.stream_config),
            idle_gap_ms = ?timing.idle_gap_ms,
            create_stream_ms = timing.create_stream_ms,
            set_active_ms = timing.set_active_ms,
            stream_reused = timing.stream_reused,
            created_new_stream = timing.created_new_stream,
            start_total_ms = timing.start_total_ms,
            "PipeWire capture started"
        );
        self.recording = true;
        Ok(())
    }

    fn set_chunk_callback(&mut self, callback: Option<AudioChunkCallback>) {
        if let Ok(mut installed) = self.chunk_callback.lock() {
            *installed = callback;
        }
    }

    fn stop_and_get_buffer(&mut self) -> Result<CapturedAudio, AudioError> {
        if !self.recording {
            return Err(AudioError::RecorderNotRecording);
        }
        let captured = finish_recording_worker(
            self.worker
                .as_mut()
                .ok_or(AudioError::RecorderNotRecording)?,
            &self.stream_config,
        );
        self.recording = false;
        let (captured, generation) = captured?;
        if capture_stream_reuse_enabled() {
            let schedule_failed = schedule_idle_stream_destroy(
                self.worker
                    .as_mut()
                    .ok_or(AudioError::RecorderNotRecording)?,
                generation,
                capture_idle_destroy_duration(),
                &self.stream_config,
            )
            .is_err();
            if schedule_failed && let Some(worker) = self.worker.take() {
                let _ = shutdown_recording_worker(worker, &self.stream_config);
            }
        } else if let Some(worker) = self.worker.take() {
            shutdown_recording_worker(worker, &self.stream_config)?;
        }
        Ok(captured)
    }

    fn cancel_recording(&mut self) -> Result<(), AudioError> {
        let mut finish_error = None;
        if self.recording {
            let result = finish_recording_worker(
                self.worker
                    .as_mut()
                    .ok_or(AudioError::RecorderNotRecording)?,
                &self.stream_config,
            )
            .map(|(captured, _generation)| captured);
            self.recording = false;
            if let Err(error) = result {
                finish_error = Some(error);
            }
        }
        if let Some(worker) = self.worker.take() {
            shutdown_recording_worker(worker, &self.stream_config)?;
        }
        finish_error.map_or(Ok(()), Err)
    }

    fn is_recording(&self) -> bool {
        self.recording
    }
}

/// Enumerate available `PipeWire` audio sources.
pub fn enumerate_audio_sources() -> Result<Vec<AudioDeviceInfo>, AudioError> {
    probe_client_linkage();

    let mainloop = pipewire::main_loop::MainLoopRc::new(None).map_err(pipewire_error)?;
    let context = pipewire::context::ContextRc::new(&mainloop, None).map_err(pipewire_error)?;
    let core = context.connect_rc(None).map_err(pipewire_error)?;
    let registry = core.get_registry_rc().map_err(pipewire_error)?;

    let devices = Rc::new(RefCell::new(Vec::new()));
    let devices_for_registry = Rc::clone(&devices);
    let _registry_listener = registry
        .add_listener_local()
        .global(move |global| {
            if let Some(device) = audio_device_from_global(global) {
                devices_for_registry.borrow_mut().push(device);
            }
        })
        .register();

    let pending_sync = Rc::new(Cell::new(None));
    let pending_sync_for_core = Rc::clone(&pending_sync);
    let mainloop_for_core = mainloop.clone();
    let _core_listener = core
        .add_listener_local()
        .done(move |id, seq| {
            if id == pipewire::core::PW_ID_CORE && pending_sync_for_core.get() == Some(seq.seq()) {
                mainloop_for_core.quit();
            }
        })
        .register();

    let sync = core.sync(0).map_err(pipewire_error)?;
    pending_sync.set(Some(sync.seq()));
    mainloop.run();

    let result = devices.borrow().clone();
    Ok(result)
}

struct PersistentPipeWireStream {
    _listener: pipewire::stream::StreamListener<()>,
    stream: pipewire::stream::StreamRc,
    config: PipeWireStreamConfig,
    samples: Rc<RefCell<Vec<i16>>>,
    accepting: Rc<Cell<bool>>,
    recording_armed_at: Rc<Cell<Option<Instant>>>,
}

#[derive(Default)]
struct PipeWireWorkerState {
    stream: Option<PersistentPipeWireStream>,
    recording: bool,
    idle_generation: u64,
    last_stream_inactive_at: Option<Instant>,
}

fn capture_stream_reuse_enabled() -> bool {
    capture_stream_reuse_enabled_from(std::env::var(CAPTURE_REUSE_ENV).ok().as_deref())
}

fn capture_stream_reuse_enabled_from(value: Option<&str>) -> bool {
    let Some(first) = value.and_then(|value| value.as_bytes().first()).copied() else {
        return true;
    };
    !matches!(first, b'0' | b'f' | b'F' | b'n' | b'N')
}

fn duration_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis())
        .unwrap_or(u64::MAX)
        .min(UNKNOWN_TIMING_MS - 1)
}

fn record_first_buffer_latency(
    recording_armed_at: Option<Instant>,
    first_buffer_latency_ms: &AtomicU64,
) -> Option<u64> {
    let armed_at = recording_armed_at?;
    let latency_ms = duration_millis(armed_at.elapsed());
    first_buffer_latency_ms
        .compare_exchange(
            UNKNOWN_TIMING_MS,
            latency_ms,
            Ordering::Relaxed,
            Ordering::Relaxed,
        )
        .ok()
        .map(|_| latency_ms)
}

fn capture_idle_destroy_duration() -> Duration {
    Duration::from_millis(capture_idle_destroy_ms_from(
        std::env::var(CAPTURE_IDLE_DESTROY_ENV).ok().as_deref(),
    ))
}

fn capture_idle_destroy_ms_from(value: Option<&str>) -> u64 {
    let Some(value) = value else {
        return DEFAULT_CAPTURE_IDLE_DESTROY_MS;
    };
    let value = value.trim_start();
    if value.is_empty() {
        return DEFAULT_CAPTURE_IDLE_DESTROY_MS;
    }
    let (negative, digits) = match value.as_bytes()[0] {
        b'+' => (false, &value[1..]),
        b'-' => (true, &value[1..]),
        _ => (false, value),
    };
    let digit_count = digits
        .as_bytes()
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digit_count == 0 {
        return DEFAULT_CAPTURE_IDLE_DESTROY_MS;
    }
    let parsed = digits[..digit_count].bytes().fold(0_u64, |value, digit| {
        value
            .saturating_mul(10)
            .saturating_add(u64::from(digit - b'0'))
    });
    if negative && parsed != 0 {
        return DEFAULT_CAPTURE_IDLE_DESTROY_MS;
    }
    parsed.min(MAX_CAPTURE_IDLE_DESTROY_MS)
}

fn can_reuse_stream(
    existing: Option<&PipeWireStreamConfig>,
    requested: &PipeWireStreamConfig,
) -> bool {
    existing.is_some_and(|existing| existing == requested)
}

fn should_expire_idle_stream(state: &PipeWireWorkerState, generation: u64) -> bool {
    !state.recording && state.idle_generation == generation
}

fn spawn_recording_worker(
    callback: Arc<Mutex<Option<AudioChunkCallback>>>,
    first_buffer_latency_ms: Arc<AtomicU64>,
    config: &PipeWireStreamConfig,
) -> Result<PipeWireRecordingWorker, AudioError> {
    let (command_tx, command_rx) = pipewire::channel::channel();
    let idle_worker_tx = command_tx.clone();
    let (idle_timer_tx, idle_timer_rx) = mpsc::channel();
    let idle_timer_join =
        thread::spawn(move || run_idle_destroy_timer(&idle_timer_rx, &idle_worker_tx));
    let (setup_tx, setup_rx) = mpsc::sync_channel(1);
    let join = thread::spawn(move || {
        run_recording_worker(&callback, &first_buffer_latency_ms, command_rx, &setup_tx)
    });
    let setup_result = setup_rx.recv().map_err(|error| {
        pipewire_recording_error(
            config,
            format!("PipeWire recorder worker exited before setup: {error}"),
        )
    })?;
    setup_result.map_err(|error| pipewire_recording_error(config, error))?;
    Ok(PipeWireRecordingWorker {
        command_tx,
        idle_timer_tx,
        join,
        idle_timer_join,
    })
}

fn send_worker_command(
    worker: &mut PipeWireRecordingWorker,
    command: WorkerCommand,
    config: &PipeWireStreamConfig,
) -> Result<(), AudioError> {
    worker.command_tx.send(command).map_err(|_| {
        pipewire_recording_error(config, "PipeWire recorder worker command channel is closed")
    })
}

fn finish_recording_worker(
    worker: &mut PipeWireRecordingWorker,
    config: &PipeWireStreamConfig,
) -> Result<(CapturedAudio, u64), AudioError> {
    let (response_tx, response_rx) = mpsc::sync_channel(1);
    send_worker_command(
        worker,
        WorkerCommand::Finish {
            response: response_tx,
        },
        config,
    )?;
    response_rx.recv().map_err(|error| {
        pipewire_recording_error(
            config,
            format!("PipeWire worker dropped finish response: {error}"),
        )
    })?
}

fn schedule_idle_stream_destroy(
    worker: &mut PipeWireRecordingWorker,
    generation: u64,
    delay: Duration,
    config: &PipeWireStreamConfig,
) -> Result<(), AudioError> {
    if delay.is_zero() {
        return worker
            .command_tx
            .send(WorkerCommand::ExpireIdle { generation })
            .map_err(|_| {
                pipewire_recording_error(
                    config,
                    "PipeWire recorder worker command channel is closed",
                )
            });
    }
    worker
        .idle_timer_tx
        .send(IdleTimerCommand::Schedule { delay, generation })
        .map_err(|_| {
            pipewire_recording_error(config, "PipeWire idle timer command channel is closed")
        })
}

fn shutdown_recording_worker(
    worker: PipeWireRecordingWorker,
    config: &PipeWireStreamConfig,
) -> Result<(), AudioError> {
    let _ = worker.idle_timer_tx.send(IdleTimerCommand::Shutdown);
    let _ = worker.idle_timer_join.join();
    let _ = worker.command_tx.send(WorkerCommand::Shutdown);
    match worker.join.join() {
        Ok(result) => result,
        Err(_) => Err(pipewire_recording_error(
            config,
            "PipeWire recorder worker panicked",
        )),
    }
}

fn run_idle_destroy_timer(
    command_rx: &mpsc::Receiver<IdleTimerCommand>,
    worker_tx: &pipewire::channel::Sender<WorkerCommand>,
) {
    let mut pending: Option<(Instant, u64)> = None;
    loop {
        let command = match pending {
            Some((deadline, generation)) => {
                match command_rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                    Ok(command) => command,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        let _ = worker_tx.send(WorkerCommand::ExpireIdle { generation });
                        pending = None;
                        continue;
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
            }
            None => match command_rx.recv() {
                Ok(command) => command,
                Err(_) => return,
            },
        };
        match command {
            IdleTimerCommand::Schedule { delay, generation } => {
                pending = Some((Instant::now() + delay, generation));
            }
            IdleTimerCommand::Shutdown => return,
        }
    }
}

fn run_recording_worker(
    callback: &Arc<Mutex<Option<AudioChunkCallback>>>,
    first_buffer_latency_ms: &Arc<AtomicU64>,
    command_rx: pipewire::channel::Receiver<WorkerCommand>,
    setup_tx: &mpsc::SyncSender<Result<(), AudioError>>,
) -> Result<(), AudioError> {
    let result =
        run_recording_worker_inner(callback, first_buffer_latency_ms, command_rx, setup_tx);
    if let Err(error) = &result {
        let _ = setup_tx.send(Err(AudioError::RecordingBackendUnavailable(
            error.to_string(),
        )));
    }
    result
}

fn run_recording_worker_inner(
    callback: &Arc<Mutex<Option<AudioChunkCallback>>>,
    first_buffer_latency_ms: &Arc<AtomicU64>,
    command_rx: pipewire::channel::Receiver<WorkerCommand>,
    setup_tx: &mpsc::SyncSender<Result<(), AudioError>>,
) -> Result<(), AudioError> {
    let default_config = PipeWireStreamConfig::for_target(CaptureTarget::default());
    probe_client_linkage();
    let mainloop = pipewire::main_loop::MainLoopRc::new(None)
        .map_err(|error| pipewire_recording_error(&default_config, error))?;
    let context = pipewire::context::ContextRc::new(&mainloop, None)
        .map_err(|error| pipewire_recording_error(&default_config, error))?;
    let core = context
        .connect_rc(None)
        .map_err(|error| pipewire_recording_error(&default_config, error))?;
    let state = Rc::new(RefCell::new(PipeWireWorkerState::default()));
    let state_for_commands = Rc::clone(&state);
    let core_for_commands = core.clone();
    let callback_for_commands = Arc::clone(callback);
    let first_buffer_for_commands = Arc::clone(first_buffer_latency_ms);
    let mainloop_for_commands = mainloop.clone();
    let _commands = command_rx.attach(mainloop.loop_(), move |command| match command {
        WorkerCommand::Begin { config, response } => {
            let result = begin_worker_recording(
                &mut state_for_commands.borrow_mut(),
                &core_for_commands,
                &callback_for_commands,
                &first_buffer_for_commands,
                config,
            );
            let _ = response.send(result);
        }
        WorkerCommand::Finish { response } => {
            let result = finish_worker_recording(&mut state_for_commands.borrow_mut());
            let _ = response.send(result);
        }
        WorkerCommand::ExpireIdle { generation } => {
            let mut state = state_for_commands.borrow_mut();
            if should_expire_idle_stream(&state, generation) {
                state.last_stream_inactive_at = Some(Instant::now());
                state.stream = None;
            }
        }
        WorkerCommand::Shutdown => {
            shutdown_worker_state(&mut state_for_commands.borrow_mut());
            mainloop_for_commands.quit();
        }
    });
    let _ = setup_tx.send(Ok(()));
    mainloop.run();
    shutdown_worker_state(&mut state.borrow_mut());
    Ok(())
}

fn begin_worker_recording(
    state: &mut PipeWireWorkerState,
    core: &pipewire::core::CoreRc,
    callback: &Arc<Mutex<Option<AudioChunkCallback>>>,
    first_buffer_latency_ms: &Arc<AtomicU64>,
    config: PipeWireStreamConfig,
) -> Result<PipeWireStartTiming, AudioError> {
    if state.recording {
        return Err(AudioError::RecorderAlreadyRecording);
    }
    let begin_at = Instant::now();
    let mut timing = PipeWireStartTiming {
        idle_gap_ms: state
            .last_stream_inactive_at
            .map(|inactive_at| duration_millis(begin_at.saturating_duration_since(inactive_at))),
        ..PipeWireStartTiming::default()
    };
    state.idle_generation = state.idle_generation.wrapping_add(1);
    let reusable = can_reuse_stream(state.stream.as_ref().map(|stream| &stream.config), &config);
    if !reusable {
        let create_at = Instant::now();
        state.stream = Some(create_persistent_stream(
            core,
            callback,
            first_buffer_latency_ms,
            config.clone(),
        )?);
        timing.create_stream_ms = duration_millis(create_at.elapsed());
    }
    let activate_at = Instant::now();
    match activate_worker_stream(state) {
        Ok(()) => {
            timing.set_active_ms = duration_millis(activate_at.elapsed());
            timing.stream_reused = reusable;
            timing.created_new_stream = !reusable;
            state.recording = true;
            Ok(timing)
        }
        Err(_error) if reusable => {
            let create_at = Instant::now();
            state.stream = Some(create_persistent_stream(
                core,
                callback,
                first_buffer_latency_ms,
                config,
            )?);
            timing.create_stream_ms = duration_millis(create_at.elapsed());
            let activate_at = Instant::now();
            activate_worker_stream(state)?;
            timing.set_active_ms = duration_millis(activate_at.elapsed());
            timing.stream_reused = false;
            timing.created_new_stream = true;
            state.recording = true;
            Ok(timing)
        }
        Err(error) => {
            state.stream = None;
            Err(error)
        }
    }
}

fn activate_worker_stream(state: &mut PipeWireWorkerState) -> Result<(), AudioError> {
    let stream = state.stream.as_mut().ok_or_else(|| {
        pipewire_recording_error(
            &PipeWireStreamConfig::for_target(CaptureTarget::default()),
            "PipeWire stream is unavailable",
        )
    })?;
    stream.samples.borrow_mut().clear();
    stream.recording_armed_at.set(Some(Instant::now()));
    stream.accepting.set(true);
    if let Err(error) = stream.stream.set_active(true) {
        stream.accepting.set(false);
        stream.recording_armed_at.set(None);
        return Err(pipewire_recording_error(
            &stream.config,
            format!("activate PipeWire stream: {error}"),
        ));
    }
    Ok(())
}

fn finish_worker_recording(
    state: &mut PipeWireWorkerState,
) -> Result<(CapturedAudio, u64), AudioError> {
    if !state.recording {
        return Err(AudioError::RecorderNotRecording);
    }
    let stream = state
        .stream
        .as_mut()
        .ok_or(AudioError::RecorderNotRecording)?;
    stream.accepting.set(false);
    if let Err(error) = stream.stream.set_active(false) {
        let config = stream.config.clone();
        stream.recording_armed_at.set(None);
        state.recording = false;
        state.stream = None;
        return Err(pipewire_recording_error(
            &config,
            format!("deactivate PipeWire stream: {error}"),
        ));
    }
    stream.recording_armed_at.set(None);
    state.recording = false;
    state.last_stream_inactive_at = Some(Instant::now());
    state.idle_generation = state.idle_generation.wrapping_add(1);
    let idle_generation = state.idle_generation;
    let samples = std::mem::take(&mut *stream.samples.borrow_mut());
    let pcm = PcmBuffer::with_spec(stream.config.pcm_spec, samples)?;
    let captured = CapturedAudio::named(pcm, pipewire_capture_source_name(&stream.config));
    Ok((captured, idle_generation))
}

fn shutdown_worker_state(state: &mut PipeWireWorkerState) {
    if let Some(stream) = state.stream.as_mut() {
        stream.accepting.set(false);
        stream.recording_armed_at.set(None);
        if state.recording {
            let _ = stream.stream.set_active(false);
        }
    }
    state.recording = false;
    state.idle_generation = state.idle_generation.wrapping_add(1);
    state.stream = None;
}

fn create_persistent_stream(
    core: &pipewire::core::CoreRc,
    callback: &Arc<Mutex<Option<AudioChunkCallback>>>,
    first_buffer_latency_ms: &Arc<AtomicU64>,
    config: PipeWireStreamConfig,
) -> Result<PersistentPipeWireStream, AudioError> {
    use pipewire::{properties::properties, spa};

    let mut props = properties! {
        *pipewire::keys::MEDIA_TYPE => "Audio",
        *pipewire::keys::MEDIA_CATEGORY => "Capture",
        *pipewire::keys::MEDIA_ROLE => "Speech",
    };
    if let Some(target) = config.target.target_object() {
        props.insert("target.object", target.to_owned());
    }
    let stream = pipewire::stream::StreamRc::new(core.clone(), "vinpst-capture", props)
        .map_err(|error| pipewire_recording_error(&config, error))?;
    let samples = Rc::new(RefCell::new(Vec::new()));
    let accepting = Rc::new(Cell::new(false));
    let recording_armed_at = Rc::new(Cell::new(None));
    let samples_for_process = Rc::clone(&samples);
    let accepting_for_process = Rc::clone(&accepting);
    let armed_at_for_process = Rc::clone(&recording_armed_at);
    let first_buffer_for_process = Arc::clone(first_buffer_latency_ms);
    let callback_for_process = Arc::clone(callback);
    let pcm_spec = config.pcm_spec;
    let listener = stream
        .add_local_listener_with_user_data(())
        .process(move |stream, ()| {
            capture_stream_buffer(
                stream,
                pcm_spec,
                &samples_for_process,
                &accepting_for_process,
                &armed_at_for_process,
                &first_buffer_for_process,
                &callback_for_process,
            );
        })
        .register()
        .map_err(|error| pipewire_recording_error(&config, error))?;
    let param_values = pipewire_recording_param_values(&config)?;
    let params = [spa::pod::Pod::from_bytes(&param_values).ok_or_else(|| {
        pipewire_recording_error(&config, "serialize PipeWire recording stream format")
    })?];
    let mut param_refs = [params[0]];
    stream
        .connect(
            spa::utils::Direction::Input,
            None,
            pipewire::stream::StreamFlags::AUTOCONNECT
                | pipewire::stream::StreamFlags::INACTIVE
                | pipewire::stream::StreamFlags::MAP_BUFFERS
                | pipewire::stream::StreamFlags::RT_PROCESS,
            &mut param_refs,
        )
        .map_err(|error| pipewire_recording_error(&config, error))?;
    Ok(PersistentPipeWireStream {
        _listener: listener,
        stream,
        config,
        samples,
        accepting,
        recording_armed_at,
    })
}

fn capture_stream_buffer(
    stream: &pipewire::stream::Stream,
    pcm_spec: PcmSpec,
    samples: &Rc<RefCell<Vec<i16>>>,
    accepting: &Rc<Cell<bool>>,
    recording_armed_at: &Rc<Cell<Option<Instant>>>,
    first_buffer_latency_ms: &Arc<AtomicU64>,
    callback: &Arc<Mutex<Option<AudioChunkCallback>>>,
) {
    let Some(mut buffer) = stream.dequeue_buffer() else {
        return;
    };
    let Some(data) = buffer.datas_mut().first_mut() else {
        return;
    };
    let chunk = data.chunk();
    let offset = chunk.offset() as usize;
    let size = chunk.size() as usize;
    let Some(bytes) = data.data() else {
        return;
    };
    let Some(end) = offset.checked_add(size) else {
        return;
    };
    let Some(bytes) = bytes.get(offset..end) else {
        return;
    };
    let chunk_samples = bytes
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    if chunk_samples.is_empty() {
        return;
    }
    if !accepting.get() {
        return;
    }
    if let Some(latency_ms) =
        record_first_buffer_latency(recording_armed_at.get(), first_buffer_latency_ms)
    {
        tracing::debug!(
            first_buffer_ms = latency_ms,
            samples = chunk_samples.len(),
            "PipeWire capture received first buffer"
        );
    }
    samples.borrow_mut().extend_from_slice(&chunk_samples);
    if let Ok(mut callback) = callback.lock()
        && let Some(callback) = callback.as_mut()
        && let Ok(pcm) = PcmBuffer::with_spec(pcm_spec, chunk_samples)
    {
        callback(&pcm);
    }
}

fn pipewire_recording_param_values(config: &PipeWireStreamConfig) -> Result<Vec<u8>, AudioError> {
    use pipewire::spa;

    let mut audio_info = spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(spa::param::audio::AudioFormat::S16LE);
    audio_info.set_rate(config.pcm_spec.sample_rate_hz);
    audio_info.set_channels(u32::from(config.pcm_spec.channels));
    let obj = spa::pod::Object {
        type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: spa::param::ParamType::EnumFormat.as_raw(),
        properties: audio_info.into(),
    };
    spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(obj),
    )
    .map(|serialized| serialized.0.into_inner())
    .map_err(|error| pipewire_recording_error(config, error))
}

fn pipewire_capture_source_name(config: &PipeWireStreamConfig) -> String {
    match &config.target {
        CaptureTarget::Default => "pipewire:default".to_owned(),
        CaptureTarget::Object(value) => format!("pipewire:{value}"),
    }
}

fn pipewire_recording_error(
    config: &PipeWireStreamConfig,
    error: impl std::fmt::Display,
) -> AudioError {
    let target = match &config.target {
        CaptureTarget::Default => "default".to_owned(),
        CaptureTarget::Object(value) => format!("object `{value}`"),
    };
    AudioError::RecordingBackendUnavailable(format!(
        "PipeWire recorder stream setup failed \
         (target: {target}, format: {}, sample_rate_hz: {}, channels: {}): {error}",
        config.format, config.pcm_spec.sample_rate_hz, config.pcm_spec.channels
    ))
}

fn pipewire_error(error: impl std::fmt::Display) -> AudioError {
    AudioError::DeviceEnumerationFailed(error.to_string())
}

#[cfg(test)]
mod tests;

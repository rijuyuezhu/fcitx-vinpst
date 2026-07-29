//! Feature-gated `PipeWire` backend scaffolding.
//!
//! Device enumeration is live when a user `PipeWire` session is available.
//! The recorder owns a live worker thread that keeps an inactive connected
//! stream across normal recordings, toggles it with `set_active`, captures
//! pinned `S16LE` PCM chunks, and returns the accumulated buffer when stopped.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{Arc, Mutex, mpsc},
    thread,
};

use crate::{
    AudioChunkCallback, AudioDeviceEnumerator, AudioDeviceInfo, AudioError, AudioRecorder,
    CaptureTarget, CapturedAudio, DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE_HZ, PcmBuffer, PcmSpec,
};

const MEDIA_CLASS_AUDIO_SOURCE: &str = "Audio/Source";
const PW_KEY_MEDIA_CLASS: &str = "media.class";
const PW_KEY_NODE_NAME: &str = "node.name";
const PW_KEY_NODE_DESCRIPTION: &str = "node.description";
const CAPTURE_REUSE_ENV: &str = "VINPUT_CAPTURE_REUSE";

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

/// Enables live `PipeWire` source enumeration tests when set in the environment.
pub const TEST_PIPEWIRE_ENUMERATE_ENV: &str = "VINPUT_TEST_PIPEWIRE_ENUMERATE";

/// Enables live `PipeWire` client context tests when set in the environment.
pub const TEST_PIPEWIRE_CONTEXT_ENV: &str = "VINPUT_TEST_PIPEWIRE_CONTEXT";

/// Enables live `PipeWire` recorder tests when set in the environment.
pub const TEST_PIPEWIRE_RECORD_ENV: &str = "VINPUT_TEST_PIPEWIRE_RECORD";

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
    last_start_reused: bool,
}

struct PipeWireRecordingWorker {
    command_tx: pipewire::channel::Sender<WorkerCommand>,
    join: thread::JoinHandle<Result<(), AudioError>>,
}

enum WorkerCommand {
    Begin {
        config: PipeWireStreamConfig,
        response: mpsc::SyncSender<Result<bool, AudioError>>,
    },
    Finish {
        response: mpsc::SyncSender<Result<CapturedAudio, AudioError>>,
    },
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
            last_start_reused: false,
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
        self.last_start_reused = false;
        self.stream_config = PipeWireStreamConfig::for_target(target);
        if !capture_stream_reuse_enabled()
            && let Some(worker) = self.worker.take()
        {
            shutdown_recording_worker(worker, &self.stream_config)?;
        }
        if self.worker.is_none() {
            self.worker = Some(spawn_recording_worker(Arc::clone(&self.chunk_callback))?);
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
        self.last_start_reused = response_rx.recv().map_err(|error| {
            pipewire_recording_error(
                &self.stream_config,
                format!("PipeWire worker dropped begin response: {error}"),
            )
        })??;
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
        let captured = captured?;
        if !capture_stream_reuse_enabled()
            && let Some(worker) = self.worker.take()
        {
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
            );
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
}

#[derive(Default)]
struct PipeWireWorkerState {
    stream: Option<PersistentPipeWireStream>,
    recording: bool,
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

fn can_reuse_stream(
    existing: Option<&PipeWireStreamConfig>,
    requested: &PipeWireStreamConfig,
) -> bool {
    existing.is_some_and(|existing| existing == requested)
}

fn spawn_recording_worker(
    callback: Arc<Mutex<Option<AudioChunkCallback>>>,
) -> Result<PipeWireRecordingWorker, AudioError> {
    let config = PipeWireStreamConfig::for_target(CaptureTarget::default());
    let (command_tx, command_rx) = pipewire::channel::channel();
    let (setup_tx, setup_rx) = mpsc::sync_channel(1);
    let join = thread::spawn(move || run_recording_worker(&callback, command_rx, &setup_tx));
    setup_rx.recv().map_err(|error| {
        pipewire_recording_error(
            &config,
            format!("PipeWire recorder worker exited before setup: {error}"),
        )
    })??;
    Ok(PipeWireRecordingWorker { command_tx, join })
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
) -> Result<CapturedAudio, AudioError> {
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

fn shutdown_recording_worker(
    worker: PipeWireRecordingWorker,
    config: &PipeWireStreamConfig,
) -> Result<(), AudioError> {
    let _ = worker.command_tx.send(WorkerCommand::Shutdown);
    match worker.join.join() {
        Ok(result) => result,
        Err(_) => Err(pipewire_recording_error(
            config,
            "PipeWire recorder worker panicked",
        )),
    }
}

fn run_recording_worker(
    callback: &Arc<Mutex<Option<AudioChunkCallback>>>,
    command_rx: pipewire::channel::Receiver<WorkerCommand>,
    setup_tx: &mpsc::SyncSender<Result<(), AudioError>>,
) -> Result<(), AudioError> {
    let result = run_recording_worker_inner(callback, command_rx, setup_tx);
    if let Err(error) = &result {
        let _ = setup_tx.send(Err(AudioError::RecordingBackendUnavailable(
            error.to_string(),
        )));
    }
    result
}

fn run_recording_worker_inner(
    callback: &Arc<Mutex<Option<AudioChunkCallback>>>,
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
    let mainloop_for_commands = mainloop.clone();
    let _commands = command_rx.attach(mainloop.loop_(), move |command| match command {
        WorkerCommand::Begin { config, response } => {
            let result = begin_worker_recording(
                &mut state_for_commands.borrow_mut(),
                &core_for_commands,
                &callback_for_commands,
                config,
            );
            let _ = response.send(result);
        }
        WorkerCommand::Finish { response } => {
            let result = finish_worker_recording(&mut state_for_commands.borrow_mut());
            let _ = response.send(result);
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
    config: PipeWireStreamConfig,
) -> Result<bool, AudioError> {
    if state.recording {
        return Err(AudioError::RecorderAlreadyRecording);
    }
    let reusable = can_reuse_stream(state.stream.as_ref().map(|stream| &stream.config), &config);
    if !reusable {
        state.stream = Some(create_persistent_stream(core, callback, config.clone())?);
    }
    match activate_worker_stream(state) {
        Ok(()) => {
            state.recording = true;
            Ok(reusable)
        }
        Err(_error) if reusable => {
            state.stream = Some(create_persistent_stream(core, callback, config)?);
            activate_worker_stream(state)?;
            state.recording = true;
            Ok(false)
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
    stream.accepting.set(true);
    if let Err(error) = stream.stream.set_active(true) {
        stream.accepting.set(false);
        return Err(pipewire_recording_error(
            &stream.config,
            format!("activate PipeWire stream: {error}"),
        ));
    }
    Ok(())
}

fn finish_worker_recording(state: &mut PipeWireWorkerState) -> Result<CapturedAudio, AudioError> {
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
        state.recording = false;
        state.stream = None;
        return Err(pipewire_recording_error(
            &config,
            format!("deactivate PipeWire stream: {error}"),
        ));
    }
    state.recording = false;
    let samples = std::mem::take(&mut *stream.samples.borrow_mut());
    let pcm = PcmBuffer::with_spec(stream.config.pcm_spec, samples)?;
    let captured = CapturedAudio::named(pcm, pipewire_capture_source_name(&stream.config));
    Ok(captured)
}

fn shutdown_worker_state(state: &mut PipeWireWorkerState) {
    if let Some(stream) = state.stream.as_mut() {
        stream.accepting.set(false);
        if state.recording {
            let _ = stream.stream.set_active(false);
        }
    }
    state.recording = false;
    state.stream = None;
}

fn create_persistent_stream(
    core: &pipewire::core::CoreRc,
    callback: &Arc<Mutex<Option<AudioChunkCallback>>>,
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
    let stream = pipewire::stream::StreamRc::new(core.clone(), "vinput-capture", props)
        .map_err(|error| pipewire_recording_error(&config, error))?;
    let samples = Rc::new(RefCell::new(Vec::new()));
    let accepting = Rc::new(Cell::new(false));
    let samples_for_process = Rc::clone(&samples);
    let accepting_for_process = Rc::clone(&accepting);
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
    })
}

fn capture_stream_buffer(
    stream: &pipewire::stream::Stream,
    pcm_spec: PcmSpec,
    samples: &Rc<RefCell<Vec<i16>>>,
    accepting: &Rc<Cell<bool>>,
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
mod tests {
    use pipewire::spa::static_dict;

    fn global_with_props(
        id: u32,
        type_: pipewire::types::ObjectType,
        props: Option<&pipewire::spa::utils::dict::DictRef>,
    ) -> pipewire::registry::GlobalObject<&pipewire::spa::utils::dict::DictRef> {
        pipewire::registry::GlobalObject {
            id,
            permissions: pipewire::permissions::PermissionFlags::empty(),
            type_,
            version: 0,
            props,
        }
    }

    #[test]
    fn pipewire_global_maps_audio_source_metadata() {
        let props = static_dict! {
            "media.class" => "Audio/Source",
            "node.name" => "alsa_input.usb-mic",
            "node.description" => "USB Microphone",
        };
        let global = global_with_props(42, pipewire::types::ObjectType::Node, Some(&props));

        let device = super::audio_device_from_global(&global).unwrap();
        assert_eq!(device.id, 42);
        assert_eq!(device.name, "alsa_input.usb-mic");
        assert_eq!(device.description, "USB Microphone");
    }

    #[test]
    fn pipewire_global_ignores_non_source_nodes() {
        let sink_props = static_dict! {
            "media.class" => "Audio/Sink",
            "node.name" => "alsa_output.speaker",
            "node.description" => "Speakers",
        };
        let source_props = static_dict! {
            "media.class" => "Audio/Source",
            "node.name" => "alsa_input.usb-mic",
        };
        let sink = global_with_props(7, pipewire::types::ObjectType::Node, Some(&sink_props));
        let device = global_with_props(8, pipewire::types::ObjectType::Device, Some(&source_props));
        let missing_props = global_with_props(9, pipewire::types::ObjectType::Node, None);

        assert_eq!(super::audio_device_from_global(&sink), None);
        assert_eq!(super::audio_device_from_global(&device), None);
        assert_eq!(super::audio_device_from_global(&missing_props), None);
    }

    #[test]
    fn pipewire_global_defaults_missing_name_fields() {
        let props = static_dict! {
            "media.class" => "Audio/Source",
        };
        let global = global_with_props(13, pipewire::types::ObjectType::Node, Some(&props));

        let device = super::audio_device_from_global(&global).unwrap();
        assert_eq!(device.id, 13);
        assert_eq!(device.name, "");
        assert_eq!(device.description, "");
    }

    #[test]
    fn pipewire_probe_initializes_client_library() {
        super::probe_client_linkage();
    }

    #[test]
    fn pipewire_live_test_env_gates_are_explicit() {
        assert_eq!(
            super::TEST_PIPEWIRE_ENUMERATE_ENV,
            "VINPUT_TEST_PIPEWIRE_ENUMERATE"
        );
        assert_eq!(
            super::TEST_PIPEWIRE_CONTEXT_ENV,
            "VINPUT_TEST_PIPEWIRE_CONTEXT"
        );
        assert_eq!(
            super::TEST_PIPEWIRE_RECORD_ENV,
            "VINPUT_TEST_PIPEWIRE_RECORD"
        );
        assert!(!super::TEST_PIPEWIRE_ENUMERATE_ENV.is_empty());
        assert!(!super::TEST_PIPEWIRE_CONTEXT_ENV.is_empty());
        assert!(!super::TEST_PIPEWIRE_RECORD_ENV.is_empty());
    }

    #[test]
    fn pipewire_capture_reuse_is_enabled_by_default_and_has_legacy_opt_outs() {
        assert_eq!(super::CAPTURE_REUSE_ENV, "VINPUT_CAPTURE_REUSE");
        for value in [None, Some(""), Some("1"), Some("true"), Some("yes")] {
            assert!(super::capture_stream_reuse_enabled_from(value));
        }
        for value in [
            Some("0"),
            Some("false"),
            Some("False"),
            Some("no"),
            Some("No"),
        ] {
            assert!(!super::capture_stream_reuse_enabled_from(value));
        }
    }

    #[test]
    fn pipewire_stream_reuse_requires_identical_target_and_pcm_plan() {
        let default = super::PipeWireStreamConfig::for_target(super::CaptureTarget::Default);
        let same = default.clone();
        let other = super::PipeWireStreamConfig::for_target(super::CaptureTarget::Object(
            "alsa_input.usb-mic".to_owned(),
        ));

        assert!(!super::can_reuse_stream(None, &default));
        assert!(super::can_reuse_stream(Some(&default), &same));
        assert!(!super::can_reuse_stream(Some(&default), &other));
    }

    #[test]
    fn pipewire_recording_pcm_policy_matches_asr_default() {
        assert_eq!(super::RECORDING_FORMAT, "S16LE");
        assert_eq!(
            super::RECORDING_SAMPLE_RATE_HZ,
            super::DEFAULT_SAMPLE_RATE_HZ
        );
        assert_eq!(super::RECORDING_CHANNELS, super::DEFAULT_CHANNELS);
        assert_eq!(
            super::recording_pcm_spec(),
            super::PcmSpec::mono_i16(super::DEFAULT_SAMPLE_RATE_HZ)
        );
    }

    #[test]
    fn pipewire_stream_config_preserves_target_and_pcm_policy() {
        let config = super::PipeWireStreamConfig::for_target(super::CaptureTarget::Object(
            "alsa_input.test".to_owned(),
        ));

        assert_eq!(
            config.target,
            super::CaptureTarget::Object("alsa_input.test".to_owned())
        );
        assert_eq!(config.format, super::RECORDING_FORMAT);
        assert_eq!(config.pcm_spec, super::recording_pcm_spec());
    }

    #[test]
    fn pipewire_recorder_tracks_idle_state_and_stream_plan() {
        let mut recorder = super::PipeWireAudioRecorder::new();

        super::AudioRecorder::set_chunk_callback(&mut recorder, None);

        assert_eq!(recorder.target(), &super::CaptureTarget::Default);
        assert_eq!(
            recorder.stream_config(),
            &super::PipeWireStreamConfig::for_target(super::CaptureTarget::Default)
        );
        assert!(!super::AudioRecorder::is_recording(&recorder));
        assert_eq!(
            super::AudioRecorder::stop_and_get_buffer(&mut recorder).unwrap_err(),
            super::AudioError::RecorderNotRecording
        );
        super::AudioRecorder::cancel_recording(&mut recorder).unwrap();
    }

    #[test]
    fn pipewire_recording_params_encode_requested_audio_policy() {
        let config = super::PipeWireStreamConfig::for_target(super::CaptureTarget::Object(
            "alsa_input.usb-mic".to_owned(),
        ));
        let values = super::pipewire_recording_param_values(&config).unwrap();
        let pod = pipewire::spa::pod::Pod::from_bytes(&values).unwrap();
        let mut audio_info = pipewire::spa::param::audio::AudioInfoRaw::new();
        audio_info.parse(pod).unwrap();

        assert_eq!(
            audio_info.format(),
            pipewire::spa::param::audio::AudioFormat::S16LE
        );
        assert_eq!(audio_info.rate(), super::RECORDING_SAMPLE_RATE_HZ);
        assert_eq!(audio_info.channels(), u32::from(super::RECORDING_CHANNELS));
        assert_eq!(
            super::pipewire_capture_source_name(&config),
            "pipewire:alsa_input.usb-mic"
        );
    }

    #[test]
    fn pipewire_recording_error_includes_stream_plan() {
        let config = super::PipeWireStreamConfig::for_target(super::CaptureTarget::Object(
            "alsa_input.usb-mic".to_owned(),
        ));

        let error = super::pipewire_recording_error(&config, "worker panicked").to_string();

        assert!(error.contains("PipeWire recorder stream setup failed"));
        assert!(error.contains("alsa_input.usb-mic"));
        assert!(error.contains("S16LE"));
        assert!(error.contains("16000"));
        assert!(error.contains("channels: 1"));
        assert!(error.contains("worker panicked"));
    }

    #[test]
    fn pipewire_recorder_live_capture_when_enabled() {
        if !super::live_test_enabled(super::TEST_PIPEWIRE_RECORD_ENV) {
            return;
        }
        let mut recorder = super::PipeWireAudioRecorder::new();
        super::AudioRecorder::begin_recording(&mut recorder, super::CaptureTarget::Default)
            .unwrap();

        assert!(super::AudioRecorder::is_recording(&recorder));
        std::thread::sleep(std::time::Duration::from_millis(100));
        let captured = super::AudioRecorder::stop_and_get_buffer(&mut recorder).unwrap();

        assert!(!super::AudioRecorder::is_recording(&recorder));
        assert_eq!(captured.pcm.spec(), super::recording_pcm_spec());
        assert_eq!(captured.source_name.as_deref(), Some("pipewire:default"));
    }

    #[test]
    fn pipewire_recorder_live_callback_survives_multiple_recordings_when_enabled() {
        if !super::live_test_enabled(super::TEST_PIPEWIRE_RECORD_ENV) {
            return;
        }
        let mut recorder = super::PipeWireAudioRecorder::new();
        let callback_count = std::sync::Arc::new(std::sync::Mutex::new(0usize));
        let callback_count_for_callback = std::sync::Arc::clone(&callback_count);
        super::AudioRecorder::set_chunk_callback(
            &mut recorder,
            Some(Box::new(move |_chunk| {
                *callback_count_for_callback.lock().unwrap() += 1;
            })),
        );

        for index in 0..2 {
            super::AudioRecorder::begin_recording(&mut recorder, super::CaptureTarget::Default)
                .unwrap();
            assert_eq!(recorder.last_start_reused, index > 0);
            std::thread::sleep(std::time::Duration::from_millis(100));
            let captured = super::AudioRecorder::stop_and_get_buffer(&mut recorder).unwrap();
            assert_eq!(captured.pcm.spec(), super::recording_pcm_spec());
        }

        assert!(*callback_count.lock().unwrap() >= 2);
    }

    #[test]
    fn pipewire_enumerator_lists_sources_when_enabled() {
        if !super::live_test_enabled(super::TEST_PIPEWIRE_ENUMERATE_ENV) {
            return;
        }
        let mut enumerator = super::PipeWireDeviceEnumerator;
        let _devices =
            super::AudioDeviceEnumerator::enumerate_audio_sources(&mut enumerator).unwrap();
    }

    #[test]
    fn pipewire_probe_creates_client_context_when_enabled() {
        if !super::live_test_enabled(super::TEST_PIPEWIRE_CONTEXT_ENV) {
            return;
        }
        super::probe_client_context().unwrap();
    }
}

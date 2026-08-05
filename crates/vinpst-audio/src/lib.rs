//! Pure PCM audio utilities used before the real `PipeWire` capture layer lands.
//!
//! This crate deliberately starts without `PipeWire`.  It owns typed PCM buffers
//! and deterministic transforms so audio behavior can be tested independently
//! from desktop/audio-server integration.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(feature = "pipewire-backend")]
pub mod pipewire_backend;

/// Default sample rate used by the original daemon's ASR pipeline.
pub const DEFAULT_SAMPLE_RATE_HZ: u32 = 16_000;

/// Default channel count for mono ASR audio.
pub const DEFAULT_CHANNELS: u16 = 1;

/// Signed 16-bit PCM layout metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PcmSpec {
    /// Sample rate in hertz.
    pub sample_rate_hz: u32,
    /// Number of interleaved channels.
    #[serde(default = "default_channels")]
    pub channels: u16,
}

impl PcmSpec {
    /// Creates a mono signed 16-bit PCM spec.
    #[must_use]
    pub const fn mono_i16(sample_rate_hz: u32) -> Self {
        Self {
            sample_rate_hz,
            channels: DEFAULT_CHANNELS,
        }
    }

    /// Validates sample rate and channel count.
    pub fn validate(self) -> Result<Self, AudioError> {
        if self.sample_rate_hz == 0 {
            return Err(AudioError::InvalidSampleRate(self.sample_rate_hz));
        }
        if self.channels == 0 {
            return Err(AudioError::InvalidChannelCount(self.channels));
        }
        Ok(self)
    }
}

impl Default for PcmSpec {
    fn default() -> Self {
        Self::mono_i16(DEFAULT_SAMPLE_RATE_HZ)
    }
}

/// Mono signed 16-bit PCM buffer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PcmBuffer {
    spec: PcmSpec,
    samples: Vec<i16>,
}

const fn default_channels() -> u16 {
    DEFAULT_CHANNELS
}

/// Encodes signed 16-bit PCM samples as little-endian bytes.
#[must_use]
pub fn i16_samples_to_le_bytes(samples: &[i16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}
/// Frame range used to split PCM into streaming-safe chunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PcmChunkRange {
    /// Start frame index in the source buffer.
    pub start_frame: usize,
    /// Number of complete interleaved frames in this chunk.
    pub frame_len: usize,
}

impl PcmChunkRange {
    fn sample_range(self, channels: usize) -> std::ops::Range<usize> {
        let start = self.start_frame * channels;
        let end = start + (self.frame_len * channels);
        start..end
    }
}

impl PcmBuffer {
    /// Creates a mono PCM buffer with the given sample rate.
    pub fn new(sample_rate_hz: u32, samples: impl Into<Vec<i16>>) -> Result<Self, AudioError> {
        Self::with_spec(PcmSpec::mono_i16(sample_rate_hz), samples)
    }

    /// Creates a PCM buffer with explicit layout metadata.
    pub fn with_spec(spec: PcmSpec, samples: impl Into<Vec<i16>>) -> Result<Self, AudioError> {
        let spec = spec.validate()?;
        let samples = samples.into();
        if samples.len() % usize::from(spec.channels) != 0 {
            return Err(AudioError::UnalignedSamples {
                samples: samples.len(),
                channels: spec.channels,
            });
        }
        Ok(Self { spec, samples })
    }

    /// Decodes an uncompressed RIFF/WAVE signed 16-bit PCM buffer.
    pub fn from_wav_pcm16le_bytes(bytes: &[u8]) -> Result<Self, AudioError> {
        decode_wav_pcm16le(bytes)
    }

    /// Decodes raw signed 16-bit little-endian PCM bytes with explicit layout metadata.
    pub fn from_pcm16le_bytes(spec: PcmSpec, bytes: &[u8]) -> Result<Self, AudioError> {
        if !bytes.len().is_multiple_of(2) {
            return Err(AudioError::OddPcmByteCount(bytes.len()));
        }
        let samples = bytes
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        Self::with_spec(spec, samples)
    }

    /// Creates a 16 kHz mono PCM buffer.
    pub fn at_default_rate(samples: impl Into<Vec<i16>>) -> Self {
        Self {
            spec: PcmSpec::default(),
            samples: samples.into(),
        }
    }

    /// Returns the PCM layout metadata.
    #[must_use]
    pub const fn spec(&self) -> PcmSpec {
        self.spec
    }

    /// Returns the sample rate.
    #[must_use]
    pub const fn sample_rate_hz(&self) -> u32 {
        self.spec.sample_rate_hz
    }

    /// Returns the channel count.
    #[must_use]
    pub const fn channels(&self) -> u16 {
        self.spec.channels
    }

    /// Returns the raw samples.
    #[must_use]
    pub fn samples(&self) -> &[i16] {
        &self.samples
    }

    /// Returns mutable raw samples.
    #[must_use]
    pub fn samples_mut(&mut self) -> &mut [i16] {
        &mut self.samples
    }

    /// Returns true when no samples are present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Returns the number of raw i16 samples.
    #[must_use]
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Returns the number of PCM frames.
    #[must_use]
    pub fn frame_len(&self) -> usize {
        self.samples.len() / usize::from(self.spec.channels)
    }

    /// Plans streaming-safe chunk ranges in complete PCM frames.
    ///
    /// Ranges are expressed in frames rather than samples, so multi-channel
    /// interleaved buffers are never split in the middle of a frame. Empty
    /// buffers return an empty range list.
    pub fn chunk_ranges_by_frames(
        &self,
        max_frames_per_chunk: usize,
    ) -> Result<Vec<PcmChunkRange>, AudioError> {
        if max_frames_per_chunk == 0 {
            return Err(AudioError::InvalidChunkFrameCount(max_frames_per_chunk));
        }
        let mut ranges = Vec::new();
        let mut start_frame = 0;
        let total_frames = self.frame_len();
        while start_frame < total_frames {
            let frame_len = (total_frames - start_frame).min(max_frames_per_chunk);
            ranges.push(PcmChunkRange {
                start_frame,
                frame_len,
            });
            start_frame += frame_len;
        }
        Ok(ranges)
    }

    /// Splits this PCM buffer into streaming-safe chunks by complete frames.
    pub fn chunks_by_frames(
        &self,
        max_frames_per_chunk: usize,
    ) -> Result<Vec<PcmBuffer>, AudioError> {
        let channels = usize::from(self.spec.channels);
        self.chunk_ranges_by_frames(max_frames_per_chunk)?
            .into_iter()
            .map(|range| {
                let samples = self.samples[range.sample_range(channels)].to_vec();
                PcmBuffer::with_spec(self.spec, samples)
            })
            .collect()
    }

    /// Returns duration in milliseconds, rounded down.
    #[must_use]
    pub fn duration_ms(&self) -> u64 {
        let frames = u64::try_from(self.frame_len()).unwrap_or(u64::MAX);
        frames.saturating_mul(1000) / u64::from(self.spec.sample_rate_hz)
    }

    /// Returns the peak absolute amplitude as an `i16`-range value.
    #[must_use]
    pub fn peak_abs(&self) -> i16 {
        self.samples
            .iter()
            .map(|sample| sample.unsigned_abs())
            .max()
            .unwrap_or(0)
            .min(i16::MAX as u16)
            .cast_signed()
    }

    /// Returns a copy with gain applied using saturating i16 conversion.
    #[must_use]
    pub fn with_gain(&self, gain: f32) -> Self {
        let mut next = self.clone();
        next.apply_gain(gain);
        next
    }

    /// Applies gain in place using saturating i16 conversion.
    pub fn apply_gain(&mut self, gain: f32) {
        if !gain.is_finite() {
            return;
        }
        for sample in &mut self.samples {
            *sample = scale_sample(*sample, gain);
        }
    }

    /// Returns a copy normalized to a target peak.
    #[must_use]
    pub fn normalized_to_peak(&self, target_peak: i16) -> Self {
        let mut next = self.clone();
        next.normalize_to_peak(target_peak);
        next
    }

    /// Normalizes in place to a target peak.
    pub fn normalize_to_peak(&mut self, target_peak: i16) {
        let current_peak = self.peak_abs();
        if current_peak == 0 || target_peak <= 0 {
            return;
        }
        let gain = f32::from(target_peak) / f32::from(current_peak);
        self.apply_gain(gain);
    }

    /// Returns whether all samples are below or equal to the silence threshold.
    #[must_use]
    pub fn is_silent(&self, threshold_abs: i16) -> bool {
        let threshold = threshold_abs.unsigned_abs();
        self.samples
            .iter()
            .all(|sample| sample.unsigned_abs() <= threshold)
    }

    /// Returns a copy with leading and trailing silent frames removed.
    #[must_use]
    pub fn trimmed_silence(&self, threshold_abs: i16) -> Self {
        let threshold = threshold_abs.unsigned_abs();
        let channels = usize::from(self.spec.channels);
        let start_frame = self
            .samples
            .chunks_exact(channels)
            .position(|frame| frame.iter().any(|sample| sample.unsigned_abs() > threshold));
        let Some(start_frame) = start_frame else {
            return Self {
                spec: self.spec,
                samples: Vec::new(),
            };
        };
        let end_frame = self
            .samples
            .chunks_exact(channels)
            .rposition(|frame| frame.iter().any(|sample| sample.unsigned_abs() > threshold))
            .expect("start frame exists, so end frame exists");
        let start = start_frame * channels;
        let end = (end_frame + 1) * channels;
        Self {
            spec: self.spec,
            samples: self.samples[start..end].to_vec(),
        }
    }
}

/// Deterministic audio processing policy applied before ASR delivery.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AudioProcessingOptions {
    /// Absolute threshold used to trim quiet leading/trailing regions.
    pub silence_threshold_abs: i16,
    /// Optional target peak for normalization.
    #[serde(default)]
    pub normalize_to_peak: Option<i16>,
    /// Gain multiplier applied after optional normalization.
    pub input_gain: f32,
}

impl AudioProcessingOptions {
    /// Creates processing options.
    #[must_use]
    pub const fn new(
        silence_threshold_abs: i16,
        normalize_to_peak: Option<i16>,
        input_gain: f32,
    ) -> Self {
        Self {
            silence_threshold_abs,
            normalize_to_peak,
            input_gain,
        }
    }

    /// Applies trim, optional normalization, and gain in deterministic order.
    #[must_use]
    pub fn process(&self, pcm: &PcmBuffer) -> PcmBuffer {
        let mut processed = pcm.trimmed_silence(self.silence_threshold_abs);
        if let Some(target_peak) = self.normalize_to_peak {
            processed.normalize_to_peak(target_peak);
        }
        processed.apply_gain(self.input_gain);
        processed
    }
}

/// PCM buffer plus capture metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CapturedAudio {
    /// Captured PCM samples.
    pub pcm: PcmBuffer,
    /// Optional source name, such as a `PipeWire` node or test fixture id.
    #[serde(default)]
    pub source_name: Option<String>,
}

impl CapturedAudio {
    /// Creates captured audio without source metadata.
    #[must_use]
    pub fn anonymous(pcm: PcmBuffer) -> Self {
        Self {
            pcm,
            source_name: None,
        }
    }

    /// Creates captured audio with a source name.
    #[must_use]
    pub fn named(pcm: PcmBuffer, source_name: impl Into<String>) -> Self {
        Self {
            pcm,
            source_name: Some(source_name.into()),
        }
    }

    /// Returns captured duration in milliseconds.
    #[must_use]
    pub fn duration_ms(&self) -> u64 {
        self.pcm.duration_ms()
    }
}

/// Capture target selected by config or UI.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum CaptureTarget {
    /// Use the default desktop audio source.
    #[default]
    Default,
    /// Use a concrete backend target object such as a `PipeWire` node name.
    Object(String),
}

impl CaptureTarget {
    /// Parses a config value such as `default` or a backend object id.
    pub fn from_config_value(value: &str) -> Result<Self, AudioError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(AudioError::InvalidCaptureTarget(value.to_owned()));
        }
        if trimmed == "default" {
            return Ok(Self::Default);
        }
        Ok(Self::Object(trimmed.to_owned()))
    }

    /// Returns the backend object value for non-default targets.
    #[must_use]
    pub fn target_object(&self) -> Option<&str> {
        match self {
            Self::Default => None,
            Self::Object(value) => Some(value),
        }
    }
}

/// Desktop audio source discovered by a capture backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AudioDeviceInfo {
    /// Backend-local device id, such as a `PipeWire` node id.
    pub id: u32,
    /// Stable backend object name used as a capture target.
    pub name: String,
    /// Human-readable device description.
    pub description: String,
}

impl AudioDeviceInfo {
    /// Creates audio device metadata.
    #[must_use]
    pub fn new(id: u32, name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            description: description.into(),
        }
    }

    /// Returns this device as a concrete capture target.
    #[must_use]
    pub fn capture_target(&self) -> CaptureTarget {
        CaptureTarget::Object(self.name.clone())
    }
}

/// Device enumeration contract for desktop capture backends.
pub trait AudioDeviceEnumerator: Send {
    /// List available audio sources in backend discovery order.
    fn enumerate_audio_sources(&mut self) -> Result<Vec<AudioDeviceInfo>, AudioError>;
}

/// Deterministic device enumerator for tests and CLI/UI wiring.
#[derive(Debug, Clone, Default)]
pub struct MockAudioDeviceEnumerator {
    devices: Vec<AudioDeviceInfo>,
}

impl MockAudioDeviceEnumerator {
    /// Creates a mock enumerator from a static device list.
    #[must_use]
    pub fn new(devices: impl Into<Vec<AudioDeviceInfo>>) -> Self {
        Self {
            devices: devices.into(),
        }
    }
}

impl AudioDeviceEnumerator for MockAudioDeviceEnumerator {
    fn enumerate_audio_sources(&mut self) -> Result<Vec<AudioDeviceInfo>, AudioError> {
        Ok(self.devices.clone())
    }
}

/// Callback used by streaming capture backends to forward PCM chunks.
pub type AudioChunkCallback = Box<dyn FnMut(&PcmBuffer) + Send>;

/// Stateful recorder contract mirroring the legacy daemon capture lifecycle.
pub trait AudioRecorder: Send {
    /// Begin a recording session for the selected target.
    fn begin_recording(&mut self, target: CaptureTarget) -> Result<(), AudioError>;

    /// Install or clear a callback for chunks observed while recording.
    fn set_chunk_callback(&mut self, callback: Option<AudioChunkCallback>);

    /// Stop recording and return the accumulated PCM buffer.
    fn stop_and_get_buffer(&mut self) -> Result<CapturedAudio, AudioError>;

    /// Stop recording and discard any accumulated audio.
    fn cancel_recording(&mut self) -> Result<(), AudioError>;

    /// Return whether a recording session is active.
    fn is_recording(&self) -> bool;
}

/// Deterministic stateful recorder for tests and runtime wiring.
pub struct MockAudioRecorder {
    recordings: Vec<CapturedAudio>,
    next: usize,
    recording: bool,
    target: CaptureTarget,
    chunk_callback: Option<AudioChunkCallback>,
    chunk_frames: Option<usize>,
}

impl MockAudioRecorder {
    /// Creates a mock recorder from a sequence of completed recordings.
    #[must_use]
    pub fn from_recordings(recordings: impl Into<Vec<CapturedAudio>>) -> Self {
        Self {
            recordings: recordings.into(),
            next: 0,
            recording: false,
            target: CaptureTarget::default(),
            chunk_callback: None,
            chunk_frames: None,
        }
    }

    /// Creates a mock recorder that returns one completed recording.
    #[must_use]
    pub fn once(recording: CapturedAudio) -> Self {
        Self::from_recordings(vec![recording])
    }

    /// Configures deterministic chunk callback delivery by complete PCM frames.
    ///
    /// Without this option the mock recorder forwards the full captured buffer as
    /// one callback chunk. With this option, callback chunks are split through
    /// `PcmBuffer::chunks_by_frames`, preserving interleaved frame boundaries.
    pub fn with_chunk_frames(mut self, max_frames_per_chunk: usize) -> Result<Self, AudioError> {
        if max_frames_per_chunk == 0 {
            return Err(AudioError::InvalidChunkFrameCount(max_frames_per_chunk));
        }
        self.chunk_frames = Some(max_frames_per_chunk);
        Ok(self)
    }

    /// Returns the last target passed to `begin_recording`.
    #[must_use]
    pub const fn target(&self) -> &CaptureTarget {
        &self.target
    }
}

impl AudioRecorder for MockAudioRecorder {
    fn begin_recording(&mut self, target: CaptureTarget) -> Result<(), AudioError> {
        if self.recording {
            return Err(AudioError::RecorderAlreadyRecording);
        }
        self.target = target;
        self.recording = true;
        Ok(())
    }

    fn set_chunk_callback(&mut self, callback: Option<AudioChunkCallback>) {
        self.chunk_callback = callback;
    }

    fn stop_and_get_buffer(&mut self) -> Result<CapturedAudio, AudioError> {
        if !self.recording {
            return Err(AudioError::RecorderNotRecording);
        }
        self.recording = false;
        let recording = self
            .recordings
            .get(self.next)
            .cloned()
            .ok_or(AudioError::SourceExhausted)?;
        self.next += 1;
        if let Some(callback) = &mut self.chunk_callback {
            if let Some(chunk_frames) = self.chunk_frames {
                for chunk in recording.pcm.chunks_by_frames(chunk_frames)? {
                    callback(&chunk);
                }
            } else {
                callback(&recording.pcm);
            }
        }
        Ok(recording)
    }

    fn cancel_recording(&mut self) -> Result<(), AudioError> {
        self.recording = false;
        Ok(())
    }

    fn is_recording(&self) -> bool {
        self.recording
    }
}

/// Compatibility adapter that exposes a stateful recorder as a one-shot source.
pub struct RecorderAudioSource<R> {
    recorder: R,
    target: CaptureTarget,
}

impl<R> RecorderAudioSource<R> {
    /// Creates a compatibility source for the given recorder and target.
    #[must_use]
    pub fn new(recorder: R, target: CaptureTarget) -> Self {
        Self { recorder, target }
    }

    /// Returns the wrapped recorder.
    #[must_use]
    pub const fn recorder(&self) -> &R {
        &self.recorder
    }

    /// Returns the wrapped recorder mutably.
    #[must_use]
    pub const fn recorder_mut(&mut self) -> &mut R {
        &mut self.recorder
    }

    /// Consumes the adapter and returns the wrapped recorder.
    #[must_use]
    pub fn into_recorder(self) -> R {
        self.recorder
    }
}

impl<R: AudioRecorder> AudioSource for RecorderAudioSource<R> {
    fn read_buffer(&mut self) -> Result<CapturedAudio, AudioError> {
        self.recorder.begin_recording(self.target.clone())?;
        match self.recorder.stop_and_get_buffer() {
            Ok(captured) => Ok(captured),
            Err(error) => {
                let _ = self.recorder.cancel_recording();
                Err(error)
            }
        }
    }
}

/// Audio source abstraction used before a concrete desktop backend is wired in.
pub trait AudioSource: Send {
    /// Read one PCM buffer.
    fn read_buffer(&mut self) -> Result<CapturedAudio, AudioError>;
}

/// Deterministic audio source for runtime wiring and tests.
#[derive(Debug, Clone)]
pub struct MockAudioSource {
    frames: Vec<CapturedAudio>,
    next: usize,
}

impl MockAudioSource {
    /// Creates a mock source from a sequence of buffers.
    #[must_use]
    pub fn from_frames(frames: impl Into<Vec<CapturedAudio>>) -> Self {
        Self {
            frames: frames.into(),
            next: 0,
        }
    }

    /// Creates a mock source that returns one buffer.
    #[must_use]
    pub fn once(frame: CapturedAudio) -> Self {
        Self::from_frames(vec![frame])
    }
}

impl AudioSource for MockAudioSource {
    fn read_buffer(&mut self) -> Result<CapturedAudio, AudioError> {
        let frame = self
            .frames
            .get(self.next)
            .cloned()
            .ok_or(AudioError::SourceExhausted)?;
        self.next += 1;
        Ok(frame)
    }
}

/// Compatibility recorder backed by a one-shot [`AudioSource`].
pub struct SourceAudioRecorder {
    source: Box<dyn AudioSource>,
    recording: bool,
    target: CaptureTarget,
    chunk_callback: Option<AudioChunkCallback>,
    chunk_frames: Option<usize>,
    pending_capture: Option<CapturedAudio>,
}

impl SourceAudioRecorder {
    /// Creates a stateful recorder facade for an existing audio source.
    #[must_use]
    pub fn new(source: Box<dyn AudioSource>) -> Self {
        Self {
            source,
            recording: false,
            target: CaptureTarget::default(),
            chunk_callback: None,
            chunk_frames: None,
            pending_capture: None,
        }
    }

    /// Configures deterministic chunk callback delivery by complete PCM frames.
    ///
    /// Without this option the recorder forwards the full source buffer as one
    /// callback chunk. With this option, callback chunks are split through
    /// `PcmBuffer::chunks_by_frames`, preserving interleaved frame boundaries.
    pub fn with_chunk_frames(mut self, max_frames_per_chunk: usize) -> Result<Self, AudioError> {
        if max_frames_per_chunk == 0 {
            return Err(AudioError::InvalidChunkFrameCount(max_frames_per_chunk));
        }
        self.chunk_frames = Some(max_frames_per_chunk);
        Ok(self)
    }

    /// Returns the last target passed to `begin_recording`.
    #[must_use]
    pub const fn target(&self) -> &CaptureTarget {
        &self.target
    }

    fn deliver_capture(&mut self, captured: &CapturedAudio) -> Result<(), AudioError> {
        let Some(callback) = &mut self.chunk_callback else {
            return Ok(());
        };
        if let Some(chunk_frames) = self.chunk_frames {
            for chunk in captured.pcm.chunks_by_frames(chunk_frames)? {
                callback(&chunk);
            }
        } else {
            callback(&captured.pcm);
        }
        Ok(())
    }
}

impl AudioRecorder for SourceAudioRecorder {
    fn begin_recording(&mut self, target: CaptureTarget) -> Result<(), AudioError> {
        if self.recording {
            return Err(AudioError::RecorderAlreadyRecording);
        }
        self.target = target;
        self.recording = true;
        if self.chunk_callback.is_some() {
            let captured = match self.source.read_buffer() {
                Ok(captured) => captured,
                Err(error) => {
                    self.recording = false;
                    return Err(error);
                }
            };
            if let Err(error) = self.deliver_capture(&captured) {
                self.recording = false;
                return Err(error);
            }
            self.pending_capture = Some(captured);
        }
        Ok(())
    }

    fn set_chunk_callback(&mut self, callback: Option<AudioChunkCallback>) {
        self.chunk_callback = callback;
    }

    fn stop_and_get_buffer(&mut self) -> Result<CapturedAudio, AudioError> {
        if !self.recording {
            return Err(AudioError::RecorderNotRecording);
        }
        self.recording = false;
        if let Some(captured) = self.pending_capture.take() {
            return Ok(captured);
        }
        let captured = self.source.read_buffer()?;
        self.deliver_capture(&captured)?;
        Ok(captured)
    }

    fn cancel_recording(&mut self) -> Result<(), AudioError> {
        self.recording = false;
        self.pending_capture = None;
        Ok(())
    }

    fn is_recording(&self) -> bool {
        self.recording
    }
}

/// Audio helper errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AudioError {
    /// Sample rate must not be zero.
    #[error("invalid sample rate: {0}")]
    InvalidSampleRate(u32),
    /// Channel count must not be zero.
    #[error("invalid channel count: {0}")]
    InvalidChannelCount(u16),
    /// Raw sample count must contain complete interleaved frames.
    #[error("sample count {samples} is not aligned to channel count {channels}")]
    UnalignedSamples {
        /// Raw sample count.
        samples: usize,
        /// Configured channel count.
        channels: u16,
    },
    /// Raw PCM input must contain complete little-endian i16 samples.
    #[error("PCM input contains an odd number of bytes: {0}")]
    OddPcmByteCount(usize),
    /// RIFF/WAVE input was not uncompressed signed 16-bit PCM.
    #[error("invalid WAV file: {0}")]
    InvalidWav(String),
    /// Chunk size must contain at least one frame.
    #[error("invalid PCM chunk frame count: {0}")]
    InvalidChunkFrameCount(usize),
    /// Empty mock buffer list.
    #[error("no more buffers")]
    SourceExhausted,
    /// Capture target is blank after trimming.
    #[error("invalid capture target: {0:?}")]
    InvalidCaptureTarget(String),
    /// Recorder was asked to begin while already recording.
    #[error("recorder is already recording")]
    RecorderAlreadyRecording,
    /// Recorder was asked to stop while idle.
    #[error("recorder is not recording")]
    RecorderNotRecording,
    /// Audio recording backend is linked but not usable yet.
    #[error("audio recording backend is unavailable: {0}")]
    RecordingBackendUnavailable(String),
    /// Audio device enumeration failed.
    #[error("audio device enumeration failed: {0}")]
    DeviceEnumerationFailed(String),
}

fn decode_wav_pcm16le(bytes: &[u8]) -> Result<PcmBuffer, AudioError> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(invalid_wav("missing RIFF/WAVE header"));
    }

    let mut format: Option<PcmSpec> = None;
    let mut data: Option<&[u8]> = None;
    let mut offset = 12;
    while offset + 8 <= bytes.len() {
        let chunk_id = &bytes[offset..offset + 4];
        let chunk_len = read_le_u32(&bytes[offset + 4..offset + 8])? as usize;
        let chunk_start = offset + 8;
        let chunk_end = chunk_start
            .checked_add(chunk_len)
            .ok_or_else(|| invalid_wav("chunk size overflow"))?;
        if chunk_end > bytes.len() {
            return Err(invalid_wav("chunk extends past end of file"));
        }
        let chunk = &bytes[chunk_start..chunk_end];
        match chunk_id {
            b"fmt " => format = Some(parse_wav_fmt(chunk)?),
            b"data" => data = Some(chunk),
            _ => {}
        }
        offset = chunk_end + (chunk_len % 2);
    }

    let spec = format.ok_or_else(|| invalid_wav("missing fmt chunk"))?;
    let data = data.ok_or_else(|| invalid_wav("missing data chunk"))?;
    if data.len() % 2 != 0 {
        return Err(invalid_wav("data chunk has an odd byte count"));
    }
    let samples = data
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    PcmBuffer::with_spec(spec, samples)
}

fn parse_wav_fmt(chunk: &[u8]) -> Result<PcmSpec, AudioError> {
    if chunk.len() < 16 {
        return Err(invalid_wav("fmt chunk is too short"));
    }
    let format_tag = read_le_u16(&chunk[0..2])?;
    let channels = read_le_u16(&chunk[2..4])?;
    let sample_rate_hz = read_le_u32(&chunk[4..8])?;
    let byte_rate = read_le_u32(&chunk[8..12])?;
    let block_align = read_le_u16(&chunk[12..14])?;
    let bits_per_sample = read_le_u16(&chunk[14..16])?;
    if format_tag != 1 {
        return Err(invalid_wav("only PCM format tag 1 is supported"));
    }
    if bits_per_sample != 16 {
        return Err(invalid_wav("only 16-bit samples are supported"));
    }
    let spec = PcmSpec {
        sample_rate_hz,
        channels,
    }
    .validate()?;
    let expected_block_align = spec
        .channels
        .checked_mul(bits_per_sample / 8)
        .ok_or_else(|| invalid_wav("block align overflow"))?;
    if block_align != expected_block_align {
        return Err(invalid_wav("block align does not match channel count"));
    }
    let expected_byte_rate = spec
        .sample_rate_hz
        .checked_mul(u32::from(expected_block_align))
        .ok_or_else(|| invalid_wav("byte rate overflow"))?;
    if byte_rate != expected_byte_rate {
        return Err(invalid_wav("byte rate does not match sample format"));
    }
    Ok(spec)
}

fn read_le_u16(bytes: &[u8]) -> Result<u16, AudioError> {
    let raw: [u8; 2] = bytes
        .try_into()
        .map_err(|_| invalid_wav("expected 2-byte little-endian integer"))?;
    Ok(u16::from_le_bytes(raw))
}

fn read_le_u32(bytes: &[u8]) -> Result<u32, AudioError> {
    let raw: [u8; 4] = bytes
        .try_into()
        .map_err(|_| invalid_wav("expected 4-byte little-endian integer"))?;
    Ok(u32::from_le_bytes(raw))
}

fn invalid_wav(message: impl Into<String>) -> AudioError {
    AudioError::InvalidWav(message.into())
}

fn scale_sample(sample: i16, gain: f32) -> i16 {
    let scaled = f32::from(sample) * gain;
    if scaled.is_nan() {
        return sample;
    }
    let rounded = scaled.round();
    if rounded <= f32::from(i16::MIN) {
        i16::MIN
    } else if rounded >= f32::from(i16::MAX) {
        i16::MAX
    } else {
        #[allow(clippy::cast_possible_truncation)]
        {
            rounded as i16
        }
    }
}

#[cfg(test)]
mod tests;

//! Shared recognition-session ownership and live chunk delivery.

use std::sync::{Arc, Mutex, MutexGuard};

use vinpst_asr::{AsrError, AudioDeliveryMode, RecognitionEvent, RecognitionSession};
use vinpst_audio::{AudioChunkCallback, PcmBuffer, PcmSpec};

const STREAMING_CHUNK_FRAMES: usize = 800;

type SharedSession = Arc<Mutex<Box<dyn RecognitionSession>>>;

/// Buffers capture chunks until ASR session construction finishes.
pub(super) struct CaptureStartGate {
    state: Arc<Mutex<CaptureStartGateState>>,
}

#[derive(Default)]
struct CaptureStartGateState {
    armed: bool,
    callback: Option<AudioChunkCallback>,
    buffered: Vec<PcmBuffer>,
}

impl CaptureStartGate {
    /// Creates a gate and the callback installed before capture begins.
    pub(super) fn new() -> (Self, AudioChunkCallback) {
        let state = Arc::new(Mutex::new(CaptureStartGateState::default()));
        let callback_state = Arc::clone(&state);
        let callback: AudioChunkCallback = Box::new(move |pcm| {
            let Ok(mut state) = callback_state.lock() else {
                return;
            };
            if state.armed {
                if let Some(callback) = state.callback.as_mut() {
                    callback(pcm);
                }
            } else {
                state.buffered.push(pcm.clone());
            }
        });
        (Self { state }, callback)
    }

    /// Installs the session callback and replays chunks captured during setup.
    pub(super) fn arm(&self, callback: Option<AudioChunkCallback>) -> Result<(), AsrError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| AsrError::Backend("capture start gate lock was poisoned".to_owned()))?;
        state.armed = true;
        state.callback = callback;
        let buffered = std::mem::take(&mut state.buffered);
        if let Some(callback) = state.callback.as_mut() {
            for pcm in &buffered {
                callback(pcm);
            }
        }
        Ok(())
    }
}

/// Active ASR session shared with an audio-recorder callback when chunked delivery is requested.
pub(super) struct ActiveRecognitionSession {
    session: SharedSession,
    delivery_mode: AudioDeliveryMode,
    streaming_state: Option<Arc<Mutex<StreamingDeliveryState>>>,
}

impl ActiveRecognitionSession {
    /// Wraps a new recognition session and returns an optional live recorder callback.
    pub(super) fn new(
        session: Box<dyn RecognitionSession>,
        delivery_mode: AudioDeliveryMode,
        input_gain: f32,
    ) -> (Self, Option<AudioChunkCallback>) {
        let session = Arc::new(Mutex::new(session));
        if delivery_mode == AudioDeliveryMode::Chunked {
            let streaming_state = Arc::new(Mutex::new(StreamingDeliveryState::new(input_gain)));
            let callback_session = Arc::clone(&session);
            let callback_state = Arc::clone(&streaming_state);
            let callback: AudioChunkCallback = Box::new(move |pcm| {
                deliver_streaming_chunk(&callback_session, &callback_state, pcm);
            });
            (
                Self {
                    session,
                    delivery_mode,
                    streaming_state: Some(streaming_state),
                },
                Some(callback),
            )
        } else {
            (
                Self {
                    session,
                    delivery_mode,
                    streaming_state: None,
                },
                None,
            )
        }
    }

    /// Returns how audio is delivered to this session.
    pub(super) const fn delivery_mode(&self) -> AudioDeliveryMode {
        self.delivery_mode
    }

    /// Pushes one complete buffer to a buffered backend.
    pub(super) fn push_buffered_pcm(&self, pcm: &PcmBuffer) -> Result<(), AsrError> {
        self.lock_session()?.push_pcm(pcm)
    }

    /// Flushes a final short streaming batch and returns events already polled by callbacks.
    pub(super) fn finish_streaming_delivery(&self) -> Result<Vec<RecognitionEvent>, AsrError> {
        let Some(state) = &self.streaming_state else {
            return Ok(Vec::new());
        };

        let final_batch = {
            let mut state = lock_streaming_state(state)?;
            if let Some(error) = state.error.take() {
                return Err(error);
            }
            state.take_final_batch()?
        };
        if let Some(batch) = final_batch {
            deliver_batch(&self.session, state, &batch)?;
        }

        let mut state = lock_streaming_state(state)?;
        if let Some(error) = state.error.take() {
            return Err(error);
        }
        Ok(std::mem::take(&mut state.events))
    }

    /// Takes new text emitted over the legacy `RecognitionPartial` signal.
    ///
    /// Upstream projects both partial and early final events onto that signal.
    /// Final events remain retained so stop-time payload construction still sees them.
    pub(super) fn take_streaming_partial_texts(&self) -> Result<Vec<String>, AsrError> {
        let Some(state) = &self.streaming_state else {
            return Ok(Vec::new());
        };
        // A streaming backend may produce output asynchronously after the audio
        // push returns (for example a long-lived command helper). Poll the
        // backend on every live tick before projecting queued text. Do this
        // before taking the streaming-state lock to preserve the session ->
        // state lock ordering used by live audio delivery.
        let new_events = self.lock_session()?.poll_events()?;
        let mut state = lock_streaming_state(state)?;
        state.events.extend(new_events);
        let mut retained = Vec::with_capacity(state.events.len());
        let mut partials = Vec::new();
        for event in std::mem::take(&mut state.events) {
            match event {
                RecognitionEvent::PartialText { text } => {
                    if !text.is_empty() {
                        partials.push(text);
                    }
                }
                RecognitionEvent::FinalText { text } => {
                    if !text.is_empty() {
                        partials.push(text.clone());
                    }
                    retained.push(RecognitionEvent::FinalText { text });
                }
                event => retained.push(event),
            }
        }
        state.events = retained;
        Ok(partials)
    }

    /// Drains events not already collected by a streaming callback.
    pub(super) fn poll_events(&self) -> Result<Vec<RecognitionEvent>, AsrError> {
        self.lock_session()?.poll_events()
    }

    /// Marks audio input complete.
    pub(super) fn finish(&self) -> Result<(), AsrError> {
        self.lock_session()?.finish()
    }

    /// Cancels the session.
    pub(super) fn cancel(&self) -> Result<(), AsrError> {
        self.lock_session()?.cancel()
    }

    fn lock_session(&self) -> Result<MutexGuard<'_, Box<dyn RecognitionSession>>, AsrError> {
        lock_session(&self.session)
    }
}

fn deliver_streaming_chunk(
    session: &SharedSession,
    state: &Arc<Mutex<StreamingDeliveryState>>,
    pcm: &PcmBuffer,
) {
    let batches = match lock_streaming_state(state).and_then(|mut state| state.push_chunk(pcm)) {
        Ok(batches) => batches,
        Err(error) => {
            record_streaming_error(state, error);
            return;
        }
    };

    for batch in batches {
        if let Err(error) = deliver_batch(session, state, &batch) {
            record_streaming_error(state, error);
            return;
        }
    }
}

fn deliver_batch(
    session: &SharedSession,
    state: &Arc<Mutex<StreamingDeliveryState>>,
    batch: &PcmBuffer,
) -> Result<(), AsrError> {
    let events = {
        let mut session = lock_session(session)?;
        session.push_pcm(batch)?;
        session.poll_events()?
    };
    lock_streaming_state(state)?.events.extend(events);
    Ok(())
}

fn record_streaming_error(state: &Arc<Mutex<StreamingDeliveryState>>, error: AsrError) {
    if let Ok(mut state) = state.lock()
        && state.error.is_none()
    {
        state.error = Some(error);
    }
}

fn lock_session(
    session: &SharedSession,
) -> Result<MutexGuard<'_, Box<dyn RecognitionSession>>, AsrError> {
    session
        .lock()
        .map_err(|_| AsrError::Backend("recognition session lock was poisoned".to_owned()))
}

fn lock_streaming_state(
    state: &Arc<Mutex<StreamingDeliveryState>>,
) -> Result<MutexGuard<'_, StreamingDeliveryState>, AsrError> {
    state
        .lock()
        .map_err(|_| AsrError::Backend("streaming delivery state lock was poisoned".to_owned()))
}

struct StreamingDeliveryState {
    input_gain: f32,
    pcm_spec: Option<PcmSpec>,
    pending_samples: Vec<i16>,
    events: Vec<RecognitionEvent>,
    error: Option<AsrError>,
}

impl StreamingDeliveryState {
    const fn new(input_gain: f32) -> Self {
        Self {
            input_gain,
            pcm_spec: None,
            pending_samples: Vec::new(),
            events: Vec::new(),
            error: None,
        }
    }

    fn push_chunk(&mut self, pcm: &PcmBuffer) -> Result<Vec<PcmBuffer>, AsrError> {
        if self.error.is_some() || pcm.is_empty() {
            return Ok(Vec::new());
        }
        self.ensure_pcm_spec(pcm.spec())?;

        let mut processed = pcm.clone();
        processed.apply_gain(self.input_gain);
        self.pending_samples.extend_from_slice(processed.samples());
        self.take_complete_batches()
    }

    fn ensure_pcm_spec(&mut self, spec: PcmSpec) -> Result<(), AsrError> {
        match self.pcm_spec {
            Some(existing) if existing != spec => Err(AsrError::Backend(format!(
                "streaming PCM metadata changed from {} Hz/{} channels to {} Hz/{} channels",
                existing.sample_rate_hz, existing.channels, spec.sample_rate_hz, spec.channels
            ))),
            Some(_) => Ok(()),
            None => {
                self.pcm_spec = Some(spec);
                Ok(())
            }
        }
    }

    fn take_complete_batches(&mut self) -> Result<Vec<PcmBuffer>, AsrError> {
        let Some(spec) = self.pcm_spec else {
            return Ok(Vec::new());
        };
        let samples_per_batch = STREAMING_CHUNK_FRAMES * usize::from(spec.channels);
        let mut batches = Vec::new();
        while self.pending_samples.len() >= samples_per_batch {
            let samples: Vec<_> = self.pending_samples.drain(..samples_per_batch).collect();
            batches.push(
                PcmBuffer::with_spec(spec, samples).map_err(|error| audio_to_asr_error(&error))?,
            );
        }
        Ok(batches)
    }

    fn take_final_batch(&mut self) -> Result<Option<PcmBuffer>, AsrError> {
        if self.pending_samples.is_empty() {
            return Ok(None);
        }
        let spec = self.pcm_spec.ok_or_else(|| {
            AsrError::Backend("streaming PCM samples have no layout metadata".to_owned())
        })?;
        let samples = std::mem::take(&mut self.pending_samples);
        PcmBuffer::with_spec(spec, samples)
            .map(Some)
            .map_err(|error| audio_to_asr_error(&error))
    }
}

fn audio_to_asr_error(error: &vinpst_audio::AudioError) -> AsrError {
    AsrError::Backend(format!("invalid streaming PCM chunk: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CapturingSession {
        pushes: Arc<Mutex<Vec<PcmBuffer>>>,
    }

    impl RecognitionSession for CapturingSession {
        fn push_audio(&mut self, samples: &[i16]) -> Result<(), AsrError> {
            self.pushes
                .lock()
                .expect("push log lock poisoned")
                .push(PcmBuffer::at_default_rate(samples.to_vec()));
            Ok(())
        }

        fn push_pcm(&mut self, pcm: &PcmBuffer) -> Result<(), AsrError> {
            self.pushes
                .lock()
                .expect("push log lock poisoned")
                .push(pcm.clone());
            Ok(())
        }

        fn finish(&mut self) -> Result<(), AsrError> {
            Ok(())
        }

        fn cancel(&mut self) -> Result<(), AsrError> {
            Ok(())
        }

        fn poll_events(&mut self) -> Result<Vec<RecognitionEvent>, AsrError> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn capture_start_gate_replays_early_chunks_in_order() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let (gate, mut startup_callback) = CaptureStartGate::new();
        startup_callback(&PcmBuffer::at_default_rate(vec![1, 2]));
        startup_callback(&PcmBuffer::at_default_rate(vec![3, 4]));

        let callback_received = Arc::clone(&received);
        let callback: AudioChunkCallback = Box::new(move |pcm| {
            callback_received
                .lock()
                .expect("capture gate output lock poisoned")
                .extend_from_slice(pcm.samples());
        });
        gate.arm(Some(callback)).unwrap();
        startup_callback(&PcmBuffer::at_default_rate(vec![5, 6]));

        assert_eq!(
            *received.lock().expect("capture gate output lock poisoned"),
            [1, 2, 3, 4, 5, 6]
        );
    }

    #[test]
    fn chunked_delivery_applies_gain_and_flushes_the_final_short_batch() {
        let pushes = Arc::new(Mutex::new(Vec::new()));
        let (session, callback) = ActiveRecognitionSession::new(
            Box::new(CapturingSession {
                pushes: Arc::clone(&pushes),
            }),
            AudioDeliveryMode::Chunked,
            2.0,
        );
        let mut callback = callback.expect("chunked session should install a callback");

        callback(&PcmBuffer::at_default_rate(vec![100; 900]));
        assert_eq!(pushes.lock().expect("push log lock poisoned").len(), 1);
        session.finish_streaming_delivery().unwrap();

        let pushes = pushes.lock().expect("push log lock poisoned");
        assert_eq!(pushes.len(), 2);
        assert_eq!(pushes[0].len(), 800);
        assert_eq!(pushes[1].len(), 100);
        assert!(
            pushes
                .iter()
                .flat_map(PcmBuffer::samples)
                .all(|sample| *sample == 200)
        );
    }

    #[test]
    fn chunked_delivery_reports_pcm_metadata_changes_at_stop() {
        let pushes = Arc::new(Mutex::new(Vec::new()));
        let (session, callback) = ActiveRecognitionSession::new(
            Box::new(CapturingSession { pushes }),
            AudioDeliveryMode::Chunked,
            1.0,
        );
        let mut callback = callback.expect("chunked session should install a callback");

        callback(&PcmBuffer::at_default_rate(vec![1; 100]));
        callback(
            &PcmBuffer::with_spec(
                PcmSpec {
                    sample_rate_hz: 48_000,
                    channels: 2,
                },
                vec![1; 200],
            )
            .unwrap(),
        );

        let error = session.finish_streaming_delivery().unwrap_err();
        assert!(matches!(
            error,
            AsrError::Backend(message)
                if message == "streaming PCM metadata changed from 16000 Hz/1 channels to 48000 Hz/2 channels"
        ));
    }

    #[test]
    fn live_partial_drain_retains_final_and_completed_events_for_stop() {
        let pushes = Arc::new(Mutex::new(Vec::new()));
        let (session, _callback) = ActiveRecognitionSession::new(
            Box::new(CapturingSession { pushes }),
            AudioDeliveryMode::Chunked,
            1.0,
        );
        let state = session
            .streaming_state
            .as_ref()
            .expect("chunked session should have streaming state");
        lock_streaming_state(state).unwrap().events.extend([
            RecognitionEvent::PartialText {
                text: "first".to_owned(),
            },
            RecognitionEvent::FinalText {
                text: "final".to_owned(),
            },
            RecognitionEvent::Completed,
            RecognitionEvent::PartialText {
                text: "second".to_owned(),
            },
        ]);

        assert_eq!(
            session.take_streaming_partial_texts().unwrap(),
            ["first", "final", "second"]
        );
        assert_eq!(
            session.finish_streaming_delivery().unwrap(),
            [
                RecognitionEvent::FinalText {
                    text: "final".to_owned(),
                },
                RecognitionEvent::Completed,
            ]
        );
    }
}

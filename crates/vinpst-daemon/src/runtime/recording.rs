//! Recording lifecycle, ASR event draining, and stop-time text finishing.

use std::time::{Duration, Instant};

use vinpst_asr::{
    AudioDeliveryMode, MIN_SAMPLES_FOR_RECOGNITION, RecognitionContext, RecognitionEvent,
    events_to_payload,
};
use vinpst_audio::PcmBuffer;
use vinpst_protocol::{RecognitionPayload, ServiceStatus};
use vinpst_text::TextRequest;

use super::{
    LiveRecognitionEvent, PendingStopRecording, PreparedStopRecording, ReadyStopRecording,
    RuntimeError, RuntimeState, StopRecordingReport,
};

fn duration_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

impl RuntimeState {
    /// Starts normal recording.
    pub fn start_recording(&mut self) -> Result<(), RuntimeError> {
        self.start_recording_internal(self.config.scenes.active_scene.clone(), None)
    }

    /// Starts command-mode recording.
    pub fn start_command_recording(
        &mut self,
        selected_text: impl Into<String>,
    ) -> Result<(), RuntimeError> {
        self.start_recording_internal(
            vinpst_config::COMMAND_SCENE_ID.to_owned(),
            Some(selected_text.into()),
        )
    }

    /// Takes newly decoded live recognition events while a recording is active.
    pub(crate) fn take_live_recognition_events(
        &mut self,
    ) -> Result<Vec<LiveRecognitionEvent>, RuntimeError> {
        if self.status != ServiceStatus::Recording {
            return Ok(Vec::new());
        }
        let live = self
            .active_session
            .as_ref()
            .ok_or(RuntimeError::MissingAsrSession)?
            .take_live_recognition_events();
        let live = match live {
            Ok(live) => live,
            Err(error) => {
                self.enter_live_recording_error();
                return Err(RuntimeError::Asr(error));
            }
        };
        let mut projected = Vec::with_capacity(live.len());
        for event in live {
            match event {
                LiveRecognitionEvent::PartialText(text) => {
                    if self.partial_text.as_deref() != Some(text.as_str()) {
                        self.partial_text = Some(text.clone());
                        projected.push(LiveRecognitionEvent::PartialText(text));
                    }
                }
                LiveRecognitionEvent::Error(message) => {
                    projected.push(LiveRecognitionEvent::Error(message));
                }
            }
        }
        Ok(projected)
    }

    /// Stops recording and returns a deterministic mock result payload.
    pub fn stop_recording(
        &mut self,
        scene_id: Option<&str>,
    ) -> Result<RecognitionPayload, RuntimeError> {
        Ok(self.stop_recording_report(scene_id)?.payload)
    }

    /// Stops recording and returns final payload plus stop-time ASR metadata.
    pub fn stop_recording_report(
        &mut self,
        scene_id: Option<&str>,
    ) -> Result<StopRecordingReport, RuntimeError> {
        match self.prepare_stop_recording(scene_id)? {
            PreparedStopRecording::TooShort => Ok(StopRecordingReport {
                payload: RecognitionPayload {
                    commit_text: String::new(),
                    candidates: Vec::new(),
                },
                partial_text: None,
                postprocess_warning: None,
            }),
            PreparedStopRecording::Ready(prepared) => {
                let pending = self.begin_stop_inference(prepared)?;
                self.finish_stop_recording(pending)
            }
        }
    }

    /// Stops capture and decides whether the recording is long enough for inference.
    pub(crate) fn prepare_stop_recording(
        &mut self,
        scene_id: Option<&str>,
    ) -> Result<PreparedStopRecording, RuntimeError> {
        if self.status != ServiceStatus::Recording {
            return Err(RuntimeError::NotRecording(self.status));
        }

        let scene = scene_id
            .map(ToOwned::to_owned)
            .or_else(|| self.current_scene.clone())
            .unwrap_or_else(|| self.config.scenes.active_scene.clone());
        let result = (|| {
            let session = self
                .active_session
                .take()
                .ok_or(RuntimeError::MissingAsrSession)?;
            let captured = match self.stop_recording_buffer() {
                Ok(pcm) => pcm,
                Err(error) => {
                    let _ = session.cancel();
                    return Err(error);
                }
            };
            self.output_ducker.restore();

            if captured.len() < MIN_SAMPLES_FOR_RECOGNITION {
                tracing::debug!(
                    sample_count = captured.len(),
                    minimum_sample_count = MIN_SAMPLES_FOR_RECOGNITION,
                    "recording too short; skipping recognition inference"
                );
                let _ = session.cancel();
                self.reset_to_idle();
                return Ok(PreparedStopRecording::TooShort);
            }

            self.status = ServiceStatus::Inferring;
            Ok(PreparedStopRecording::Ready(Box::new(ReadyStopRecording {
                session,
                captured,
                scene: self.scene_definition(&scene),
                selected_text: self.selected_text.clone(),
            })))
        })();

        if result.is_err() {
            if self.audio_recorder.is_recording() {
                let _ = self.audio_recorder.cancel_recording();
            }
            self.audio_recorder.set_chunk_callback(None);
            self.output_ducker.restore();
            self.reset_to_idle();
        }
        result
    }

    /// Runs ASR inference after capture length has passed the upstream minimum gate.
    pub(crate) fn begin_stop_inference(
        &mut self,
        prepared: Box<ReadyStopRecording>,
    ) -> Result<PendingStopRecording, RuntimeError> {
        if self.status != ServiceStatus::Inferring {
            let _ = prepared.session.cancel();
            return Err(RuntimeError::Busy(self.status));
        }

        let ReadyStopRecording {
            session,
            captured,
            scene,
            selected_text,
        } = *prepared;
        let result = (|| {
            let mut events = match session.delivery_mode() {
                AudioDeliveryMode::Buffered => {
                    let pcm = self.process_captured_pcm(&captured);
                    if let Err(error) = session.push_buffered_pcm(&pcm) {
                        let _ = session.cancel();
                        return Err(RuntimeError::Asr(error));
                    }
                    Vec::new()
                }
                AudioDeliveryMode::Chunked => match session.finish_streaming_delivery() {
                    Ok(events) => events,
                    Err(error) => {
                        let _ = session.cancel();
                        return Err(RuntimeError::Asr(error));
                    }
                },
            };
            match self.drain_pending_events(&session) {
                Ok(new_events) => events.extend(new_events),
                Err(error) => {
                    let _ = session.cancel();
                    return Err(error);
                }
            }
            if let Err(error) = session.finish() {
                let _ = session.cancel();
                return Err(RuntimeError::Asr(error));
            }
            match self.drain_pending_events(&session) {
                Ok(new_events) => events.extend(new_events),
                Err(error) => {
                    let _ = session.cancel();
                    return Err(error);
                }
            }
            let partial_text = latest_partial_text(&events).or_else(|| self.partial_text.clone());
            let raw_payload = match events_to_payload(&events) {
                Ok(payload) => payload,
                Err(error) => {
                    let _ = session.cancel();
                    return Err(RuntimeError::Asr(error));
                }
            };
            Ok(PendingStopRecording {
                session,
                raw_payload,
                scene,
                selected_text,
                partial_text,
            })
        })();

        match &result {
            Ok(pending) if pending.enters_postprocessing() => {
                self.status = ServiceStatus::Postprocessing;
            }
            Ok(_) => {}
            Err(_) => self.reset_to_idle(),
        }
        result
    }

    /// Applies scene text processing and returns the runtime to idle.
    pub(crate) fn finish_stop_recording(
        &mut self,
        pending: PendingStopRecording,
    ) -> Result<StopRecordingReport, RuntimeError> {
        let needs_text_processing = pending.needs_text_processing();
        let expected_status = if pending.enters_postprocessing() {
            ServiceStatus::Postprocessing
        } else {
            ServiceStatus::Inferring
        };
        if self.status != expected_status {
            let _ = pending.session.cancel();
            return Err(RuntimeError::Busy(self.status));
        }

        let result = if needs_text_processing {
            self.text_processor
                .finish_report(&TextRequest {
                    raw_text: &pending.raw_payload.commit_text,
                    scene: &pending.scene,
                    selected_text: pending.selected_text.as_deref(),
                })
                .map(|report| StopRecordingReport {
                    payload: report.payload,
                    partial_text: pending.partial_text,
                    postprocess_warning: report.warning,
                })
                .map_err(|error| {
                    let _ = pending.session.cancel();
                    RuntimeError::Finish(error)
                })
        } else {
            Ok(StopRecordingReport {
                payload: pending.raw_payload,
                partial_text: pending.partial_text,
                postprocess_warning: None,
            })
        };
        self.reset_to_idle();
        result
    }

    /// Cancels a pending postprocessing operation and returns to idle.
    pub(crate) fn abort_stop_recording(&mut self, pending: &PendingStopRecording) {
        let _ = pending.session.cancel();
        if self.audio_recorder.is_recording() {
            let _ = self.audio_recorder.cancel_recording();
        }
        self.audio_recorder.set_chunk_callback(None);
        self.output_ducker.restore();
        self.reset_to_idle();
    }

    fn start_recording_internal(
        &mut self,
        scene_id: String,
        selected_text: Option<String>,
    ) -> Result<(), RuntimeError> {
        let start_at = Instant::now();
        self.ensure_idle()?;
        let capture_target = self.capture_target_for_runtime()?;
        let context = self.recognition_context(&scene_id, selected_text.as_deref());
        let delivery_mode = self.asr_backend.describe().capabilities.delivery_mode;
        let (capture_gate, startup_callback) = super::CaptureStartGate::new();
        self.audio_recorder
            .set_chunk_callback(Some(startup_callback));
        let capture_at = Instant::now();
        if let Err(error) = self.audio_recorder.begin_recording(capture_target) {
            tracing::debug!(
                scene_id = %scene_id,
                capture_open_ms = duration_millis(capture_at.elapsed()),
                start_total_ms = duration_millis(start_at.elapsed()),
                error = %error,
                "recording capture startup failed"
            );
            self.audio_recorder.set_chunk_callback(None);
            return Err(RuntimeError::Audio(error));
        }
        let capture_open_ms = duration_millis(capture_at.elapsed());
        let session_at = Instant::now();
        let session = match self.asr_backend.create_session(context) {
            Ok(session) => session,
            Err(error) => {
                tracing::debug!(
                    scene_id = %scene_id,
                    capture_open_ms,
                    session_create_ms = duration_millis(session_at.elapsed()),
                    start_total_ms = duration_millis(start_at.elapsed()),
                    error = %error,
                    "recording ASR session startup failed"
                );
                let _ = self.audio_recorder.cancel_recording();
                self.audio_recorder.set_chunk_callback(None);
                return Err(RuntimeError::Asr(error));
            }
        };
        let session_create_ms = duration_millis(session_at.elapsed());
        let (session, chunk_callback) = super::ActiveRecognitionSession::new(
            session,
            delivery_mode,
            self.config.asr.input_gain,
        );
        if let Err(error) = capture_gate.arm(chunk_callback) {
            let _ = session.cancel();
            let _ = self.audio_recorder.cancel_recording();
            self.audio_recorder.set_chunk_callback(None);
            return Err(RuntimeError::Asr(error));
        }
        tracing::debug!(
            scene_id = %scene_id,
            delivery_mode = ?delivery_mode,
            capture_open_ms,
            session_create_ms,
            start_total_ms = duration_millis(start_at.elapsed()),
            "recording startup completed"
        );
        self.status = ServiceStatus::Recording;
        self.current_scene = Some(scene_id);
        self.selected_text = selected_text;
        self.active_session = Some(session);
        if self.config.global.duck_output_while_recording {
            self.output_ducker
                .duck(self.config.global.duck_output_volume);
        }
        Ok(())
    }

    fn drain_pending_events(
        &mut self,
        session: &super::ActiveRecognitionSession,
    ) -> Result<Vec<RecognitionEvent>, RuntimeError> {
        let mut events = Vec::new();
        for event in session.poll_events().map_err(RuntimeError::Asr)? {
            if let vinpst_asr::RecognitionEvent::PartialText { text } = &event {
                self.partial_text = Some(text.clone());
            }
            events.push(event);
        }
        Ok(events)
    }

    fn recognition_context(
        &self,
        scene_id: &str,
        selected_text: Option<&str>,
    ) -> RecognitionContext {
        if scene_id == vinpst_config::COMMAND_SCENE_ID {
            RecognitionContext::command(
                scene_id.to_owned(),
                Some(self.config.global.default_language.clone()),
                selected_text.unwrap_or_default().to_owned(),
            )
        } else {
            RecognitionContext::normal(
                scene_id.to_owned(),
                Some(self.config.global.default_language.clone()),
            )
        }
    }

    fn stop_recording_buffer(&mut self) -> Result<PcmBuffer, RuntimeError> {
        let result = self.audio_recorder.stop_and_get_buffer();
        self.audio_recorder.set_chunk_callback(None);
        result
            .map(|captured| captured.pcm)
            .map_err(RuntimeError::Audio)
    }

    fn process_captured_pcm(&self, pcm: &PcmBuffer) -> PcmBuffer {
        super::process_buffered_pcm(
            pcm,
            self.config.asr.input_gain,
            self.config.asr.normalize_audio,
        )
    }

    fn scene_definition(&self, scene_id: &str) -> vinpst_config::SceneDefinition {
        self.config
            .scenes
            .definitions
            .iter()
            .find(|scene| scene.id == scene_id)
            .cloned()
            .unwrap_or_else(|| vinpst_config::SceneDefinition {
                id: scene_id.to_owned(),
                label: scene_id.to_owned(),
                prompt: None,
                provider_id: None,
                model: None,
                candidate_count: 0,
                timeout_ms: None,
                context_lines: 0,
            })
    }

    fn enter_live_recording_error(&mut self) {
        self.status = ServiceStatus::Error;
        self.audio_recorder.set_chunk_callback(None);
        if let Some(session) = self.active_session.take() {
            let _ = session.cancel();
        }
        if self.audio_recorder.is_recording() {
            let _ = self.audio_recorder.cancel_recording();
        }
        self.output_ducker.restore();
        self.current_scene = None;
        self.selected_text = None;
        self.partial_text = None;
    }

    pub(crate) fn recover_live_recording_error(&mut self) {
        if self.status == ServiceStatus::Error {
            self.reset_to_idle();
        }
    }

    fn reset_to_idle(&mut self) {
        self.output_ducker.restore();
        self.status = ServiceStatus::Idle;
        self.current_scene = None;
        self.selected_text = None;
        self.partial_text = None;
        self.active_session = None;
        self.apply_pending_asr_backend_reload();
    }
}

fn latest_partial_text(events: &[RecognitionEvent]) -> Option<String> {
    events.iter().rev().find_map(|event| match event {
        RecognitionEvent::PartialText { text } => Some(text.clone()),
        RecognitionEvent::FinalText { .. }
        | RecognitionEvent::Error { .. }
        | RecognitionEvent::Completed => None,
    })
}

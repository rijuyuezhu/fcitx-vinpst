//! Recording lifecycle, ASR event draining, and stop-time text finishing.

use std::time::{Duration, Instant};

use vinput_asr::{AudioDeliveryMode, RecognitionContext, RecognitionEvent, events_to_payload};
use vinput_audio::{AudioProcessingOptions, PcmBuffer};
use vinput_protocol::{RecognitionPayload, ServiceStatus};
use vinput_text::TextRequest;

use super::{
    MOCK_SILENCE_THRESHOLD, PendingStopRecording, RuntimeError, RuntimeState, StopRecordingReport,
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
            vinput_config::COMMAND_SCENE_ID.to_owned(),
            Some(selected_text.into()),
        )
    }

    /// Takes newly decoded streaming partials while a recording is active.
    pub fn take_live_partial_texts(&mut self) -> Result<Vec<String>, RuntimeError> {
        if self.status != ServiceStatus::Recording {
            return Ok(Vec::new());
        }
        let session = self
            .active_session
            .as_ref()
            .ok_or(RuntimeError::MissingAsrSession)?;
        let partials = session
            .take_streaming_partial_texts()
            .map_err(RuntimeError::Asr)?;
        let mut new_partials = Vec::new();
        for text in partials {
            if self.partial_text.as_deref() == Some(text.as_str()) {
                continue;
            }
            self.partial_text = Some(text.clone());
            new_partials.push(text);
        }
        Ok(new_partials)
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
        let pending = self.begin_stop_recording(scene_id)?;
        self.finish_stop_recording(pending)
    }

    /// Stops capture and ASR, leaving the runtime in the postprocessing phase.
    pub(crate) fn begin_stop_recording(
        &mut self,
        scene_id: Option<&str>,
    ) -> Result<PendingStopRecording, RuntimeError> {
        if self.status != ServiceStatus::Recording {
            return Err(RuntimeError::NotRecording(self.status));
        }

        self.status = ServiceStatus::Inferring;
        let scene = scene_id
            .map(ToOwned::to_owned)
            .or_else(|| self.current_scene.clone())
            .unwrap_or_else(|| self.config.scenes.active_scene.clone());

        let result = (|| {
            let session = self
                .active_session
                .take()
                .ok_or(RuntimeError::MissingAsrSession)?;
            let captured_result = self.stop_recording_buffer();
            self.output_ducker.restore();
            let captured = match captured_result {
                Ok(pcm) => pcm,
                Err(error) => {
                    let _ = session.cancel();
                    return Err(error);
                }
            };

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
                scene: self.scene_definition(&scene),
                selected_text: self.selected_text.clone(),
                partial_text,
            })
        })();

        if result.is_err() && self.audio_recorder.is_recording() {
            let _ = self.audio_recorder.cancel_recording();
        }
        self.audio_recorder.set_chunk_callback(None);
        self.output_ducker.restore();
        if result.is_ok() {
            self.status = ServiceStatus::Postprocessing;
        } else {
            self.reset_to_idle();
        }
        result
    }

    /// Applies scene text processing and returns the runtime to idle.
    pub(crate) fn finish_stop_recording(
        &mut self,
        pending: PendingStopRecording,
    ) -> Result<StopRecordingReport, RuntimeError> {
        if self.status != ServiceStatus::Postprocessing {
            let _ = pending.session.cancel();
            return Err(RuntimeError::Busy(self.status));
        }

        let result = self
            .text_processor
            .finish(&TextRequest {
                raw_text: &pending.raw_payload.commit_text,
                scene: &pending.scene,
                selected_text: pending.selected_text.as_deref(),
            })
            .map(|payload| StopRecordingReport {
                payload,
                partial_text: pending.partial_text,
            })
            .map_err(|error| {
                let _ = pending.session.cancel();
                RuntimeError::Finish(error)
            });
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
            if let vinput_asr::RecognitionEvent::PartialText { text } = &event {
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
        if scene_id == vinput_config::COMMAND_SCENE_ID {
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
        self.audio_processing_options().process(pcm)
    }

    fn audio_processing_options(&self) -> AudioProcessingOptions {
        AudioProcessingOptions::new(
            MOCK_SILENCE_THRESHOLD,
            self.config.asr.normalize_audio.then_some(16_000),
            self.config.asr.input_gain,
        )
    }

    fn scene_definition(&self, scene_id: &str) -> vinput_config::SceneDefinition {
        self.config
            .scenes
            .definitions
            .iter()
            .find(|scene| scene.id == scene_id)
            .cloned()
            .unwrap_or_else(|| vinput_config::SceneDefinition {
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

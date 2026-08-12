//! Core text request, processor traits, and built-in finisher implementations.

use vinpst_config::{COMMAND_SCENE_ID, RAW_SCENE_ID, SceneDefinition};
use vinpst_protocol::RecognitionPayload;

use crate::{PromptTemplate, TextError};

/// Input to the text finishing stage.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRequest<'a> {
    /// Raw ASR text.
    pub raw_text: &'a str,
    /// Scene definition selected by the frontend/runtime.
    pub scene: &'a SceneDefinition,
    /// Optional selected text used by command mode.
    pub selected_text: Option<&'a str>,
}

/// Result of one text-processing pass, including a recoverable post-processing warning.
#[derive(Debug, PartialEq, Eq)]
pub struct TextProcessReport {
    /// Payload that should still be delivered to the frontend.
    pub payload: RecognitionPayload,
    /// Recoverable post-processing failure that should be surfaced separately.
    pub warning: Option<TextError>,
}

impl TextProcessReport {
    /// Builds a successful report without a warning.
    #[must_use]
    pub const fn success(payload: RecognitionPayload) -> Self {
        Self {
            payload,
            warning: None,
        }
    }
}

/// Synchronous text post-processing seam used by daemon runtime and tests.
pub trait TextProcessor: Send {
    /// Finishes raw recognition text into a payload suitable for the frontend.
    fn finish(&self, request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError>;
    /// Finishes text and preserves a payload when a backend reports a recoverable warning.
    fn finish_report(&self, request: &TextRequest<'_>) -> Result<TextProcessReport, TextError> {
        self.finish(request).map(TextProcessReport::success)
    }
}

/// Adapter seam for real scene post-processing backends.
pub trait TextAdapter: Send + Sync {
    /// Finishes a scene that requires post-processing.
    fn finish(&self, request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError>;
}

/// Text processor that delegates post-processing scenes to an adapter.
#[derive(Debug, Clone)]
pub struct LlmTextProcessor<A> {
    adapter: A,
}

impl<A> LlmTextProcessor<A> {
    /// Creates a text processor backed by one adapter implementation.
    #[must_use]
    pub const fn new(adapter: A) -> Self {
        Self { adapter }
    }

    /// Returns the configured adapter.
    #[must_use]
    pub const fn adapter(&self) -> &A {
        &self.adapter
    }
}

impl<A: TextAdapter> TextProcessor for LlmTextProcessor<A> {
    fn finish(&self, request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError> {
        if request.scene.id == RAW_SCENE_ID || !scene_needs_postprocessing(request.scene) {
            return Ok(RecognitionPayload::raw(request.raw_text));
        }
        self.adapter.finish(request)
    }
}

/// Explicit fallback used when no usable text adapter is selected.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnsupportedTextAdapter;

impl UnsupportedTextAdapter {
    /// Creates the explicit unavailable-adapter fallback.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl TextAdapter for UnsupportedTextAdapter {
    fn finish(&self, request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError> {
        Err(TextError::UnsupportedAdapter(request.scene.id.clone()))
    }
}

/// Production-safe fallback used when no configured post-processor is selected.
///
/// It only commits raw/no-op scenes whose candidate count disables
/// post-processing. Any non-raw scene with a positive candidate count returns
/// a typed error instead of fabricating mock text.
#[derive(Debug, Clone, Copy, Default)]
pub struct TextFinisher;

impl TextFinisher {
    /// Creates a finisher.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Finishes raw recognition text into a payload.
    pub fn finish(request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError> {
        <Self as TextProcessor>::finish(&Self, request)
    }
}

impl TextProcessor for TextFinisher {
    fn finish(&self, request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError> {
        if request.scene.id == RAW_SCENE_ID || !scene_needs_postprocessing(request.scene) {
            return Ok(RecognitionPayload::raw(request.raw_text));
        }
        Err(TextError::AdapterRequired(request.scene.id.clone()))
    }
}

/// Mock text processor for daemon prototypes and tests.
#[derive(Debug, Clone, Copy, Default)]
pub struct MockTextProcessor;

impl MockTextProcessor {
    /// Creates a mock text processor.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl TextProcessor for MockTextProcessor {
    fn finish(&self, request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError> {
        if request.scene.id == RAW_SCENE_ID || !scene_needs_postprocessing(request.scene) {
            return Ok(RecognitionPayload::raw(request.raw_text));
        }
        if request.scene.id == COMMAND_SCENE_ID {
            return Ok(RecognitionPayload::raw(command_placeholder_text(request)));
        }
        Ok(RecognitionPayload::raw(
            PromptTemplate::new("mock postprocess result: {raw_text}").render_request(request),
        ))
    }
}

pub(crate) fn scene_needs_postprocessing(scene: &SceneDefinition) -> bool {
    scene.candidate_count > 0
}

fn command_placeholder_text(request: &TextRequest<'_>) -> String {
    if request.selected_text.unwrap_or_default().is_empty() {
        PromptTemplate::new("mock command result: {raw_text}").render_request(request)
    } else {
        PromptTemplate::new("mock command result for: {selected_text}").render_request(request)
    }
}

//! Recognition event to legacy payload conversion.

use vinpst_protocol::{Candidate, CandidateSource, RecognitionPayload};

use crate::{AsrError, RecognitionEvent};

/// Converts recognition events into a legacy result payload.
pub fn events_to_payload(events: &[RecognitionEvent]) -> Result<RecognitionPayload, AsrError> {
    if let Some(message) = events.iter().rev().find_map(|event| match event {
        RecognitionEvent::Error { message } => Some(message.as_str()),
        RecognitionEvent::PartialText { .. }
        | RecognitionEvent::FinalText { .. }
        | RecognitionEvent::Completed => None,
    }) {
        return Err(AsrError::Backend(message.to_owned()));
    }

    let final_text = events.iter().rev().find_map(|event| match event {
        RecognitionEvent::FinalText { text } => Some(text.as_str()),
        RecognitionEvent::PartialText { .. }
        | RecognitionEvent::Error { .. }
        | RecognitionEvent::Completed => None,
    });

    Ok(match final_text {
        Some(text) => RecognitionPayload {
            commit_text: text.to_owned(),
            candidates: vec![Candidate::new(text, CandidateSource::Raw)],
        },
        None => RecognitionPayload {
            commit_text: String::new(),
            candidates: Vec::new(),
        },
    })
}

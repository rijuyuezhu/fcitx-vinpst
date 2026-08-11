//! Recognition payload helpers for command-mode text processing.

use vinpst_protocol::{Candidate, CandidateSource, RecognitionPayload};

pub(crate) fn trim_ascii_whitespace(text: &str) -> &str {
    text.trim_matches(|character| matches!(character, ' ' | '\t' | '\r' | '\n'))
}

/// Builds the legacy normal-scene payload with raw ASR followed by LLM rewrites.
#[must_use]
pub(crate) fn normal_mode_payload(
    raw_text: &str,
    llm_candidates: impl IntoIterator<Item = String>,
) -> RecognitionPayload {
    let raw_text = trim_ascii_whitespace(raw_text);
    let mut candidates = Vec::new();
    append_trimmed_candidate(&mut candidates, raw_text, CandidateSource::Raw);
    let mut first_llm_candidate = None;
    for candidate in llm_candidates {
        let candidate = trim_ascii_whitespace(&candidate).to_owned();
        if candidate.is_empty() {
            continue;
        }
        if first_llm_candidate.is_none() {
            first_llm_candidate = Some(candidate.clone());
        }
        candidates.push(Candidate::new(candidate, CandidateSource::Llm));
    }
    RecognitionPayload {
        commit_text: first_llm_candidate.unwrap_or_else(|| raw_text.to_owned()),
        candidates,
    }
}

/// Builds the legacy command-mode payload candidate order.
///
/// The menu order is selected text (`raw`), recognized ASR command (`asr`), then
/// LLM rewrites (`llm`). Empty/whitespace-only candidates are skipped after
/// trimming. The default commit text is the first LLM rewrite when available;
/// otherwise it remains the original selected text, matching legacy command
/// mode fallback behavior.
#[must_use]
pub fn command_mode_payload(
    selected_text: &str,
    asr_text: &str,
    llm_candidates: impl IntoIterator<Item = String>,
) -> RecognitionPayload {
    let mut candidates = Vec::new();
    append_trimmed_candidate(&mut candidates, selected_text, CandidateSource::Raw);
    append_trimmed_candidate(&mut candidates, asr_text, CandidateSource::Asr);

    let mut first_llm_candidate = None;
    for candidate in llm_candidates {
        let candidate = trim_ascii_whitespace(&candidate).to_owned();
        if candidate.is_empty() {
            continue;
        }
        if first_llm_candidate.is_none() {
            first_llm_candidate = Some(candidate.clone());
        }
        candidates.push(Candidate::new(candidate, CandidateSource::Llm));
    }

    RecognitionPayload {
        commit_text: first_llm_candidate.unwrap_or_else(|| selected_text.to_owned()),
        candidates,
    }
}

fn append_trimmed_candidate(candidates: &mut Vec<Candidate>, text: &str, source: CandidateSource) {
    let text = trim_ascii_whitespace(text);
    if !text.is_empty() {
        candidates.push(Candidate::new(text, source));
    }
}

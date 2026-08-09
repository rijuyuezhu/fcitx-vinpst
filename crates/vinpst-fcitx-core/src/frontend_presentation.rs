//! Platform-neutral projection of frontend outcomes into executable UI actions.

use crate::{CandidateSource, FrontendOutcome, FrontendOutcomeKind};

/// Localized fragments used to render result candidates.
#[derive(Debug, Clone, Copy)]
pub struct ResultCandidateText<'a> {
    /// Label for the original/raw result.
    pub original: &'a str,
    /// Label for the direct ASR or voice-command result.
    pub voice_command: &'a str,
    /// Label for a non-committing cancel row.
    pub cancel: &'a str,
}

/// One platform-neutral result candidate row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentedResultCandidate {
    /// Candidate text shown by the platform frontend.
    pub text: String,
    /// Localized annotation shown beside the candidate.
    pub comment: String,
    /// Whether selecting this row commits its text.
    pub commit: bool,
    /// Explicit recent-context source to append before committing this row.
    pub context_source: String,
    /// Whether the matching Fcitx commit event must be suppressed as duplicate user input.
    pub suppress_commit_context: bool,
}

/// One explicit recent-input context entry emitted before a frontend action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontendContextEntry {
    /// Text written to the JSONL context cache.
    pub text: String,
    /// Stable source label (`asr` or `llm`).
    pub source: String,
}

/// Complete platform-neutral frontend application plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontendPresentation {
    /// Final action kind after applying empty-result fallbacks.
    pub kind: FrontendOutcomeKind,
    /// Preedit, commit, error, or candidate-menu fallback text.
    pub text: String,
    /// Whether a commit or candidate selection replaces surrounding selected text.
    pub replace_selection: bool,
    /// Fully rendered candidate rows.
    pub candidates: Vec<PresentedResultCandidate>,
    /// Preferred candidate cursor position.
    pub cursor_index: usize,
    /// Explicit ASR/LLM history entries emitted before the presentation action.
    pub context_entries: Vec<FrontendContextEntry>,
    /// Whether the presentation's direct commit must be suppressed from user history.
    pub suppress_commit_context: bool,
}

impl FrontendPresentation {
    fn simple(kind: FrontendOutcomeKind, text: &str) -> Self {
        Self {
            kind,
            text: text.to_owned(),
            replace_selection: false,
            candidates: Vec::new(),
            cursor_index: 0,
            context_entries: Vec::new(),
            suppress_commit_context: false,
        }
    }
}

fn recognition_context_entries(
    outcome: &FrontendOutcome,
    include_direct_llm: bool,
) -> Vec<FrontendContextEntry> {
    let mut entries = Vec::new();
    let asr_candidate = outcome
        .payload()
        .candidates
        .iter()
        .find(|candidate| candidate.source == CandidateSource::Asr && !candidate.text.is_empty())
        .or_else(|| {
            outcome.payload().candidates.iter().find(|candidate| {
                candidate.source == CandidateSource::Raw
                    && !candidate.text.is_empty()
                    && (!outcome.command_mode()
                        || outcome.selected_text() != Some(candidate.text.as_str()))
            })
        });
    if let Some(candidate) = asr_candidate {
        entries.push(FrontendContextEntry {
            text: candidate.text.clone(),
            source: "asr".to_owned(),
        });
    }
    if include_direct_llm
        && let Some(candidate) = outcome.payload().candidates.iter().find(|candidate| {
            candidate.source == CandidateSource::Llm
                && candidate.text == outcome.payload().commit_text
                && !candidate.text.is_empty()
        })
    {
        entries.push(FrontendContextEntry {
            text: candidate.text.clone(),
            source: "llm".to_owned(),
        });
    }
    entries
}

/// Projects one Rust frontend outcome into a platform-executable presentation plan.
#[must_use]
pub fn present_frontend_outcome(
    outcome: &FrontendOutcome,
    candidate_text: ResultCandidateText<'_>,
) -> FrontendPresentation {
    match outcome.kind() {
        FrontendOutcomeKind::None => FrontendPresentation::simple(FrontendOutcomeKind::None, ""),
        FrontendOutcomeKind::Preedit => {
            FrontendPresentation::simple(FrontendOutcomeKind::Preedit, outcome.text())
        }
        FrontendOutcomeKind::Clear => FrontendPresentation::simple(FrontendOutcomeKind::Clear, ""),
        FrontendOutcomeKind::Error => {
            FrontendPresentation::simple(FrontendOutcomeKind::Error, outcome.text())
        }
        FrontendOutcomeKind::Commit => {
            let text = if outcome.text().is_empty() {
                &outcome.payload().commit_text
            } else {
                outcome.text()
            };
            if text.is_empty() {
                return FrontendPresentation::simple(FrontendOutcomeKind::Clear, "");
            }
            FrontendPresentation {
                kind: FrontendOutcomeKind::Commit,
                text: text.to_owned(),
                replace_selection: outcome.command_mode(),
                candidates: Vec::new(),
                cursor_index: 0,
                context_entries: recognition_context_entries(outcome, true),
                suppress_commit_context: true,
            }
        }
        FrontendOutcomeKind::CandidateMenu => {
            let mut llm_index = 0usize;
            let mut cursor_index = 0usize;
            let candidates = outcome
                .payload()
                .candidates
                .iter()
                .enumerate()
                .map(|(index, candidate)| {
                    if candidate.text == outcome.payload().commit_text {
                        cursor_index = index;
                    }
                    let comment = match candidate.source {
                        CandidateSource::Raw => candidate_text.original.to_owned(),
                        CandidateSource::Asr => candidate_text.voice_command.to_owned(),
                        CandidateSource::Llm => {
                            llm_index += 1;
                            llm_index.to_string()
                        }
                        CandidateSource::Cancel => candidate_text.cancel.to_owned(),
                    };
                    PresentedResultCandidate {
                        text: candidate.text.clone(),
                        comment,
                        commit: candidate.source != CandidateSource::Cancel
                            && !candidate.text.is_empty(),
                        context_source: if candidate.source == CandidateSource::Llm {
                            "llm".to_owned()
                        } else {
                            String::new()
                        },
                        suppress_commit_context: candidate.source != CandidateSource::Cancel
                            && !candidate.text.is_empty(),
                    }
                })
                .collect::<Vec<_>>();

            if candidates.is_empty() {
                let text = &outcome.payload().commit_text;
                if text.is_empty() {
                    return FrontendPresentation::simple(FrontendOutcomeKind::Clear, "");
                }
                return FrontendPresentation {
                    kind: FrontendOutcomeKind::Commit,
                    text: text.clone(),
                    replace_selection: outcome.command_mode(),
                    candidates,
                    cursor_index: 0,
                    context_entries: recognition_context_entries(outcome, true),
                    suppress_commit_context: true,
                };
            }

            FrontendPresentation {
                kind: FrontendOutcomeKind::CandidateMenu,
                text: outcome.payload().commit_text.clone(),
                replace_selection: outcome.command_mode(),
                candidates,
                cursor_index,
                context_entries: recognition_context_entries(outcome, false),
                suppress_commit_context: false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FrontendContextEntry, ResultCandidateText, present_frontend_outcome};
    use crate::{FrontendController, FrontendOutcome, FrontendOutcomeKind};

    const TEXT: ResultCandidateText<'static> = ResultCandidateText {
        original: "Original",
        voice_command: "Voice Command",
        cancel: "Cancel",
    };

    #[test]
    fn projects_candidate_policy_and_cursor_in_rust() {
        let outcome = FrontendOutcome::from_payload(
            r#"{"commit_text":"second","candidates":[{"text":"raw","source":"raw"},{"text":"first","source":"llm"},{"text":"second","source":"llm"},{"text":"voice","source":"asr"},{"text":"","source":"cancel"}]}"#,
            true,
        );

        let presentation = present_frontend_outcome(&outcome, TEXT);
        assert_eq!(presentation.kind, FrontendOutcomeKind::CandidateMenu);
        assert!(presentation.replace_selection);
        assert_eq!(presentation.cursor_index, 2);
        assert_eq!(presentation.text, "second");
        assert_eq!(
            presentation.context_entries,
            vec![FrontendContextEntry {
                text: "voice".to_owned(),
                source: "asr".to_owned(),
            }]
        );
        assert!(!presentation.suppress_commit_context);
        assert_eq!(presentation.candidates[0].comment, "Original");
        assert!(presentation.candidates[0].context_source.is_empty());
        assert!(presentation.candidates[0].suppress_commit_context);
        assert_eq!(presentation.candidates[1].comment, "1");
        assert_eq!(presentation.candidates[1].context_source, "llm");
        assert!(presentation.candidates[1].suppress_commit_context);
        assert_eq!(presentation.candidates[2].comment, "2");
        assert_eq!(presentation.candidates[2].context_source, "llm");
        assert!(presentation.candidates[2].suppress_commit_context);
        assert_eq!(presentation.candidates[3].comment, "Voice Command");
        assert!(presentation.candidates[3].context_source.is_empty());
        assert!(presentation.candidates[3].suppress_commit_context);
        assert_eq!(presentation.candidates[4].comment, "Cancel");
        assert!(!presentation.candidates[4].commit);
        assert!(presentation.candidates[4].context_source.is_empty());
        assert!(!presentation.candidates[4].suppress_commit_context);
    }

    #[test]
    fn projects_commit_selection_replacement() {
        let outcome = FrontendOutcome::from_payload(r#"{"commit_text":"changed"}"#, true);
        let presentation = present_frontend_outcome(&outcome, TEXT);
        assert_eq!(presentation.kind, FrontendOutcomeKind::Commit);
        assert_eq!(presentation.text, "changed");
        assert!(presentation.replace_selection);
        assert!(presentation.candidates.is_empty());
        assert_eq!(
            presentation.context_entries,
            vec![FrontendContextEntry {
                text: "changed".to_owned(),
                source: "asr".to_owned(),
            }]
        );
        assert!(presentation.suppress_commit_context);
    }

    #[test]
    fn projects_direct_asr_and_llm_history_entries() {
        let outcome = FrontendOutcome::from_payload(
            r#"{"commit_text":"changed","candidates":[{"text":"voice","source":"asr"},{"text":"changed","source":"llm"}]}"#,
            false,
        );
        let presentation = present_frontend_outcome(&outcome, TEXT);
        assert_eq!(presentation.kind, FrontendOutcomeKind::Commit);
        assert_eq!(presentation.text, "changed");
        assert!(!presentation.replace_selection);
        assert_eq!(
            presentation.context_entries,
            vec![
                FrontendContextEntry {
                    text: "voice".to_owned(),
                    source: "asr".to_owned(),
                },
                FrontendContextEntry {
                    text: "changed".to_owned(),
                    source: "llm".to_owned(),
                },
            ]
        );
        assert!(presentation.suppress_commit_context);
    }

    #[test]
    fn treats_normal_raw_candidate_as_asr_history() {
        let outcome = FrontendOutcome::from_payload(
            r#"{"commit_text":"voice","candidates":[{"text":"voice","source":"raw"}]}"#,
            false,
        );
        let presentation = present_frontend_outcome(&outcome, TEXT);
        assert_eq!(
            presentation.context_entries,
            vec![FrontendContextEntry {
                text: "voice".to_owned(),
                source: "asr".to_owned(),
            }]
        );
    }

    #[test]
    fn command_raw_fallback_excludes_original_selection() {
        let mut controller = FrontendController::default();
        let _ = controller.start_command("selected", None);
        let _ = controller.complete(true, "");
        let outcome = controller.complete_recognition_result(
            r#"{"commit_text":"voice","candidates":[{"text":"selected","source":"raw"},{"text":"voice","source":"raw"}]}"#,
        );
        let presentation = present_frontend_outcome(&outcome, TEXT);
        assert_eq!(
            presentation.context_entries,
            vec![FrontendContextEntry {
                text: "voice".to_owned(),
                source: "asr".to_owned(),
            }]
        );
    }

    #[test]
    fn preserves_non_committing_empty_outcomes() {
        let outcome = FrontendOutcome::from_payload(
            r#"{"candidates":[{"text":"","source":"cancel"}]}"#,
            false,
        );
        let presentation = present_frontend_outcome(&outcome, TEXT);
        assert_eq!(presentation.kind, FrontendOutcomeKind::Clear);
        assert!(presentation.text.is_empty());
        assert!(presentation.candidates.is_empty());
    }
}

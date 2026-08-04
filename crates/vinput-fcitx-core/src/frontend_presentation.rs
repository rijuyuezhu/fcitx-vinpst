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
}

impl FrontendPresentation {
    fn simple(kind: FrontendOutcomeKind, text: &str) -> Self {
        Self {
            kind,
            text: text.to_owned(),
            replace_selection: false,
            candidates: Vec::new(),
            cursor_index: 0,
        }
    }
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
                };
            }

            FrontendPresentation {
                kind: FrontendOutcomeKind::CandidateMenu,
                text: outcome.payload().commit_text.clone(),
                replace_selection: outcome.command_mode(),
                candidates,
                cursor_index,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ResultCandidateText, present_frontend_outcome};
    use crate::{FrontendOutcome, FrontendOutcomeKind};

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
        assert_eq!(presentation.candidates[0].comment, "Original");
        assert_eq!(presentation.candidates[1].comment, "1");
        assert_eq!(presentation.candidates[2].comment, "2");
        assert_eq!(presentation.candidates[3].comment, "Voice Command");
        assert_eq!(presentation.candidates[4].comment, "Cancel");
        assert!(!presentation.candidates[4].commit);
    }

    #[test]
    fn projects_commit_selection_replacement() {
        let outcome = FrontendOutcome::from_payload(r#"{"commit_text":"changed"}"#, true);
        let presentation = present_frontend_outcome(&outcome, TEXT);
        assert_eq!(presentation.kind, FrontendOutcomeKind::Commit);
        assert_eq!(presentation.text, "changed");
        assert!(presentation.replace_selection);
        assert!(presentation.candidates.is_empty());
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

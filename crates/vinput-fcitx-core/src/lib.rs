//! Pure frontend behavior shared by the retained Fcitx C++ adapter.
//!
//! This crate must not depend on Fcitx types or perform platform integration.
//! It receives owned wire data and returns frontend decisions that can be tested
//! without loading Fcitx.

mod menu;
mod menu_action;
mod menu_projection;
mod menu_snapshot;
mod trigger_mode;

pub use menu::{MenuFilterState, clamp_menu_page};
pub use menu_action::{MENU_PAGE_SIZE, MenuKeyAction, MenuKeyInput, MenuSemanticKey};
pub use menu_projection::{
    AsrMenuItem, AsrMenuProjectionState, ProjectedMenuItem, SceneMenuItem, SceneMenuProjection,
    is_effective_asr_target, project_asr_menu, project_scene_menu,
};
pub use menu_snapshot::{
    AsrDisplaySnapshot, AsrDisplaySnapshotItem, SceneSnapshot, SceneSnapshotItem,
};

pub use trigger_mode::{
    TRIGGER_DEBOUNCE_NS, TRIGGER_HOLD_THRESHOLD_NS, TriggerAction, TriggerKind, TriggerMode,
    TriggerModeState,
};
pub use vinput_protocol::{Candidate, CandidateSource, RecognitionPayload};

/// Parsed recognition payload plus the presentation decision for the frontend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitPlan {
    /// Normalized daemon recognition payload.
    pub payload: RecognitionPayload,
    /// Whether the frontend should present a result candidate menu.
    pub show_candidate_menu: bool,
}

/// Session state owned by the Rust frontend core.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FrontendState {
    recording: bool,
    command_mode: bool,
    active_scene_id: Option<String>,
}

impl FrontendState {
    /// Returns whether a recording session is active.
    #[must_use]
    pub const fn recording(&self) -> bool {
        self.recording
    }

    /// Returns whether the active session is command mode.
    #[must_use]
    pub const fn command_mode(&self) -> bool {
        self.command_mode
    }

    /// Returns the scene captured when the current recording started.
    #[must_use]
    pub fn active_scene_id(&self) -> Option<&str> {
        self.active_scene_id.as_deref()
    }

    /// Records a successful normal-dictation start.
    pub fn start_normal(&mut self, scene_id: Option<&str>) {
        let active_scene_id = scene_id.map(str::to_owned);
        self.recording = true;
        self.command_mode = false;
        self.active_scene_id = active_scene_id;
    }

    /// Records a successful command-mode start.
    pub fn start_command(&mut self, scene_id: Option<&str>) {
        let active_scene_id = scene_id.map(str::to_owned);
        self.recording = true;
        self.command_mode = true;
        self.active_scene_id = active_scene_id;
    }

    /// Adopts a recording session already active in the daemon.
    pub fn adopt_recording(&mut self, command_mode: bool, scene_id: &str) {
        let active_scene_id = scene_id.to_owned();
        self.recording = true;
        self.command_mode = command_mode;
        self.active_scene_id = Some(active_scene_id);
    }

    /// Resolves the stop scene while a recording is active.
    #[must_use]
    pub fn stop_scene_id<'a>(&'a self, fallback: &'a str) -> Option<&'a str> {
        self.recording
            .then(|| self.active_scene_id.as_deref().unwrap_or(fallback))
    }

    /// Returns the state to idle.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
/// Parses the daemon recognition payload using the shared compatibility contract.
#[must_use]
pub fn parse_recognition_payload(json: &str) -> RecognitionPayload {
    RecognitionPayload::from_json_str(json).unwrap_or_else(|_| RecognitionPayload {
        commit_text: String::new(),
        candidates: Vec::new(),
    })
}

/// Returns whether the frontend should show a result candidate menu.
///
/// Command mode preserves every alternative when more than one candidate is
/// present. Normal mode opens a menu only for multiple LLM alternatives.
#[must_use]
pub fn should_show_candidate_menu(payload: &RecognitionPayload, command_mode: bool) -> bool {
    if command_mode && payload.candidates.len() > 1 {
        return true;
    }

    payload
        .candidates
        .iter()
        .filter(|candidate| candidate.source == CandidateSource::Llm)
        .take(2)
        .count()
        > 1
}

/// Parses a daemon payload and computes its frontend presentation plan.
#[must_use]
pub fn make_commit_plan(json: &str, command_mode: bool) -> CommitPlan {
    let payload = parse_recognition_payload(json);
    let show_candidate_menu = should_show_candidate_menu(&payload, command_mode);
    CommitPlan {
        payload,
        show_candidate_menu,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Candidate, CandidateSource, FrontendState, make_commit_plan, parse_recognition_payload,
        should_show_candidate_menu,
    };

    #[test]
    fn parses_and_normalizes_shared_recognition_payload() {
        let payload = parse_recognition_payload(
            r#"{"commit_text":"","candidates":[{"text":"first","source":"asr"}]}"#,
        );

        assert_eq!(payload.commit_text, "first");
        assert_eq!(
            payload.candidates,
            vec![Candidate::new("first", CandidateSource::Asr)]
        );
    }

    #[test]
    fn invalid_payload_maps_to_explicit_empty_frontend_outcome() {
        let payload = parse_recognition_payload("not json");

        assert!(payload.commit_text.is_empty());
        assert!(payload.candidates.is_empty());
    }

    #[test]
    fn normal_mode_shows_only_multiple_llm_alternatives() {
        let one_llm = parse_recognition_payload(
            r#"{"commit_text":"polished","candidates":[{"text":"raw","source":"raw"},{"text":"polished","source":"llm"}]}"#,
        );
        assert!(!should_show_candidate_menu(&one_llm, false));

        let two_llm = parse_recognition_payload(
            r#"{"commit_text":"first","candidates":[{"text":"raw","source":"raw"},{"text":"first","source":"llm"},{"text":"second","source":"llm"}]}"#,
        );
        assert!(should_show_candidate_menu(&two_llm, false));
    }

    #[test]
    fn command_mode_preserves_multiple_non_llm_alternatives() {
        let plan = make_commit_plan(
            r#"{"commit_text":"selected","candidates":[{"text":"selected","source":"raw"},{"text":"command","source":"asr"}]}"#,
            true,
        );

        assert!(plan.show_candidate_menu);
        assert_eq!(plan.payload.commit_text, "selected");
    }

    #[test]
    fn cancellation_does_not_open_a_menu() {
        let plan = make_commit_plan(r#"{"candidates":[{"text":"","source":"cancel"}]}"#, false);

        assert!(!plan.show_candidate_menu);
        assert_eq!(plan.payload.candidates[0].source, CandidateSource::Cancel);
    }

    #[test]
    fn tracks_normal_and_command_sessions() {
        let mut state = FrontendState::default();
        assert!(!state.recording());
        assert!(!state.command_mode());

        state.start_normal(Some("normal-scene"));
        assert!(state.recording());
        assert!(!state.command_mode());
        assert_eq!(state.active_scene_id(), Some("normal-scene"));

        state.start_command(None);
        assert!(state.recording());
        assert!(state.command_mode());
        assert_eq!(state.active_scene_id(), None);
    }

    #[test]
    fn resolves_started_scene_before_stop_fallback() {
        let mut state = FrontendState::default();
        assert_eq!(state.stop_scene_id("fallback"), None);

        state.start_normal(Some("started"));
        assert_eq!(state.stop_scene_id("fallback"), Some("started"));

        state.start_normal(None);
        assert_eq!(state.stop_scene_id("fallback"), Some("fallback"));
    }

    #[test]
    fn adopts_and_resets_remote_recording() {
        let mut state = FrontendState::default();
        state.adopt_recording(true, "remote-scene");
        assert!(state.recording());
        assert!(state.command_mode());
        assert_eq!(state.stop_scene_id("fallback"), Some("remote-scene"));

        state.reset();
        assert_eq!(state, FrontendState::default());
    }
}

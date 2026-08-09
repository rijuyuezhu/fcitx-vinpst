//! Pure frontend session control and recognition outcome planning.

use vinpst_protocol::{CandidateSource, RecognitionPayload};

/// Recording preedit shown after a successful normal start.
pub const RECORDING_PREEDIT: &str = "... Recording ...";
/// Recording preedit shown after a successful command start.
pub const COMMANDING_PREEDIT: &str = "... Commanding ...";
/// Error shown when command mode has no selected text.
pub const NO_SELECTION_ERROR: &str = "Please select text first.";
/// Fallback error when the daemon call supplies no diagnostic.
pub const DAEMON_UNAVAILABLE_ERROR: &str = "Voice input daemon is unavailable.";

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
        self.recording = true;
        self.command_mode = false;
        self.active_scene_id = scene_id.map(str::to_owned);
    }

    /// Records a successful command-mode start.
    pub fn start_command(&mut self, scene_id: Option<&str>) {
        self.recording = true;
        self.command_mode = true;
        self.active_scene_id = scene_id.map(str::to_owned);
    }

    /// Adopts a recording session already active in the daemon.
    pub fn adopt_recording(&mut self, command_mode: bool, scene_id: &str) {
        self.recording = true;
        self.command_mode = command_mode;
        self.active_scene_id = Some(scene_id.to_owned());
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

/// External daemon operation requested by the frontend controller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrontendCall {
    /// Start normal dictation.
    StartNormal,
    /// Start command dictation with selected text.
    StartCommand {
        /// Selected text passed to the command recorder.
        selected_text: String,
    },
    /// Stop the active recording using the captured scene.
    Stop {
        /// Scene captured when recording started, or the stop fallback.
        scene_id: String,
        /// Whether the completed recording is command mode.
        command_mode: bool,
    },
}

impl FrontendCall {
    /// Returns the one string argument required by the daemon operation.
    #[must_use]
    pub fn argument(&self) -> &str {
        match self {
            Self::StartNormal => "",
            Self::StartCommand { selected_text } => selected_text,
            Self::Stop { scene_id, .. } => scene_id,
        }
    }
}

/// Stable frontend presentation outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontendOutcomeKind {
    /// No frontend action is required.
    None,
    /// Show recording preedit.
    Preedit,
    /// Clear preedit and result candidates.
    Clear,
    /// Commit final text.
    Commit,
    /// Show result candidates.
    CandidateMenu,
    /// Show an error.
    Error,
}

/// Complete frontend result produced by Rust control and data-plane logic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontendOutcome {
    kind: FrontendOutcomeKind,
    text: String,
    payload: RecognitionPayload,
    command_mode: bool,
}

impl Default for FrontendOutcome {
    fn default() -> Self {
        Self {
            kind: FrontendOutcomeKind::None,
            text: String::new(),
            payload: RecognitionPayload {
                commit_text: String::new(),
                candidates: Vec::new(),
            },
            command_mode: false,
        }
    }
}

impl FrontendOutcome {
    /// Returns the presentation kind.
    #[must_use]
    pub const fn kind(&self) -> FrontendOutcomeKind {
        self.kind
    }

    /// Returns the primary preedit, commit, or error text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the normalized recognition payload.
    #[must_use]
    pub const fn payload(&self) -> &RecognitionPayload {
        &self.payload
    }

    /// Returns whether the completed operation was command mode.
    #[must_use]
    pub const fn command_mode(&self) -> bool {
        self.command_mode
    }

    fn preedit(text: &str) -> Self {
        Self {
            kind: FrontendOutcomeKind::Preedit,
            text: text.to_owned(),
            ..Self::default()
        }
    }

    fn error(error: &str) -> Self {
        Self {
            kind: FrontendOutcomeKind::Error,
            text: fallback_error(error).to_owned(),
            ..Self::default()
        }
    }

    /// Builds a final frontend outcome from a daemon recognition payload.
    #[must_use]
    pub fn from_payload(json: &str, command_mode: bool) -> Self {
        let plan = make_commit_plan(json, command_mode);
        if plan.payload.commit_text.is_empty() {
            return Self {
                kind: FrontendOutcomeKind::Clear,
                payload: plan.payload,
                command_mode,
                ..Self::default()
            };
        }
        if plan.show_candidate_menu {
            return Self {
                kind: FrontendOutcomeKind::CandidateMenu,
                payload: plan.payload,
                command_mode,
                ..Self::default()
            };
        }

        Self {
            kind: FrontendOutcomeKind::Commit,
            text: plan.payload.commit_text.clone(),
            payload: plan.payload,
            command_mode,
        }
    }
}

/// Semantic trigger request classified by the retained Fcitx key adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontendTriggerRequest {
    /// No semantic action.
    None,
    /// Start normal dictation.
    StartNormal,
    /// Stop normal dictation.
    StopNormal,
    /// Start command dictation.
    StartCommand,
    /// Stop command dictation.
    StopCommand,
    /// Open the scene menu.
    ShowSceneMenu,
    /// Consume the scene-menu trigger release.
    ConsumeSceneMenuRelease,
    /// Open the ASR menu.
    ShowAsrMenu,
    /// Consume the ASR-menu trigger release.
    ConsumeAsrMenuRelease,
}

/// Frontend intent after applying session-state gating in Rust.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontendTriggerIntent {
    /// Ignore the request without changing frontend state.
    None,
    /// Reconcile and start normal dictation.
    StartNormal,
    /// Stop the active normal dictation session.
    StopNormal,
    /// Reconcile and start command dictation.
    StartCommand,
    /// Stop the active command dictation session.
    StopCommand,
    /// Open the scene menu.
    ShowSceneMenu,
    /// Open the ASR menu.
    ShowAsrMenu,
}

/// Result of preparing an external daemon operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrontendStep {
    /// C++ must execute the pending daemon call and report its result.
    CallReady,
    /// The operation completed without a daemon call.
    Outcome(FrontendOutcome),
}

/// Owns frontend session state and all start/stop control decisions.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FrontendController {
    state: FrontendState,
    pending_call: Option<FrontendCall>,
}

impl FrontendController {
    /// Returns whether a recording session is active.
    #[must_use]
    pub const fn recording(&self) -> bool {
        self.state.recording()
    }

    /// Returns whether the active session is command mode.
    #[must_use]
    pub const fn command_mode(&self) -> bool {
        self.state.command_mode()
    }

    /// Returns the pending daemon operation.
    #[must_use]
    pub const fn pending_call(&self) -> Option<&FrontendCall> {
        self.pending_call.as_ref()
    }

    /// Applies current session-state gating to one semantic trigger request.
    #[must_use]
    pub const fn plan_trigger(&self, request: FrontendTriggerRequest) -> FrontendTriggerIntent {
        match request {
            FrontendTriggerRequest::StartNormal if !self.state.recording() => {
                FrontendTriggerIntent::StartNormal
            }
            FrontendTriggerRequest::StopNormal
                if self.state.recording() && !self.state.command_mode() =>
            {
                FrontendTriggerIntent::StopNormal
            }
            FrontendTriggerRequest::StartCommand if !self.state.recording() => {
                FrontendTriggerIntent::StartCommand
            }
            FrontendTriggerRequest::StopCommand
                if self.state.recording() && self.state.command_mode() =>
            {
                FrontendTriggerIntent::StopCommand
            }
            FrontendTriggerRequest::ShowSceneMenu if !self.state.recording() => {
                FrontendTriggerIntent::ShowSceneMenu
            }
            FrontendTriggerRequest::ShowAsrMenu if !self.state.recording() => {
                FrontendTriggerIntent::ShowAsrMenu
            }
            FrontendTriggerRequest::None
            | FrontendTriggerRequest::StartNormal
            | FrontendTriggerRequest::StopNormal
            | FrontendTriggerRequest::StartCommand
            | FrontendTriggerRequest::StopCommand
            | FrontendTriggerRequest::ShowSceneMenu
            | FrontendTriggerRequest::ConsumeSceneMenuRelease
            | FrontendTriggerRequest::ShowAsrMenu
            | FrontendTriggerRequest::ConsumeAsrMenuRelease => FrontendTriggerIntent::None,
        }
    }

    /// Prepares a normal-dictation start.
    pub fn start_normal(&mut self, scene_id: Option<&str>) -> FrontendStep {
        self.pending_call = Some(FrontendCall::StartNormal);
        self.state.start_normal(scene_id);
        FrontendStep::CallReady
    }

    /// Prepares a command-mode start or returns the no-selection error.
    pub fn start_command(&mut self, selected_text: &str, scene_id: Option<&str>) -> FrontendStep {
        if selected_text.is_empty() {
            self.reset();
            return FrontendStep::Outcome(FrontendOutcome::error(NO_SELECTION_ERROR));
        }

        self.pending_call = Some(FrontendCall::StartCommand {
            selected_text: selected_text.to_owned(),
        });
        self.state.start_command(scene_id);
        FrontendStep::CallReady
    }

    /// Prepares a stop call or returns a no-op outcome when idle.
    pub fn stop(&mut self, fallback_scene_id: &str) -> FrontendStep {
        let Some(scene_id) = self.state.stop_scene_id(fallback_scene_id) else {
            self.pending_call = None;
            return FrontendStep::Outcome(FrontendOutcome::default());
        };
        self.pending_call = Some(FrontendCall::Stop {
            scene_id: scene_id.to_owned(),
            command_mode: self.state.command_mode(),
        });
        FrontendStep::CallReady
    }

    /// Completes the pending daemon operation.
    pub fn complete(&mut self, success: bool, response: &str) -> FrontendOutcome {
        let Some(call) = self.pending_call.take() else {
            return FrontendOutcome::default();
        };

        match call {
            FrontendCall::StartNormal => {
                if success {
                    FrontendOutcome::preedit(RECORDING_PREEDIT)
                } else {
                    self.state.reset();
                    FrontendOutcome::error(response)
                }
            }
            FrontendCall::StartCommand { .. } => {
                if success {
                    FrontendOutcome::preedit(COMMANDING_PREEDIT)
                } else {
                    self.state.reset();
                    FrontendOutcome::error(response)
                }
            }
            FrontendCall::Stop { command_mode, .. } => {
                self.state.reset();
                if success {
                    FrontendOutcome::from_payload(response, command_mode)
                } else {
                    FrontendOutcome::error(response)
                }
            }
        }
    }

    /// Completes the active frontend session from the daemon `RecognitionResult` signal.
    ///
    /// Final-result delivery is independent from the `StopRecording` reply: an
    /// externally started session has no local pending Stop call, while a local
    /// Stop may emit this signal before its method callback runs.
    pub fn complete_recognition_result(&mut self, response: &str) -> FrontendOutcome {
        if !self.state.recording() {
            return FrontendOutcome::default();
        }
        let command_mode = self.state.command_mode();
        self.state.reset();
        if matches!(self.pending_call, Some(FrontendCall::Stop { .. })) {
            self.pending_call = None;
        }
        FrontendOutcome::from_payload(response, command_mode)
    }

    /// Adopts a recording session already active in the daemon.
    pub fn adopt_recording(&mut self, command_mode: bool, scene_id: &str) {
        self.pending_call = None;
        self.state.adopt_recording(command_mode, scene_id);
    }

    /// Returns the controller to idle.
    pub fn reset(&mut self) {
        self.pending_call = None;
        self.state.reset();
    }
}

fn fallback_error(error: &str) -> &str {
    if error.is_empty() {
        DAEMON_UNAVAILABLE_ERROR
    } else {
        error
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
        COMMANDING_PREEDIT, DAEMON_UNAVAILABLE_ERROR, FrontendCall, FrontendController,
        FrontendOutcome, FrontendOutcomeKind, FrontendState, FrontendStep, FrontendTriggerIntent,
        FrontendTriggerRequest, NO_SELECTION_ERROR, RECORDING_PREEDIT, make_commit_plan,
        parse_recognition_payload, should_show_candidate_menu,
    };
    use vinpst_protocol::{Candidate, CandidateSource};

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
        state.start_normal(Some("normal-scene"));
        assert!(state.recording());
        assert!(!state.command_mode());
        assert_eq!(state.active_scene_id(), Some("normal-scene"));
        state.start_command(None);
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
        state.reset();
        assert_eq!(state, FrontendState::default());
    }

    #[test]
    fn controls_normal_start_and_stop_end_to_end() {
        let mut controller = FrontendController::default();
        assert_eq!(
            controller.start_normal(Some("started")),
            FrontendStep::CallReady
        );
        assert_eq!(controller.pending_call(), Some(&FrontendCall::StartNormal));
        let start = controller.complete(true, "");
        assert_eq!(start.kind(), FrontendOutcomeKind::Preedit);
        assert_eq!(start.text(), RECORDING_PREEDIT);
        assert!(controller.recording());

        assert_eq!(controller.stop("fallback"), FrontendStep::CallReady);
        assert_eq!(
            controller.pending_call().map(FrontendCall::argument),
            Some("started")
        );
        let stop = controller.complete(
            true,
            r#"{"commit_text":"done","candidates":[{"text":"done","source":"asr"}]}"#,
        );
        assert_eq!(stop.kind(), FrontendOutcomeKind::Commit);
        assert_eq!(stop.text(), "done");
        assert!(!stop.command_mode());
        assert!(!controller.recording());
    }

    #[test]
    fn controls_command_start_and_candidate_outcome() {
        let mut controller = FrontendController::default();
        assert_eq!(
            controller.start_command("selected", Some("command-scene")),
            FrontendStep::CallReady
        );
        assert_eq!(
            controller.pending_call().map(FrontendCall::argument),
            Some("selected")
        );
        let start = controller.complete(true, "");
        assert_eq!(start.text(), COMMANDING_PREEDIT);
        assert!(controller.command_mode());

        assert_eq!(controller.stop("fallback"), FrontendStep::CallReady);
        let stop = controller.complete(
            true,
            r#"{"commit_text":"selected","candidates":[{"text":"selected","source":"raw"},{"text":"changed","source":"asr"}]}"#,
        );
        assert_eq!(stop.kind(), FrontendOutcomeKind::CandidateMenu);
        assert!(stop.command_mode());
        assert_eq!(stop.payload().candidates.len(), 2);
        assert!(!controller.recording());
    }

    #[test]
    fn rejects_empty_command_before_daemon_call() {
        let mut controller = FrontendController::default();
        let FrontendStep::Outcome(outcome) = controller.start_command("", None) else {
            panic!("empty selection must finish immediately");
        };
        assert_eq!(outcome.kind(), FrontendOutcomeKind::Error);
        assert_eq!(outcome.text(), NO_SELECTION_ERROR);
        assert!(controller.pending_call().is_none());
    }

    #[test]
    fn daemon_failures_reset_state_and_use_fallback_error() {
        let mut controller = FrontendController::default();
        controller.start_normal(None);
        let outcome = controller.complete(false, "");
        assert_eq!(outcome.kind(), FrontendOutcomeKind::Error);
        assert_eq!(outcome.text(), DAEMON_UNAVAILABLE_ERROR);
        assert!(!controller.recording());

        controller.start_command("selected", None);
        let outcome = controller.complete(false, "specific error");
        assert_eq!(outcome.text(), "specific error");
        assert!(!controller.recording());
    }

    #[test]
    fn idle_stop_is_a_noop_and_adopted_sessions_stop_normally() {
        let mut controller = FrontendController::default();
        assert_eq!(
            controller.stop("fallback"),
            FrontendStep::Outcome(FrontendOutcome::default())
        );
        controller.adopt_recording(true, "adopted");
        assert_eq!(controller.stop("fallback"), FrontendStep::CallReady);
        assert_eq!(
            controller.pending_call().map(FrontendCall::argument),
            Some("adopted")
        );
        let outcome = controller.complete(true, r#"{"commit_text":"result"}"#);
        assert_eq!(outcome.kind(), FrontendOutcomeKind::Commit);
        assert!(outcome.command_mode());
    }

    #[test]
    fn recognition_result_completes_adopted_session_without_pending_stop() {
        let mut controller = FrontendController::default();
        controller.adopt_recording(false, "raw");
        let outcome = controller.complete_recognition_result(
            r#"{"commit_text":"external","candidates":[{"text":"external","source":"asr"}]}"#,
        );
        assert_eq!(outcome.kind(), FrontendOutcomeKind::Commit);
        assert_eq!(outcome.text(), "external");
        assert!(!controller.recording());
        assert!(controller.pending_call().is_none());
    }

    #[test]
    fn invalid_and_cancel_payloads_clear_with_captured_mode() {
        let invalid = FrontendOutcome::from_payload("not json", true);
        assert_eq!(invalid.kind(), FrontendOutcomeKind::Clear);
        assert!(invalid.command_mode());
        let cancel = FrontendOutcome::from_payload(
            r#"{"candidates":[{"text":"","source":"cancel"}]}"#,
            false,
        );
        assert_eq!(cancel.kind(), FrontendOutcomeKind::Clear);
    }
    #[test]
    fn gates_semantic_triggers_from_rust_session_state() {
        let mut controller = FrontendController::default();
        assert_eq!(
            controller.plan_trigger(FrontendTriggerRequest::StartNormal),
            FrontendTriggerIntent::StartNormal
        );
        assert_eq!(
            controller.plan_trigger(FrontendTriggerRequest::StopNormal),
            FrontendTriggerIntent::None
        );
        assert_eq!(
            controller.plan_trigger(FrontendTriggerRequest::ShowSceneMenu),
            FrontendTriggerIntent::ShowSceneMenu
        );
        assert_eq!(
            controller.plan_trigger(FrontendTriggerRequest::ShowAsrMenu),
            FrontendTriggerIntent::ShowAsrMenu
        );
        assert_eq!(
            controller.plan_trigger(FrontendTriggerRequest::ConsumeSceneMenuRelease),
            FrontendTriggerIntent::None
        );

        controller.start_normal(Some("scene"));
        assert_eq!(
            controller.plan_trigger(FrontendTriggerRequest::StopNormal),
            FrontendTriggerIntent::StopNormal
        );
        assert_eq!(
            controller.plan_trigger(FrontendTriggerRequest::StartCommand),
            FrontendTriggerIntent::None
        );
        assert_eq!(
            controller.plan_trigger(FrontendTriggerRequest::ShowSceneMenu),
            FrontendTriggerIntent::None
        );

        controller.start_command("selected", None);
        assert_eq!(
            controller.plan_trigger(FrontendTriggerRequest::StopCommand),
            FrontendTriggerIntent::StopCommand
        );
        assert_eq!(
            controller.plan_trigger(FrontendTriggerRequest::StopNormal),
            FrontendTriggerIntent::None
        );
    }
}

//! Pure frontend behavior shared by the retained Fcitx C++ adapter.
//!
//! This crate must not depend on Fcitx types or perform platform integration.
//! It receives owned wire data and returns frontend decisions that can be tested
//! without loading Fcitx.

mod frontend;
mod menu;
mod menu_action;
mod menu_projection;
mod menu_snapshot;
mod trigger_mode;

pub use frontend::{
    COMMANDING_PREEDIT, CommitPlan, DAEMON_UNAVAILABLE_ERROR, FrontendCall, FrontendController,
    FrontendOutcome, FrontendOutcomeKind, FrontendState, FrontendStep, NO_SELECTION_ERROR,
    RECORDING_PREEDIT, make_commit_plan, parse_recognition_payload, should_show_candidate_menu,
};
pub use menu::{MenuFilterState, clamp_menu_page};
pub use menu_action::{MENU_PAGE_SIZE, MenuKeyAction, MenuKeyInput, MenuSemanticKey};
pub use menu_projection::{
    AsrMenuItem, AsrMenuProjectionState, MenuControl, ProjectedMenuItem, SceneMenuItem,
    SceneMenuProjection, is_effective_asr_target, project_asr_menu, project_scene_menu,
};
pub use menu_snapshot::{
    AsrDisplaySnapshot, AsrDisplaySnapshotItem, SceneSnapshot, SceneSnapshotItem,
};
pub use trigger_mode::{
    TRIGGER_DEBOUNCE_NS, TRIGGER_HOLD_THRESHOLD_NS, TriggerAction, TriggerEvent, TriggerKind,
    TriggerMode, TriggerModeState, TriggerStateView,
};
pub use vinput_protocol::{Candidate, CandidateSource, RecognitionPayload};

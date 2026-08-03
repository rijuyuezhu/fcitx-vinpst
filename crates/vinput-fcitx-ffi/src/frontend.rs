//! Thin C ABI over the Rust frontend controller and outcome models.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use vinput_fcitx_core::{
    Candidate, CandidateSource, FrontendCall, FrontendController, FrontendOutcome,
    FrontendOutcomeKind, FrontendStep, FrontendTriggerIntent, FrontendTriggerRequest,
};

/// Request preparation failed.
pub const FRONTEND_STEP_INVALID: u8 = 0;
/// A daemon call is ready.
pub const FRONTEND_STEP_CALL_READY: u8 = 1;
/// The request completed immediately with an outcome.
pub const FRONTEND_STEP_OUTCOME_READY: u8 = 2;

/// No pending daemon call.
pub const FRONTEND_CALL_NONE: u8 = 0;
/// Start normal recording.
pub const FRONTEND_CALL_START_NORMAL: u8 = 1;
/// Start command recording.
pub const FRONTEND_CALL_START_COMMAND: u8 = 2;
/// Stop recording.
pub const FRONTEND_CALL_STOP: u8 = 3;

const FRONTEND_TRIGGER_REQUEST_NONE: u8 = 0;
const FRONTEND_TRIGGER_REQUEST_START_NORMAL: u8 = 1;
const FRONTEND_TRIGGER_REQUEST_STOP_NORMAL: u8 = 2;
const FRONTEND_TRIGGER_REQUEST_START_COMMAND: u8 = 3;
const FRONTEND_TRIGGER_REQUEST_STOP_COMMAND: u8 = 4;
const FRONTEND_TRIGGER_REQUEST_SHOW_SCENE_MENU: u8 = 5;
const FRONTEND_TRIGGER_REQUEST_CONSUME_SCENE_MENU_RELEASE: u8 = 6;
const FRONTEND_TRIGGER_REQUEST_SHOW_ASR_MENU: u8 = 7;
const FRONTEND_TRIGGER_REQUEST_CONSUME_ASR_MENU_RELEASE: u8 = 8;

const FRONTEND_TRIGGER_INTENT_NONE: u8 = 0;
const FRONTEND_TRIGGER_INTENT_START_NORMAL: u8 = 1;
const FRONTEND_TRIGGER_INTENT_STOP_NORMAL: u8 = 2;
const FRONTEND_TRIGGER_INTENT_START_COMMAND: u8 = 3;
const FRONTEND_TRIGGER_INTENT_STOP_COMMAND: u8 = 4;
const FRONTEND_TRIGGER_INTENT_SHOW_SCENE_MENU: u8 = 5;
const FRONTEND_TRIGGER_INTENT_SHOW_ASR_MENU: u8 = 6;

/// Opaque Rust frontend controller.
pub struct VinputFcitxFrontendController {
    controller: FrontendController,
}

/// Opaque Rust frontend outcome.
pub struct VinputFcitxFrontendOutcome {
    outcome: FrontendOutcome,
}

/// Borrowed UTF-8 byte view valid while its owner handle remains alive.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxStringView {
    /// UTF-8 bytes, or null when `len` is zero.
    pub data: *const u8,
    /// Number of readable bytes.
    pub len: usize,
}

/// Borrowed frontend outcome summary.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxFrontendOutcomeView {
    /// Stable `VINPUT_FCITX_FRONTEND_OUTCOME_*` value.
    pub kind: u8,
    /// Whether this result belongs to command mode.
    pub command_mode: u8,
    /// Primary preedit, commit, or error text.
    pub text: VinputFcitxStringView,
    /// Normalized commit text from the recognition payload.
    pub commit_text: VinputFcitxStringView,
    /// Number of normalized candidates.
    pub candidate_count: usize,
}

/// Borrowed candidate row.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxCandidateView {
    /// Candidate text.
    pub text: VinputFcitxStringView,
    /// Stable `VINPUT_FCITX_CANDIDATE_SOURCE_*` value.
    pub source: u8,
}

unsafe fn text_input<'a>(data: *const u8, len: usize) -> Option<&'a str> {
    if data.is_null() {
        return (len == 0).then_some("");
    }
    // SAFETY: Forwarded from each exported function's caller contract.
    std::str::from_utf8(unsafe { std::slice::from_raw_parts(data, len) }).ok()
}

unsafe fn optional_text_input<'a>(
    data: *const u8,
    len: usize,
    present: u8,
) -> Result<Option<&'a str>, ()> {
    if present == 0 {
        return if data.is_null() && len == 0 {
            Ok(None)
        } else {
            Err(())
        };
    }
    // SAFETY: Forwarded from each exported function's caller contract.
    unsafe { text_input(data, len) }.map(Some).ok_or(())
}

fn string_view(value: &str) -> VinputFcitxStringView {
    VinputFcitxStringView {
        data: if value.is_empty() {
            ptr::null()
        } else {
            value.as_ptr()
        },
        len: value.len(),
    }
}

fn call_kind(call: &FrontendCall) -> u8 {
    match call {
        FrontendCall::StartNormal => FRONTEND_CALL_START_NORMAL,
        FrontendCall::StartCommand { .. } => FRONTEND_CALL_START_COMMAND,
        FrontendCall::Stop { .. } => FRONTEND_CALL_STOP,
    }
}

fn outcome_kind(kind: FrontendOutcomeKind) -> u8 {
    match kind {
        FrontendOutcomeKind::None => 0,
        FrontendOutcomeKind::Preedit => 1,
        FrontendOutcomeKind::Clear => 2,
        FrontendOutcomeKind::Commit => 3,
        FrontendOutcomeKind::CandidateMenu => 4,
        FrontendOutcomeKind::Error => 5,
    }
}

fn trigger_request(value: u8) -> Option<FrontendTriggerRequest> {
    match value {
        FRONTEND_TRIGGER_REQUEST_NONE => Some(FrontendTriggerRequest::None),
        FRONTEND_TRIGGER_REQUEST_START_NORMAL => Some(FrontendTriggerRequest::StartNormal),
        FRONTEND_TRIGGER_REQUEST_STOP_NORMAL => Some(FrontendTriggerRequest::StopNormal),
        FRONTEND_TRIGGER_REQUEST_START_COMMAND => Some(FrontendTriggerRequest::StartCommand),
        FRONTEND_TRIGGER_REQUEST_STOP_COMMAND => Some(FrontendTriggerRequest::StopCommand),
        FRONTEND_TRIGGER_REQUEST_SHOW_SCENE_MENU => Some(FrontendTriggerRequest::ShowSceneMenu),
        FRONTEND_TRIGGER_REQUEST_CONSUME_SCENE_MENU_RELEASE => {
            Some(FrontendTriggerRequest::ConsumeSceneMenuRelease)
        }
        FRONTEND_TRIGGER_REQUEST_SHOW_ASR_MENU => Some(FrontendTriggerRequest::ShowAsrMenu),
        FRONTEND_TRIGGER_REQUEST_CONSUME_ASR_MENU_RELEASE => {
            Some(FrontendTriggerRequest::ConsumeAsrMenuRelease)
        }
        _ => None,
    }
}

const fn trigger_intent(intent: FrontendTriggerIntent) -> u8 {
    match intent {
        FrontendTriggerIntent::None => FRONTEND_TRIGGER_INTENT_NONE,
        FrontendTriggerIntent::StartNormal => FRONTEND_TRIGGER_INTENT_START_NORMAL,
        FrontendTriggerIntent::StopNormal => FRONTEND_TRIGGER_INTENT_STOP_NORMAL,
        FrontendTriggerIntent::StartCommand => FRONTEND_TRIGGER_INTENT_START_COMMAND,
        FrontendTriggerIntent::StopCommand => FRONTEND_TRIGGER_INTENT_STOP_COMMAND,
        FrontendTriggerIntent::ShowSceneMenu => FRONTEND_TRIGGER_INTENT_SHOW_SCENE_MENU,
        FrontendTriggerIntent::ShowAsrMenu => FRONTEND_TRIGGER_INTENT_SHOW_ASR_MENU,
    }
}

fn candidate_source(source: CandidateSource) -> u8 {
    match source {
        CandidateSource::Raw => 0,
        CandidateSource::Llm => 1,
        CandidateSource::Asr => 2,
        CandidateSource::Cancel => 3,
    }
}

fn boxed_outcome(outcome: FrontendOutcome) -> *mut VinputFcitxFrontendOutcome {
    Box::into_raw(Box::new(VinputFcitxFrontendOutcome { outcome }))
}

unsafe fn prepare_step(
    step: FrontendStep,
    outcome_out: *mut *mut VinputFcitxFrontendOutcome,
) -> u8 {
    if outcome_out.is_null() {
        return FRONTEND_STEP_INVALID;
    }
    // SAFETY: The caller guarantees a writable output pointer.
    unsafe { outcome_out.write(ptr::null_mut()) };
    match step {
        FrontendStep::CallReady => FRONTEND_STEP_CALL_READY,
        FrontendStep::Outcome(outcome) => {
            // SAFETY: The caller guarantees a writable output pointer.
            unsafe { outcome_out.write(boxed_outcome(outcome)) };
            FRONTEND_STEP_OUTCOME_READY
        }
    }
}

/// Creates an idle frontend controller.
#[unsafe(no_mangle)]
pub extern "C" fn vinput_fcitx_frontend_controller_new() -> *mut VinputFcitxFrontendController {
    catch_unwind(|| {
        Box::into_raw(Box::new(VinputFcitxFrontendController {
            controller: FrontendController::default(),
        }))
    })
    .unwrap_or(ptr::null_mut())
}

/// Releases a frontend controller.
///
/// # Safety
///
/// A non-null pointer must be a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_controller_free(
    controller: *mut VinputFcitxFrontendController,
) {
    if !controller.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            drop(unsafe { Box::from_raw(controller) });
        }));
    }
}

/// Returns one when a recording session is active.
///
/// # Safety
///
/// `controller` must be null or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_controller_recording(
    controller: *const VinputFcitxFrontendController,
) -> u8 {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { controller.as_ref() }.map_or(0, |value| u8::from(value.controller.recording()))
}

/// Returns one when the active session is command mode.
///
/// # Safety
///
/// `controller` must be null or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_controller_command_mode(
    controller: *const VinputFcitxFrontendController,
) -> u8 {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { controller.as_ref() }.map_or(0, |value| u8::from(value.controller.command_mode()))
}

/// Applies Rust session-state gating to one semantic trigger request.
///
/// Invalid handles or request values fail without changing `intent_out`.
///
/// # Safety
///
/// `controller` must be live and `intent_out` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_controller_plan_trigger(
    controller: *const VinputFcitxFrontendController,
    request: u8,
    intent_out: *mut u8,
) -> u8 {
    if intent_out.is_null() {
        return 0;
    }
    // SAFETY: Forwarded from this function's caller contract.
    let Some(controller) = (unsafe { controller.as_ref() }) else {
        return 0;
    };
    let Some(request) = trigger_request(request) else {
        return 0;
    };
    // SAFETY: The caller guarantees a writable output pointer.
    unsafe { intent_out.write(trigger_intent(controller.controller.plan_trigger(request))) };
    1
}

/// Prepares a normal recording start.
///
/// # Safety
///
/// Input pointers must reference their lengths and `outcome_out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_controller_start_normal(
    controller: *mut VinputFcitxFrontendController,
    scene_data: *const u8,
    scene_len: usize,
    has_scene: u8,
    outcome_out: *mut *mut VinputFcitxFrontendOutcome,
) -> u8 {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(controller) = (unsafe { controller.as_mut() }) else {
            return FRONTEND_STEP_INVALID;
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Ok(scene) = (unsafe { optional_text_input(scene_data, scene_len, has_scene) }) else {
            return FRONTEND_STEP_INVALID;
        };
        // SAFETY: Forwarded from this function's caller contract.
        unsafe { prepare_step(controller.controller.start_normal(scene), outcome_out) }
    }))
    .unwrap_or(FRONTEND_STEP_INVALID)
}

/// Prepares a command recording start.
///
/// # Safety
///
/// Input pointers must reference their lengths and `outcome_out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_controller_start_command(
    controller: *mut VinputFcitxFrontendController,
    selected_data: *const u8,
    selected_len: usize,
    scene_data: *const u8,
    scene_len: usize,
    has_scene: u8,
    outcome_out: *mut *mut VinputFcitxFrontendOutcome,
) -> u8 {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(controller) = (unsafe { controller.as_mut() }) else {
            return FRONTEND_STEP_INVALID;
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(selected) = (unsafe { text_input(selected_data, selected_len) }) else {
            return FRONTEND_STEP_INVALID;
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Ok(scene) = (unsafe { optional_text_input(scene_data, scene_len, has_scene) }) else {
            return FRONTEND_STEP_INVALID;
        };
        // SAFETY: Forwarded from this function's caller contract.
        unsafe {
            prepare_step(
                controller.controller.start_command(selected, scene),
                outcome_out,
            )
        }
    }))
    .unwrap_or(FRONTEND_STEP_INVALID)
}

/// Prepares a recording stop.
///
/// # Safety
///
/// Input pointers must reference their lengths and `outcome_out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_controller_stop(
    controller: *mut VinputFcitxFrontendController,
    fallback_scene_data: *const u8,
    fallback_scene_len: usize,
    outcome_out: *mut *mut VinputFcitxFrontendOutcome,
) -> u8 {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(controller) = (unsafe { controller.as_mut() }) else {
            return FRONTEND_STEP_INVALID;
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(fallback) = (unsafe { text_input(fallback_scene_data, fallback_scene_len) })
        else {
            return FRONTEND_STEP_INVALID;
        };
        // SAFETY: Forwarded from this function's caller contract.
        unsafe { prepare_step(controller.controller.stop(fallback), outcome_out) }
    }))
    .unwrap_or(FRONTEND_STEP_INVALID)
}

/// Returns the pending daemon call and its string argument.
///
/// # Safety
///
/// All pointers must be valid for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_controller_pending_call(
    controller: *const VinputFcitxFrontendController,
    kind_out: *mut u8,
    argument_out: *mut VinputFcitxStringView,
) -> u8 {
    if kind_out.is_null() || argument_out.is_null() {
        return 0;
    }
    // SAFETY: Forwarded from this function's caller contract.
    let Some(controller) = (unsafe { controller.as_ref() }) else {
        return 0;
    };
    let Some(call) = controller.controller.pending_call() else {
        // SAFETY: The caller guarantees writable output pointers.
        unsafe {
            kind_out.write(FRONTEND_CALL_NONE);
            argument_out.write(string_view(""));
        }
        return 1;
    };
    // SAFETY: The caller guarantees writable output pointers.
    unsafe {
        kind_out.write(call_kind(call));
        argument_out.write(string_view(call.argument()));
    }
    1
}

/// Completes the pending daemon call and returns an owned outcome.
///
/// # Safety
///
/// `response_data` must reference `response_len` bytes and the controller must be live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_controller_complete(
    controller: *mut VinputFcitxFrontendController,
    success: u8,
    response_data: *const u8,
    response_len: usize,
) -> *mut VinputFcitxFrontendOutcome {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(controller) = (unsafe { controller.as_mut() }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(response) = (unsafe { text_input(response_data, response_len) }) else {
            return ptr::null_mut();
        };
        boxed_outcome(controller.controller.complete(success != 0, response))
    }))
    .unwrap_or(ptr::null_mut())
}

/// Adopts a recording already active in the daemon.
///
/// # Safety
///
/// `scene_data` must reference `scene_len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_controller_adopt(
    controller: *mut VinputFcitxFrontendController,
    command_mode: u8,
    scene_data: *const u8,
    scene_len: usize,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(controller) = (unsafe { controller.as_mut() }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(scene) = (unsafe { text_input(scene_data, scene_len) }) else {
                return false;
            };
            controller
                .controller
                .adopt_recording(command_mode != 0, scene);
            true
        }))
        .unwrap_or(false),
    )
}

/// Resets a frontend controller to idle.
///
/// # Safety
///
/// `controller` must be null or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_controller_reset(
    controller: *mut VinputFcitxFrontendController,
) -> u8 {
    // SAFETY: Forwarded from this function's caller contract.
    let Some(controller) = (unsafe { controller.as_mut() }) else {
        return 0;
    };
    controller.controller.reset();
    1
}

/// Creates a frontend outcome directly from recognition JSON.
///
/// # Safety
///
/// `json_data` must reference `json_len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_outcome_from_payload(
    json_data: *const u8,
    json_len: usize,
    command_mode: u8,
) -> *mut VinputFcitxFrontendOutcome {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(json) = (unsafe { text_input(json_data, json_len) }) else {
            return ptr::null_mut();
        };
        boxed_outcome(FrontendOutcome::from_payload(json, command_mode != 0))
    }))
    .unwrap_or(ptr::null_mut())
}

/// Releases a frontend outcome.
///
/// # Safety
///
/// A non-null pointer must be a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_outcome_free(
    outcome: *mut VinputFcitxFrontendOutcome,
) {
    if !outcome.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            drop(unsafe { Box::from_raw(outcome) });
        }));
    }
}

/// Borrows the complete outcome summary.
///
/// # Safety
///
/// `outcome` must be live and `view_out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_outcome_view(
    outcome: *const VinputFcitxFrontendOutcome,
    view_out: *mut VinputFcitxFrontendOutcomeView,
) -> u8 {
    if view_out.is_null() {
        return 0;
    }
    // SAFETY: Forwarded from this function's caller contract.
    let Some(outcome) = (unsafe { outcome.as_ref() }) else {
        return 0;
    };
    let payload = outcome.outcome.payload();
    // SAFETY: The caller guarantees a writable output pointer.
    unsafe {
        view_out.write(VinputFcitxFrontendOutcomeView {
            kind: outcome_kind(outcome.outcome.kind()),
            command_mode: u8::from(outcome.outcome.command_mode()),
            text: string_view(outcome.outcome.text()),
            commit_text: string_view(&payload.commit_text),
            candidate_count: payload.candidates.len(),
        });
    }
    1
}

/// Borrows one candidate row.
///
/// # Safety
///
/// `outcome` must be live and `view_out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_outcome_candidate(
    outcome: *const VinputFcitxFrontendOutcome,
    index: usize,
    view_out: *mut VinputFcitxCandidateView,
) -> u8 {
    if view_out.is_null() {
        return 0;
    }
    // SAFETY: Forwarded from this function's caller contract.
    let Some(candidate) = (unsafe { outcome.as_ref() })
        .and_then(|outcome| outcome.outcome.payload().candidates.get(index))
    else {
        return 0;
    };
    write_candidate(candidate, view_out)
}

fn write_candidate(candidate: &Candidate, view_out: *mut VinputFcitxCandidateView) -> u8 {
    // SAFETY: Callers validate the output pointer before entering this helper.
    unsafe {
        view_out.write(VinputFcitxCandidateView {
            text: string_view(&candidate.text),
            source: candidate_source(candidate.source),
        });
    }
    1
}

#[cfg(test)]
mod tests;

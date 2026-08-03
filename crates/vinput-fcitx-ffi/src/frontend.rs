//! Thin C ABI over the Rust frontend controller and outcome models.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use vinput_fcitx_core::{
    FrontendCall, FrontendController, FrontendOutcome, FrontendOutcomeKind, FrontendPresentation,
    FrontendStep, FrontendTriggerIntent, FrontendTriggerRequest, PresentedResultCandidate,
    ResultCandidateText, present_frontend_outcome,
};
use vinput_fcitx_dbus::{DaemonOperation, DaemonResponse};

use crate::{
    daemon::VinputFcitxDaemonClient,
    menu_snapshot::{VinputFcitxSceneSnapshot, scene_core_ref},
};

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

/// Opaque Rust-owned platform-neutral frontend presentation.
pub struct VinputFcitxFrontendPresentation {
    presentation: FrontendPresentation,
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

/// Borrowed platform-neutral frontend presentation summary.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxFrontendPresentationView {
    /// Stable `VINPUT_FCITX_FRONTEND_OUTCOME_*` value after fallback normalization.
    pub kind: u8,
    /// Whether commits and candidate selections replace surrounding selected text.
    pub replace_selection: u8,
    /// Preedit, commit, error, or candidate-menu fallback text.
    pub text: VinputFcitxStringView,
    /// Number of fully rendered result candidates.
    pub candidate_count: usize,
    /// Preferred candidate cursor position.
    pub cursor_index: usize,
}

/// Borrowed fully rendered result candidate row.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxPresentedCandidateView {
    /// Candidate text.
    pub text: VinputFcitxStringView,
    /// Localized candidate annotation.
    pub comment: VinputFcitxStringView,
    /// Whether selecting this row commits its text.
    pub commit: u8,
}

unsafe fn text_input<'a>(data: *const u8, len: usize) -> Option<&'a str> {
    if data.is_null() {
        return (len == 0).then_some("");
    }
    // SAFETY: Forwarded from each exported function's caller contract.
    std::str::from_utf8(unsafe { std::slice::from_raw_parts(data, len) }).ok()
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

fn boxed_outcome(outcome: FrontendOutcome) -> *mut VinputFcitxFrontendOutcome {
    Box::into_raw(Box::new(VinputFcitxFrontendOutcome { outcome }))
}

fn execute_step_with(
    controller: &mut FrontendController,
    step: FrontendStep,
    mut call_daemon: impl FnMut(DaemonOperation, &str) -> Result<DaemonResponse, String>,
) -> FrontendOutcome {
    let FrontendStep::CallReady = step else {
        let FrontendStep::Outcome(outcome) = step else {
            unreachable!();
        };
        return outcome;
    };
    let Some(call) = controller.pending_call().cloned() else {
        return FrontendOutcome::default();
    };
    let (operation, expects_text) = match &call {
        FrontendCall::StartNormal => (DaemonOperation::StartRecording, false),
        FrontendCall::StartCommand { .. } => (DaemonOperation::StartCommandRecording, false),
        FrontendCall::Stop { .. } => (DaemonOperation::StopRecording, true),
    };
    let (success, response) = match call_daemon(operation, call.argument()) {
        Ok(DaemonResponse::None) if !expects_text => (true, String::new()),
        Ok(DaemonResponse::Text(text)) if expects_text => (true, text),
        Ok(_) => (false, String::new()),
        Err(error) => (false, error),
    };
    controller.complete(success, &response)
}

fn execute_step(
    controller: &mut VinputFcitxFrontendController,
    daemon: Option<&VinputFcitxDaemonClient>,
    step: FrontendStep,
) -> *mut VinputFcitxFrontendOutcome {
    boxed_outcome(execute_step_with(
        &mut controller.controller,
        step,
        |operation, argument| {
            let Some(daemon) = daemon else {
                return Err(String::new());
            };
            daemon
                .client
                .call(operation, argument, "")
                .map_err(|error| error.to_string())
        },
    ))
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

/// Starts normal recording and executes the prepared daemon call in Rust.
///
/// # Safety
///
/// Handles must be live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_controller_start_normal_with_daemon(
    controller: *mut VinputFcitxFrontendController,
    daemon: *const VinputFcitxDaemonClient,
    scene_snapshot: *const VinputFcitxSceneSnapshot,
) -> *mut VinputFcitxFrontendOutcome {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(controller) = (unsafe { controller.as_mut() }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(scene_snapshot) = (unsafe { scene_core_ref(scene_snapshot) }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let daemon = unsafe { daemon.as_ref() };
        let step = controller
            .controller
            .start_normal(Some(scene_snapshot.active_scene_id()));
        execute_step(controller, daemon, step)
    }))
    .unwrap_or(ptr::null_mut())
}

/// Starts command recording and executes the prepared daemon call in Rust.
///
/// # Safety
///
/// Handles must be live and input byte pointers must reference their lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_controller_start_command_with_daemon(
    controller: *mut VinputFcitxFrontendController,
    daemon: *const VinputFcitxDaemonClient,
    selected_data: *const u8,
    selected_len: usize,
    scene_data: *const u8,
    scene_len: usize,
) -> *mut VinputFcitxFrontendOutcome {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(controller) = (unsafe { controller.as_mut() }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(selected) = (unsafe { text_input(selected_data, selected_len) }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(scene) = (unsafe { text_input(scene_data, scene_len) }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let daemon = unsafe { daemon.as_ref() };
        let step = controller.controller.start_command(selected, Some(scene));
        execute_step(controller, daemon, step)
    }))
    .unwrap_or(ptr::null_mut())
}

/// Stops recording and executes the prepared daemon call in Rust.
///
/// # Safety
///
/// Handles must be live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_controller_stop_with_daemon(
    controller: *mut VinputFcitxFrontendController,
    daemon: *const VinputFcitxDaemonClient,
    scene_snapshot: *const VinputFcitxSceneSnapshot,
) -> *mut VinputFcitxFrontendOutcome {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(controller) = (unsafe { controller.as_mut() }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract. A missing snapshot
        // is valid when the controller already owns a started scene.
        let fallback = unsafe { scene_core_ref(scene_snapshot) }
            .map_or("", vinput_fcitx_core::SceneSnapshot::active_scene_id);
        // SAFETY: Forwarded from this function's caller contract.
        let daemon = unsafe { daemon.as_ref() };
        let step = controller.controller.stop(fallback);
        execute_step(controller, daemon, step)
    }))
    .unwrap_or(ptr::null_mut())
}

/// Adopts an externally started recording and stops it through the Rust daemon client.
///
/// # Safety
///
/// Handles must be live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_controller_adopt_and_stop_with_daemon(
    controller: *mut VinputFcitxFrontendController,
    daemon: *const VinputFcitxDaemonClient,
    command_mode: u8,
    scene_snapshot: *const VinputFcitxSceneSnapshot,
) -> *mut VinputFcitxFrontendOutcome {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(controller) = (unsafe { controller.as_mut() }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(scene_snapshot) = (unsafe { scene_core_ref(scene_snapshot) }) else {
            return ptr::null_mut();
        };
        let scene = scene_snapshot.active_scene_id();
        // SAFETY: Forwarded from this function's caller contract.
        let daemon = unsafe { daemon.as_ref() };
        controller
            .controller
            .adopt_recording(command_mode != 0, scene);
        let step = controller.controller.stop(scene);
        execute_step(controller, daemon, step)
    }))
    .unwrap_or(ptr::null_mut())
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

/// Projects one frontend outcome into a Rust-owned platform-neutral presentation.
///
/// # Safety
///
/// `outcome` must be live and localization byte pointers must reference their lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_presentation_new(
    outcome: *const VinputFcitxFrontendOutcome,
    original_data: *const u8,
    original_len: usize,
    voice_command_data: *const u8,
    voice_command_len: usize,
    cancel_data: *const u8,
    cancel_len: usize,
) -> *mut VinputFcitxFrontendPresentation {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(outcome) = (unsafe { outcome.as_ref() }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(original) = (unsafe { text_input(original_data, original_len) }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(voice_command) = (unsafe { text_input(voice_command_data, voice_command_len) })
        else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(cancel) = (unsafe { text_input(cancel_data, cancel_len) }) else {
            return ptr::null_mut();
        };
        Box::into_raw(Box::new(VinputFcitxFrontendPresentation {
            presentation: present_frontend_outcome(
                &outcome.outcome,
                ResultCandidateText {
                    original,
                    voice_command,
                    cancel,
                },
            ),
        }))
    }))
    .unwrap_or(ptr::null_mut())
}

/// Releases a frontend presentation.
///
/// # Safety
///
/// A non-null pointer must be a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_presentation_free(
    presentation: *mut VinputFcitxFrontendPresentation,
) {
    if !presentation.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            drop(unsafe { Box::from_raw(presentation) });
        }));
    }
}

/// Borrows the complete frontend presentation summary.
///
/// # Safety
///
/// `presentation` must be live and `view_out` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_presentation_view(
    presentation: *const VinputFcitxFrontendPresentation,
    view_out: *mut VinputFcitxFrontendPresentationView,
) -> u8 {
    if view_out.is_null() {
        return 0;
    }
    // SAFETY: Forwarded from this function's caller contract.
    let Some(presentation) = (unsafe { presentation.as_ref() }) else {
        return 0;
    };
    // SAFETY: The caller guarantees a writable output pointer.
    unsafe {
        view_out.write(VinputFcitxFrontendPresentationView {
            kind: outcome_kind(presentation.presentation.kind),
            replace_selection: u8::from(presentation.presentation.replace_selection),
            text: string_view(&presentation.presentation.text),
            candidate_count: presentation.presentation.candidates.len(),
            cursor_index: presentation.presentation.cursor_index,
        });
    }
    1
}

/// Borrows one fully rendered frontend candidate row.
///
/// # Safety
///
/// `presentation` must be live and `view_out` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_presentation_candidate(
    presentation: *const VinputFcitxFrontendPresentation,
    index: usize,
    view_out: *mut VinputFcitxPresentedCandidateView,
) -> u8 {
    if view_out.is_null() {
        return 0;
    }
    // SAFETY: Forwarded from this function's caller contract.
    let Some(candidate) = (unsafe { presentation.as_ref() })
        .and_then(|value| value.presentation.candidates.get(index))
    else {
        return 0;
    };
    write_presented_candidate(candidate, view_out)
}

fn write_presented_candidate(
    candidate: &PresentedResultCandidate,
    view_out: *mut VinputFcitxPresentedCandidateView,
) -> u8 {
    // SAFETY: Callers validate the output pointer before entering this helper.
    unsafe {
        view_out.write(VinputFcitxPresentedCandidateView {
            text: string_view(&candidate.text),
            comment: string_view(&candidate.comment),
            commit: u8::from(candidate.commit),
        });
    }
    1
}

#[cfg(test)]
mod tests;

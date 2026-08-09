//! Thin C ABI over the Rust frontend controller and outcome models.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use vinpst_fcitx_core::{
    FrontendCall, FrontendController, FrontendOutcome, FrontendOutcomeKind, FrontendPresentation,
    FrontendStep, FrontendTriggerIntent, FrontendTriggerRequest, PresentedResultCandidate,
    ResultCandidateText, present_frontend_outcome,
};
use vinpst_fcitx_dbus::{DaemonOperation, DaemonResponse};

use crate::{
    daemon::VinpstFcitxDaemonClient,
    ffi_string::{VinpstFcitxStringView, string_view, text_input},
    menu_controller::{VinpstFcitxSceneMenuController, scene_controller_ref},
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
pub struct VinpstFcitxFrontendController {
    controller: FrontendController,
}

/// Opaque Rust frontend outcome.
pub struct VinpstFcitxFrontendOutcome {
    outcome: FrontendOutcome,
}

/// Opaque Rust-owned platform-neutral frontend presentation.
pub struct VinpstFcitxFrontendPresentation {
    presentation: FrontendPresentation,
}

/// Borrowed platform-neutral frontend presentation summary.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinpstFcitxFrontendPresentationView {
    /// Stable `VINPST_FCITX_FRONTEND_OUTCOME_*` value after fallback normalization.
    pub kind: u8,
    /// Whether commits and candidate selections replace surrounding selected text.
    pub replace_selection: u8,
    /// Preedit, commit, error, or candidate-menu fallback text.
    pub text: VinpstFcitxStringView,
    /// Number of fully rendered result candidates.
    pub candidate_count: usize,
    /// Preferred candidate cursor position.
    pub cursor_index: usize,
}

/// Borrowed fully rendered result candidate row.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinpstFcitxPresentedCandidateView {
    /// Candidate text.
    pub text: VinpstFcitxStringView,
    /// Localized candidate annotation.
    pub comment: VinpstFcitxStringView,
    /// Whether selecting this row commits its text.
    pub commit: u8,
}

/// Borrowed localized candidate annotations used to build a presentation.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinpstFcitxFrontendPresentationTextView {
    /// Label for the unmodified recognition candidate.
    pub original: VinpstFcitxStringView,
    /// Label for voice-command-derived candidates.
    pub voice_command: VinpstFcitxStringView,
    /// Label for the non-committing cancel row.
    pub cancel: VinpstFcitxStringView,
}

impl VinpstFcitxFrontendPresentationTextView {
    unsafe fn borrow(&self) -> Option<ResultCandidateText<'_>> {
        // SAFETY: Forwarded from the exported function's caller contract.
        let original = unsafe { text_input(self.original.data, self.original.len) }?;
        // SAFETY: Forwarded from the exported function's caller contract.
        let voice_command = unsafe { text_input(self.voice_command.data, self.voice_command.len) }?;
        // SAFETY: Forwarded from the exported function's caller contract.
        let cancel = unsafe { text_input(self.cancel.data, self.cancel.len) }?;
        Some(ResultCandidateText {
            original,
            voice_command,
            cancel,
        })
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

fn boxed_outcome(outcome: FrontendOutcome) -> *mut VinpstFcitxFrontendOutcome {
    Box::into_raw(Box::new(VinpstFcitxFrontendOutcome { outcome }))
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
    controller: &mut VinpstFcitxFrontendController,
    daemon: Option<&VinpstFcitxDaemonClient>,
    step: FrontendStep,
) -> *mut VinpstFcitxFrontendOutcome {
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

fn call_ready(step: &FrontendStep) -> bool {
    matches!(step, FrontendStep::CallReady)
}

/// Creates an idle frontend controller.
#[unsafe(no_mangle)]
pub extern "C" fn vinpst_fcitx_frontend_controller_new() -> *mut VinpstFcitxFrontendController {
    catch_unwind(|| {
        Box::into_raw(Box::new(VinpstFcitxFrontendController {
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
pub unsafe extern "C" fn vinpst_fcitx_frontend_controller_free(
    controller: *mut VinpstFcitxFrontendController,
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
pub unsafe extern "C" fn vinpst_fcitx_frontend_controller_recording(
    controller: *const VinpstFcitxFrontendController,
) -> u8 {
    crate::ffi_catch(0, || {
        // SAFETY: Forwarded from this function's caller contract.
        unsafe { controller.as_ref() }.map_or(0, |value| u8::from(value.controller.recording()))
    })
}

/// Returns one when the active session is command mode.
///
/// # Safety
///
/// `controller` must be null or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_frontend_controller_command_mode(
    controller: *const VinpstFcitxFrontendController,
) -> u8 {
    crate::ffi_catch(0, || {
        // SAFETY: Forwarded from this function's caller contract.
        unsafe { controller.as_ref() }.map_or(0, |value| u8::from(value.controller.command_mode()))
    })
}

/// Applies Rust session-state gating to one semantic trigger request.
///
/// Invalid handles or request values fail without changing `intent_out`.
///
/// # Safety
///
/// `controller` must be live and `intent_out` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_frontend_controller_plan_trigger(
    controller: *const VinpstFcitxFrontendController,
    request: u8,
    intent_out: *mut u8,
) -> u8 {
    crate::ffi_catch(0, || {
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
    })
}

/// Prepares a normal recording call without performing D-Bus I/O.
///
/// # Safety
///
/// Handles must be live and the scene controller must contain a snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_frontend_controller_prepare_start_normal(
    controller: *mut VinpstFcitxFrontendController,
    scene_controller: *const VinpstFcitxSceneMenuController,
) -> u8 {
    crate::ffi_catch(0, || {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(controller) = (unsafe { controller.as_mut() }) else {
            return 0;
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(scene_controller) = (unsafe { scene_controller_ref(scene_controller) }) else {
            return 0;
        };
        let Some(scene_snapshot) = scene_controller.snapshot() else {
            return 0;
        };
        u8::from(call_ready(
            &controller
                .controller
                .start_normal(Some(scene_snapshot.active_scene_id())),
        ))
    })
}

/// Prepares a command recording call without performing D-Bus I/O.
///
/// # Safety
///
/// `controller` must be live and input pointers must reference their lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_frontend_controller_prepare_start_command(
    controller: *mut VinpstFcitxFrontendController,
    selected_data: *const u8,
    selected_len: usize,
    scene_data: *const u8,
    scene_len: usize,
) -> u8 {
    crate::ffi_catch(0, || {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(controller) = (unsafe { controller.as_mut() }) else {
            return 0;
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(selected) = (unsafe { text_input(selected_data, selected_len) }) else {
            return 0;
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(scene) = (unsafe { text_input(scene_data, scene_len) }) else {
            return 0;
        };
        u8::from(call_ready(
            &controller.controller.start_command(selected, Some(scene)),
        ))
    })
}

/// Prepares a stop call without performing D-Bus I/O.
///
/// # Safety
///
/// `controller` must be live. A missing scene snapshot supplies an empty fallback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_frontend_controller_prepare_stop(
    controller: *mut VinpstFcitxFrontendController,
    scene_controller: *const VinpstFcitxSceneMenuController,
) -> u8 {
    crate::ffi_catch(0, || {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(controller) = (unsafe { controller.as_mut() }) else {
            return 0;
        };
        // SAFETY: Forwarded from this function's caller contract.
        let fallback = unsafe { scene_controller_ref(scene_controller) }
            .and_then(|value| value.snapshot())
            .map_or("", vinpst_fcitx_core::SceneSnapshot::active_scene_id);
        u8::from(call_ready(&controller.controller.stop(fallback)))
    })
}

/// Adopts an external recording and prepares its stop call without D-Bus I/O.
///
/// # Safety
///
/// Handles must be live and the scene controller must contain a snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_frontend_controller_prepare_adopt_and_stop(
    controller: *mut VinpstFcitxFrontendController,
    command_mode: u8,
    scene_controller: *const VinpstFcitxSceneMenuController,
) -> u8 {
    crate::ffi_catch(0, || {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(controller) = (unsafe { controller.as_mut() }) else {
            return 0;
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(scene_controller) = (unsafe { scene_controller_ref(scene_controller) }) else {
            return 0;
        };
        let Some(scene_snapshot) = scene_controller.snapshot() else {
            return 0;
        };
        let scene = scene_snapshot.active_scene_id();
        controller
            .controller
            .adopt_recording(command_mode != 0, scene);
        u8::from(call_ready(&controller.controller.stop(scene)))
    })
}

/// Adopts an unsolicited external daemon session without preparing a method call.
///
/// # Safety
///
/// Handles must be live and the scene controller must contain a snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_frontend_controller_adopt_external_recording(
    controller: *mut VinpstFcitxFrontendController,
    command_mode: u8,
    scene_controller: *const VinpstFcitxSceneMenuController,
) -> u8 {
    crate::ffi_catch(0, || {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(controller) = (unsafe { controller.as_mut() }) else {
            return 0;
        };
        // SAFETY: Forwarded from this function's caller contract.
        let scene = unsafe { scene_controller_ref(scene_controller) }
            .and_then(|value| value.snapshot())
            .map_or("", vinpst_fcitx_core::SceneSnapshot::active_scene_id);
        controller
            .controller
            .adopt_recording(command_mode != 0, scene);
        1
    })
}

/// Borrows the argument of the currently prepared daemon call.
///
/// The returned view remains valid until the controller is next mutated.
///
/// # Safety
///
/// `controller` must be live and `argument_out` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_frontend_controller_pending_argument(
    controller: *const VinpstFcitxFrontendController,
    argument_out: *mut VinpstFcitxStringView,
) -> u8 {
    crate::ffi_catch(0, || {
        if argument_out.is_null() {
            return 0;
        }
        // SAFETY: Forwarded from this function's caller contract.
        let Some(controller) = (unsafe { controller.as_ref() }) else {
            return 0;
        };
        let Some(call) = controller.controller.pending_call() else {
            return 0;
        };
        // SAFETY: The caller guarantees a writable output pointer.
        unsafe { argument_out.write(string_view(call.argument())) };
        1
    })
}

/// Completes the currently prepared daemon call from an async transport result.
///
/// # Safety
///
/// `controller` must be live and `response_data` must reference `response_len`
/// readable bytes when non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_frontend_controller_complete(
    controller: *mut VinpstFcitxFrontendController,
    success: u8,
    response_data: *const u8,
    response_len: usize,
) -> *mut VinpstFcitxFrontendOutcome {
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

/// Completes the active frontend session from a `RecognitionResult` signal.
///
/// # Safety
///
/// `controller` must be live and `response_data` must reference `response_len`
/// readable bytes when non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_frontend_controller_complete_recognition_result(
    controller: *mut VinpstFcitxFrontendController,
    response_data: *const u8,
    response_len: usize,
) -> *mut VinpstFcitxFrontendOutcome {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(controller) = (unsafe { controller.as_mut() }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(response) = (unsafe { text_input(response_data, response_len) }) else {
            return ptr::null_mut();
        };
        boxed_outcome(controller.controller.complete_recognition_result(response))
    }))
    .unwrap_or(ptr::null_mut())
}

/// Starts normal recording using the scene snapshot owned by a Rust controller.
///
/// # Safety
///
/// Handles must be live and the scene controller must contain a snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_frontend_controller_start_normal_with_daemon(
    controller: *mut VinpstFcitxFrontendController,
    daemon: *const VinpstFcitxDaemonClient,
    scene_controller: *const VinpstFcitxSceneMenuController,
) -> *mut VinpstFcitxFrontendOutcome {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(controller) = (unsafe { controller.as_mut() }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(scene_controller) = (unsafe { scene_controller_ref(scene_controller) }) else {
            return ptr::null_mut();
        };
        let Some(scene_snapshot) = scene_controller.snapshot() else {
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
pub unsafe extern "C" fn vinpst_fcitx_frontend_controller_start_command_with_daemon(
    controller: *mut VinpstFcitxFrontendController,
    daemon: *const VinpstFcitxDaemonClient,
    selected_data: *const u8,
    selected_len: usize,
    scene_data: *const u8,
    scene_len: usize,
) -> *mut VinpstFcitxFrontendOutcome {
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

/// Stops recording using the fallback scene owned by a Rust controller.
///
/// # Safety
///
/// Handles must be live. A controller without a snapshot supplies an empty fallback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_frontend_controller_stop_with_daemon(
    controller: *mut VinpstFcitxFrontendController,
    daemon: *const VinpstFcitxDaemonClient,
    scene_controller: *const VinpstFcitxSceneMenuController,
) -> *mut VinpstFcitxFrontendOutcome {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(controller) = (unsafe { controller.as_mut() }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let fallback = unsafe { scene_controller_ref(scene_controller) }
            .and_then(|value| value.snapshot())
            .map_or("", vinpst_fcitx_core::SceneSnapshot::active_scene_id);
        // SAFETY: Forwarded from this function's caller contract.
        let daemon = unsafe { daemon.as_ref() };
        let step = controller.controller.stop(fallback);
        execute_step(controller, daemon, step)
    }))
    .unwrap_or(ptr::null_mut())
}

/// Adopts and stops an external recording using a Rust-owned scene controller.
///
/// # Safety
///
/// Handles must be live and the scene controller must contain a snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_frontend_controller_adopt_and_stop_with_daemon(
    controller: *mut VinpstFcitxFrontendController,
    daemon: *const VinpstFcitxDaemonClient,
    command_mode: u8,
    scene_controller: *const VinpstFcitxSceneMenuController,
) -> *mut VinpstFcitxFrontendOutcome {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(controller) = (unsafe { controller.as_mut() }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(scene_controller) = (unsafe { scene_controller_ref(scene_controller) }) else {
            return ptr::null_mut();
        };
        let Some(scene_snapshot) = scene_controller.snapshot() else {
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
pub unsafe extern "C" fn vinpst_fcitx_frontend_controller_reset(
    controller: *mut VinpstFcitxFrontendController,
) -> u8 {
    crate::ffi_catch(0, || {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(controller) = (unsafe { controller.as_mut() }) else {
            return 0;
        };
        controller.controller.reset();
        1
    })
}

/// Releases a frontend outcome.
///
/// # Safety
///
/// A non-null pointer must be a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_frontend_outcome_free(
    outcome: *mut VinpstFcitxFrontendOutcome,
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
/// `outcome` must be live and localization views must reference valid UTF-8.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_frontend_presentation_new(
    outcome: *const VinpstFcitxFrontendOutcome,
    text: *const VinpstFcitxFrontendPresentationTextView,
) -> *mut VinpstFcitxFrontendPresentation {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(outcome) = (unsafe { outcome.as_ref() }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(text) = (unsafe { text.as_ref() }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(text) = (unsafe { text.borrow() }) else {
            return ptr::null_mut();
        };
        Box::into_raw(Box::new(VinpstFcitxFrontendPresentation {
            presentation: present_frontend_outcome(&outcome.outcome, text),
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
pub unsafe extern "C" fn vinpst_fcitx_frontend_presentation_free(
    presentation: *mut VinpstFcitxFrontendPresentation,
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
pub unsafe extern "C" fn vinpst_fcitx_frontend_presentation_view(
    presentation: *const VinpstFcitxFrontendPresentation,
    view_out: *mut VinpstFcitxFrontendPresentationView,
) -> u8 {
    crate::ffi_catch(0, || {
        if view_out.is_null() {
            return 0;
        }
        // SAFETY: Forwarded from this function's caller contract.
        let Some(presentation) = (unsafe { presentation.as_ref() }) else {
            return 0;
        };
        // SAFETY: The caller guarantees a writable output pointer.
        unsafe {
            view_out.write(VinpstFcitxFrontendPresentationView {
                kind: outcome_kind(presentation.presentation.kind),
                replace_selection: u8::from(presentation.presentation.replace_selection),
                text: string_view(&presentation.presentation.text),
                candidate_count: presentation.presentation.candidates.len(),
                cursor_index: presentation.presentation.cursor_index,
            });
        }
        1
    })
}

/// Borrows one fully rendered frontend candidate row.
///
/// # Safety
///
/// `presentation` must be live and `view_out` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_frontend_presentation_candidate(
    presentation: *const VinpstFcitxFrontendPresentation,
    index: usize,
    view_out: *mut VinpstFcitxPresentedCandidateView,
) -> u8 {
    crate::ffi_catch(0, || {
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
    })
}

fn write_presented_candidate(
    candidate: &PresentedResultCandidate,
    view_out: *mut VinpstFcitxPresentedCandidateView,
) -> u8 {
    // SAFETY: Callers validate the output pointer before entering this helper.
    unsafe {
        view_out.write(VinpstFcitxPresentedCandidateView {
            text: string_view(&candidate.text),
            comment: string_view(&candidate.comment),
            commit: u8::from(candidate.commit),
        });
    }
    1
}

#[cfg(test)]
mod tests;

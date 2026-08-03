//! Borrowed C views over pure daemon-signal presentation decisions.

use std::ptr;

use vinput_fcitx_core::{
    DaemonControlContext, DaemonControlEvent, DaemonControlPlan, DaemonNotificationKind,
    DaemonStatusPreedit, plan_daemon_control, plan_daemon_notification, plan_daemon_status_preedit,
};

use crate::frontend::VinputFcitxStringView;

/// Borrowed semantic signal presentation.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxDaemonSignalPlanView {
    /// Stable `VINPUT_FCITX_DAEMON_SIGNAL_PLAN_*` value.
    pub kind: u8,
    /// Whether `text` is a gettext message id.
    pub translate: u8,
    /// Borrowed daemon text or gettext message id.
    pub text: VinputFcitxStringView,
}

const SIGNAL_PLAN_CLEAR: u8 = 0;
const SIGNAL_PLAN_PARTIAL: u8 = 1;
const SIGNAL_PLAN_RECORDING: u8 = 2;
const SIGNAL_PLAN_COMMANDING: u8 = 3;
const SIGNAL_PLAN_RECOGNIZING: u8 = 4;
const SIGNAL_PLAN_POSTPROCESSING: u8 = 5;
const SIGNAL_PLAN_NOTIFICATION_INFO: u8 = 6;
const SIGNAL_PLAN_NOTIFICATION_ERROR: u8 = 7;

const CONTROL_EVENT_AVAILABILITY_CHANGED: u8 = 0;
const CONTROL_EVENT_STATUS_CHANGED: u8 = 1;
const CONTROL_EVENT_RECONCILE_BEFORE_START: u8 = 2;

const CONTROL_PLAN_NONE: u8 = 0;
const CONTROL_PLAN_RESET_UNAVAILABLE: u8 = 1;
const CONTROL_PLAN_CLEAR_REMOTE_STATUS: u8 = 2;
const CONTROL_PLAN_RESET_LOCAL_RECORDING: u8 = 3;
const CONTROL_PLAN_UPDATE_LOCAL_PREEDIT: u8 = 4;
const CONTROL_PLAN_PRESENT_REMOTE_STATUS: u8 = 5;
const CONTROL_PLAN_ADOPT_AND_STOP_NORMAL: u8 = 6;
const CONTROL_PLAN_CLEAR_DAEMON_ERROR: u8 = 7;

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

fn control_plan_value(plan: DaemonControlPlan) -> u8 {
    match plan {
        DaemonControlPlan::None => CONTROL_PLAN_NONE,
        DaemonControlPlan::ResetUnavailable => CONTROL_PLAN_RESET_UNAVAILABLE,
        DaemonControlPlan::ClearRemoteStatus => CONTROL_PLAN_CLEAR_REMOTE_STATUS,
        DaemonControlPlan::ResetLocalRecording => CONTROL_PLAN_RESET_LOCAL_RECORDING,
        DaemonControlPlan::UpdateLocalPreedit => CONTROL_PLAN_UPDATE_LOCAL_PREEDIT,
        DaemonControlPlan::PresentRemoteStatus => CONTROL_PLAN_PRESENT_REMOTE_STATUS,
        DaemonControlPlan::AdoptAndStopNormal => CONTROL_PLAN_ADOPT_AND_STOP_NORMAL,
        DaemonControlPlan::ClearDaemonError => CONTROL_PLAN_CLEAR_DAEMON_ERROR,
    }
}

/// Plans one daemon availability/status/reconciliation control event.
///
/// `flag` is availability for event 0 and requested command mode for event 2.
/// Invalid ABI inputs map to the no-op plan.
///
/// # Safety
///
/// Status bytes must be readable for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_daemon_control_plan(
    event: u8,
    status_data: *const u8,
    status_len: usize,
    flag: u8,
    recording: u8,
    remote_status_active: u8,
) -> u8 {
    // SAFETY: Forwarded from this function's caller contract.
    let Some(status) = (unsafe { text_input(status_data, status_len) }) else {
        return CONTROL_PLAN_NONE;
    };
    let event = match event {
        CONTROL_EVENT_AVAILABILITY_CHANGED => DaemonControlEvent::AvailabilityChanged {
            available: flag != 0,
        },
        CONTROL_EVENT_STATUS_CHANGED => DaemonControlEvent::StatusChanged { status },
        CONTROL_EVENT_RECONCILE_BEFORE_START => DaemonControlEvent::ReconcileBeforeStart {
            status,
            requested_command_mode: flag != 0,
        },
        _ => return CONTROL_PLAN_NONE,
    };
    control_plan_value(plan_daemon_control(
        event,
        DaemonControlContext {
            recording: recording != 0,
            remote_status_active: remote_status_active != 0,
        },
    ))
}

/// Plans one status/partial preedit update.
///
/// # Safety
///
/// Input pointers must reference their declared lengths and `view_out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_daemon_status_preedit_plan(
    status_data: *const u8,
    status_len: usize,
    command_mode: u8,
    partial_data: *const u8,
    partial_len: usize,
    view_out: *mut VinputFcitxDaemonSignalPlanView,
) -> u8 {
    if view_out.is_null() {
        return 0;
    }
    // SAFETY: Forwarded from this function's caller contract.
    let Some(status) = (unsafe { text_input(status_data, status_len) }) else {
        return 0;
    };
    // SAFETY: Forwarded from this function's caller contract.
    let Some(partial) = (unsafe { text_input(partial_data, partial_len) }) else {
        return 0;
    };
    let (kind, translate, text) =
        match plan_daemon_status_preedit(status, command_mode != 0, partial) {
            DaemonStatusPreedit::Clear => (SIGNAL_PLAN_CLEAR, false, ""),
            DaemonStatusPreedit::Partial(text) => (SIGNAL_PLAN_PARTIAL, false, text),
            DaemonStatusPreedit::Recording => (SIGNAL_PLAN_RECORDING, true, "... Recording ..."),
            DaemonStatusPreedit::Commanding => (SIGNAL_PLAN_COMMANDING, true, "... Commanding ..."),
            DaemonStatusPreedit::Recognizing => {
                (SIGNAL_PLAN_RECOGNIZING, true, "... Recognizing ...")
            }
            DaemonStatusPreedit::Postprocessing => {
                (SIGNAL_PLAN_POSTPROCESSING, true, "... Postprocessing ...")
            }
        };
    // SAFETY: The caller guarantees a writable output pointer.
    unsafe {
        view_out.write(VinputFcitxDaemonSignalPlanView {
            kind,
            translate: u8::from(translate),
            text: string_view(text),
        });
    }
    1
}

/// Plans one structured daemon notification.
///
/// # Safety
///
/// Input pointers must reference their declared lengths and `view_out` must be writable.
#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_daemon_notification_plan(
    code_data: *const u8,
    code_len: usize,
    subject_data: *const u8,
    subject_len: usize,
    detail_data: *const u8,
    detail_len: usize,
    raw_data: *const u8,
    raw_len: usize,
    view_out: *mut VinputFcitxDaemonSignalPlanView,
) -> u8 {
    if view_out.is_null() {
        return 0;
    }
    // SAFETY: Forwarded from this function's caller contract.
    let Some(code) = (unsafe { text_input(code_data, code_len) }) else {
        return 0;
    };
    // SAFETY: Forwarded from this function's caller contract.
    let Some(subject) = (unsafe { text_input(subject_data, subject_len) }) else {
        return 0;
    };
    // SAFETY: Forwarded from this function's caller contract.
    let Some(detail) = (unsafe { text_input(detail_data, detail_len) }) else {
        return 0;
    };
    // SAFETY: Forwarded from this function's caller contract.
    let Some(raw) = (unsafe { text_input(raw_data, raw_len) }) else {
        return 0;
    };
    let plan = plan_daemon_notification(code, subject, detail, raw);
    let kind = match plan.kind {
        DaemonNotificationKind::Info => SIGNAL_PLAN_NOTIFICATION_INFO,
        DaemonNotificationKind::Error => SIGNAL_PLAN_NOTIFICATION_ERROR,
    };
    // SAFETY: The caller guarantees a writable output pointer.
    let (translate, text) = plan
        .text
        .map_or((true, "Unknown error."), |text| (false, text));
    unsafe {
        view_out.write(VinputFcitxDaemonSignalPlanView {
            kind,
            translate: u8::from(translate),
            text: string_view(text),
        });
    }
    1
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use super::{
        CONTROL_EVENT_AVAILABILITY_CHANGED, CONTROL_EVENT_RECONCILE_BEFORE_START,
        CONTROL_EVENT_STATUS_CHANGED, CONTROL_PLAN_ADOPT_AND_STOP_NORMAL,
        CONTROL_PLAN_CLEAR_REMOTE_STATUS, CONTROL_PLAN_PRESENT_REMOTE_STATUS,
        CONTROL_PLAN_RESET_LOCAL_RECORDING, CONTROL_PLAN_RESET_UNAVAILABLE,
        CONTROL_PLAN_UPDATE_LOCAL_PREEDIT, SIGNAL_PLAN_COMMANDING, SIGNAL_PLAN_NOTIFICATION_ERROR,
        SIGNAL_PLAN_NOTIFICATION_INFO, SIGNAL_PLAN_PARTIAL, VinputFcitxDaemonSignalPlanView,
        vinput_fcitx_daemon_control_plan, vinput_fcitx_daemon_notification_plan,
        vinput_fcitx_daemon_status_preedit_plan,
    };
    use crate::frontend::VinputFcitxStringView;

    unsafe fn bytes(view: VinputFcitxStringView) -> &'static [u8] {
        if view.data.is_null() {
            return &[];
        }
        // SAFETY: Test callers keep source inputs alive.
        unsafe { std::slice::from_raw_parts(view.data, view.len) }
    }

    fn empty_view() -> VinputFcitxDaemonSignalPlanView {
        VinputFcitxDaemonSignalPlanView {
            kind: u8::MAX,
            translate: u8::MAX,
            text: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
        }
    }

    #[test]
    fn plans_status_and_partial_without_allocating_handles() {
        // SAFETY: Local byte slices outlive calls and output is writable.
        unsafe {
            let mut view = empty_view();
            assert_eq!(
                vinput_fcitx_daemon_status_preedit_plan(
                    b"recording".as_ptr(),
                    9,
                    1,
                    ptr::null(),
                    0,
                    &raw mut view,
                ),
                1,
            );
            assert_eq!(view.kind, SIGNAL_PLAN_COMMANDING);
            assert_eq!(view.translate, 1);
            assert_eq!(bytes(view.text), b"... Commanding ...");
            assert_eq!(
                vinput_fcitx_daemon_status_preedit_plan(
                    b"recording".as_ptr(),
                    9,
                    0,
                    b"partial".as_ptr(),
                    7,
                    &raw mut view,
                ),
                1,
            );
            assert_eq!(view.kind, SIGNAL_PLAN_PARTIAL);
            assert_eq!(view.translate, 0);
            assert_eq!(bytes(view.text), b"partial");
        }
    }

    #[test]
    fn plans_notification_priority_and_unknown_fallback() {
        // SAFETY: Local byte slices outlive calls and output is writable.
        unsafe {
            let mut view = empty_view();
            assert_eq!(
                vinput_fcitx_daemon_notification_plan(
                    b"code".as_ptr(),
                    4,
                    b"subject".as_ptr(),
                    7,
                    b"detail".as_ptr(),
                    6,
                    b"raw".as_ptr(),
                    3,
                    &raw mut view,
                ),
                1,
            );
            assert_eq!(view.kind, SIGNAL_PLAN_NOTIFICATION_ERROR);
            assert_eq!(view.translate, 0);
            assert_eq!(bytes(view.text), b"raw");

            assert_eq!(
                vinput_fcitx_daemon_notification_plan(
                    b"unknown".as_ptr(),
                    7,
                    ptr::null(),
                    0,
                    ptr::null(),
                    0,
                    ptr::null(),
                    0,
                    &raw mut view,
                ),
                1,
            );
            assert_eq!(view.kind, SIGNAL_PLAN_NOTIFICATION_INFO);
            assert_eq!(view.translate, 1);
            assert_eq!(bytes(view.text), b"Unknown error.");
        }
    }
    #[test]
    fn plans_daemon_control_events_without_state_handles() {
        // SAFETY: Local byte slices outlive all calls.
        unsafe {
            assert_eq!(
                vinput_fcitx_daemon_control_plan(
                    CONTROL_EVENT_AVAILABILITY_CHANGED,
                    ptr::null(),
                    0,
                    0,
                    1,
                    0,
                ),
                CONTROL_PLAN_RESET_UNAVAILABLE,
            );
            assert_eq!(
                vinput_fcitx_daemon_control_plan(
                    CONTROL_EVENT_STATUS_CHANGED,
                    b"idle".as_ptr(),
                    4,
                    0,
                    0,
                    1,
                ),
                CONTROL_PLAN_CLEAR_REMOTE_STATUS,
            );
            assert_eq!(
                vinput_fcitx_daemon_control_plan(
                    CONTROL_EVENT_STATUS_CHANGED,
                    b"inferring".as_ptr(),
                    9,
                    0,
                    0,
                    1,
                ),
                CONTROL_PLAN_PRESENT_REMOTE_STATUS,
            );
            assert_eq!(
                vinput_fcitx_daemon_control_plan(
                    CONTROL_EVENT_STATUS_CHANGED,
                    b"recording".as_ptr(),
                    9,
                    0,
                    1,
                    0,
                ),
                CONTROL_PLAN_UPDATE_LOCAL_PREEDIT,
            );
            assert_eq!(
                vinput_fcitx_daemon_control_plan(
                    CONTROL_EVENT_STATUS_CHANGED,
                    b"error".as_ptr(),
                    5,
                    0,
                    1,
                    0,
                ),
                CONTROL_PLAN_RESET_LOCAL_RECORDING,
            );
            assert_eq!(
                vinput_fcitx_daemon_control_plan(
                    CONTROL_EVENT_RECONCILE_BEFORE_START,
                    b"recording".as_ptr(),
                    9,
                    0,
                    0,
                    0,
                ),
                CONTROL_PLAN_ADOPT_AND_STOP_NORMAL,
            );
            assert_eq!(
                vinput_fcitx_daemon_control_plan(99, ptr::null(), 0, 0, 0, 0),
                0,
            );
        }
    }
}

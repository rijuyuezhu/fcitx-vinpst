//! Borrowed C views over pure daemon-signal presentation decisions.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use vinpst_fcitx_core::{
    DaemonControlContext, DaemonControlEvent, DaemonControlPlan, DaemonLiveState,
    DaemonNotificationKind, DaemonStatusPreedit, plan_daemon_control, plan_daemon_notification,
    plan_daemon_status_preedit,
};

use crate::ffi_string::{VinpstFcitxStringView, string_view, text_input};

/// Borrowed semantic signal presentation.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinpstFcitxDaemonSignalPlanView {
    /// Stable `VINPST_FCITX_DAEMON_SIGNAL_PLAN_*` value.
    pub kind: u8,
    /// Whether `text` is a gettext message id.
    pub translate: u8,
    /// Borrowed daemon text or gettext message id.
    pub text: VinpstFcitxStringView,
}

/// Borrowed structured daemon notification fields.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinpstFcitxDaemonNotificationView {
    /// Stable notification code.
    pub code: VinpstFcitxStringView,
    /// Optional notification subject.
    pub subject: VinpstFcitxStringView,
    /// Optional notification detail.
    pub detail: VinpstFcitxStringView,
    /// Original daemon message used as the highest-priority fallback.
    pub raw: VinpstFcitxStringView,
}

/// Borrowed daemon status and partial preedit context.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinpstFcitxDaemonStatusView {
    /// Current daemon status.
    pub status: VinpstFcitxStringView,
    /// Whether the active remote recording is command mode.
    pub command_mode: u8,
    /// Latest partial recognition text.
    pub partial: VinpstFcitxStringView,
}

/// Borrowed daemon control event and current frontend state.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinpstFcitxDaemonControlView {
    /// Stable `VINPST_FCITX_DAEMON_CONTROL_EVENT_*` value.
    pub event: u8,
    /// Current daemon status.
    pub status: VinpstFcitxStringView,
    /// Event-specific availability or requested-command-mode flag.
    pub flag: u8,
    /// Whether this frontend currently owns a recording.
    pub recording: u8,
    /// Whether a remote daemon status is currently presented.
    pub remote_status_active: u8,
}

struct DaemonNotificationInput<'a> {
    code: &'a str,
    subject: &'a str,
    detail: &'a str,
    raw: &'a str,
}

impl VinpstFcitxDaemonNotificationView {
    unsafe fn borrow(&self) -> Option<DaemonNotificationInput<'_>> {
        // SAFETY: Forwarded from the exported function's caller contract.
        let code = unsafe { text_input(self.code.data, self.code.len) }?;
        // SAFETY: Forwarded from the exported function's caller contract.
        let subject = unsafe { text_input(self.subject.data, self.subject.len) }?;
        // SAFETY: Forwarded from the exported function's caller contract.
        let detail = unsafe { text_input(self.detail.data, self.detail.len) }?;
        // SAFETY: Forwarded from the exported function's caller contract.
        let raw = unsafe { text_input(self.raw.data, self.raw.len) }?;
        Some(DaemonNotificationInput {
            code,
            subject,
            detail,
            raw,
        })
    }
}

impl VinpstFcitxDaemonStatusView {
    unsafe fn borrow(&self) -> Option<(&str, bool, &str)> {
        // SAFETY: Forwarded from the exported function's caller contract.
        let status = unsafe { text_input(self.status.data, self.status.len) }?;
        // SAFETY: Forwarded from the exported function's caller contract.
        let partial = unsafe { text_input(self.partial.data, self.partial.len) }?;
        Some((status, self.command_mode != 0, partial))
    }
}

/// Opaque Rust-owned live daemon presentation state.
pub struct VinpstFcitxDaemonLiveState {
    state: DaemonLiveState,
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

fn write_status_preedit(
    preedit: DaemonStatusPreedit<'_>,
    view_out: *mut VinpstFcitxDaemonSignalPlanView,
) -> u8 {
    if view_out.is_null() {
        return 0;
    }
    let (kind, translate, text) = match preedit {
        DaemonStatusPreedit::Clear => (SIGNAL_PLAN_CLEAR, false, ""),
        DaemonStatusPreedit::Partial(text) => (SIGNAL_PLAN_PARTIAL, false, text),
        DaemonStatusPreedit::Recording => (SIGNAL_PLAN_RECORDING, true, "... Recording ..."),
        DaemonStatusPreedit::Commanding => (SIGNAL_PLAN_COMMANDING, true, "... Commanding ..."),
        DaemonStatusPreedit::Recognizing => (SIGNAL_PLAN_RECOGNIZING, true, "... Recognizing ..."),
        DaemonStatusPreedit::Postprocessing => {
            (SIGNAL_PLAN_POSTPROCESSING, true, "... Postprocessing ...")
        }
    };
    // SAFETY: The caller guarantees a writable output pointer.
    unsafe {
        view_out.write(VinpstFcitxDaemonSignalPlanView {
            kind,
            translate: u8::from(translate),
            text: string_view(text),
        });
    }
    1
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
/// `control.flag` is availability for event 0 and requested command mode for event 2.
/// Invalid ABI inputs map to the no-op plan.
///
/// # Safety
///
/// The control view and its status bytes must be readable for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_daemon_control_plan(
    control: *const VinpstFcitxDaemonControlView,
) -> u8 {
    crate::ffi_catch(CONTROL_PLAN_NONE, || {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(control) = (unsafe { control.as_ref() }) else {
            return CONTROL_PLAN_NONE;
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(status) = (unsafe { text_input(control.status.data, control.status.len) }) else {
            return CONTROL_PLAN_NONE;
        };
        let event = match control.event {
            CONTROL_EVENT_AVAILABILITY_CHANGED => DaemonControlEvent::AvailabilityChanged {
                available: control.flag != 0,
            },
            CONTROL_EVENT_STATUS_CHANGED => DaemonControlEvent::StatusChanged { status },
            CONTROL_EVENT_RECONCILE_BEFORE_START => DaemonControlEvent::ReconcileBeforeStart {
                status,
                requested_command_mode: control.flag != 0,
            },
            _ => return CONTROL_PLAN_NONE,
        };
        control_plan_value(plan_daemon_control(
            event,
            DaemonControlContext {
                recording: control.recording != 0,
                remote_status_active: control.remote_status_active != 0,
            },
        ))
    })
}

/// Allocates empty Rust-owned live daemon presentation state.
#[unsafe(no_mangle)]
pub extern "C" fn vinpst_fcitx_daemon_live_state_new() -> *mut VinpstFcitxDaemonLiveState {
    catch_unwind(|| {
        Box::into_raw(Box::new(VinpstFcitxDaemonLiveState {
            state: DaemonLiveState::default(),
        }))
    })
    .unwrap_or(ptr::null_mut())
}

/// Releases Rust-owned live daemon presentation state.
///
/// # Safety
///
/// A non-null pointer must be a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_daemon_live_state_free(
    state: *mut VinpstFcitxDaemonLiveState,
) {
    if !state.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            drop(unsafe { Box::from_raw(state) });
        }));
    }
}

/// Clears all live status and partial-recognition state.
///
/// # Safety
///
/// `state` must be a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_daemon_live_state_reset(
    state: *mut VinpstFcitxDaemonLiveState,
) -> u8 {
    crate::ffi_catch(0, || {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(state) = (unsafe { state.as_mut() }) else {
            return 0;
        };
        state.state.reset();
        1
    })
}

/// Starts a new daemon status presentation and stores its command mode.
///
/// # Safety
///
/// `state` must be live and status bytes readable for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_daemon_live_state_begin_status(
    state: *mut VinpstFcitxDaemonLiveState,
    status_data: *const u8,
    status_len: usize,
    command_mode: u8,
) -> u8 {
    crate::ffi_catch(0, || {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(state) = (unsafe { state.as_mut() }) else {
            return 0;
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(status) = (unsafe { text_input(status_data, status_len) }) else {
            return 0;
        };
        state.state.begin_status(status, command_mode != 0);
        1
    })
}

/// Replaces the current daemon status without changing mode or partial state.
///
/// # Safety
///
/// `state` must be live and status bytes readable for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_daemon_live_state_update_status(
    state: *mut VinpstFcitxDaemonLiveState,
    status_data: *const u8,
    status_len: usize,
) -> u8 {
    crate::ffi_catch(0, || {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(state) = (unsafe { state.as_mut() }) else {
            return 0;
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(status) = (unsafe { text_input(status_data, status_len) }) else {
            return 0;
        };
        state.state.update_status(status);
        1
    })
}

/// Stores one distinct live partial while local recording is active.
///
/// Returns one only when the visible partial changed.
///
/// # Safety
///
/// `state` must be live and partial bytes readable for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_daemon_live_state_update_partial(
    state: *mut VinpstFcitxDaemonLiveState,
    partial_data: *const u8,
    partial_len: usize,
    recording: u8,
) -> u8 {
    crate::ffi_catch(0, || {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(state) = (unsafe { state.as_mut() }) else {
            return 0;
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(partial) = (unsafe { text_input(partial_data, partial_len) }) else {
            return 0;
        };
        u8::from(state.state.update_partial(partial, recording != 0))
    })
}

/// Borrows the semantic preedit for the current live state.
///
/// # Safety
///
/// `state` must be live and `view_out` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_daemon_live_state_preedit_plan(
    state: *const VinpstFcitxDaemonLiveState,
    view_out: *mut VinpstFcitxDaemonSignalPlanView,
) -> u8 {
    crate::ffi_catch(0, || {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(state) = (unsafe { state.as_ref() }) else {
            return 0;
        };
        write_status_preedit(state.state.preedit(), view_out)
    })
}

/// Returns whether the current live presentation is command mode.
///
/// # Safety
///
/// `state` must be a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_daemon_live_state_command_mode(
    state: *const VinpstFcitxDaemonLiveState,
) -> u8 {
    crate::ffi_catch(0, || {
        // SAFETY: Forwarded from this function's caller contract.
        unsafe { state.as_ref() }.map_or(0, |state| u8::from(state.state.command_mode()))
    })
}

/// Plans one status/partial preedit update.
///
/// # Safety
///
/// Input views must reference valid UTF-8 and `view_out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_daemon_status_preedit_plan(
    status: *const VinpstFcitxDaemonStatusView,
    view_out: *mut VinpstFcitxDaemonSignalPlanView,
) -> u8 {
    crate::ffi_catch(0, || {
        if view_out.is_null() {
            return 0;
        }
        // SAFETY: Forwarded from this function's caller contract.
        let Some(status) = (unsafe { status.as_ref() }) else {
            return 0;
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some((status, command_mode, partial)) = (unsafe { status.borrow() }) else {
            return 0;
        };
        write_status_preedit(
            plan_daemon_status_preedit(status, command_mode, partial),
            view_out,
        )
    })
}

/// Plans one structured daemon notification.
///
/// # Safety
///
/// Input pointers must reference their declared lengths and `view_out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_daemon_notification_plan(
    notification: *const VinpstFcitxDaemonNotificationView,
    view_out: *mut VinpstFcitxDaemonSignalPlanView,
) -> u8 {
    crate::ffi_catch(0, || {
        if view_out.is_null() {
            return 0;
        }
        // SAFETY: Forwarded from this function's caller contract.
        let Some(notification) = (unsafe { notification.as_ref() }) else {
            return 0;
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(notification) = (unsafe { notification.borrow() }) else {
            return 0;
        };
        let plan = plan_daemon_notification(
            notification.code,
            notification.subject,
            notification.detail,
            notification.raw,
        );
        let kind = match plan.kind {
            DaemonNotificationKind::Info => SIGNAL_PLAN_NOTIFICATION_INFO,
            DaemonNotificationKind::Error => SIGNAL_PLAN_NOTIFICATION_ERROR,
        };
        let (translate, text) = plan
            .text
            .map_or((true, "Unknown error."), |text| (false, text));
        // SAFETY: The caller guarantees a writable output pointer.
        unsafe {
            view_out.write(VinpstFcitxDaemonSignalPlanView {
                kind,
                translate: u8::from(translate),
                text: string_view(text),
            });
        }
        1
    })
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use super::{
        CONTROL_EVENT_AVAILABILITY_CHANGED, CONTROL_EVENT_RECONCILE_BEFORE_START,
        CONTROL_EVENT_STATUS_CHANGED, CONTROL_PLAN_ADOPT_AND_STOP_NORMAL,
        CONTROL_PLAN_CLEAR_REMOTE_STATUS, CONTROL_PLAN_NONE, CONTROL_PLAN_PRESENT_REMOTE_STATUS,
        CONTROL_PLAN_RESET_LOCAL_RECORDING, CONTROL_PLAN_RESET_UNAVAILABLE,
        CONTROL_PLAN_UPDATE_LOCAL_PREEDIT, SIGNAL_PLAN_CLEAR, SIGNAL_PLAN_COMMANDING,
        SIGNAL_PLAN_NOTIFICATION_ERROR, SIGNAL_PLAN_NOTIFICATION_INFO, SIGNAL_PLAN_PARTIAL,
        VinpstFcitxDaemonControlView, VinpstFcitxDaemonNotificationView,
        VinpstFcitxDaemonSignalPlanView, VinpstFcitxDaemonStatusView,
        vinpst_fcitx_daemon_control_plan, vinpst_fcitx_daemon_live_state_begin_status,
        vinpst_fcitx_daemon_live_state_command_mode, vinpst_fcitx_daemon_live_state_free,
        vinpst_fcitx_daemon_live_state_new, vinpst_fcitx_daemon_live_state_preedit_plan,
        vinpst_fcitx_daemon_live_state_reset, vinpst_fcitx_daemon_live_state_update_partial,
        vinpst_fcitx_daemon_live_state_update_status, vinpst_fcitx_daemon_notification_plan,
        vinpst_fcitx_daemon_status_preedit_plan,
    };
    use crate::ffi_string::VinpstFcitxStringView;

    unsafe fn bytes(view: VinpstFcitxStringView) -> &'static [u8] {
        if view.data.is_null() {
            return &[];
        }
        // SAFETY: Test callers keep source inputs alive.
        unsafe { std::slice::from_raw_parts(view.data, view.len) }
    }

    fn empty_view() -> VinpstFcitxDaemonSignalPlanView {
        VinpstFcitxDaemonSignalPlanView {
            kind: u8::MAX,
            translate: u8::MAX,
            text: VinpstFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
        }
    }

    fn string_view(value: &[u8]) -> VinpstFcitxStringView {
        VinpstFcitxStringView {
            data: if value.is_empty() {
                ptr::null()
            } else {
                value.as_ptr()
            },
            len: value.len(),
        }
    }

    fn notification_view(
        code: &[u8],
        subject: &[u8],
        detail: &[u8],
        raw: &[u8],
    ) -> VinpstFcitxDaemonNotificationView {
        VinpstFcitxDaemonNotificationView {
            code: string_view(code),
            subject: string_view(subject),
            detail: string_view(detail),
            raw: string_view(raw),
        }
    }

    fn status_view(
        status: &[u8],
        command_mode: bool,
        partial: &[u8],
    ) -> VinpstFcitxDaemonStatusView {
        VinpstFcitxDaemonStatusView {
            status: string_view(status),
            command_mode: u8::from(command_mode),
            partial: string_view(partial),
        }
    }

    unsafe fn control_plan(
        event: u8,
        status: &[u8],
        flag: bool,
        recording: bool,
        remote_status_active: bool,
    ) -> u8 {
        let control = VinpstFcitxDaemonControlView {
            event,
            status: string_view(status),
            flag: u8::from(flag),
            recording: u8::from(recording),
            remote_status_active: u8::from(remote_status_active),
        };
        // SAFETY: The local view and source bytes are live for this call.
        unsafe { vinpst_fcitx_daemon_control_plan(&raw const control) }
    }

    #[test]
    fn plans_status_and_partial_without_allocating_handles() {
        // SAFETY: Local byte slices outlive calls and output is writable.
        unsafe {
            let mut view = empty_view();
            let status = status_view(b"recording", true, b"");
            assert_eq!(
                vinpst_fcitx_daemon_status_preedit_plan(&raw const status, &raw mut view,),
                1,
            );
            assert_eq!(view.kind, SIGNAL_PLAN_COMMANDING);
            assert_eq!(view.translate, 1);
            assert_eq!(bytes(view.text), b"... Commanding ...");

            let status = status_view(b"recording", false, b"partial");
            assert_eq!(
                vinpst_fcitx_daemon_status_preedit_plan(&raw const status, &raw mut view,),
                1,
            );
            assert_eq!(view.kind, SIGNAL_PLAN_PARTIAL);
            assert_eq!(view.translate, 0);
            assert_eq!(bytes(view.text), b"partial");

            let invalid = [0xff];
            let status = VinpstFcitxDaemonStatusView {
                status: string_view(b"recording"),
                command_mode: 0,
                partial: string_view(&invalid),
            };
            view.kind = 91;
            view.translate = 92;
            assert_eq!(
                vinpst_fcitx_daemon_status_preedit_plan(&raw const status, &raw mut view,),
                0,
            );
            assert_eq!((view.kind, view.translate), (91, 92));
        }
    }

    #[test]
    fn owns_live_status_partial_and_deduplication() {
        // SAFETY: The state handle is live for all calls and freed exactly once.
        unsafe {
            let state = vinpst_fcitx_daemon_live_state_new();
            assert!(!state.is_null());
            let mut view = empty_view();

            assert_eq!(
                vinpst_fcitx_daemon_live_state_begin_status(state, b"recording".as_ptr(), 9, 1,),
                1,
            );
            assert_eq!(vinpst_fcitx_daemon_live_state_command_mode(state), 1);
            assert_eq!(
                vinpst_fcitx_daemon_live_state_preedit_plan(state, &raw mut view),
                1,
            );
            assert_eq!(view.kind, SIGNAL_PLAN_COMMANDING);

            assert_eq!(
                vinpst_fcitx_daemon_live_state_update_partial(state, b"partial".as_ptr(), 7, 1,),
                1,
            );
            assert_eq!(
                vinpst_fcitx_daemon_live_state_update_partial(state, b"partial".as_ptr(), 7, 1,),
                0,
            );
            assert_eq!(
                vinpst_fcitx_daemon_live_state_preedit_plan(state, &raw mut view),
                1,
            );
            assert_eq!(view.kind, SIGNAL_PLAN_PARTIAL);
            assert_eq!(bytes(view.text), b"partial");

            assert_eq!(
                vinpst_fcitx_daemon_live_state_update_status(state, b"postprocessing".as_ptr(), 14,),
                1,
            );
            assert_eq!(vinpst_fcitx_daemon_live_state_command_mode(state), 1);
            assert_eq!(
                vinpst_fcitx_daemon_live_state_preedit_plan(state, &raw mut view),
                1,
            );
            assert_eq!(view.kind, SIGNAL_PLAN_PARTIAL);

            assert_eq!(vinpst_fcitx_daemon_live_state_reset(state), 1);
            assert_eq!(vinpst_fcitx_daemon_live_state_command_mode(state), 0);
            assert_eq!(
                vinpst_fcitx_daemon_live_state_preedit_plan(state, &raw mut view),
                1,
            );
            assert_eq!(view.kind, SIGNAL_PLAN_CLEAR);
            vinpst_fcitx_daemon_live_state_free(state);
        }
    }

    #[test]
    fn plans_notification_priority_and_unknown_fallback() {
        // SAFETY: Local byte slices outlive calls and output is writable.
        unsafe {
            let mut view = empty_view();
            let notification = notification_view(b"code", b"subject", b"detail", b"raw");
            assert_eq!(
                vinpst_fcitx_daemon_notification_plan(&raw const notification, &raw mut view,),
                1,
            );
            assert_eq!(view.kind, SIGNAL_PLAN_NOTIFICATION_ERROR);
            assert_eq!(view.translate, 0);
            assert_eq!(bytes(view.text), b"raw");

            let notification = notification_view(b"unknown", b"", b"", b"");
            assert_eq!(
                vinpst_fcitx_daemon_notification_plan(&raw const notification, &raw mut view,),
                1,
            );
            assert_eq!(view.kind, SIGNAL_PLAN_NOTIFICATION_INFO);
            assert_eq!(view.translate, 1);
            assert_eq!(bytes(view.text), b"Unknown error.");

            let invalid = [0xff];
            let notification = VinpstFcitxDaemonNotificationView {
                code: string_view(b"code"),
                subject: string_view(&invalid),
                detail: string_view(b"detail"),
                raw: string_view(b"raw"),
            };
            view.kind = 91;
            view.translate = 92;
            assert_eq!(
                vinpst_fcitx_daemon_notification_plan(&raw const notification, &raw mut view,),
                0,
            );
            assert_eq!((view.kind, view.translate), (91, 92));
        }
    }
    #[test]
    fn plans_daemon_control_events_without_state_handles() {
        // SAFETY: Local byte slices outlive all calls.
        unsafe {
            assert_eq!(
                control_plan(CONTROL_EVENT_AVAILABILITY_CHANGED, b"", false, true, false),
                CONTROL_PLAN_RESET_UNAVAILABLE,
            );
            assert_eq!(
                control_plan(CONTROL_EVENT_STATUS_CHANGED, b"idle", false, false, true),
                CONTROL_PLAN_CLEAR_REMOTE_STATUS,
            );
            assert_eq!(
                control_plan(
                    CONTROL_EVENT_STATUS_CHANGED,
                    b"inferring",
                    false,
                    false,
                    true,
                ),
                CONTROL_PLAN_PRESENT_REMOTE_STATUS,
            );
            assert_eq!(
                control_plan(
                    CONTROL_EVENT_STATUS_CHANGED,
                    b"recording",
                    false,
                    true,
                    false,
                ),
                CONTROL_PLAN_UPDATE_LOCAL_PREEDIT,
            );
            assert_eq!(
                control_plan(CONTROL_EVENT_STATUS_CHANGED, b"error", false, true, false),
                CONTROL_PLAN_RESET_LOCAL_RECORDING,
            );
            assert_eq!(
                control_plan(
                    CONTROL_EVENT_RECONCILE_BEFORE_START,
                    b"recording",
                    false,
                    false,
                    false,
                ),
                CONTROL_PLAN_ADOPT_AND_STOP_NORMAL,
            );
            assert_eq!(
                control_plan(99, b"", false, false, false),
                CONTROL_PLAN_NONE
            );

            let invalid = [0xff];
            assert_eq!(
                control_plan(CONTROL_EVENT_STATUS_CHANGED, &invalid, false, false, false,),
                CONTROL_PLAN_NONE,
            );
        }
    }
}

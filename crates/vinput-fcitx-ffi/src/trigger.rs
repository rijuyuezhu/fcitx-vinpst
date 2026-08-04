//! Compact C ABI for the Rust-owned trigger controller.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use vinput_fcitx_core::{TriggerEvent, TriggerKind, TriggerMode, TriggerModeState};

/// Opaque trigger controller owned by Rust.
pub struct VinputFcitxTriggerState {
    state: TriggerModeState,
}

/// One trigger event supplied by the Fcitx adapter.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxTriggerEventView {
    /// Stable `VINPUT_FCITX_TRIGGER_EVENT_*` value.
    pub kind: u8,
    /// Trigger mode or trigger kind, depending on `kind`.
    pub value: u8,
    /// Boolean event flag, depending on `kind`.
    pub flag: u8,
    /// Monotonic timestamp used by press and release events.
    pub now_ns: i64,
}

const TRIGGER_EVENT_SET_MODE: u8 = 0;
const TRIGGER_EVENT_PRESS: u8 = 1;
const TRIGGER_EVENT_RELEASE: u8 = 2;
const TRIGGER_EVENT_FIRE_PENDING_START: u8 = 3;
const TRIGGER_EVENT_FIRE_PENDING_STOP: u8 = 4;
const TRIGGER_EVENT_CONFIRM_START: u8 = 5;
const TRIGGER_EVENT_RECORDING_STOPPED: u8 = 6;

fn trigger_mode(value: u8) -> Option<TriggerMode> {
    match value {
        0 => Some(TriggerMode::Tap),
        1 => Some(TriggerMode::Hold),
        2 => Some(TriggerMode::Both),
        _ => None,
    }
}

fn trigger_kind(value: u8) -> Option<TriggerKind> {
    match value {
        0 => Some(TriggerKind::Normal),
        1 => Some(TriggerKind::Command),
        _ => None,
    }
}

fn trigger_event(event: VinputFcitxTriggerEventView) -> Option<TriggerEvent> {
    match event.kind {
        TRIGGER_EVENT_SET_MODE => trigger_mode(event.value).map(TriggerEvent::SetMode),
        TRIGGER_EVENT_PRESS => trigger_kind(event.value).map(|kind| TriggerEvent::Press {
            kind,
            now_ns: event.now_ns,
            recording: event.flag != 0,
        }),
        TRIGGER_EVENT_RELEASE => Some(TriggerEvent::Release {
            now_ns: event.now_ns,
            active_release: event.flag != 0,
        }),
        TRIGGER_EVENT_FIRE_PENDING_START => Some(TriggerEvent::FirePendingStart),
        TRIGGER_EVENT_FIRE_PENDING_STOP => Some(TriggerEvent::FirePendingStop),
        TRIGGER_EVENT_CONFIRM_START => Some(TriggerEvent::ConfirmStart {
            recording_started: event.flag != 0,
        }),
        TRIGGER_EVENT_RECORDING_STOPPED => Some(TriggerEvent::RecordingStopped),
        _ => None,
    }
}

/// Creates an idle trigger controller.
#[unsafe(no_mangle)]
pub extern "C" fn vinput_fcitx_trigger_state_new(mode: u8) -> *mut VinputFcitxTriggerState {
    catch_unwind(|| {
        let Some(mode) = trigger_mode(mode) else {
            return ptr::null_mut();
        };
        Box::into_raw(Box::new(VinputFcitxTriggerState {
            state: TriggerModeState::new(mode),
        }))
    })
    .unwrap_or(ptr::null_mut())
}

/// Releases a trigger controller.
///
/// # Safety
///
/// A non-null pointer must be a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_trigger_state_free(state: *mut VinputFcitxTriggerState) {
    if !state.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            drop(unsafe { Box::from_raw(state) });
        }));
    }
}

/// Applies one trigger event and returns its requested action.
///
/// Invalid events fail without mutating the controller or output.
///
/// # Safety
///
/// `state` must be live; `event` must be readable and `action_out` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_trigger_state_dispatch(
    state: *mut VinputFcitxTriggerState,
    event: *const VinputFcitxTriggerEventView,
    action_out: *mut u8,
) -> u8 {
    if action_out.is_null() {
        return 0;
    }
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(state) = (unsafe { state.as_mut() }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(event) = (unsafe { event.as_ref() }).copied().and_then(trigger_event) else {
                return false;
            };
            let action = state.state.dispatch(event);
            // SAFETY: The caller guarantees a writable output pointer.
            unsafe { action_out.write(action as u8) };
            true
        }))
        .unwrap_or(false),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        TRIGGER_EVENT_CONFIRM_START, TRIGGER_EVENT_FIRE_PENDING_START,
        TRIGGER_EVENT_FIRE_PENDING_STOP, TRIGGER_EVENT_PRESS, TRIGGER_EVENT_RECORDING_STOPPED,
        TRIGGER_EVENT_RELEASE, TRIGGER_EVENT_SET_MODE, VinputFcitxTriggerEventView,
        vinput_fcitx_trigger_state_dispatch, vinput_fcitx_trigger_state_free,
        vinput_fcitx_trigger_state_new,
    };

    unsafe fn dispatch(
        state: *mut super::VinputFcitxTriggerState,
        event: VinputFcitxTriggerEventView,
    ) -> Option<u8> {
        let mut action = u8::MAX;
        // SAFETY: Test callers pass a live state and local readable/writable values.
        let success = unsafe {
            vinput_fcitx_trigger_state_dispatch(state, &raw const event, &raw mut action)
        };
        (success != 0).then_some(action)
    }

    #[test]
    fn dispatches_complete_hold_lifecycle() {
        // SAFETY: The handle is live for every call and freed exactly once.
        unsafe {
            let state = vinput_fcitx_trigger_state_new(1);
            assert!(!state.is_null());
            assert_eq!(
                dispatch(
                    state,
                    VinputFcitxTriggerEventView {
                        kind: TRIGGER_EVENT_PRESS,
                        value: 1,
                        flag: 0,
                        now_ns: 0,
                    },
                ),
                Some(6),
            );
            assert_eq!(
                dispatch(
                    state,
                    VinputFcitxTriggerEventView {
                        kind: TRIGGER_EVENT_FIRE_PENDING_START,
                        value: 0,
                        flag: 0,
                        now_ns: 0,
                    },
                ),
                Some(3),
            );
            assert_eq!(
                dispatch(
                    state,
                    VinputFcitxTriggerEventView {
                        kind: TRIGGER_EVENT_CONFIRM_START,
                        value: 0,
                        flag: 1,
                        now_ns: 0,
                    },
                ),
                Some(0),
            );
            assert_eq!(
                dispatch(
                    state,
                    VinputFcitxTriggerEventView {
                        kind: TRIGGER_EVENT_RELEASE,
                        value: 0,
                        flag: 1,
                        now_ns: 400_000_000,
                    },
                ),
                Some(8),
            );
            assert_eq!(
                dispatch(
                    state,
                    VinputFcitxTriggerEventView {
                        kind: TRIGGER_EVENT_FIRE_PENDING_STOP,
                        value: 0,
                        flag: 0,
                        now_ns: 0,
                    },
                ),
                Some(4),
            );
            assert_eq!(
                dispatch(
                    state,
                    VinputFcitxTriggerEventView {
                        kind: TRIGGER_EVENT_RECORDING_STOPPED,
                        value: 0,
                        flag: 0,
                        now_ns: 0,
                    },
                ),
                Some(0),
            );
            assert_eq!(
                dispatch(
                    state,
                    VinputFcitxTriggerEventView {
                        kind: TRIGGER_EVENT_RELEASE,
                        value: 0,
                        flag: 1,
                        now_ns: 500_000_000,
                    },
                ),
                Some(1),
            );
            vinput_fcitx_trigger_state_free(state);
        }
    }

    #[test]
    fn set_mode_and_invalid_events_are_atomic() {
        assert!(vinput_fcitx_trigger_state_new(9).is_null());
        // SAFETY: The handle is live for every call and freed exactly once.
        unsafe {
            let state = vinput_fcitx_trigger_state_new(1);
            assert_eq!(
                dispatch(
                    state,
                    VinputFcitxTriggerEventView {
                        kind: TRIGGER_EVENT_SET_MODE,
                        value: 0,
                        flag: 0,
                        now_ns: 0,
                    },
                ),
                Some(0),
            );
            let invalid = VinputFcitxTriggerEventView {
                kind: TRIGGER_EVENT_PRESS,
                value: 9,
                flag: 0,
                now_ns: 0,
            };
            let mut action = 77;
            assert_eq!(
                vinput_fcitx_trigger_state_dispatch(state, &raw const invalid, &raw mut action,),
                0,
            );
            assert_eq!(action, 77);
            assert_eq!(
                dispatch(
                    state,
                    VinputFcitxTriggerEventView {
                        kind: TRIGGER_EVENT_PRESS,
                        value: 0,
                        flag: 0,
                        now_ns: 0,
                    },
                ),
                Some(2),
            );
            vinput_fcitx_trigger_state_free(state);
        }
    }
}

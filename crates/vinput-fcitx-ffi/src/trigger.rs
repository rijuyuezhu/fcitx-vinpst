//! Raw-pointer C ABI for the trigger mode state machine.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use vinput_fcitx_core::{TriggerAction, TriggerKind, TriggerMode, TriggerModeState};

/// Opaque trigger mode state owned by Rust.
pub struct VinputFcitxTriggerState {
    state: TriggerModeState,
}

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

const fn action_code(action: TriggerAction) -> u8 {
    action as u8
}

unsafe fn state_ref<'a>(
    state: *const VinputFcitxTriggerState,
) -> Option<&'a VinputFcitxTriggerState> {
    // SAFETY: The caller guarantees that a non-null pointer was returned by
    // `vinput_fcitx_trigger_state_new` and has not been freed.
    unsafe { state.as_ref() }
}

unsafe fn state_mut<'a>(
    state: *mut VinputFcitxTriggerState,
) -> Option<&'a mut VinputFcitxTriggerState> {
    // SAFETY: The caller guarantees exclusive access to a live trigger handle.
    unsafe { state.as_mut() }
}

/// Creates an idle trigger state for a wire trigger-mode value.
///
/// Invalid mode values or caught Rust panics return null.
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

/// Releases a trigger state handle.
///
/// A null handle is ignored.
///
/// # Safety
///
/// A non-null `state` must be a live handle returned by this crate and must not
/// be freed more than once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_trigger_state_free(state: *mut VinputFcitxTriggerState) {
    if state.is_null() {
        return;
    }

    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        drop(unsafe { Box::from_raw(state) });
    }));
}

/// Changes the trigger mode and returns one on success.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_trigger_state_set_mode(
    state: *mut VinputFcitxTriggerState,
    mode: u8,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(state) = (unsafe { state_mut(state) }) else {
                return false;
            };
            let Some(mode) = trigger_mode(mode) else {
                return false;
            };
            state.state.set_mode(mode);
            true
        }))
        .unwrap_or(false),
    )
}

/// Returns the current trigger-mode wire value.
///
/// Invalid handles return the `Both` value.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_trigger_state_mode(
    state: *const VinputFcitxTriggerState,
) -> u8 {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { state_ref(state) }.map_or(TriggerMode::Both as u8, |state| state.state.mode() as u8)
}

/// Handles a trigger press and returns a trigger-action wire value.
///
/// Invalid handles or kind values return `None`.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_trigger_state_on_press(
    state: *mut VinputFcitxTriggerState,
    kind: u8,
    now_ns: i64,
    recording: u8,
) -> u8 {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(state) = (unsafe { state_mut(state) }) else {
            return action_code(TriggerAction::None);
        };
        let Some(kind) = trigger_kind(kind) else {
            return action_code(TriggerAction::None);
        };
        action_code(state.state.on_press(kind, now_ns, recording != 0))
    }))
    .unwrap_or_else(|_| action_code(TriggerAction::None))
}

/// Handles a release and returns a trigger-action wire value.
///
/// `active_release` is computed by the Fcitx C++ adapter so Rust does not need
/// to reproduce Fcitx modifier-key semantics.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_trigger_state_on_release(
    state: *mut VinputFcitxTriggerState,
    now_ns: i64,
    active_release: u8,
) -> u8 {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        unsafe { state_mut(state) }.map_or(action_code(TriggerAction::None), |state| {
            action_code(state.state.on_release(now_ns, active_release != 0))
        })
    }))
    .unwrap_or_else(|_| action_code(TriggerAction::None))
}

/// Fires the pending hold-start timer.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_trigger_state_fire_pending_start(
    state: *mut VinputFcitxTriggerState,
) -> u8 {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        unsafe { state_mut(state) }.map_or(action_code(TriggerAction::None), |state| {
            action_code(state.state.fire_pending_start())
        })
    }))
    .unwrap_or_else(|_| action_code(TriggerAction::None))
}

/// Fires the pending release-tail stop timer.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_trigger_state_fire_pending_stop(
    state: *mut VinputFcitxTriggerState,
) -> u8 {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        unsafe { state_mut(state) }.map_or(action_code(TriggerAction::None), |state| {
            action_code(state.state.fire_pending_stop())
        })
    }))
    .unwrap_or_else(|_| action_code(TriggerAction::None))
}

/// Reconciles the state after a recording-start attempt.
///
/// Returns one for a valid handle.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_trigger_state_confirm_start(
    state: *mut VinputFcitxTriggerState,
    recording_started: u8,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(state) = (unsafe { state_mut(state) }) else {
                return false;
            };
            state.state.confirm_start(recording_started != 0);
            true
        }))
        .unwrap_or(false),
    )
}

/// Clears trigger state after recording stops.
///
/// Returns one for a valid handle.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_trigger_state_recording_stopped(
    state: *mut VinputFcitxTriggerState,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(state) = (unsafe { state_mut(state) }) else {
                return false;
            };
            state.state.recording_stopped();
            true
        }))
        .unwrap_or(false),
    )
}

/// Returns one when a delayed hold start is pending.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_trigger_state_has_pending_start(
    state: *const VinputFcitxTriggerState,
) -> u8 {
    // SAFETY: Forwarded from this function's caller contract.
    u8::from(unsafe { state_ref(state) }.is_some_and(|state| state.state.has_pending_start()))
}

/// Returns one when a trigger owns the active recording.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_trigger_state_has_active_trigger(
    state: *const VinputFcitxTriggerState,
) -> u8 {
    // SAFETY: Forwarded from this function's caller contract.
    u8::from(unsafe { state_ref(state) }.is_some_and(|state| state.state.has_active_trigger()))
}

#[cfg(test)]
mod tests {
    use super::{
        vinput_fcitx_trigger_state_confirm_start, vinput_fcitx_trigger_state_fire_pending_start,
        vinput_fcitx_trigger_state_fire_pending_stop, vinput_fcitx_trigger_state_free,
        vinput_fcitx_trigger_state_has_active_trigger,
        vinput_fcitx_trigger_state_has_pending_start, vinput_fcitx_trigger_state_new,
        vinput_fcitx_trigger_state_on_press, vinput_fcitx_trigger_state_on_release,
        vinput_fcitx_trigger_state_recording_stopped, vinput_fcitx_trigger_state_set_mode,
    };

    #[test]
    fn exposes_hold_trigger_lifecycle() {
        // SAFETY: The handle is live for every call and released exactly once.
        unsafe {
            let state = vinput_fcitx_trigger_state_new(1);
            assert!(!state.is_null());
            assert_eq!(vinput_fcitx_trigger_state_on_press(state, 0, 0, 0), 5);
            assert_eq!(vinput_fcitx_trigger_state_has_pending_start(state), 1);
            assert_eq!(
                vinput_fcitx_trigger_state_on_release(state, 100_000_000, 1),
                7
            );
            assert_eq!(vinput_fcitx_trigger_state_fire_pending_start(state), 0);

            assert_eq!(
                vinput_fcitx_trigger_state_on_press(state, 1, 1_000_000_000, 0),
                6
            );
            assert_eq!(vinput_fcitx_trigger_state_fire_pending_start(state), 3);
            assert_eq!(vinput_fcitx_trigger_state_confirm_start(state, 1), 1);
            assert_eq!(vinput_fcitx_trigger_state_has_active_trigger(state), 1);
            assert_eq!(
                vinput_fcitx_trigger_state_on_release(state, 1_400_000_000, 1),
                8
            );
            assert_eq!(vinput_fcitx_trigger_state_fire_pending_stop(state), 4);
            assert_eq!(vinput_fcitx_trigger_state_recording_stopped(state), 1);
            assert_eq!(vinput_fcitx_trigger_state_has_active_trigger(state), 0);
            vinput_fcitx_trigger_state_free(state);
        }
    }

    #[test]
    fn rejects_invalid_wire_values_without_mutation() {
        assert!(vinput_fcitx_trigger_state_new(9).is_null());

        // SAFETY: The handle is live for every call and released exactly once.
        unsafe {
            let state = vinput_fcitx_trigger_state_new(2);
            assert!(!state.is_null());
            assert_eq!(vinput_fcitx_trigger_state_on_press(state, 9, 0, 0), 0);
            assert_eq!(vinput_fcitx_trigger_state_has_active_trigger(state), 0);
            assert_eq!(vinput_fcitx_trigger_state_set_mode(state, 9), 0);
            assert_eq!(vinput_fcitx_trigger_state_has_pending_start(state), 0);
            vinput_fcitx_trigger_state_free(state);
        }
    }
}

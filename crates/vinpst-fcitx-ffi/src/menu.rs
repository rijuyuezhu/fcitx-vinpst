//! Compact C ABI for Rust-owned menu sessions and action decisions.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use vinpst_fcitx_core::{
    MenuFilterState, MenuKeyAction, MenuSemanticKey, MenuSessionKeyInput, MenuSessionState,
    ResultMenuKeyInput, clamp_menu_page, plan_result_menu_key,
};

use crate::ffi_string::{VinpstFcitxStringView, string_view, text_input};

/// Opaque complete menu session owned by Rust.
pub struct VinpstFcitxMenuSession {
    session: MenuSessionState,
    decorated_title: String,
}

/// One semantic key decision.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinpstFcitxMenuKeyDecisionView {
    /// Stable `VINPST_FCITX_MENU_ACTION_*` value.
    pub action: u8,
    /// Action-specific page or visible-row value.
    pub value: i64,
}

/// Borrowed semantic key and candidate cursor context.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinpstFcitxMenuKeyInputView {
    /// Whether this is a key-release event.
    pub release: u8,
    /// Stable `VINPST_FCITX_MENU_KEY_*` value.
    pub key_kind: u8,
    /// Page or digit value for value-carrying semantic keys.
    pub key_value: i64,
    /// Borrowed UTF-8 text for `VINPST_FCITX_MENU_KEY_TEXT`.
    pub text: VinpstFcitxStringView,
    /// Whether the current Fcitx candidate list supports cursor movement.
    pub cursor_available: u8,
    /// Global selected-row index, or `-1` when no row is selected.
    pub current_selection: i64,
    /// Number of visible projected menu rows.
    pub visible_item_count: usize,
}

const MENU_KEY_OTHER: u8 = 0;
const MENU_KEY_PASSIVE: u8 = 1;
const MENU_KEY_ESCAPE: u8 = 2;
const MENU_KEY_SLASH: u8 = 3;
const MENU_KEY_BACKSPACE: u8 = 4;
const MENU_KEY_DELETE_WORD: u8 = 5;
const MENU_KEY_CLEAR_FILTER: u8 = 6;
const MENU_KEY_TEXT: u8 = 7;
const MENU_KEY_PAGE: u8 = 8;
const MENU_KEY_DIGIT: u8 = 9;
const MENU_KEY_MOVE_PREVIOUS: u8 = 10;
const MENU_KEY_MOVE_NEXT: u8 = 11;
const MENU_KEY_ENTER: u8 = 12;

const MENU_ACTION_PASS: u8 = 0;
const MENU_ACTION_CONSUME: u8 = 1;
const MENU_ACTION_CLOSE_AND_PASS: u8 = 2;
const MENU_ACTION_CLOSE_AND_CONSUME: u8 = 3;
const MENU_ACTION_REBUILD: u8 = 4;
const MENU_ACTION_MOVE_PREVIOUS: u8 = 5;
const MENU_ACTION_MOVE_NEXT: u8 = 6;
const MENU_ACTION_SELECT: u8 = 7;

pub(crate) unsafe fn menu_session_filter_ref<'a>(
    session: *const VinpstFcitxMenuSession,
) -> Option<&'a MenuFilterState> {
    // SAFETY: Forwarded from the caller contract.
    unsafe { session.as_ref() }.map(|value| value.session.filter())
}

#[cfg(test)]
pub(crate) fn boxed_menu_session(state: MenuFilterState) -> *mut VinpstFcitxMenuSession {
    let mut session = MenuSessionState::default();
    *session.filter_mut() = state;
    Box::into_raw(Box::new(VinpstFcitxMenuSession {
        session,
        decorated_title: String::new(),
    }))
}

unsafe fn menu_semantic_key(input: &VinpstFcitxMenuKeyInputView) -> Option<MenuSemanticKey<'_>> {
    match input.key_kind {
        MENU_KEY_OTHER => Some(MenuSemanticKey::Other),
        MENU_KEY_PASSIVE => Some(MenuSemanticKey::Passive),
        MENU_KEY_ESCAPE => Some(MenuSemanticKey::Escape),
        MENU_KEY_SLASH => Some(MenuSemanticKey::Slash),
        MENU_KEY_BACKSPACE => Some(MenuSemanticKey::Backspace),
        MENU_KEY_DELETE_WORD => Some(MenuSemanticKey::DeleteWord),
        MENU_KEY_CLEAR_FILTER => Some(MenuSemanticKey::ClearFilter),
        MENU_KEY_TEXT => {
            // SAFETY: Forwarded from the caller contract.
            unsafe { text_input(input.text.data, input.text.len) }.map(MenuSemanticKey::Text)
        }
        MENU_KEY_PAGE => i32::try_from(input.key_value)
            .ok()
            .map(MenuSemanticKey::Page),
        MENU_KEY_DIGIT => usize::try_from(input.key_value)
            .ok()
            .map(MenuSemanticKey::Digit),
        MENU_KEY_MOVE_PREVIOUS => Some(MenuSemanticKey::MovePrevious),
        MENU_KEY_MOVE_NEXT => Some(MenuSemanticKey::MoveNext),
        MENU_KEY_ENTER => Some(MenuSemanticKey::Enter),
        _ => None,
    }
}

fn menu_current_selection(input: &VinpstFcitxMenuKeyInputView) -> Option<usize> {
    usize::try_from(input.current_selection).ok()
}

unsafe fn menu_key_input(input: &VinpstFcitxMenuKeyInputView) -> Option<MenuSessionKeyInput<'_>> {
    // SAFETY: Forwarded from this helper's caller contract.
    let key = unsafe { menu_semantic_key(input) }?;
    let current_selection = menu_current_selection(input);
    Some(MenuSessionKeyInput {
        release: input.release != 0,
        key,
        cursor_available: input.cursor_available != 0,
        current_selection,
        visible_item_count: input.visible_item_count,
    })
}

fn decision_view(action: MenuKeyAction) -> VinpstFcitxMenuKeyDecisionView {
    match action {
        MenuKeyAction::Pass => VinpstFcitxMenuKeyDecisionView {
            action: MENU_ACTION_PASS,
            value: 0,
        },
        MenuKeyAction::Consume => VinpstFcitxMenuKeyDecisionView {
            action: MENU_ACTION_CONSUME,
            value: 0,
        },
        MenuKeyAction::CloseAndPass => VinpstFcitxMenuKeyDecisionView {
            action: MENU_ACTION_CLOSE_AND_PASS,
            value: 0,
        },
        MenuKeyAction::CloseAndConsume => VinpstFcitxMenuKeyDecisionView {
            action: MENU_ACTION_CLOSE_AND_CONSUME,
            value: 0,
        },
        MenuKeyAction::Rebuild { page } => VinpstFcitxMenuKeyDecisionView {
            action: MENU_ACTION_REBUILD,
            value: i64::from(page),
        },
        MenuKeyAction::MovePrevious => VinpstFcitxMenuKeyDecisionView {
            action: MENU_ACTION_MOVE_PREVIOUS,
            value: 0,
        },
        MenuKeyAction::MoveNext => VinpstFcitxMenuKeyDecisionView {
            action: MENU_ACTION_MOVE_NEXT,
            value: 0,
        },
        MenuKeyAction::Select { visible_index } => VinpstFcitxMenuKeyDecisionView {
            action: MENU_ACTION_SELECT,
            value: i64::try_from(visible_index).unwrap_or(i64::MAX),
        },
    }
}

/// Plans one key for the non-filtering five-row result candidate menu.
///
/// # Safety
///
/// `input` must be readable, its text range must satisfy the string-view
/// contract, and `decision_out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_result_menu_plan_key(
    input: *const VinpstFcitxMenuKeyInputView,
    current_page: i32,
    decision_out: *mut VinpstFcitxMenuKeyDecisionView,
) -> u8 {
    crate::ffi_catch(0, || {
        if decision_out.is_null() {
            return 0;
        }
        // SAFETY: Forwarded from this function's caller contract.
        let Some(input) = (unsafe { input.as_ref() }) else {
            return 0;
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(key) = (unsafe { menu_semantic_key(input) }) else {
            return 0;
        };
        let current_selection = menu_current_selection(input);
        let decision = decision_view(plan_result_menu_key(ResultMenuKeyInput {
            release: input.release != 0,
            key,
            cursor_available: input.cursor_available != 0,
            current_selection,
            current_page,
            item_count: input.visible_item_count,
        }));
        // SAFETY: `decision_out` is non-null and writable by contract.
        unsafe { decision_out.write(decision) };
        1
    })
}

/// Creates a closed empty menu session.
#[unsafe(no_mangle)]
pub extern "C" fn vinpst_fcitx_menu_session_new() -> *mut VinpstFcitxMenuSession {
    catch_unwind(|| {
        Box::into_raw(Box::new(VinpstFcitxMenuSession {
            session: MenuSessionState::default(),
            decorated_title: String::new(),
        }))
    })
    .unwrap_or(ptr::null_mut())
}

/// Releases a menu session.
///
/// # Safety
///
/// A non-null pointer must be a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_menu_session_free(session: *mut VinpstFcitxMenuSession) {
    if !session.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            drop(unsafe { Box::from_raw(session) });
        }));
    }
}

/// Opens a fresh menu session and resets its page and filter.
///
/// # Safety
///
/// `state` must be a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_menu_session_open(state: *mut VinpstFcitxMenuSession) -> u8 {
    crate::ffi_catch(0, || {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(state) = (unsafe { state.as_mut() }) else {
            return 0;
        };
        state.session.open();
        state.decorated_title.clear();
        1
    })
}

/// Closes a menu session and clears its page and filter.
///
/// # Safety
///
/// `state` must be a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_menu_session_close(state: *mut VinpstFcitxMenuSession) -> u8 {
    crate::ffi_catch(0, || {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(state) = (unsafe { state.as_mut() }) else {
            return 0;
        };
        state.session.close();
        state.decorated_title.clear();
        1
    })
}

/// Reads whether a menu session is open.
///
/// # Safety
///
/// `state` must be live and `open_out` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_menu_session_is_open(
    state: *const VinpstFcitxMenuSession,
    open_out: *mut u8,
) -> u8 {
    crate::ffi_catch(0, || {
        if open_out.is_null() {
            return 0;
        }
        // SAFETY: Forwarded from this function's caller contract.
        let Some(state) = (unsafe { state.as_ref() }) else {
            return 0;
        };
        // SAFETY: The caller guarantees a writable output pointer.
        unsafe { open_out.write(u8::from(state.session.is_open())) };
        1
    })
}

/// Stores the actual zero-based page after Fcitx clamps a rebuild request.
///
/// # Safety
///
/// `state` must be a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_menu_session_set_page(
    state: *mut VinpstFcitxMenuSession,
    page: i32,
) -> u8 {
    crate::ffi_catch(0, || {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(state) = (unsafe { state.as_mut() }) else {
            return 0;
        };
        u8::from(state.session.set_page(page))
    })
}

/// Reads the active flag.
///
/// # Safety
///
/// `state` must be live and `active_out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_menu_session_filter_active(
    state: *const VinpstFcitxMenuSession,
    active_out: *mut u8,
) -> u8 {
    crate::ffi_catch(0, || {
        if active_out.is_null() {
            return 0;
        }
        // SAFETY: Forwarded from this function's caller contract.
        let Some(state) = (unsafe { state.as_ref() }) else {
            return 0;
        };
        // SAFETY: The caller guarantees a writable output pointer.
        unsafe { active_out.write(u8::from(state.session.filter().active())) };
        1
    })
}

/// Computes and borrows the decorated menu title.
///
/// # Safety
///
/// Input bytes must be readable and `title_out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_menu_session_decorate_title(
    state: *mut VinpstFcitxMenuSession,
    base_data: *const u8,
    base_len: usize,
    title_out: *mut VinpstFcitxStringView,
) -> u8 {
    if title_out.is_null() {
        return 0;
    }
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(state) = (unsafe { state.as_mut() }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(base_title) = (unsafe { text_input(base_data, base_len) }) else {
                return false;
            };
            state.decorated_title = state.session.filter().decorate_title(base_title);
            // SAFETY: The caller guarantees a writable output pointer.
            unsafe { title_out.write(string_view(&state.decorated_title)) };
            true
        }))
        .unwrap_or(false),
    )
}

/// Applies one semantic key to an open Rust-owned menu session.
///
/// The current page and terminal close/select transitions are owned by Rust.
///
/// # Safety
///
/// Input bytes must be readable and `decision_out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_menu_session_handle_key(
    state: *mut VinpstFcitxMenuSession,
    input: *const VinpstFcitxMenuKeyInputView,
    decision_out: *mut VinpstFcitxMenuKeyDecisionView,
) -> u8 {
    if decision_out.is_null() {
        return 0;
    }
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(state) = (unsafe { state.as_mut() }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(input) = (unsafe { input.as_ref() }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(input) = (unsafe { menu_key_input(input) }) else {
                return false;
            };

            let Some(action) = state.session.handle_key(input) else {
                return false;
            };
            state.decorated_title.clear();
            // SAFETY: The caller guarantees a writable output pointer.
            unsafe { decision_out.write(decision_view(action)) };
            true
        }))
        .unwrap_or(false),
    )
}

/// Clamps a requested zero-based page, returning `-1` when no pages exist.
#[unsafe(no_mangle)]
pub extern "C" fn vinpst_fcitx_clamp_menu_page(total_pages: i32, requested_page: i32) -> i32 {
    catch_unwind(|| clamp_menu_page(total_pages, requested_page).unwrap_or(-1)).unwrap_or(-1)
}

#[cfg(test)]
mod tests;

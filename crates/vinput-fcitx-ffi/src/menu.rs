//! Compact C ABI for Rust-owned menu filtering and action decisions.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use vinput_fcitx_core::{
    MenuFilterState, MenuKeyAction, MenuKeyInput, MenuSemanticKey, clamp_menu_page,
};

use crate::frontend::VinputFcitxStringView;

/// Opaque menu filter state owned by Rust.
pub struct VinputFcitxMenuFilterState {
    state: MenuFilterState,
    decorated_title: String,
}

/// Borrowed menu filter summary.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxMenuFilterView {
    /// Whether filter-entry mode is active.
    pub active: u8,
    /// Current UTF-8 query.
    pub query: VinputFcitxStringView,
}

/// One semantic key decision.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxMenuKeyDecisionView {
    /// Stable `VINPUT_FCITX_MENU_ACTION_*` value.
    pub action: u8,
    /// Action-specific page or visible-row value.
    pub value: i64,
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

unsafe fn semantic_key<'a>(
    kind: u8,
    value: i64,
    text_data: *const u8,
    text_len: usize,
) -> Option<MenuSemanticKey<'a>> {
    match kind {
        MENU_KEY_OTHER => Some(MenuSemanticKey::Other),
        MENU_KEY_PASSIVE => Some(MenuSemanticKey::Passive),
        MENU_KEY_ESCAPE => Some(MenuSemanticKey::Escape),
        MENU_KEY_SLASH => Some(MenuSemanticKey::Slash),
        MENU_KEY_BACKSPACE => Some(MenuSemanticKey::Backspace),
        MENU_KEY_DELETE_WORD => Some(MenuSemanticKey::DeleteWord),
        MENU_KEY_CLEAR_FILTER => Some(MenuSemanticKey::ClearFilter),
        MENU_KEY_TEXT => {
            // SAFETY: Forwarded from the caller contract.
            unsafe { text_input(text_data, text_len) }.map(MenuSemanticKey::Text)
        }
        MENU_KEY_PAGE => i32::try_from(value).ok().map(MenuSemanticKey::Page),
        MENU_KEY_DIGIT => usize::try_from(value).ok().map(MenuSemanticKey::Digit),
        MENU_KEY_MOVE_PREVIOUS => Some(MenuSemanticKey::MovePrevious),
        MENU_KEY_MOVE_NEXT => Some(MenuSemanticKey::MoveNext),
        MENU_KEY_ENTER => Some(MenuSemanticKey::Enter),
        _ => None,
    }
}

fn decision_view(action: MenuKeyAction) -> VinputFcitxMenuKeyDecisionView {
    match action {
        MenuKeyAction::Pass => VinputFcitxMenuKeyDecisionView {
            action: MENU_ACTION_PASS,
            value: 0,
        },
        MenuKeyAction::Consume => VinputFcitxMenuKeyDecisionView {
            action: MENU_ACTION_CONSUME,
            value: 0,
        },
        MenuKeyAction::CloseAndPass => VinputFcitxMenuKeyDecisionView {
            action: MENU_ACTION_CLOSE_AND_PASS,
            value: 0,
        },
        MenuKeyAction::CloseAndConsume => VinputFcitxMenuKeyDecisionView {
            action: MENU_ACTION_CLOSE_AND_CONSUME,
            value: 0,
        },
        MenuKeyAction::Rebuild { page } => VinputFcitxMenuKeyDecisionView {
            action: MENU_ACTION_REBUILD,
            value: i64::from(page),
        },
        MenuKeyAction::MovePrevious => VinputFcitxMenuKeyDecisionView {
            action: MENU_ACTION_MOVE_PREVIOUS,
            value: 0,
        },
        MenuKeyAction::MoveNext => VinputFcitxMenuKeyDecisionView {
            action: MENU_ACTION_MOVE_NEXT,
            value: 0,
        },
        MenuKeyAction::Select { visible_index } => VinputFcitxMenuKeyDecisionView {
            action: MENU_ACTION_SELECT,
            value: i64::try_from(visible_index).unwrap_or(i64::MAX),
        },
    }
}

/// Creates an inactive empty menu filter state.
#[unsafe(no_mangle)]
pub extern "C" fn vinput_fcitx_menu_filter_state_new() -> *mut VinputFcitxMenuFilterState {
    catch_unwind(|| {
        Box::into_raw(Box::new(VinputFcitxMenuFilterState {
            state: MenuFilterState::default(),
            decorated_title: String::new(),
        }))
    })
    .unwrap_or(ptr::null_mut())
}

/// Releases a menu filter state.
///
/// # Safety
///
/// A non-null pointer must be a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_menu_filter_state_free(
    state: *mut VinputFcitxMenuFilterState,
) {
    if !state.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            drop(unsafe { Box::from_raw(state) });
        }));
    }
}

/// Clears and deactivates the filter.
///
/// # Safety
///
/// `state` must be null or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_menu_filter_state_reset(
    state: *mut VinputFcitxMenuFilterState,
) -> u8 {
    // SAFETY: Forwarded from this function's caller contract.
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    state.state.reset();
    state.decorated_title.clear();
    1
}

/// Borrows the active flag and current query.
///
/// # Safety
///
/// `state` must be live and `view_out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_menu_filter_state_view(
    state: *const VinputFcitxMenuFilterState,
    view_out: *mut VinputFcitxMenuFilterView,
) -> u8 {
    if view_out.is_null() {
        return 0;
    }
    // SAFETY: Forwarded from this function's caller contract.
    let Some(state) = (unsafe { state.as_ref() }) else {
        return 0;
    };
    // SAFETY: The caller guarantees a writable output pointer.
    unsafe {
        view_out.write(VinputFcitxMenuFilterView {
            active: u8::from(state.state.active()),
            query: string_view(state.state.query()),
        });
    }
    1
}

/// Computes and borrows the decorated menu title.
///
/// # Safety
///
/// Input bytes must be readable and `title_out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_menu_filter_state_decorate_title(
    state: *mut VinputFcitxMenuFilterState,
    base_data: *const u8,
    base_len: usize,
    title_out: *mut VinputFcitxStringView,
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
            state.decorated_title = state.state.decorate_title(base_title);
            // SAFETY: The caller guarantees a writable output pointer.
            unsafe { title_out.write(string_view(&state.decorated_title)) };
            true
        }))
        .unwrap_or(false),
    )
}

/// Applies one semantic key atomically and returns its action.
///
/// # Safety
///
/// Input bytes must be readable and `decision_out` must be writable.
#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_menu_filter_state_handle_key(
    state: *mut VinputFcitxMenuFilterState,
    release: u8,
    key_kind: u8,
    key_value: i64,
    text_data: *const u8,
    text_len: usize,
    cursor_available: u8,
    current_selection: i64,
    current_page: i32,
    visible_item_count: usize,
    decision_out: *mut VinputFcitxMenuKeyDecisionView,
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
            let Some(key) = (unsafe { semantic_key(key_kind, key_value, text_data, text_len) })
            else {
                return false;
            };
            let selection = if current_selection < 0 {
                None
            } else {
                usize::try_from(current_selection).ok()
            };
            if current_selection >= 0 && selection.is_none() {
                return false;
            }

            let mut updated = state.state.clone();
            let action = updated.handle_key(MenuKeyInput {
                release: release != 0,
                key,
                cursor_available: cursor_available != 0,
                current_selection: selection,
                current_page,
                visible_item_count,
            });
            let decision = decision_view(action);
            state.state = updated;
            state.decorated_title.clear();
            // SAFETY: The caller guarantees a writable output pointer.
            unsafe { decision_out.write(decision) };
            true
        }))
        .unwrap_or(false),
    )
}

/// Clamps a requested zero-based page, returning `-1` when no pages exist.
#[unsafe(no_mangle)]
pub extern "C" fn vinput_fcitx_clamp_menu_page(total_pages: i32, requested_page: i32) -> i32 {
    catch_unwind(|| clamp_menu_page(total_pages, requested_page).unwrap_or(-1)).unwrap_or(-1)
}

#[cfg(test)]
mod tests;

//! Raw-pointer C ABI for menu filtering and paging primitives.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use vinput_fcitx_core::{
    MenuFilterState, MenuKeyAction, MenuKeyInput, MenuSemanticKey, clamp_menu_page,
};

/// Opaque menu filter state owned by Rust.
pub struct VinputFcitxMenuFilterState {
    state: MenuFilterState,
    decorated_title: String,
}

unsafe fn text_input<'a>(data: *const u8, len: usize) -> Option<&'a str> {
    if data.is_null() {
        return (len == 0).then_some("");
    }

    // SAFETY: The caller guarantees that `data` points to `len` readable bytes
    // for the duration of this call.
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    std::str::from_utf8(bytes).ok()
}

unsafe fn state_ref<'a>(
    state: *const VinputFcitxMenuFilterState,
) -> Option<&'a VinputFcitxMenuFilterState> {
    // SAFETY: The caller guarantees that a non-null pointer was returned by
    // `vinput_fcitx_menu_filter_state_new` and has not been freed.
    unsafe { state.as_ref() }
}

unsafe fn state_mut<'a>(
    state: *mut VinputFcitxMenuFilterState,
) -> Option<&'a mut VinputFcitxMenuFilterState> {
    // SAFETY: The caller guarantees exclusive access to a live menu handle.
    unsafe { state.as_mut() }
}

fn string_data(value: &str) -> *const u8 {
    if value.is_empty() {
        ptr::null()
    } else {
        value.as_ptr()
    }
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

fn action_wire(action: MenuKeyAction) -> (u8, i64) {
    match action {
        MenuKeyAction::Pass => (MENU_ACTION_PASS, 0),
        MenuKeyAction::Consume => (MENU_ACTION_CONSUME, 0),
        MenuKeyAction::CloseAndPass => (MENU_ACTION_CLOSE_AND_PASS, 0),
        MenuKeyAction::CloseAndConsume => (MENU_ACTION_CLOSE_AND_CONSUME, 0),
        MenuKeyAction::Rebuild { page } => (MENU_ACTION_REBUILD, i64::from(page)),
        MenuKeyAction::MovePrevious => (MENU_ACTION_MOVE_PREVIOUS, 0),
        MenuKeyAction::MoveNext => (MENU_ACTION_MOVE_NEXT, 0),
        MenuKeyAction::Select { visible_index } => (
            MENU_ACTION_SELECT,
            i64::try_from(visible_index).unwrap_or(i64::MAX),
        ),
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

/// Releases a menu filter state handle.
///
/// A null handle is ignored.
///
/// # Safety
///
/// A non-null `state` must be a live handle returned by this crate and must not
/// be freed more than once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_menu_filter_state_free(
    state: *mut VinputFcitxMenuFilterState,
) {
    if state.is_null() {
        return;
    }

    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        drop(unsafe { Box::from_raw(state) });
    }));
}

/// Clears and deactivates the filter.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_menu_filter_state_reset(
    state: *mut VinputFcitxMenuFilterState,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(state) = (unsafe { state_mut(state) }) else {
                return false;
            };
            state.state.reset();
            true
        }))
        .unwrap_or(false),
    )
}

/// Activates filter-entry mode.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_menu_filter_state_activate(
    state: *mut VinputFcitxMenuFilterState,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(state) = (unsafe { state_mut(state) }) else {
                return false;
            };
            state.state.activate();
            true
        }))
        .unwrap_or(false),
    )
}

/// Clears and deactivates the filter.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_menu_filter_state_clear_and_deactivate(
    state: *mut VinputFcitxMenuFilterState,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(state) = (unsafe { state_mut(state) }) else {
                return false;
            };
            state.state.clear_and_deactivate();
            true
        }))
        .unwrap_or(false),
    )
}

/// Removes one Unicode scalar value from the query.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_menu_filter_state_backspace(
    state: *mut VinputFcitxMenuFilterState,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(state) = (unsafe { state_mut(state) }) else {
                return false;
            };
            state.state.backspace();
            true
        }))
        .unwrap_or(false),
    )
}

/// Deletes the final query word using the retained ASCII-whitespace semantics.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_menu_filter_state_delete_last_word(
    state: *mut VinputFcitxMenuFilterState,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(state) = (unsafe { state_mut(state) }) else {
                return false;
            };
            state.state.delete_last_word();
            true
        }))
        .unwrap_or(false),
    )
}

/// Appends valid UTF-8 to an active query.
///
/// Invalid pointers or UTF-8 return zero without changing the state.
///
/// # Safety
///
/// `text_data` must point to `text_len` readable bytes, unless both are
/// null/zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_menu_filter_state_append_text(
    state: *mut VinputFcitxMenuFilterState,
    text_data: *const u8,
    text_len: usize,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(state) = (unsafe { state_mut(state) }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(text) = (unsafe { text_input(text_data, text_len) }) else {
                return false;
            };
            let mut updated = state.state.clone();
            updated.append_text(text);
            state.state = updated;
            true
        }))
        .unwrap_or(false),
    )
}

/// Applies one semantic menu key and writes the resulting action/value pair.
///
/// The filter mutation and returned decision are atomic: invalid inputs or a
/// caught Rust panic return zero without changing the filter state or output
/// values. `current_selection` uses `-1` for no selected visible row.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate. `text_data`
/// must point to `text_len` readable bytes when `key_kind` is the text value,
/// unless both are null/zero. `action_out` and `value_out` must be writable.
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
    action_out: *mut u8,
    value_out: *mut i64,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(action_out) = (unsafe { action_out.as_mut() }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(value_out) = (unsafe { value_out.as_mut() }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(state) = (unsafe { state_mut(state) }) else {
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
            let (action, value) = action_wire(action);
            state.state = updated;
            *action_out = action;
            *value_out = value;
            true
        }))
        .unwrap_or(false),
    )
}

/// Returns one when filter-entry mode is active.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_menu_filter_state_active(
    state: *const VinputFcitxMenuFilterState,
) -> u8 {
    // SAFETY: Forwarded from this function's caller contract.
    u8::from(unsafe { state_ref(state) }.is_some_and(|state| state.state.active()))
}

/// Returns the query byte pointer owned by `state`.
///
/// The pointer remains valid until the state is mutated or freed. Empty queries
/// return null.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_menu_filter_state_query_data(
    state: *const VinputFcitxMenuFilterState,
) -> *const u8 {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { state_ref(state) }.map_or(ptr::null(), |state| string_data(state.state.query()))
}

/// Returns the query length in bytes.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_menu_filter_state_query_len(
    state: *const VinputFcitxMenuFilterState,
) -> usize {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { state_ref(state) }.map_or(0, |state| state.state.query().len())
}

/// Returns one when `search_text` matches every current query term.
///
/// Invalid pointers or UTF-8 fail closed.
///
/// # Safety
///
/// `search_data` must point to `search_len` readable bytes, unless both are
/// null/zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_menu_filter_state_matches(
    state: *const VinputFcitxMenuFilterState,
    search_data: *const u8,
    search_len: usize,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(state) = (unsafe { state_ref(state) }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(search_text) = (unsafe { text_input(search_data, search_len) }) else {
                return false;
            };
            state.state.matches(search_text)
        }))
        .unwrap_or(false),
    )
}

/// Computes and stores the decorated menu title.
///
/// The resulting view remains valid until the next decorate call, mutation, or
/// free. Invalid pointers or UTF-8 return zero without replacing the prior view.
///
/// # Safety
///
/// `base_data` must point to `base_len` readable bytes, unless both are
/// null/zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_menu_filter_state_decorate_title(
    state: *mut VinputFcitxMenuFilterState,
    base_data: *const u8,
    base_len: usize,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(state) = (unsafe { state_mut(state) }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(base_title) = (unsafe { text_input(base_data, base_len) }) else {
                return false;
            };
            let decorated = state.state.decorate_title(base_title);
            state.decorated_title = decorated;
            true
        }))
        .unwrap_or(false),
    )
}

/// Returns the most recently decorated title byte pointer.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_menu_filter_state_decorated_title_data(
    state: *const VinputFcitxMenuFilterState,
) -> *const u8 {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { state_ref(state) }.map_or(ptr::null(), |state| string_data(&state.decorated_title))
}

/// Returns the most recently decorated title length in bytes.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_menu_filter_state_decorated_title_len(
    state: *const VinputFcitxMenuFilterState,
) -> usize {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { state_ref(state) }.map_or(0, |state| state.decorated_title.len())
}

/// Clamps a requested zero-based page, returning `-1` when no pages exist.
#[unsafe(no_mangle)]
pub extern "C" fn vinput_fcitx_clamp_menu_page(total_pages: i32, requested_page: i32) -> i32 {
    catch_unwind(|| clamp_menu_page(total_pages, requested_page).unwrap_or(-1)).unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use super::{
        MENU_ACTION_CLOSE_AND_CONSUME, MENU_ACTION_CLOSE_AND_PASS, MENU_ACTION_CONSUME,
        MENU_ACTION_MOVE_PREVIOUS, MENU_ACTION_PASS, MENU_ACTION_REBUILD, MENU_ACTION_SELECT,
        MENU_KEY_DIGIT, MENU_KEY_ENTER, MENU_KEY_ESCAPE, MENU_KEY_MOVE_PREVIOUS, MENU_KEY_OTHER,
        MENU_KEY_PAGE, MENU_KEY_SLASH, MENU_KEY_TEXT, vinput_fcitx_clamp_menu_page,
        vinput_fcitx_menu_filter_state_activate, vinput_fcitx_menu_filter_state_active,
        vinput_fcitx_menu_filter_state_append_text, vinput_fcitx_menu_filter_state_backspace,
        vinput_fcitx_menu_filter_state_decorate_title,
        vinput_fcitx_menu_filter_state_decorated_title_data,
        vinput_fcitx_menu_filter_state_decorated_title_len,
        vinput_fcitx_menu_filter_state_delete_last_word, vinput_fcitx_menu_filter_state_free,
        vinput_fcitx_menu_filter_state_handle_key, vinput_fcitx_menu_filter_state_matches,
        vinput_fcitx_menu_filter_state_new, vinput_fcitx_menu_filter_state_query_data,
        vinput_fcitx_menu_filter_state_query_len, vinput_fcitx_menu_filter_state_reset,
    };

    unsafe fn bytes_from_view<'a>(data: *const u8, len: usize) -> &'a [u8] {
        if data.is_null() {
            return &[];
        }
        // SAFETY: Test callers keep the owning menu state alive for the view lifetime.
        unsafe { std::slice::from_raw_parts(data, len) }
    }

    #[derive(Clone, Copy, Default)]
    struct KeyCall<'a> {
        release: bool,
        kind: u8,
        value: i64,
        text: &'a [u8],
        cursor_available: bool,
        current_selection: i64,
        current_page: i32,
        visible_item_count: usize,
    }

    unsafe fn handle_key(
        state: *mut super::VinputFcitxMenuFilterState,
        call: KeyCall<'_>,
    ) -> Option<(u8, i64)> {
        let mut action = u8::MAX;
        let mut value = i64::MAX;
        // SAFETY: Forwarded from this helper's caller; output pointers refer to
        // live local values for the duration of the call.
        let success = unsafe {
            vinput_fcitx_menu_filter_state_handle_key(
                state,
                u8::from(call.release),
                call.kind,
                call.value,
                call.text.as_ptr(),
                call.text.len(),
                u8::from(call.cursor_available),
                call.current_selection,
                call.current_page,
                call.visible_item_count,
                &raw mut action,
                &raw mut value,
            )
        };
        (success != 0).then_some((action, value))
    }

    #[test]
    fn exposes_menu_filter_lifecycle() {
        let text = b"MOON en";
        let suffix = " 中a".as_bytes();
        let matching = b"moonshine English provider";
        let missing = b"moonshine Chinese provider";
        let title = b"Models /";

        // SAFETY: All input views point to live local slices, and the state
        // handle is released exactly once after its final use.
        unsafe {
            let state = vinput_fcitx_menu_filter_state_new();
            assert!(!state.is_null());
            assert_eq!(vinput_fcitx_menu_filter_state_active(state), 0);
            assert_eq!(vinput_fcitx_menu_filter_state_activate(state), 1);
            assert_eq!(
                vinput_fcitx_menu_filter_state_append_text(state, text.as_ptr(), text.len()),
                1
            );
            assert_eq!(
                vinput_fcitx_menu_filter_state_matches(state, matching.as_ptr(), matching.len()),
                1
            );
            assert_eq!(
                vinput_fcitx_menu_filter_state_matches(state, missing.as_ptr(), missing.len()),
                0
            );
            assert_eq!(
                vinput_fcitx_menu_filter_state_append_text(state, suffix.as_ptr(), suffix.len(),),
                1
            );
            assert_eq!(vinput_fcitx_menu_filter_state_backspace(state), 1);
            assert_eq!(
                bytes_from_view(
                    vinput_fcitx_menu_filter_state_query_data(state),
                    vinput_fcitx_menu_filter_state_query_len(state),
                ),
                "MOON en 中".as_bytes()
            );
            assert_eq!(vinput_fcitx_menu_filter_state_backspace(state), 1);
            assert_eq!(vinput_fcitx_menu_filter_state_delete_last_word(state), 1);
            assert_eq!(
                bytes_from_view(
                    vinput_fcitx_menu_filter_state_query_data(state),
                    vinput_fcitx_menu_filter_state_query_len(state),
                ),
                b"MOON "
            );
            assert_eq!(
                vinput_fcitx_menu_filter_state_decorate_title(state, title.as_ptr(), title.len(),),
                1
            );
            assert_eq!(
                bytes_from_view(
                    vinput_fcitx_menu_filter_state_decorated_title_data(state),
                    vinput_fcitx_menu_filter_state_decorated_title_len(state),
                ),
                b"Models /MOON "
            );
            assert_eq!(vinput_fcitx_menu_filter_state_reset(state), 1);
            assert_eq!(vinput_fcitx_menu_filter_state_active(state), 0);
            assert_eq!(vinput_fcitx_menu_filter_state_query_len(state), 0);
            vinput_fcitx_menu_filter_state_free(state);
        }
    }

    #[test]
    fn invalid_utf8_does_not_change_query() {
        let valid = b"abc";
        let invalid = [0xff];

        // SAFETY: Input views point to live local slices; invalid UTF-8 is
        // deliberate and the state handle is released exactly once.
        unsafe {
            let state = vinput_fcitx_menu_filter_state_new();
            assert!(!state.is_null());
            assert_eq!(vinput_fcitx_menu_filter_state_activate(state), 1);
            assert_eq!(
                vinput_fcitx_menu_filter_state_append_text(state, valid.as_ptr(), valid.len()),
                1
            );
            assert_eq!(
                vinput_fcitx_menu_filter_state_append_text(state, invalid.as_ptr(), invalid.len(),),
                0
            );
            assert_eq!(
                bytes_from_view(
                    vinput_fcitx_menu_filter_state_query_data(state),
                    vinput_fcitx_menu_filter_state_query_len(state),
                ),
                valid
            );
            vinput_fcitx_menu_filter_state_free(state);
        }
    }

    #[test]
    fn exposes_filter_key_decisions_and_mutation() {
        // SAFETY: The state handle is live for all calls and released exactly once.
        unsafe {
            let state = vinput_fcitx_menu_filter_state_new();
            assert!(!state.is_null());
            assert_eq!(
                handle_key(
                    state,
                    KeyCall {
                        kind: MENU_KEY_SLASH,
                        ..KeyCall::default()
                    }
                ),
                Some((MENU_ACTION_REBUILD, 0))
            );
            assert_eq!(
                handle_key(
                    state,
                    KeyCall {
                        kind: MENU_KEY_TEXT,
                        text: b"moon",
                        ..KeyCall::default()
                    }
                ),
                Some((MENU_ACTION_REBUILD, 0))
            );
            assert_eq!(
                bytes_from_view(
                    vinput_fcitx_menu_filter_state_query_data(state),
                    vinput_fcitx_menu_filter_state_query_len(state),
                ),
                b"moon"
            );
            assert_eq!(
                handle_key(
                    state,
                    KeyCall {
                        kind: MENU_KEY_ESCAPE,
                        ..KeyCall::default()
                    }
                ),
                Some((MENU_ACTION_REBUILD, 0))
            );
            assert_eq!(
                handle_key(
                    state,
                    KeyCall {
                        kind: MENU_KEY_ESCAPE,
                        ..KeyCall::default()
                    }
                ),
                Some((MENU_ACTION_CLOSE_AND_CONSUME, 0))
            );
            vinput_fcitx_menu_filter_state_free(state);
        }
    }

    #[test]
    fn exposes_page_and_digit_key_decisions() {
        // SAFETY: The state handle is live for all calls and released exactly once.
        unsafe {
            let state = vinput_fcitx_menu_filter_state_new();
            assert!(!state.is_null());
            let page = KeyCall {
                kind: MENU_KEY_PAGE,
                value: 1,
                current_selection: -1,
                current_page: 1,
                visible_item_count: 13,
                ..KeyCall::default()
            };
            assert_eq!(handle_key(state, page), Some((MENU_ACTION_REBUILD, 2)));
            assert_eq!(
                handle_key(
                    state,
                    KeyCall {
                        kind: MENU_KEY_DIGIT,
                        value: 2,
                        current_selection: -1,
                        current_page: 1,
                        visible_item_count: 13,
                        ..KeyCall::default()
                    }
                ),
                Some((MENU_ACTION_SELECT, 12))
            );
            assert_eq!(
                handle_key(
                    state,
                    KeyCall {
                        kind: MENU_KEY_DIGIT,
                        value: 3,
                        current_selection: -1,
                        current_page: 1,
                        visible_item_count: 13,
                        ..KeyCall::default()
                    }
                ),
                Some((MENU_ACTION_CONSUME, 0))
            );
            vinput_fcitx_menu_filter_state_free(state);
        }
    }

    #[test]
    fn exposes_cursor_enter_and_release_decisions() {
        // SAFETY: The state handle is live for all calls and released exactly once.
        unsafe {
            let state = vinput_fcitx_menu_filter_state_new();
            assert!(!state.is_null());
            assert_eq!(
                handle_key(
                    state,
                    KeyCall {
                        kind: MENU_KEY_MOVE_PREVIOUS,
                        cursor_available: true,
                        current_selection: -1,
                        visible_item_count: 13,
                        ..KeyCall::default()
                    }
                ),
                Some((MENU_ACTION_MOVE_PREVIOUS, 0))
            );
            assert_eq!(
                handle_key(
                    state,
                    KeyCall {
                        kind: MENU_KEY_ENTER,
                        current_selection: -1,
                        current_page: 1,
                        visible_item_count: 13,
                        ..KeyCall::default()
                    }
                ),
                Some((MENU_ACTION_SELECT, 10))
            );
            assert_eq!(
                handle_key(
                    state,
                    KeyCall {
                        release: true,
                        kind: MENU_KEY_OTHER,
                        current_selection: -1,
                        ..KeyCall::default()
                    }
                ),
                Some((MENU_ACTION_PASS, 0))
            );
            assert_eq!(
                handle_key(
                    state,
                    KeyCall {
                        release: true,
                        kind: MENU_KEY_ESCAPE,
                        current_selection: -1,
                        ..KeyCall::default()
                    }
                ),
                Some((MENU_ACTION_CONSUME, 0))
            );
            assert_eq!(
                handle_key(
                    state,
                    KeyCall {
                        kind: MENU_KEY_OTHER,
                        current_selection: -1,
                        ..KeyCall::default()
                    }
                ),
                Some((MENU_ACTION_CLOSE_AND_PASS, 0))
            );
            vinput_fcitx_menu_filter_state_free(state);
        }
    }

    #[test]
    fn invalid_key_text_preserves_state_and_outputs() {
        let invalid = [0xff];
        // SAFETY: The invalid UTF-8 slice is live, outputs are writable, and the
        // state handle is released exactly once.
        unsafe {
            let state = vinput_fcitx_menu_filter_state_new();
            assert!(!state.is_null());
            assert_eq!(vinput_fcitx_menu_filter_state_activate(state), 1);
            assert_eq!(
                vinput_fcitx_menu_filter_state_append_text(state, b"old".as_ptr(), 3),
                1
            );
            let mut action = 91;
            let mut value = 92;
            assert_eq!(
                vinput_fcitx_menu_filter_state_handle_key(
                    state,
                    0,
                    MENU_KEY_TEXT,
                    0,
                    invalid.as_ptr(),
                    invalid.len(),
                    0,
                    -1,
                    0,
                    0,
                    &raw mut action,
                    &raw mut value,
                ),
                0
            );
            assert_eq!(action, 91);
            assert_eq!(value, 92);
            assert_eq!(
                bytes_from_view(
                    vinput_fcitx_menu_filter_state_query_data(state),
                    vinput_fcitx_menu_filter_state_query_len(state),
                ),
                b"old"
            );
            vinput_fcitx_menu_filter_state_free(state);
        }
    }

    #[test]
    fn exposes_page_clamping() {
        assert_eq!(vinput_fcitx_clamp_menu_page(0, 0), -1);
        assert_eq!(vinput_fcitx_clamp_menu_page(2, -1), 0);
        assert_eq!(vinput_fcitx_clamp_menu_page(2, 1), 1);
        assert_eq!(vinput_fcitx_clamp_menu_page(2, 99), 1);
    }
}

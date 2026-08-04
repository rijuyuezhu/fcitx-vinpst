use std::ptr;

use super::{
    MENU_ACTION_CLOSE_AND_CONSUME, MENU_ACTION_CONSUME, MENU_ACTION_MOVE_PREVIOUS,
    MENU_ACTION_PASS, MENU_ACTION_REBUILD, MENU_ACTION_SELECT, MENU_KEY_DIGIT, MENU_KEY_ENTER,
    MENU_KEY_ESCAPE, MENU_KEY_MOVE_PREVIOUS, MENU_KEY_OTHER, MENU_KEY_PAGE, MENU_KEY_SLASH,
    MENU_KEY_TEXT, VinputFcitxMenuKeyDecisionView, VinputFcitxMenuKeyInputView,
    menu_session_filter_ref, vinput_fcitx_clamp_menu_page, vinput_fcitx_menu_session_close,
    vinput_fcitx_menu_session_decorate_title, vinput_fcitx_menu_session_filter_active,
    vinput_fcitx_menu_session_free, vinput_fcitx_menu_session_handle_key,
    vinput_fcitx_menu_session_is_open, vinput_fcitx_menu_session_new,
    vinput_fcitx_menu_session_open, vinput_fcitx_menu_session_set_page,
};
use crate::ffi_string::VinputFcitxStringView;

#[derive(Clone, Copy, Default)]
struct KeyCall<'a> {
    release: bool,
    kind: u8,
    value: i64,
    text: &'a [u8],
    cursor_available: bool,
    current_selection: i64,
    visible_item_count: usize,
}

impl KeyCall<'_> {
    fn view(&self) -> VinputFcitxMenuKeyInputView {
        VinputFcitxMenuKeyInputView {
            release: u8::from(self.release),
            key_kind: self.kind,
            key_value: self.value,
            text: VinputFcitxStringView {
                data: self.text.as_ptr(),
                len: self.text.len(),
            },
            cursor_available: u8::from(self.cursor_available),
            current_selection: self.current_selection,
            visible_item_count: self.visible_item_count,
        }
    }
}

unsafe fn bytes(view: VinputFcitxStringView) -> &'static [u8] {
    if view.data.is_null() {
        return &[];
    }
    // SAFETY: Test callers keep the owning state alive.
    unsafe { std::slice::from_raw_parts(view.data, view.len) }
}

unsafe fn state_active(state: *const super::VinputFcitxMenuSession) -> u8 {
    let mut active = u8::MAX;
    // SAFETY: Test callers pass live state and writable output.
    assert_eq!(
        unsafe { vinput_fcitx_menu_session_filter_active(state, &raw mut active) },
        1
    );
    active
}

unsafe fn session_handle_key(
    state: *mut super::VinputFcitxMenuSession,
    call: KeyCall<'_>,
) -> Option<VinputFcitxMenuKeyDecisionView> {
    let mut decision = VinputFcitxMenuKeyDecisionView {
        action: u8::MAX,
        value: i64::MAX,
    };
    let input = call.view();
    // SAFETY: Test callers provide live inputs and writable output.
    let success =
        unsafe { vinput_fcitx_menu_session_handle_key(state, &raw const input, &raw mut decision) };
    (success != 0).then_some(decision)
}

#[test]
fn drives_filter_lifecycle_only_through_semantic_keys() {
    // SAFETY: State and local byte views remain live and are freed exactly once.
    unsafe {
        let state = vinput_fcitx_menu_session_new();
        assert!(!state.is_null());
        assert_eq!(vinput_fcitx_menu_session_open(state), 1);
        assert_eq!(state_active(state), 0);
        assert_eq!(
            session_handle_key(
                state,
                KeyCall {
                    kind: MENU_KEY_SLASH,
                    ..KeyCall::default()
                },
            )
            .map(|value| value.action),
            Some(MENU_ACTION_REBUILD),
        );
        assert_eq!(
            session_handle_key(
                state,
                KeyCall {
                    kind: MENU_KEY_TEXT,
                    text: "MOON 中".as_bytes(),
                    ..KeyCall::default()
                },
            )
            .map(|value| value.action),
            Some(MENU_ACTION_REBUILD),
        );
        assert_eq!(state_active(state), 1);

        let mut title = VinputFcitxStringView {
            data: ptr::null(),
            len: 0,
        };
        assert_eq!(
            vinput_fcitx_menu_session_decorate_title(
                state,
                b"Models /".as_ptr(),
                8,
                &raw mut title,
            ),
            1,
        );
        assert_eq!(bytes(title), "Models /MOON 中".as_bytes());

        assert_eq!(
            session_handle_key(
                state,
                KeyCall {
                    kind: MENU_KEY_ESCAPE,
                    ..KeyCall::default()
                },
            )
            .map(|value| value.action),
            Some(MENU_ACTION_REBUILD),
        );
        assert_eq!(state_active(state), 0);
        assert_eq!(
            session_handle_key(
                state,
                KeyCall {
                    kind: MENU_KEY_ESCAPE,
                    ..KeyCall::default()
                },
            )
            .map(|value| value.action),
            Some(MENU_ACTION_CLOSE_AND_CONSUME),
        );
        vinput_fcitx_menu_session_free(state);
    }
}

#[test]
fn exposes_page_digit_cursor_enter_and_release_decisions() {
    // SAFETY: State is live for all calls and freed exactly once.
    unsafe {
        let state = vinput_fcitx_menu_session_new();
        assert_eq!(vinput_fcitx_menu_session_open(state), 1);
        assert_eq!(vinput_fcitx_menu_session_set_page(state, 1), 1);
        let page = session_handle_key(
            state,
            KeyCall {
                kind: MENU_KEY_PAGE,
                value: 1,
                current_selection: -1,
                visible_item_count: 13,
                ..KeyCall::default()
            },
        )
        .expect("page decision");
        assert_eq!((page.action, page.value), (MENU_ACTION_REBUILD, 2));
        let digit = session_handle_key(
            state,
            KeyCall {
                kind: MENU_KEY_DIGIT,
                value: 2,
                current_selection: -1,
                visible_item_count: 13,
                ..KeyCall::default()
            },
        )
        .expect("digit decision");
        assert_eq!((digit.action, digit.value), (MENU_ACTION_SELECT, 12));
        assert_eq!(vinput_fcitx_menu_session_open(state), 1);
        assert_eq!(vinput_fcitx_menu_session_set_page(state, 1), 1);
        assert_eq!(
            session_handle_key(
                state,
                KeyCall {
                    kind: MENU_KEY_DIGIT,
                    value: 3,
                    current_selection: -1,
                    visible_item_count: 13,
                    ..KeyCall::default()
                },
            )
            .map(|value| value.action),
            Some(MENU_ACTION_CONSUME),
        );
        assert_eq!(
            session_handle_key(
                state,
                KeyCall {
                    kind: MENU_KEY_MOVE_PREVIOUS,
                    cursor_available: true,
                    current_selection: -1,
                    visible_item_count: 13,
                    ..KeyCall::default()
                },
            )
            .map(|value| value.action),
            Some(MENU_ACTION_MOVE_PREVIOUS),
        );
        assert_eq!(vinput_fcitx_menu_session_set_page(state, 1), 1);
        assert_eq!(
            session_handle_key(
                state,
                KeyCall {
                    kind: MENU_KEY_ENTER,
                    current_selection: -1,
                    visible_item_count: 13,
                    ..KeyCall::default()
                },
            )
            .map(|value| (value.action, value.value)),
            Some((MENU_ACTION_SELECT, 10)),
        );
        assert_eq!(vinput_fcitx_menu_session_open(state), 1);
        assert_eq!(
            session_handle_key(
                state,
                KeyCall {
                    release: true,
                    kind: MENU_KEY_OTHER,
                    current_selection: -1,
                    ..KeyCall::default()
                },
            )
            .map(|value| value.action),
            Some(MENU_ACTION_PASS),
        );
        vinput_fcitx_menu_session_free(state);
    }
}

#[test]
fn invalid_key_text_preserves_state_and_output() {
    // SAFETY: State and invalid local bytes remain live and state is freed once.
    unsafe {
        let state = vinput_fcitx_menu_session_new();
        assert_eq!(vinput_fcitx_menu_session_open(state), 1);
        assert!(
            session_handle_key(
                state,
                KeyCall {
                    kind: MENU_KEY_SLASH,
                    ..KeyCall::default()
                },
            )
            .is_some()
        );
        assert!(
            session_handle_key(
                state,
                KeyCall {
                    kind: MENU_KEY_TEXT,
                    text: b"old",
                    ..KeyCall::default()
                },
            )
            .is_some()
        );
        let invalid = [0xff];
        let mut decision = VinputFcitxMenuKeyDecisionView {
            action: 91,
            value: 92,
        };
        let input = VinputFcitxMenuKeyInputView {
            release: 0,
            key_kind: MENU_KEY_TEXT,
            key_value: 0,
            text: VinputFcitxStringView {
                data: invalid.as_ptr(),
                len: invalid.len(),
            },
            cursor_available: 0,
            current_selection: -1,
            visible_item_count: 0,
        };
        assert_eq!(
            vinput_fcitx_menu_session_handle_key(state, &raw const input, &raw mut decision,),
            0,
        );
        assert_eq!((decision.action, decision.value), (91, 92));
        assert_eq!(
            menu_session_filter_ref(state).map(vinput_fcitx_core::MenuFilterState::query),
            Some("old")
        );
        vinput_fcitx_menu_session_free(state);
    }
}

#[test]
fn exposes_page_clamping() {
    assert_eq!(vinput_fcitx_clamp_menu_page(0, 0), -1);
    assert_eq!(vinput_fcitx_clamp_menu_page(2, -1), 0);
    assert_eq!(vinput_fcitx_clamp_menu_page(2, 1), 1);
    assert_eq!(vinput_fcitx_clamp_menu_page(2, 99), 1);
}

#[test]
fn menu_session_owns_open_page_and_terminal_key_transitions() {
    // SAFETY: State is live for all calls and freed exactly once.
    unsafe {
        let state = vinput_fcitx_menu_session_new();
        assert!(!state.is_null());

        let mut open = u8::MAX;
        assert_eq!(vinput_fcitx_menu_session_is_open(state, &raw mut open), 1);
        assert_eq!(open, 0);
        assert_eq!(vinput_fcitx_menu_session_set_page(state, 1), 0);
        assert!(session_handle_key(state, KeyCall::default()).is_none());

        assert_eq!(vinput_fcitx_menu_session_open(state), 1);
        assert_eq!(vinput_fcitx_menu_session_set_page(state, 2), 1);
        assert_eq!(
            session_handle_key(
                state,
                KeyCall {
                    kind: MENU_KEY_DIGIT,
                    value: 3,
                    current_selection: -1,
                    visible_item_count: 30,
                    ..KeyCall::default()
                },
            )
            .map(|value| (value.action, value.value)),
            Some((MENU_ACTION_SELECT, 23)),
        );
        assert_eq!(vinput_fcitx_menu_session_is_open(state, &raw mut open), 1);
        assert_eq!(open, 0);

        assert_eq!(vinput_fcitx_menu_session_open(state), 1);
        assert_eq!(vinput_fcitx_menu_session_close(state), 1);
        vinput_fcitx_menu_session_free(state);
    }
}

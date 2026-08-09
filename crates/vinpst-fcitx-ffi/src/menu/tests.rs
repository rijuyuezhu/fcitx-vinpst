use std::ptr;

use super::{
    MENU_ACTION_CLOSE_AND_CONSUME, MENU_ACTION_CONSUME, MENU_ACTION_MOVE_PREVIOUS,
    MENU_ACTION_PASS, MENU_ACTION_REBUILD, MENU_ACTION_SELECT, MENU_KEY_DIGIT, MENU_KEY_ENTER,
    MENU_KEY_ESCAPE, MENU_KEY_MOVE_PREVIOUS, MENU_KEY_OTHER, MENU_KEY_PAGE, MENU_KEY_SLASH,
    MENU_KEY_TEXT, VinpstFcitxMenuKeyDecisionView, VinpstFcitxMenuKeyInputView,
    menu_session_filter_ref, vinpst_fcitx_clamp_menu_page, vinpst_fcitx_menu_session_close,
    vinpst_fcitx_menu_session_decorate_title, vinpst_fcitx_menu_session_filter_active,
    vinpst_fcitx_menu_session_free, vinpst_fcitx_menu_session_handle_key,
    vinpst_fcitx_menu_session_is_open, vinpst_fcitx_menu_session_new,
    vinpst_fcitx_menu_session_open, vinpst_fcitx_menu_session_set_page,
    vinpst_fcitx_result_menu_plan_key,
};
use crate::ffi_string::VinpstFcitxStringView;

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
    fn view(&self) -> VinpstFcitxMenuKeyInputView {
        VinpstFcitxMenuKeyInputView {
            release: u8::from(self.release),
            key_kind: self.kind,
            key_value: self.value,
            text: VinpstFcitxStringView {
                data: self.text.as_ptr(),
                len: self.text.len(),
            },
            cursor_available: u8::from(self.cursor_available),
            current_selection: self.current_selection,
            visible_item_count: self.visible_item_count,
        }
    }
}

unsafe fn bytes(view: VinpstFcitxStringView) -> &'static [u8] {
    if view.data.is_null() {
        return &[];
    }
    // SAFETY: Test callers keep the owning state alive.
    unsafe { std::slice::from_raw_parts(view.data, view.len) }
}

unsafe fn state_active(state: *const super::VinpstFcitxMenuSession) -> u8 {
    let mut active = u8::MAX;
    // SAFETY: Test callers pass live state and writable output.
    assert_eq!(
        unsafe { vinpst_fcitx_menu_session_filter_active(state, &raw mut active) },
        1
    );
    active
}

unsafe fn session_handle_key(
    state: *mut super::VinpstFcitxMenuSession,
    call: KeyCall<'_>,
) -> Option<VinpstFcitxMenuKeyDecisionView> {
    let mut decision = VinpstFcitxMenuKeyDecisionView {
        action: u8::MAX,
        value: i64::MAX,
    };
    let input = call.view();
    // SAFETY: Test callers provide live inputs and writable output.
    let success =
        unsafe { vinpst_fcitx_menu_session_handle_key(state, &raw const input, &raw mut decision) };
    (success != 0).then_some(decision)
}

unsafe fn result_menu_plan_key(
    call: KeyCall<'_>,
    current_page: i32,
) -> Option<VinpstFcitxMenuKeyDecisionView> {
    let mut decision = VinpstFcitxMenuKeyDecisionView {
        action: u8::MAX,
        value: i64::MAX,
    };
    let input = call.view();
    // SAFETY: Test callers provide live inputs and writable output.
    let success = unsafe {
        vinpst_fcitx_result_menu_plan_key(&raw const input, current_page, &raw mut decision)
    };
    (success != 0).then_some(decision)
}

#[test]
fn drives_filter_lifecycle_only_through_semantic_keys() {
    // SAFETY: State and local byte views remain live and are freed exactly once.
    unsafe {
        let state = vinpst_fcitx_menu_session_new();
        assert!(!state.is_null());
        assert_eq!(vinpst_fcitx_menu_session_open(state), 1);
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

        let mut title = VinpstFcitxStringView {
            data: ptr::null(),
            len: 0,
        };
        assert_eq!(
            vinpst_fcitx_menu_session_decorate_title(
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
        vinpst_fcitx_menu_session_free(state);
    }
}

#[test]
fn exposes_page_digit_cursor_enter_and_release_decisions() {
    // SAFETY: State is live for all calls and freed exactly once.
    unsafe {
        let state = vinpst_fcitx_menu_session_new();
        assert_eq!(vinpst_fcitx_menu_session_open(state), 1);
        assert_eq!(vinpst_fcitx_menu_session_set_page(state, 1), 1);
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
        assert_eq!(vinpst_fcitx_menu_session_open(state), 1);
        assert_eq!(vinpst_fcitx_menu_session_set_page(state, 1), 1);
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
        assert_eq!(vinpst_fcitx_menu_session_set_page(state, 1), 1);
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
        assert_eq!(vinpst_fcitx_menu_session_open(state), 1);
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
        vinpst_fcitx_menu_session_free(state);
    }
}

#[test]
fn exposes_five_row_result_menu_decisions() {
    // SAFETY: All views borrow local data for the duration of each call.
    unsafe {
        let digit = result_menu_plan_key(
            KeyCall {
                kind: MENU_KEY_DIGIT,
                value: 0,
                cursor_available: true,
                current_selection: 5,
                visible_item_count: 6,
                ..KeyCall::default()
            },
            1,
        )
        .expect("result digit decision");
        assert_eq!((digit.action, digit.value), (MENU_ACTION_SELECT, 5));

        let invalid_digit = result_menu_plan_key(
            KeyCall {
                kind: MENU_KEY_DIGIT,
                value: 1,
                cursor_available: true,
                current_selection: 5,
                visible_item_count: 6,
                ..KeyCall::default()
            },
            1,
        )
        .expect("result invalid digit decision");
        assert_eq!(invalid_digit.action, super::MENU_ACTION_CLOSE_AND_PASS);

        let release = result_menu_plan_key(
            KeyCall {
                release: true,
                kind: MENU_KEY_ESCAPE,
                visible_item_count: 6,
                ..KeyCall::default()
            },
            1,
        )
        .expect("result release decision");
        assert_eq!(release.action, MENU_ACTION_CONSUME);
    }
}

#[test]
fn invalid_key_text_preserves_state_and_output() {
    // SAFETY: State and invalid local bytes remain live and state is freed once.
    unsafe {
        let state = vinpst_fcitx_menu_session_new();
        assert_eq!(vinpst_fcitx_menu_session_open(state), 1);
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
        let mut decision = VinpstFcitxMenuKeyDecisionView {
            action: 91,
            value: 92,
        };
        let input = VinpstFcitxMenuKeyInputView {
            release: 0,
            key_kind: MENU_KEY_TEXT,
            key_value: 0,
            text: VinpstFcitxStringView {
                data: invalid.as_ptr(),
                len: invalid.len(),
            },
            cursor_available: 0,
            current_selection: -1,
            visible_item_count: 0,
        };
        assert_eq!(
            vinpst_fcitx_menu_session_handle_key(state, &raw const input, &raw mut decision,),
            0,
        );
        assert_eq!((decision.action, decision.value), (91, 92));
        assert_eq!(
            menu_session_filter_ref(state).map(vinpst_fcitx_core::MenuFilterState::query),
            Some("old")
        );
        vinpst_fcitx_menu_session_free(state);
    }
}

#[test]
fn exposes_page_clamping() {
    assert_eq!(vinpst_fcitx_clamp_menu_page(0, 0), -1);
    assert_eq!(vinpst_fcitx_clamp_menu_page(2, -1), 0);
    assert_eq!(vinpst_fcitx_clamp_menu_page(2, 1), 1);
    assert_eq!(vinpst_fcitx_clamp_menu_page(2, 99), 1);
}

#[test]
fn menu_session_owns_open_page_and_terminal_key_transitions() {
    // SAFETY: State is live for all calls and freed exactly once.
    unsafe {
        let state = vinpst_fcitx_menu_session_new();
        assert!(!state.is_null());

        let mut open = u8::MAX;
        assert_eq!(vinpst_fcitx_menu_session_is_open(state, &raw mut open), 1);
        assert_eq!(open, 0);
        assert_eq!(vinpst_fcitx_menu_session_set_page(state, 1), 0);
        assert!(session_handle_key(state, KeyCall::default()).is_none());

        assert_eq!(vinpst_fcitx_menu_session_open(state), 1);
        assert_eq!(vinpst_fcitx_menu_session_set_page(state, 2), 1);
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
        assert_eq!(vinpst_fcitx_menu_session_is_open(state, &raw mut open), 1);
        assert_eq!(open, 0);

        assert_eq!(vinpst_fcitx_menu_session_open(state), 1);
        assert_eq!(vinpst_fcitx_menu_session_close(state), 1);
        vinpst_fcitx_menu_session_free(state);
    }
}

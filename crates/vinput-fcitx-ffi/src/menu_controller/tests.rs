use std::ptr;

use vinput_fcitx_core::{AsrDisplaySnapshot, AsrDisplaySnapshotItem, SceneSnapshot};

use super::{
    VinputFcitxAsrMenuTextView, boxed_asr_controller, boxed_scene_controller,
    vinput_fcitx_asr_menu_controller_free, vinput_fcitx_asr_menu_controller_projection_new,
    vinput_fcitx_scene_menu_controller_free, vinput_fcitx_scene_menu_controller_projection_new,
};
use crate::{
    ffi_string::VinputFcitxStringView,
    menu::{vinput_fcitx_menu_session_free, vinput_fcitx_menu_session_new},
    menu_projection::{
        VinputFcitxMenuProjectionView, vinput_fcitx_menu_projection_free,
        vinput_fcitx_menu_projection_view,
    },
};

unsafe fn bytes(view: VinputFcitxStringView) -> &'static [u8] {
    if view.data.is_null() {
        return &[];
    }
    // SAFETY: Test callers keep the owning projection alive.
    unsafe { std::slice::from_raw_parts(view.data, view.len) }
}

fn string_view(value: &[u8]) -> VinputFcitxStringView {
    VinputFcitxStringView {
        data: value.as_ptr(),
        len: value.len(),
    }
}

#[test]
fn scene_controller_projects_latest_state() {
    // SAFETY: Every handle is live for each call and released exactly once.
    unsafe {
        let empty_controller = boxed_scene_controller(None);
        let session = vinput_fcitx_menu_session_new();
        assert!(!empty_controller.is_null());
        assert!(!session.is_null());
        assert!(
            vinput_fcitx_scene_menu_controller_projection_new(empty_controller, session).is_null()
        );
        vinput_fcitx_scene_menu_controller_free(empty_controller);

        let mut snapshot = SceneSnapshot::new("raw".to_owned());
        snapshot.push("raw".to_owned(), "Raw".to_owned());
        snapshot.push("meeting".to_owned(), "Meeting".to_owned());
        let controller = boxed_scene_controller(Some(snapshot));
        let projection = vinput_fcitx_scene_menu_controller_projection_new(controller, session);
        assert!(!projection.is_null());
        let mut view = VinputFcitxMenuProjectionView {
            summary: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
            item_count: 0,
        };
        assert_eq!(
            vinput_fcitx_menu_projection_view(projection, &raw mut view),
            1
        );
        assert_eq!(bytes(view.summary), b"Raw");
        assert_eq!(view.item_count, 1);

        vinput_fcitx_menu_projection_free(projection);
        vinput_fcitx_menu_session_free(session);
        vinput_fcitx_scene_menu_controller_free(controller);
    }
}

#[test]
fn asr_controller_projects_and_validates_localized_state() {
    // SAFETY: Every handle is live for each call and released exactly once.
    unsafe {
        let session = vinput_fcitx_menu_session_new();
        let mut snapshot = AsrDisplaySnapshot::new(
            "remote".to_owned(),
            "cloud".to_owned(),
            "local".to_owned(),
            "small".to_owned(),
            true,
            String::new(),
        );
        snapshot.push(AsrDisplaySnapshotItem {
            provider_id: "local".to_owned(),
            kind: "local".to_owned(),
            item_id: "small".to_owned(),
            display_title: "Small".to_owned(),
            model_value: "small".to_owned(),
        });
        snapshot.push(AsrDisplaySnapshotItem {
            provider_id: "remote".to_owned(),
            kind: "remote".to_owned(),
            item_id: "cloud".to_owned(),
            display_title: "Cloud".to_owned(),
            model_value: "cloud".to_owned(),
        });
        let controller = boxed_asr_controller(Some(snapshot));
        let args = [
            b"Local".as_slice(),
            b"Remote".as_slice(),
            b"Command".as_slice(),
            b" (loading)".as_slice(),
            b"unavailable".as_slice(),
            b"Loading: ".as_slice(),
            b"Error: ".as_slice(),
        ];
        let text = VinputFcitxAsrMenuTextView {
            local: string_view(args[0]),
            remote: string_view(args[1]),
            command: string_view(args[2]),
            loading_suffix: string_view(args[3]),
            unavailable: string_view(args[4]),
            loading_prefix: string_view(args[5]),
            error_prefix: string_view(args[6]),
        };
        let projection =
            vinput_fcitx_asr_menu_controller_projection_new(controller, session, &raw const text);
        assert!(!projection.is_null());
        let mut view = VinputFcitxMenuProjectionView {
            summary: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
            item_count: 0,
        };
        assert_eq!(
            vinput_fcitx_menu_projection_view(projection, &raw mut view),
            1
        );
        assert_eq!(bytes(view.summary), b"Small | Loading: remote/Cloud");
        assert_eq!(view.item_count, 1);
        vinput_fcitx_menu_projection_free(projection);

        let invalid = [0xff];
        let invalid_text = VinputFcitxAsrMenuTextView {
            local: string_view(&invalid),
            ..text
        };
        assert!(
            vinput_fcitx_asr_menu_controller_projection_new(
                controller,
                session,
                &raw const invalid_text,
            )
            .is_null()
        );

        vinput_fcitx_menu_session_free(session);
        vinput_fcitx_asr_menu_controller_free(controller);
    }
}

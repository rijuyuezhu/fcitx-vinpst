use std::ptr;

use vinput_fcitx_core::{MenuFilterState, SceneSnapshot};

use super::{
    VinputFcitxSceneProjectionView, vinput_fcitx_scene_projection_free,
    vinput_fcitx_scene_projection_item_view, vinput_fcitx_scene_projection_new,
    vinput_fcitx_scene_projection_view,
};
use crate::{
    asr_projection::VinputFcitxProjectedMenuItemView,
    frontend::VinputFcitxStringView,
    menu::{boxed_menu_filter_state, vinput_fcitx_menu_filter_state_free},
    menu_snapshot::{boxed_scene_snapshot, vinput_fcitx_scene_snapshot_free},
};

unsafe fn bytes(view: VinputFcitxStringView) -> &'static [u8] {
    if view.data.is_null() {
        return &[];
    }
    // SAFETY: Test callers keep the owning projection alive.
    unsafe { std::slice::from_raw_parts(view.data, view.len) }
}

fn snapshot(
    active: &str,
    rows: &[(&str, &str)],
) -> *mut crate::menu_snapshot::VinputFcitxSceneSnapshot {
    let mut snapshot = SceneSnapshot::new(active.to_owned());
    for (id, label) in rows {
        snapshot.push((*id).to_owned(), (*label).to_owned());
    }
    boxed_scene_snapshot(snapshot)
}

fn filter(query: &str) -> *mut crate::menu::VinputFcitxMenuFilterState {
    let mut filter = MenuFilterState::default();
    if !query.is_empty() {
        filter.activate();
        filter.append_text(query);
    }
    boxed_menu_filter_state(filter)
}

#[test]
fn projects_directly_from_scene_snapshot() {
    // SAFETY: Both handles are live for all calls and freed exactly once.
    unsafe {
        let snapshot = snapshot(
            "meeting",
            &[
                ("raw", "Raw Dictation"),
                ("meeting", "Meeting Notes"),
                ("code", "Code Review"),
            ],
        );
        let filter = filter("code");
        let projection = vinput_fcitx_scene_projection_new(snapshot, filter);
        assert!(!projection.is_null());

        let mut summary = VinputFcitxSceneProjectionView {
            active_label: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
            item_count: 0,
        };
        assert_eq!(
            vinput_fcitx_scene_projection_view(projection, &raw mut summary),
            1,
        );
        assert_eq!(bytes(summary.active_label), b"Meeting Notes");
        assert_eq!(summary.item_count, 1);

        let empty = VinputFcitxStringView {
            data: ptr::null(),
            len: 0,
        };
        let mut item = VinputFcitxProjectedMenuItemView {
            label: empty,
            control_kind: 0,
            control_first: empty,
            control_second: empty,
            control_label: empty,
        };
        assert_eq!(
            vinput_fcitx_scene_projection_item_view(projection, 0, &raw mut item),
            1,
        );
        assert_eq!(bytes(item.label), b"Code Review");
        assert_eq!(item.control_kind, 1);
        assert_eq!(bytes(item.control_first), b"code");
        assert!(bytes(item.control_second).is_empty());
        assert_eq!(bytes(item.control_label), b"Code Review");
        assert_eq!(
            vinput_fcitx_scene_projection_item_view(projection, 1, &raw mut item),
            0,
        );
        vinput_fcitx_scene_projection_free(projection);
        vinput_fcitx_menu_filter_state_free(filter);
        vinput_fcitx_scene_snapshot_free(snapshot);
    }
}

#[test]
fn falls_back_to_active_id_and_rejects_missing_filter() {
    // SAFETY: Both handles are live for all calls and freed exactly once.
    unsafe {
        let snapshot = snapshot("missing", &[("other", "Other")]);
        let filter = filter("");
        let projection = vinput_fcitx_scene_projection_new(snapshot, filter);
        let mut summary = VinputFcitxSceneProjectionView {
            active_label: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
            item_count: 0,
        };
        assert_eq!(
            vinput_fcitx_scene_projection_view(projection, &raw mut summary),
            1,
        );
        assert_eq!(bytes(summary.active_label), b"missing");
        assert_eq!(summary.item_count, 1);
        vinput_fcitx_scene_projection_free(projection);

        assert!(vinput_fcitx_scene_projection_new(snapshot, ptr::null()).is_null());
        vinput_fcitx_menu_filter_state_free(filter);
        vinput_fcitx_scene_snapshot_free(snapshot);
    }
}

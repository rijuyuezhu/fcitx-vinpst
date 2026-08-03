use std::ptr;

use super::{
    VinputFcitxSceneProjectionView, vinput_fcitx_scene_projection_free,
    vinput_fcitx_scene_projection_item_view, vinput_fcitx_scene_projection_new,
    vinput_fcitx_scene_projection_view,
};
use crate::{
    asr_projection::VinputFcitxProjectedMenuItemView,
    frontend::VinputFcitxStringView,
    menu_snapshot::{
        vinput_fcitx_scene_snapshot_add, vinput_fcitx_scene_snapshot_free,
        vinput_fcitx_scene_snapshot_new,
    },
};

unsafe fn bytes(view: VinputFcitxStringView) -> &'static [u8] {
    if view.data.is_null() {
        return &[];
    }
    // SAFETY: Test callers keep the owning projection alive.
    unsafe { std::slice::from_raw_parts(view.data, view.len) }
}

#[test]
fn projects_directly_from_scene_snapshot() {
    // SAFETY: Local byte slices outlive calls and both handles are freed once.
    unsafe {
        let snapshot = vinput_fcitx_scene_snapshot_new(b"meeting".as_ptr(), 7);
        assert!(!snapshot.is_null());
        assert_eq!(
            vinput_fcitx_scene_snapshot_add(
                snapshot,
                b"raw".as_ptr(),
                3,
                b"Raw Dictation".as_ptr(),
                13,
            ),
            1,
        );
        assert_eq!(
            vinput_fcitx_scene_snapshot_add(
                snapshot,
                b"meeting".as_ptr(),
                7,
                b"Meeting Notes".as_ptr(),
                13,
            ),
            1,
        );
        assert_eq!(
            vinput_fcitx_scene_snapshot_add(
                snapshot,
                b"code".as_ptr(),
                4,
                b"Code Review".as_ptr(),
                11,
            ),
            1,
        );
        let projection = vinput_fcitx_scene_projection_new(snapshot, b"code".as_ptr(), 4);
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
        vinput_fcitx_scene_snapshot_free(snapshot);
    }
}

#[test]
fn falls_back_to_active_id_and_rejects_invalid_query() {
    // SAFETY: Local byte slices outlive calls and handles are freed once.
    unsafe {
        let snapshot = vinput_fcitx_scene_snapshot_new(b"missing".as_ptr(), 7);
        assert_eq!(
            vinput_fcitx_scene_snapshot_add(snapshot, b"other".as_ptr(), 5, b"Other".as_ptr(), 5,),
            1,
        );
        let projection = vinput_fcitx_scene_projection_new(snapshot, ptr::null(), 0);
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

        let invalid = [0xff];
        assert!(
            vinput_fcitx_scene_projection_new(snapshot, invalid.as_ptr(), invalid.len()).is_null()
        );
        vinput_fcitx_scene_snapshot_free(snapshot);
    }
}

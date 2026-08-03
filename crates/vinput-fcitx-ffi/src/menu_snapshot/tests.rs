use std::ptr;

use super::{
    VinputFcitxAsrDisplaySnapshotItemView, VinputFcitxAsrDisplaySnapshotView,
    VinputFcitxSceneSnapshotItemView, VinputFcitxSceneSnapshotView,
    vinput_fcitx_asr_display_snapshot_add, vinput_fcitx_asr_display_snapshot_free,
    vinput_fcitx_asr_display_snapshot_item_view, vinput_fcitx_asr_display_snapshot_new,
    vinput_fcitx_asr_display_snapshot_view, vinput_fcitx_scene_snapshot_add,
    vinput_fcitx_scene_snapshot_free, vinput_fcitx_scene_snapshot_item_view,
    vinput_fcitx_scene_snapshot_new, vinput_fcitx_scene_snapshot_set_active,
    vinput_fcitx_scene_snapshot_view,
};
use crate::frontend::VinputFcitxStringView;

unsafe fn bytes(view: VinputFcitxStringView) -> &'static [u8] {
    if view.data.is_null() {
        return &[];
    }
    // SAFETY: Test callers keep the owning snapshot alive.
    unsafe { std::slice::from_raw_parts(view.data, view.len) }
}

#[test]
fn exposes_scene_snapshot_through_two_views() {
    // SAFETY: All local byte slices outlive calls and the handle is freed once.
    unsafe {
        let snapshot = vinput_fcitx_scene_snapshot_new(b"meeting".as_ptr(), 7);
        assert!(!snapshot.is_null());
        assert_eq!(
            vinput_fcitx_scene_snapshot_add(
                snapshot,
                b"meeting".as_ptr(),
                7,
                b"Meeting Notes".as_ptr(),
                13,
            ),
            1
        );
        let mut view = VinputFcitxSceneSnapshotView {
            active_scene_id: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
            active_label: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
            item_count: 0,
        };
        assert_eq!(vinput_fcitx_scene_snapshot_view(snapshot, &raw mut view), 1);
        assert_eq!(bytes(view.active_scene_id), b"meeting");
        assert_eq!(bytes(view.active_label), b"Meeting Notes");
        assert_eq!(view.item_count, 1);

        let mut item = VinputFcitxSceneSnapshotItemView {
            id: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
            label: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
        };
        assert_eq!(
            vinput_fcitx_scene_snapshot_item_view(snapshot, 0, &raw mut item),
            1
        );
        assert_eq!(bytes(item.id), b"meeting");
        assert_eq!(bytes(item.label), b"Meeting Notes");
        assert_eq!(
            vinput_fcitx_scene_snapshot_set_active(snapshot, b"missing".as_ptr(), 7),
            1
        );
        assert_eq!(vinput_fcitx_scene_snapshot_view(snapshot, &raw mut view), 1);
        assert_eq!(bytes(view.active_label), b"missing");
        vinput_fcitx_scene_snapshot_free(snapshot);
    }
}

#[test]
fn exposes_asr_snapshot_through_two_views() {
    // SAFETY: All local byte slices outlive calls and the handle is freed once.
    unsafe {
        let snapshot = vinput_fcitx_asr_display_snapshot_new(
            b"sherpa".as_ptr(),
            6,
            b"requested".as_ptr(),
            9,
            b"sherpa".as_ptr(),
            6,
            b"effective".as_ptr(),
            9,
            1,
            ptr::null(),
            0,
        );
        assert!(!snapshot.is_null());
        assert_eq!(
            vinput_fcitx_asr_display_snapshot_add(
                snapshot,
                b"sherpa".as_ptr(),
                6,
                b"local".as_ptr(),
                5,
                b"requested".as_ptr(),
                9,
                ptr::null(),
                0,
                b"requested".as_ptr(),
                9,
            ),
            1
        );
        let mut view = VinputFcitxAsrDisplaySnapshotView {
            target_provider_id: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
            target_model_id: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
            effective_provider_id: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
            effective_model_id: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
            last_error: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
            effective_base_label: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
            target_base_label: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
            reload_in_progress: 0,
            item_count: 0,
        };
        assert_eq!(
            vinput_fcitx_asr_display_snapshot_view(snapshot, &raw mut view),
            1
        );
        assert_eq!(bytes(view.target_provider_id), b"sherpa");
        assert_eq!(bytes(view.target_base_label), b"requested");
        assert_eq!(bytes(view.effective_base_label), b"effective");
        assert_eq!(view.reload_in_progress, 1);
        assert_eq!(view.item_count, 1);

        let empty = VinputFcitxStringView {
            data: ptr::null(),
            len: 0,
        };
        let mut item = VinputFcitxAsrDisplaySnapshotItemView {
            provider_id: empty,
            kind: empty,
            item_id: empty,
            display_title: empty,
            model_value: empty,
            base_label: empty,
            is_loading: 0,
        };
        assert_eq!(
            vinput_fcitx_asr_display_snapshot_item_view(snapshot, 0, &raw mut item),
            1
        );
        assert_eq!(bytes(item.provider_id), b"sherpa");
        assert_eq!(bytes(item.base_label), b"requested");
        assert_eq!(item.is_loading, 1);
        assert_eq!(
            vinput_fcitx_asr_display_snapshot_item_view(snapshot, 1, &raw mut item),
            0
        );
        vinput_fcitx_asr_display_snapshot_free(snapshot);
    }
}

#[test]
fn invalid_row_utf8_does_not_mutate_snapshot() {
    // SAFETY: All local byte slices outlive calls and the handle is freed once.
    unsafe {
        let snapshot = vinput_fcitx_scene_snapshot_new(ptr::null(), 0);
        let invalid = [0xff];
        assert_eq!(
            vinput_fcitx_scene_snapshot_add(
                snapshot,
                invalid.as_ptr(),
                invalid.len(),
                b"label".as_ptr(),
                5,
            ),
            0
        );
        let mut view = VinputFcitxSceneSnapshotView {
            active_scene_id: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
            active_label: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
            item_count: 9,
        };
        assert_eq!(vinput_fcitx_scene_snapshot_view(snapshot, &raw mut view), 1);
        assert_eq!(view.item_count, 0);
        vinput_fcitx_scene_snapshot_free(snapshot);
    }
}

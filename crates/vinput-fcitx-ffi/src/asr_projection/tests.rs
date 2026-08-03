use std::ptr;

use super::{
    VinputFcitxProjectedMenuItemView, VinputFcitxProjectionView,
    vinput_fcitx_asr_projection_finish, vinput_fcitx_asr_projection_free,
    vinput_fcitx_asr_projection_item_view, vinput_fcitx_asr_projection_new,
    vinput_fcitx_asr_projection_set_label, vinput_fcitx_asr_projection_view,
};
use crate::{
    frontend::VinputFcitxStringView,
    menu_snapshot::{
        vinput_fcitx_asr_display_snapshot_add, vinput_fcitx_asr_display_snapshot_free,
        vinput_fcitx_asr_display_snapshot_new,
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
#[allow(clippy::too_many_lines)]
fn projects_localized_rows_directly_from_snapshot() {
    // SAFETY: Local byte slices outlive all calls and handles are freed once.
    unsafe {
        let snapshot = vinput_fcitx_asr_display_snapshot_new(
            b"sherpa".as_ptr(),
            6,
            b"moonshine-en".as_ptr(),
            12,
            b"sherpa".as_ptr(),
            6,
            b"moonshine-en".as_ptr(),
            12,
            0,
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
                b"moonshine-en".as_ptr(),
                12,
                b"Moonshine English".as_ptr(),
                17,
                b"moonshine-en".as_ptr(),
                12,
            ),
            1,
        );
        assert_eq!(
            vinput_fcitx_asr_display_snapshot_add(
                snapshot,
                b"sherpa".as_ptr(),
                6,
                b"local".as_ptr(),
                5,
                b"paraformer-zh".as_ptr(),
                13,
                b"Paraformer Chinese".as_ptr(),
                18,
                b"paraformer-zh".as_ptr(),
                13,
            ),
            1,
        );

        let projection = vinput_fcitx_asr_projection_new(snapshot, b"chinese local".as_ptr(), 13);
        assert!(!projection.is_null());
        assert_eq!(
            vinput_fcitx_asr_projection_set_label(
                projection,
                0,
                b"Moonshine English [Local]".as_ptr(),
                25,
            ),
            1,
        );
        assert_eq!(
            vinput_fcitx_asr_projection_set_label(
                projection,
                1,
                b"Paraformer Chinese [Local]".as_ptr(),
                26,
            ),
            1,
        );
        assert_eq!(vinput_fcitx_asr_projection_finish(projection), 1);

        let mut summary = VinputFcitxProjectionView { item_count: 0 };
        assert_eq!(
            vinput_fcitx_asr_projection_view(projection, &raw mut summary),
            1,
        );
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
            vinput_fcitx_asr_projection_item_view(projection, 0, &raw mut item),
            1,
        );
        assert_eq!(bytes(item.label), b"Paraformer Chinese [Local]");
        assert_eq!(item.control_kind, 2);
        assert_eq!(bytes(item.control_first), b"sherpa");
        assert_eq!(bytes(item.control_second), b"paraformer-zh");
        assert_eq!(bytes(item.control_label), b"Paraformer Chinese");
        assert_eq!(
            vinput_fcitx_asr_projection_item_view(projection, 1, &raw mut item),
            0,
        );
        vinput_fcitx_asr_projection_free(projection);
        vinput_fcitx_asr_display_snapshot_free(snapshot);
    }
}

#[test]
fn keeps_requested_loading_row_visible() {
    // SAFETY: Local byte slices outlive all calls and handles are freed once.
    unsafe {
        let snapshot = vinput_fcitx_asr_display_snapshot_new(
            b"sherpa".as_ptr(),
            6,
            b"requested".as_ptr(),
            9,
            b"sherpa".as_ptr(),
            6,
            b"legacy".as_ptr(),
            6,
            1,
            ptr::null(),
            0,
        );
        assert_eq!(
            vinput_fcitx_asr_display_snapshot_add(
                snapshot,
                b"sherpa".as_ptr(),
                6,
                b"local".as_ptr(),
                5,
                b"requested".as_ptr(),
                9,
                b"Requested".as_ptr(),
                9,
                b"requested".as_ptr(),
                9,
            ),
            1,
        );
        let projection = vinput_fcitx_asr_projection_new(snapshot, ptr::null(), 0);
        assert_eq!(
            vinput_fcitx_asr_projection_set_label(
                projection,
                0,
                b"Requested [Local] (loading)".as_ptr(),
                27,
            ),
            1,
        );
        assert_eq!(vinput_fcitx_asr_projection_finish(projection), 1);
        let mut summary = VinputFcitxProjectionView { item_count: 0 };
        assert_eq!(
            vinput_fcitx_asr_projection_view(projection, &raw mut summary),
            1,
        );
        assert_eq!(summary.item_count, 1);
        vinput_fcitx_asr_projection_free(projection);
        vinput_fcitx_asr_display_snapshot_free(snapshot);
    }
}

#[test]
fn incomplete_or_invalid_labels_do_not_finalize() {
    // SAFETY: Local byte slices outlive all calls and handles are freed once.
    unsafe {
        let snapshot = vinput_fcitx_asr_display_snapshot_new(
            ptr::null(),
            0,
            ptr::null(),
            0,
            ptr::null(),
            0,
            ptr::null(),
            0,
            0,
            ptr::null(),
            0,
        );
        assert_eq!(
            vinput_fcitx_asr_display_snapshot_add(
                snapshot,
                b"provider".as_ptr(),
                8,
                b"local".as_ptr(),
                5,
                b"model".as_ptr(),
                5,
                ptr::null(),
                0,
                b"model".as_ptr(),
                5,
            ),
            1,
        );
        let projection = vinput_fcitx_asr_projection_new(snapshot, ptr::null(), 0);
        assert_eq!(vinput_fcitx_asr_projection_finish(projection), 0);
        let invalid = [0xff];
        assert_eq!(
            vinput_fcitx_asr_projection_set_label(projection, 0, invalid.as_ptr(), invalid.len(),),
            0,
        );
        assert_eq!(vinput_fcitx_asr_projection_finish(projection), 0);
        assert_eq!(
            vinput_fcitx_asr_projection_set_label(projection, 0, b"Model".as_ptr(), 5),
            1,
        );
        assert_eq!(vinput_fcitx_asr_projection_finish(projection), 1);
        assert_eq!(
            vinput_fcitx_asr_projection_set_label(projection, 0, b"Other".as_ptr(), 5),
            0,
        );
        vinput_fcitx_asr_projection_free(projection);
        vinput_fcitx_asr_display_snapshot_free(snapshot);
    }
}

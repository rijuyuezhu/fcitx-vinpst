use std::ptr;

use super::{
    VinputFcitxProjectedMenuItemView, VinputFcitxProjectionView, vinput_fcitx_asr_projection_free,
    vinput_fcitx_asr_projection_item_view, vinput_fcitx_asr_projection_new,
    vinput_fcitx_asr_projection_view,
};
use crate::{
    frontend::VinputFcitxStringView,
    menu_snapshot::{
        VinputFcitxAsrDisplaySnapshot, vinput_fcitx_asr_display_snapshot_add,
        vinput_fcitx_asr_display_snapshot_free, vinput_fcitx_asr_display_snapshot_new,
    },
};

unsafe fn bytes(view: VinputFcitxStringView) -> &'static [u8] {
    if view.data.is_null() {
        return &[];
    }
    // SAFETY: Test callers keep the owning projection alive.
    unsafe { std::slice::from_raw_parts(view.data, view.len) }
}

unsafe fn projection(
    snapshot: *const VinputFcitxAsrDisplaySnapshot,
    query: &[u8],
) -> *mut super::VinputFcitxAsrProjection {
    // SAFETY: Test byte slices outlive the projection constructor call.
    unsafe {
        vinput_fcitx_asr_projection_new(
            snapshot,
            query.as_ptr(),
            query.len(),
            b"Local".as_ptr(),
            5,
            b"Remote".as_ptr(),
            6,
            b"Command".as_ptr(),
            7,
            b" (loading)".as_ptr(),
            10,
            b"unavailable".as_ptr(),
            11,
            b"Loading: ".as_ptr(),
            9,
            b"Error: ".as_ptr(),
            7,
        )
    }
}

fn empty_string_view() -> VinputFcitxStringView {
    VinputFcitxStringView {
        data: ptr::null(),
        len: 0,
    }
}

#[test]
fn projects_localized_rows_and_effective_label_directly_from_snapshot() {
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

        let projection = projection(snapshot, b"chinese local");
        assert!(!projection.is_null());
        let mut summary = VinputFcitxProjectionView {
            effective_label: empty_string_view(),
            item_count: 0,
        };
        assert_eq!(
            vinput_fcitx_asr_projection_view(projection, &raw mut summary),
            1,
        );
        assert_eq!(bytes(summary.effective_label), b"Moonshine English");
        assert_eq!(summary.item_count, 1);

        let empty = empty_string_view();
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

        vinput_fcitx_asr_projection_free(projection);
        vinput_fcitx_asr_display_snapshot_free(snapshot);
    }
}

#[test]
fn renders_loading_row_and_current_backend_summary() {
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
            b"reload failed".as_ptr(),
            13,
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

        let projection = projection(snapshot, b"");
        assert!(!projection.is_null());
        let mut summary = VinputFcitxProjectionView {
            effective_label: empty_string_view(),
            item_count: 0,
        };
        assert_eq!(
            vinput_fcitx_asr_projection_view(projection, &raw mut summary),
            1,
        );
        assert_eq!(
            bytes(summary.effective_label),
            b"legacy | Loading: sherpa/Requested | Error: reload failed"
        );
        assert_eq!(summary.item_count, 1);

        let empty = empty_string_view();
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
        assert_eq!(bytes(item.label), b"Requested [Local] (loading)");

        vinput_fcitx_asr_projection_free(projection);
        vinput_fcitx_asr_display_snapshot_free(snapshot);
    }
}

#[test]
fn invalid_localized_fragment_rejects_projection() {
    // SAFETY: Local byte slices outlive the constructor call and the snapshot is freed once.
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
        let invalid = [0xff];
        let projection = vinput_fcitx_asr_projection_new(
            snapshot,
            ptr::null(),
            0,
            invalid.as_ptr(),
            invalid.len(),
            b"Remote".as_ptr(),
            6,
            b"Command".as_ptr(),
            7,
            b" (loading)".as_ptr(),
            10,
            b"unavailable".as_ptr(),
            11,
            b"Loading: ".as_ptr(),
            9,
            b"Error: ".as_ptr(),
            7,
        );
        assert!(projection.is_null());
        vinput_fcitx_asr_display_snapshot_free(snapshot);
    }
}

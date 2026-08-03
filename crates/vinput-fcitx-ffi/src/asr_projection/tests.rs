use std::ptr;

use vinput_fcitx_core::{AsrDisplaySnapshot, AsrDisplaySnapshotItem, MenuFilterState};

use super::{
    VinputFcitxProjectedMenuItemView, VinputFcitxProjectionView, vinput_fcitx_asr_projection_free,
    vinput_fcitx_asr_projection_item_view, vinput_fcitx_asr_projection_new,
    vinput_fcitx_asr_projection_view,
};
use crate::{
    frontend::VinputFcitxStringView,
    menu::{boxed_menu_filter_state, vinput_fcitx_menu_filter_state_free},
    menu_snapshot::{
        VinputFcitxAsrDisplaySnapshot, boxed_asr_display_snapshot,
        vinput_fcitx_asr_display_snapshot_free,
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
    filter: *const crate::menu::VinputFcitxMenuFilterState,
) -> *mut super::VinputFcitxAsrProjection {
    // SAFETY: Test byte slices outlive the projection constructor call.
    unsafe {
        vinput_fcitx_asr_projection_new(
            snapshot,
            filter,
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

fn filter(query: &str) -> *mut crate::menu::VinputFcitxMenuFilterState {
    let mut filter = MenuFilterState::default();
    if !query.is_empty() {
        filter.activate();
        filter.append_text(query);
    }
    boxed_menu_filter_state(filter)
}

fn snapshot(
    target_provider: &str,
    target_model: &str,
    effective_provider: &str,
    effective_model: &str,
    reload: bool,
    error: &str,
    rows: &[(&str, &str, &str, &str, &str)],
) -> *mut VinputFcitxAsrDisplaySnapshot {
    let mut snapshot = AsrDisplaySnapshot::new(
        target_provider.to_owned(),
        target_model.to_owned(),
        effective_provider.to_owned(),
        effective_model.to_owned(),
        reload,
        error.to_owned(),
    );
    for (provider_id, kind, item_id, display_title, model_value) in rows {
        snapshot.push(AsrDisplaySnapshotItem {
            provider_id: (*provider_id).to_owned(),
            kind: (*kind).to_owned(),
            item_id: (*item_id).to_owned(),
            display_title: (*display_title).to_owned(),
            model_value: (*model_value).to_owned(),
        });
    }
    boxed_asr_display_snapshot(snapshot)
}

fn empty_string_view() -> VinputFcitxStringView {
    VinputFcitxStringView {
        data: ptr::null(),
        len: 0,
    }
}

#[test]
fn projects_localized_rows_and_effective_label_directly_from_snapshot() {
    // SAFETY: Both handles are live for all calls and freed exactly once.
    unsafe {
        let snapshot = snapshot(
            "sherpa",
            "moonshine-en",
            "sherpa",
            "moonshine-en",
            false,
            "",
            &[
                (
                    "sherpa",
                    "local",
                    "moonshine-en",
                    "Moonshine English",
                    "moonshine-en",
                ),
                (
                    "sherpa",
                    "local",
                    "paraformer-zh",
                    "Paraformer Chinese",
                    "paraformer-zh",
                ),
            ],
        );
        let filter = filter("chinese local");
        let projection = projection(snapshot, filter);
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
        vinput_fcitx_menu_filter_state_free(filter);
        vinput_fcitx_asr_display_snapshot_free(snapshot);
    }
}

#[test]
fn renders_loading_row_and_current_backend_summary() {
    // SAFETY: Both handles are live for all calls and freed exactly once.
    unsafe {
        let snapshot = snapshot(
            "sherpa",
            "requested",
            "sherpa",
            "legacy",
            true,
            "reload failed",
            &[("sherpa", "local", "requested", "Requested", "requested")],
        );
        let filter = filter("");
        let projection = projection(snapshot, filter);
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
        vinput_fcitx_menu_filter_state_free(filter);
        vinput_fcitx_asr_display_snapshot_free(snapshot);
    }
}

#[test]
fn invalid_localized_fragment_rejects_projection() {
    // SAFETY: The snapshot is live for the constructor call and freed exactly once.
    unsafe {
        let snapshot = snapshot("", "", "", "", false, "", &[]);
        let filter = filter("");
        let invalid = [0xff];
        let projection = vinput_fcitx_asr_projection_new(
            snapshot,
            filter,
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
        vinput_fcitx_menu_filter_state_free(filter);
        vinput_fcitx_asr_display_snapshot_free(snapshot);
    }
}

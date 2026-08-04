use std::ptr;

use vinput_fcitx_core::{AsrDisplaySnapshot, AsrDisplaySnapshotItem, MenuFilterState};

use super::{
    VinputFcitxMenuProjection, VinputFcitxMenuProjectionView, VinputFcitxProjectedMenuItemView,
    vinput_fcitx_menu_projection_free, vinput_fcitx_menu_projection_item_view,
    vinput_fcitx_menu_projection_view,
};
use crate::{
    ffi_string::VinputFcitxStringView,
    menu::{boxed_menu_session, vinput_fcitx_menu_session_free},
    menu_controller::{
        VinputFcitxAsrMenuController, VinputFcitxAsrMenuTextView, boxed_asr_controller,
        vinput_fcitx_asr_menu_controller_free, vinput_fcitx_asr_menu_controller_projection_new,
    },
};

unsafe fn bytes(view: VinputFcitxStringView) -> &'static [u8] {
    if view.data.is_null() {
        return &[];
    }
    // SAFETY: Test callers keep the owning projection alive.
    unsafe { std::slice::from_raw_parts(view.data, view.len) }
}

unsafe fn menu_projection(
    controller: *const VinputFcitxAsrMenuController,
    filter: *const crate::menu::VinputFcitxMenuSession,
) -> *mut VinputFcitxMenuProjection {
    let text = VinputFcitxAsrMenuTextView {
        local: string_view(b"Local"),
        remote: string_view(b"Remote"),
        command: string_view(b"Command"),
        loading_suffix: string_view(b" (loading)"),
        unavailable: string_view(b"unavailable"),
        loading_prefix: string_view(b"Loading: "),
        error_prefix: string_view(b"Error: "),
    };
    // SAFETY: Test byte slices outlive the projection constructor call.
    unsafe { vinput_fcitx_asr_menu_controller_projection_new(controller, filter, &raw const text) }
}

fn string_view(value: &[u8]) -> VinputFcitxStringView {
    VinputFcitxStringView {
        data: value.as_ptr(),
        len: value.len(),
    }
}

fn filter(query: &str) -> *mut crate::menu::VinputFcitxMenuSession {
    let mut filter = MenuFilterState::default();
    if !query.is_empty() {
        filter.activate();
        filter.append_text(query);
    }
    boxed_menu_session(filter)
}

fn controller(
    target_provider: &str,
    target_model: &str,
    effective_provider: &str,
    effective_model: &str,
    reload: bool,
    error: &str,
    rows: &[(&str, &str, &str, &str, &str)],
) -> *mut VinputFcitxAsrMenuController {
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
    boxed_asr_controller(Some(snapshot))
}

fn empty_string_view() -> VinputFcitxStringView {
    VinputFcitxStringView {
        data: ptr::null(),
        len: 0,
    }
}

#[test]
fn projects_localized_rows_and_effective_label_from_controller() {
    // SAFETY: Both handles are live for all calls and freed exactly once.
    unsafe {
        let controller = controller(
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
        let projection = menu_projection(controller, filter);
        assert!(!projection.is_null());
        let mut summary = VinputFcitxMenuProjectionView {
            summary: empty_string_view(),
            item_count: 0,
        };
        assert_eq!(
            vinput_fcitx_menu_projection_view(projection, &raw mut summary),
            1,
        );
        assert_eq!(bytes(summary.summary), b"Moonshine English");
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
            vinput_fcitx_menu_projection_item_view(projection, 0, &raw mut item),
            1,
        );
        assert_eq!(bytes(item.label), b"Paraformer Chinese [Local]");
        assert_eq!(item.control_kind, 2);
        assert_eq!(bytes(item.control_first), b"sherpa");
        assert_eq!(bytes(item.control_second), b"paraformer-zh");
        assert_eq!(bytes(item.control_label), b"Paraformer Chinese");

        vinput_fcitx_menu_projection_free(projection);
        vinput_fcitx_menu_session_free(filter);
        vinput_fcitx_asr_menu_controller_free(controller);
    }
}

#[test]
fn renders_loading_row_and_current_backend_summary() {
    // SAFETY: Both handles are live for all calls and freed exactly once.
    unsafe {
        let controller = controller(
            "sherpa",
            "requested",
            "sherpa",
            "legacy",
            true,
            "reload failed",
            &[("sherpa", "local", "requested", "Requested", "requested")],
        );
        let filter = filter("");
        let projection = menu_projection(controller, filter);
        assert!(!projection.is_null());
        let mut summary = VinputFcitxMenuProjectionView {
            summary: empty_string_view(),
            item_count: 0,
        };
        assert_eq!(
            vinput_fcitx_menu_projection_view(projection, &raw mut summary),
            1,
        );
        assert_eq!(
            bytes(summary.summary),
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
            vinput_fcitx_menu_projection_item_view(projection, 0, &raw mut item),
            1,
        );
        assert_eq!(bytes(item.label), b"Requested [Local] (loading)");

        vinput_fcitx_menu_projection_free(projection);
        vinput_fcitx_menu_session_free(filter);
        vinput_fcitx_asr_menu_controller_free(controller);
    }
}

#[test]
fn invalid_localized_fragment_rejects_projection() {
    // SAFETY: The controller is live for the constructor call and freed exactly once.
    unsafe {
        let controller = controller("", "", "", "", false, "", &[]);
        let filter = filter("");
        let invalid = [0xff];
        let text = VinputFcitxAsrMenuTextView {
            local: string_view(&invalid),
            remote: string_view(b"Remote"),
            command: string_view(b"Command"),
            loading_suffix: string_view(b" (loading)"),
            unavailable: string_view(b"unavailable"),
            loading_prefix: string_view(b"Loading: "),
            error_prefix: string_view(b"Error: "),
        };
        let projection =
            vinput_fcitx_asr_menu_controller_projection_new(controller, filter, &raw const text);
        assert!(projection.is_null());
        vinput_fcitx_menu_session_free(filter);
        vinput_fcitx_asr_menu_controller_free(controller);
    }
}

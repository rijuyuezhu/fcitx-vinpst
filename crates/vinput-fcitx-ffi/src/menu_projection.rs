//! Shared C ABI for finalized Rust-owned menu projections.

use std::panic::{AssertUnwindSafe, catch_unwind};

use vinput_fcitx_core::{MenuControl, ProjectedMenuItem};

use crate::ffi_string::{VinputFcitxStringView, string_view};

/// Opaque finalized menu projection shared by Scene and ASR menus.
pub struct VinputFcitxMenuProjection {
    pub(crate) summary: String,
    pub(crate) items: Vec<ProjectedMenuItem>,
}

/// Borrowed generic menu projection summary.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxMenuProjectionView {
    /// Fully rendered current-selection summary.
    pub summary: VinputFcitxStringView,
    /// Number of visible projected rows.
    pub item_count: usize,
}

/// Borrowed projected row.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxProjectedMenuItemView {
    /// Fully rendered label.
    pub label: VinputFcitxStringView,
    /// Stable `VINPUT_FCITX_MENU_CONTROL_*` value.
    pub control_kind: u8,
    /// Scene id or ASR provider id.
    pub control_first: VinputFcitxStringView,
    /// Empty for scenes, model value for ASR targets.
    pub control_second: VinputFcitxStringView,
    /// User-visible base label for notifications.
    pub control_label: VinputFcitxStringView,
}

const MENU_CONTROL_SET_ACTIVE_SCENE: u8 = 1;
const MENU_CONTROL_SET_ACTIVE_ASR_TARGET: u8 = 2;

pub(crate) fn projected_item_view(item: &ProjectedMenuItem) -> VinputFcitxProjectedMenuItemView {
    let (control_kind, control_first, control_second, control_label) = match &item.control {
        MenuControl::SetActiveScene {
            scene_id,
            display_label,
        } => (
            MENU_CONTROL_SET_ACTIVE_SCENE,
            string_view(scene_id),
            string_view(""),
            string_view(display_label),
        ),
        MenuControl::SetActiveAsrTarget {
            provider_id,
            model_value,
            display_label,
        } => (
            MENU_CONTROL_SET_ACTIVE_ASR_TARGET,
            string_view(provider_id),
            string_view(model_value),
            string_view(display_label),
        ),
    };
    VinputFcitxProjectedMenuItemView {
        label: string_view(&item.label),
        control_kind,
        control_first,
        control_second,
        control_label,
    }
}

/// Releases a shared menu projection.
///
/// # Safety
///
/// A non-null pointer must be a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_menu_projection_free(
    projection: *mut VinputFcitxMenuProjection,
) {
    if !projection.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            drop(unsafe { Box::from_raw(projection) });
        }));
    }
}

/// Borrows the shared projection summary.
///
/// # Safety
///
/// `projection` must be live and `view_out` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_menu_projection_view(
    projection: *const VinputFcitxMenuProjection,
    view_out: *mut VinputFcitxMenuProjectionView,
) -> u8 {
    crate::ffi_catch(0, || {
        if view_out.is_null() {
            return 0;
        }
        // SAFETY: Forwarded from this function's caller contract.
        let Some(projection) = (unsafe { projection.as_ref() }) else {
            return 0;
        };
        // SAFETY: The caller guarantees a writable output pointer.
        unsafe {
            view_out.write(VinputFcitxMenuProjectionView {
                summary: string_view(&projection.summary),
                item_count: projection.items.len(),
            });
        }
        1
    })
}

/// Borrows one row from a shared menu projection.
///
/// # Safety
///
/// `projection` must be live and `view_out` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_menu_projection_item_view(
    projection: *const VinputFcitxMenuProjection,
    index: usize,
    view_out: *mut VinputFcitxProjectedMenuItemView,
) -> u8 {
    crate::ffi_catch(0, || {
        if view_out.is_null() {
            return 0;
        }
        // SAFETY: Forwarded from this function's caller contract.
        let Some(item) = (unsafe { projection.as_ref() }).and_then(|value| value.items.get(index))
        else {
            return 0;
        };
        // SAFETY: The caller guarantees a writable output pointer.
        unsafe { view_out.write(projected_item_view(item)) };
        1
    })
}

#[cfg(test)]
mod tests;

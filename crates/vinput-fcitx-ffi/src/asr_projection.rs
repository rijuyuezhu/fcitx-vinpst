//! Thin C ABI for projecting a Rust-owned ASR display snapshot.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use vinput_fcitx_core::{
    AsrDisplaySnapshot, AsrDisplayText, AsrMenuItem, AsrMenuProjectionState, MenuControl,
    MenuFilterState, ProjectedMenuItem, project_asr_menu,
};

use crate::{
    frontend::VinputFcitxStringView,
    menu_snapshot::{VinputFcitxAsrDisplaySnapshot, asr_core_ref},
};

/// Opaque ASR projection state and result.
pub struct VinputFcitxAsrProjection {
    effective_label: String,
    projection: Vec<ProjectedMenuItem>,
}

/// Borrowed projection summary.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxProjectionView {
    /// Fully rendered effective-backend summary.
    pub effective_label: VinputFcitxStringView,
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

unsafe fn text_input<'a>(data: *const u8, len: usize) -> Option<&'a str> {
    if data.is_null() {
        return (len == 0).then_some("");
    }
    // SAFETY: Forwarded from each exported function's caller contract.
    std::str::from_utf8(unsafe { std::slice::from_raw_parts(data, len) }).ok()
}

fn string_view(value: &str) -> VinputFcitxStringView {
    VinputFcitxStringView {
        data: if value.is_empty() {
            ptr::null()
        } else {
            value.as_ptr()
        },
        len: value.len(),
    }
}

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

fn filter_from_query(query: &str) -> MenuFilterState {
    let mut filter = MenuFilterState::default();
    filter.activate();
    filter.append_text(query);
    filter
}

fn projection_state(snapshot: &AsrDisplaySnapshot) -> AsrMenuProjectionState {
    AsrMenuProjectionState {
        target_provider_id: snapshot.target_provider_id().to_owned(),
        target_model_id: snapshot.target_model_id().to_owned(),
        effective_provider_id: snapshot.effective_provider_id().to_owned(),
        effective_model_id: snapshot.effective_model_id().to_owned(),
        reload_in_progress: snapshot.reload_in_progress(),
        last_error: snapshot.last_error().to_owned(),
    }
}

/// Creates a localized ASR projection from a Rust-owned snapshot.
///
/// # Safety
///
/// `snapshot` must be live and every input pointer must reference its declared length.
#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_projection_new(
    snapshot: *const VinputFcitxAsrDisplaySnapshot,
    query_data: *const u8,
    query_len: usize,
    local_data: *const u8,
    local_len: usize,
    remote_data: *const u8,
    remote_len: usize,
    command_data: *const u8,
    command_len: usize,
    loading_suffix_data: *const u8,
    loading_suffix_len: usize,
    unavailable_data: *const u8,
    unavailable_len: usize,
    loading_prefix_data: *const u8,
    loading_prefix_len: usize,
    error_prefix_data: *const u8,
    error_prefix_len: usize,
) -> *mut VinputFcitxAsrProjection {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(snapshot) = (unsafe { asr_core_ref(snapshot) }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(query) = (unsafe { text_input(query_data, query_len) }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(local) = (unsafe { text_input(local_data, local_len) }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(remote) = (unsafe { text_input(remote_data, remote_len) }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(command) = (unsafe { text_input(command_data, command_len) }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(loading_suffix) = (unsafe { text_input(loading_suffix_data, loading_suffix_len) })
        else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(unavailable) = (unsafe { text_input(unavailable_data, unavailable_len) }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(loading_prefix) = (unsafe { text_input(loading_prefix_data, loading_prefix_len) })
        else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(error_prefix) = (unsafe { text_input(error_prefix_data, error_prefix_len) })
        else {
            return ptr::null_mut();
        };
        let text = AsrDisplayText {
            local,
            remote,
            command,
            loading_suffix,
            unavailable,
            loading_prefix,
            error_prefix,
        };
        let filter = filter_from_query(query);
        let targets = snapshot
            .targets()
            .iter()
            .map(|target| AsrMenuItem {
                provider_id: target.provider_id.clone(),
                kind: target.kind.clone(),
                item_id: target.item_id.clone(),
                display_title: target.display_title.clone(),
                model_value: target.model_value.clone(),
                rendered_label: snapshot.render_target_label(target, &text),
            })
            .collect::<Vec<_>>();
        Box::into_raw(Box::new(VinputFcitxAsrProjection {
            effective_label: snapshot.render_effective_label(&text),
            projection: project_asr_menu(&projection_state(snapshot), &targets, &filter),
        }))
    }))
    .unwrap_or(ptr::null_mut())
}

/// Releases an ASR projection.
///
/// # Safety
///
/// A non-null pointer must be a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_projection_free(
    projection: *mut VinputFcitxAsrProjection,
) {
    if !projection.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            drop(unsafe { Box::from_raw(projection) });
        }));
    }
}

/// Borrows the projection summary.
///
/// # Safety
///
/// `projection` must be live and `view_out` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_projection_view(
    projection: *const VinputFcitxAsrProjection,
    view_out: *mut VinputFcitxProjectionView,
) -> u8 {
    if view_out.is_null() {
        return 0;
    }
    // SAFETY: Forwarded from this function's caller contract.
    let Some(projection) = (unsafe { projection.as_ref() }) else {
        return 0;
    };
    // SAFETY: The caller guarantees a writable output pointer.
    unsafe {
        view_out.write(VinputFcitxProjectionView {
            effective_label: string_view(&projection.effective_label),
            item_count: projection.projection.len(),
        });
    }
    1
}

/// Borrows one projected row.
///
/// # Safety
///
/// `projection` must be live and `view_out` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_projection_item_view(
    projection: *const VinputFcitxAsrProjection,
    index: usize,
    view_out: *mut VinputFcitxProjectedMenuItemView,
) -> u8 {
    if view_out.is_null() {
        return 0;
    }
    // SAFETY: Forwarded from this function's caller contract.
    let Some(item) = (unsafe { projection.as_ref() }).and_then(|value| value.projection.get(index))
    else {
        return 0;
    };
    // SAFETY: The caller guarantees a writable output pointer.
    unsafe {
        view_out.write(projected_item_view(item));
    }
    1
}

#[cfg(test)]
mod tests;

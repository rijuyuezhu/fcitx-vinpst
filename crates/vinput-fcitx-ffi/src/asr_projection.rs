//! Thin C ABI for projecting a Rust-owned ASR display snapshot.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use vinput_fcitx_core::{
    AsrDisplaySnapshot, AsrMenuItem, AsrMenuProjectionState, MenuControl, MenuFilterState,
    ProjectedMenuItem, project_asr_menu,
};

use crate::{
    frontend::VinputFcitxStringView,
    menu_snapshot::{VinputFcitxAsrDisplaySnapshot, asr_core_ref},
};

/// Opaque ASR projection state and result.
pub struct VinputFcitxAsrProjection {
    snapshot: AsrDisplaySnapshot,
    filter: MenuFilterState,
    labels: Vec<Option<String>>,
    projection: Option<Vec<ProjectedMenuItem>>,
}

/// Borrowed projection summary.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxProjectionView {
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

/// Creates an ASR projection from a Rust-owned snapshot.
///
/// # Safety
///
/// `snapshot` must be live and query bytes must be readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_projection_new(
    snapshot: *const VinputFcitxAsrDisplaySnapshot,
    query_data: *const u8,
    query_len: usize,
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
        Box::into_raw(Box::new(VinputFcitxAsrProjection {
            snapshot: snapshot.clone(),
            filter: filter_from_query(query),
            labels: vec![None; snapshot.targets().len()],
            projection: None,
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

/// Supplies one gettext-rendered row label.
///
/// # Safety
///
/// `projection` must be live and label bytes must be readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_projection_set_label(
    projection: *mut VinputFcitxAsrProjection,
    row_index: usize,
    label_data: *const u8,
    label_len: usize,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(projection) = (unsafe { projection.as_mut() }) else {
                return false;
            };
            if projection.projection.is_some() {
                return false;
            }
            // SAFETY: Forwarded from this function's caller contract.
            let Some(label) = (unsafe { text_input(label_data, label_len) }) else {
                return false;
            };
            let Some(slot) = projection.labels.get_mut(row_index) else {
                return false;
            };
            *slot = Some(label.to_owned());
            true
        }))
        .unwrap_or(false),
    )
}

/// Finalizes the ASR projection.
///
/// # Safety
///
/// `projection` must be null or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_projection_finish(
    projection: *mut VinputFcitxAsrProjection,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(projection) = (unsafe { projection.as_mut() }) else {
                return false;
            };
            if projection.projection.is_some() {
                return true;
            }
            if projection.labels.iter().any(Option::is_none) {
                return false;
            }
            let targets = projection
                .snapshot
                .targets()
                .iter()
                .zip(&projection.labels)
                .map(|(target, label)| AsrMenuItem {
                    provider_id: target.provider_id.clone(),
                    kind: target.kind.clone(),
                    item_id: target.item_id.clone(),
                    display_title: target.display_title.clone(),
                    model_value: target.model_value.clone(),
                    rendered_label: label.as_deref().unwrap_or_default().to_owned(),
                })
                .collect::<Vec<_>>();
            projection.projection = Some(project_asr_menu(
                &projection_state(&projection.snapshot),
                &targets,
                &projection.filter,
            ));
            true
        }))
        .unwrap_or(false),
    )
}

/// Borrows the finalized projection summary.
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
    let Some(items) = (unsafe { projection.as_ref() }).and_then(|value| value.projection.as_ref())
    else {
        return 0;
    };
    // SAFETY: The caller guarantees a writable output pointer.
    unsafe {
        view_out.write(VinputFcitxProjectionView {
            item_count: items.len(),
        });
    }
    1
}

/// Borrows one finalized projected row.
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
    let Some(item) = (unsafe { projection.as_ref() })
        .and_then(|value| value.projection.as_ref())
        .and_then(|items| items.get(index))
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

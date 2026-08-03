//! Thin C ABI for projecting a Rust-owned scene snapshot.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use vinput_fcitx_core::{MenuFilterState, SceneMenuItem, SceneMenuProjection, project_scene_menu};

use crate::{
    asr_projection::{VinputFcitxProjectedMenuItemView, projected_item_view},
    frontend::VinputFcitxStringView,
    menu_snapshot::{VinputFcitxSceneSnapshot, scene_core_ref},
};

/// Opaque finalized scene projection.
pub struct VinputFcitxSceneProjection {
    projection: SceneMenuProjection,
}

/// Borrowed scene projection summary.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxSceneProjectionView {
    /// Active scene label with stable-id fallback.
    pub active_label: VinputFcitxStringView,
    /// Number of visible projected rows.
    pub item_count: usize,
}

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

fn filter_from_query(query: &str) -> MenuFilterState {
    let mut filter = MenuFilterState::default();
    filter.activate();
    filter.append_text(query);
    filter
}

/// Creates and finalizes a projection from a Rust-owned scene snapshot.
///
/// # Safety
///
/// `snapshot` must be live and query bytes must be readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_projection_new(
    snapshot: *const VinputFcitxSceneSnapshot,
    query_data: *const u8,
    query_len: usize,
) -> *mut VinputFcitxSceneProjection {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(snapshot) = (unsafe { scene_core_ref(snapshot) }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(query) = (unsafe { text_input(query_data, query_len) }) else {
            return ptr::null_mut();
        };
        let scenes = snapshot
            .scenes()
            .iter()
            .map(|scene| SceneMenuItem {
                id: scene.id.clone(),
                label: scene.label.clone(),
            })
            .collect::<Vec<_>>();
        let projection = project_scene_menu(
            snapshot.active_scene_id(),
            &scenes,
            &filter_from_query(query),
        );
        Box::into_raw(Box::new(VinputFcitxSceneProjection { projection }))
    }))
    .unwrap_or(ptr::null_mut())
}

/// Releases a scene projection.
///
/// # Safety
///
/// A non-null pointer must be a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_projection_free(
    projection: *mut VinputFcitxSceneProjection,
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
pub unsafe extern "C" fn vinput_fcitx_scene_projection_view(
    projection: *const VinputFcitxSceneProjection,
    view_out: *mut VinputFcitxSceneProjectionView,
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
        view_out.write(VinputFcitxSceneProjectionView {
            active_label: string_view(&projection.projection.active_label),
            item_count: projection.projection.items.len(),
        });
    }
    1
}

/// Borrows one projected scene row.
///
/// # Safety
///
/// `projection` must be live and `view_out` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_projection_item_view(
    projection: *const VinputFcitxSceneProjection,
    index: usize,
    view_out: *mut VinputFcitxProjectedMenuItemView,
) -> u8 {
    if view_out.is_null() {
        return 0;
    }
    // SAFETY: Forwarded from this function's caller contract.
    let Some(item) = (unsafe { projection.as_ref() })
        .and_then(|projection| projection.projection.items.get(index))
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

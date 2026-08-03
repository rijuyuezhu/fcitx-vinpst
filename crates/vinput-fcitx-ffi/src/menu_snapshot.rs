//! Compact C views over Rust-owned Scene and ASR display snapshots.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use vinput_fcitx_core::{
    AsrDisplaySnapshot, AsrDisplaySnapshotItem, SceneSnapshot, SceneSnapshotItem,
};

use crate::frontend::VinputFcitxStringView;

/// Opaque Rust-owned scene snapshot.
pub struct VinputFcitxSceneSnapshot {
    snapshot: SceneSnapshot,
}

/// Opaque Rust-owned ASR display snapshot.
pub struct VinputFcitxAsrDisplaySnapshot {
    snapshot: AsrDisplaySnapshot,
}

/// Borrowed scene snapshot summary.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxSceneSnapshotView {
    /// Active scene identifier.
    pub active_scene_id: VinputFcitxStringView,
    /// Active scene label with stable-id fallback.
    pub active_label: VinputFcitxStringView,
    /// Number of scene rows.
    pub item_count: usize,
}

/// Borrowed scene row.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxSceneSnapshotItemView {
    /// Stable scene identifier.
    pub id: VinputFcitxStringView,
    /// User-visible scene label.
    pub label: VinputFcitxStringView,
}

/// Borrowed ASR display snapshot summary.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxAsrDisplaySnapshotView {
    /// Requested provider identifier.
    pub target_provider_id: VinputFcitxStringView,
    /// Requested model identifier.
    pub target_model_id: VinputFcitxStringView,
    /// Effective provider identifier.
    pub effective_provider_id: VinputFcitxStringView,
    /// Effective model identifier.
    pub effective_model_id: VinputFcitxStringView,
    /// Last reload error.
    pub last_error: VinputFcitxStringView,
    /// Preferred effective-backend base label.
    pub effective_base_label: VinputFcitxStringView,
    /// Preferred requested-backend base label.
    pub target_base_label: VinputFcitxStringView,
    /// Whether backend reload is in progress.
    pub reload_in_progress: u8,
    /// Number of target rows.
    pub item_count: usize,
}

/// Borrowed ASR target row.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxAsrDisplaySnapshotItemView {
    /// Stable provider identifier.
    pub provider_id: VinputFcitxStringView,
    /// Provider implementation kind.
    pub kind: VinputFcitxStringView,
    /// Stable row identifier.
    pub item_id: VinputFcitxStringView,
    /// Registry or localized display title.
    pub display_title: VinputFcitxStringView,
    /// Concrete model value passed back to the daemon.
    pub model_value: VinputFcitxStringView,
    /// Preferred user-visible base label.
    pub base_label: VinputFcitxStringView,
    /// Whether this row is the requested backend currently loading.
    pub is_loading: u8,
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

pub(crate) unsafe fn scene_core_ref<'a>(
    snapshot: *const VinputFcitxSceneSnapshot,
) -> Option<&'a SceneSnapshot> {
    // SAFETY: Forwarded from the caller contract.
    unsafe { snapshot.as_ref() }.map(|value| &value.snapshot)
}

pub(crate) unsafe fn asr_core_ref<'a>(
    snapshot: *const VinputFcitxAsrDisplaySnapshot,
) -> Option<&'a AsrDisplaySnapshot> {
    // SAFETY: Forwarded from the caller contract.
    unsafe { snapshot.as_ref() }.map(|value| &value.snapshot)
}

/// Creates an empty scene snapshot.
///
/// # Safety
///
/// `active_scene_data` must reference `active_scene_len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_snapshot_new(
    active_scene_data: *const u8,
    active_scene_len: usize,
) -> *mut VinputFcitxSceneSnapshot {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(active_scene_id) = (unsafe { text_input(active_scene_data, active_scene_len) })
        else {
            return ptr::null_mut();
        };
        Box::into_raw(Box::new(VinputFcitxSceneSnapshot {
            snapshot: SceneSnapshot::new(active_scene_id.to_owned()),
        }))
    }))
    .unwrap_or(ptr::null_mut())
}

/// Releases a scene snapshot.
///
/// # Safety
///
/// A non-null pointer must be a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_snapshot_free(snapshot: *mut VinputFcitxSceneSnapshot) {
    if !snapshot.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            drop(unsafe { Box::from_raw(snapshot) });
        }));
    }
}

/// Appends one scene row.
///
/// # Safety
///
/// Input pointers must reference their declared lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_snapshot_add(
    snapshot: *mut VinputFcitxSceneSnapshot,
    id_data: *const u8,
    id_len: usize,
    label_data: *const u8,
    label_len: usize,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(snapshot) = (unsafe { snapshot.as_mut() }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(id) = (unsafe { text_input(id_data, id_len) }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(label) = (unsafe { text_input(label_data, label_len) }) else {
                return false;
            };
            snapshot.snapshot.push(id.to_owned(), label.to_owned());
            true
        }))
        .unwrap_or(false),
    )
}

/// Updates the active scene identifier.
///
/// # Safety
///
/// Input pointers must reference their declared lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_snapshot_set_active(
    snapshot: *mut VinputFcitxSceneSnapshot,
    active_scene_data: *const u8,
    active_scene_len: usize,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(snapshot) = (unsafe { snapshot.as_mut() }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(active) = (unsafe { text_input(active_scene_data, active_scene_len) }) else {
                return false;
            };
            snapshot.snapshot.set_active_scene_id(active.to_owned());
            true
        }))
        .unwrap_or(false),
    )
}

/// Borrows the scene snapshot summary.
///
/// # Safety
///
/// `snapshot` must be live and `view_out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_snapshot_view(
    snapshot: *const VinputFcitxSceneSnapshot,
    view_out: *mut VinputFcitxSceneSnapshotView,
) -> u8 {
    if view_out.is_null() {
        return 0;
    }
    // SAFETY: Forwarded from this function's caller contract.
    let Some(snapshot) = (unsafe { scene_core_ref(snapshot) }) else {
        return 0;
    };
    // SAFETY: The caller guarantees a writable output pointer.
    unsafe {
        view_out.write(VinputFcitxSceneSnapshotView {
            active_scene_id: string_view(snapshot.active_scene_id()),
            active_label: string_view(snapshot.active_label()),
            item_count: snapshot.scenes().len(),
        });
    }
    1
}

/// Borrows one scene row.
///
/// # Safety
///
/// `snapshot` must be live and `view_out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_snapshot_item_view(
    snapshot: *const VinputFcitxSceneSnapshot,
    index: usize,
    view_out: *mut VinputFcitxSceneSnapshotItemView,
) -> u8 {
    if view_out.is_null() {
        return 0;
    }
    // SAFETY: Forwarded from this function's caller contract.
    let Some(item) =
        (unsafe { scene_core_ref(snapshot) }).and_then(|snapshot| snapshot.scenes().get(index))
    else {
        return 0;
    };
    write_scene_item(item, view_out)
}

fn write_scene_item(item: &SceneSnapshotItem, out: *mut VinputFcitxSceneSnapshotItemView) -> u8 {
    // SAFETY: Callers validate `out` before entering this helper.
    unsafe {
        out.write(VinputFcitxSceneSnapshotItemView {
            id: string_view(&item.id),
            label: string_view(&item.label),
        });
    }
    1
}

/// Creates an empty ASR display snapshot.
///
/// # Safety
///
/// Every input pointer must reference its declared length.
#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_display_snapshot_new(
    target_provider_data: *const u8,
    target_provider_len: usize,
    target_model_data: *const u8,
    target_model_len: usize,
    effective_provider_data: *const u8,
    effective_provider_len: usize,
    effective_model_data: *const u8,
    effective_model_len: usize,
    reload_in_progress: u8,
    last_error_data: *const u8,
    last_error_len: usize,
) -> *mut VinputFcitxAsrDisplaySnapshot {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(target_provider) =
            (unsafe { text_input(target_provider_data, target_provider_len) })
        else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(target_model) = (unsafe { text_input(target_model_data, target_model_len) })
        else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(effective_provider) =
            (unsafe { text_input(effective_provider_data, effective_provider_len) })
        else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(effective_model) =
            (unsafe { text_input(effective_model_data, effective_model_len) })
        else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(last_error) = (unsafe { text_input(last_error_data, last_error_len) }) else {
            return ptr::null_mut();
        };
        Box::into_raw(Box::new(VinputFcitxAsrDisplaySnapshot {
            snapshot: AsrDisplaySnapshot::new(
                target_provider.to_owned(),
                target_model.to_owned(),
                effective_provider.to_owned(),
                effective_model.to_owned(),
                reload_in_progress != 0,
                last_error.to_owned(),
            ),
        }))
    }))
    .unwrap_or(ptr::null_mut())
}

/// Releases an ASR display snapshot.
///
/// # Safety
///
/// A non-null pointer must be a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_display_snapshot_free(
    snapshot: *mut VinputFcitxAsrDisplaySnapshot,
) {
    if !snapshot.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            drop(unsafe { Box::from_raw(snapshot) });
        }));
    }
}

/// Appends one ASR display row.
///
/// # Safety
///
/// Every input pointer must reference its declared length.
#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_display_snapshot_add(
    snapshot: *mut VinputFcitxAsrDisplaySnapshot,
    provider_data: *const u8,
    provider_len: usize,
    kind_data: *const u8,
    kind_len: usize,
    item_id_data: *const u8,
    item_id_len: usize,
    display_title_data: *const u8,
    display_title_len: usize,
    model_value_data: *const u8,
    model_value_len: usize,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(snapshot) = (unsafe { snapshot.as_mut() }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(provider_id) = (unsafe { text_input(provider_data, provider_len) }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(kind) = (unsafe { text_input(kind_data, kind_len) }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(item_id) = (unsafe { text_input(item_id_data, item_id_len) }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(display_title) =
                (unsafe { text_input(display_title_data, display_title_len) })
            else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(model_value) = (unsafe { text_input(model_value_data, model_value_len) })
            else {
                return false;
            };
            snapshot.snapshot.push(AsrDisplaySnapshotItem {
                provider_id: provider_id.to_owned(),
                kind: kind.to_owned(),
                item_id: item_id.to_owned(),
                display_title: display_title.to_owned(),
                model_value: model_value.to_owned(),
            });
            true
        }))
        .unwrap_or(false),
    )
}

/// Borrows the ASR display snapshot summary.
///
/// # Safety
///
/// `snapshot` must be live and `view_out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_display_snapshot_view(
    snapshot: *const VinputFcitxAsrDisplaySnapshot,
    view_out: *mut VinputFcitxAsrDisplaySnapshotView,
) -> u8 {
    if view_out.is_null() {
        return 0;
    }
    // SAFETY: Forwarded from this function's caller contract.
    let Some(snapshot) = (unsafe { asr_core_ref(snapshot) }) else {
        return 0;
    };
    // SAFETY: The caller guarantees a writable output pointer.
    unsafe {
        view_out.write(VinputFcitxAsrDisplaySnapshotView {
            target_provider_id: string_view(snapshot.target_provider_id()),
            target_model_id: string_view(snapshot.target_model_id()),
            effective_provider_id: string_view(snapshot.effective_provider_id()),
            effective_model_id: string_view(snapshot.effective_model_id()),
            last_error: string_view(snapshot.last_error()),
            effective_base_label: string_view(snapshot.effective_base_label()),
            target_base_label: string_view(snapshot.target_base_label()),
            reload_in_progress: u8::from(snapshot.reload_in_progress()),
            item_count: snapshot.targets().len(),
        });
    }
    1
}

/// Borrows one ASR display row.
///
/// # Safety
///
/// `snapshot` must be live and `view_out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_display_snapshot_item_view(
    snapshot: *const VinputFcitxAsrDisplaySnapshot,
    index: usize,
    view_out: *mut VinputFcitxAsrDisplaySnapshotItemView,
) -> u8 {
    if view_out.is_null() {
        return 0;
    }
    // SAFETY: Forwarded from this function's caller contract.
    let Some(snapshot) = (unsafe { asr_core_ref(snapshot) }) else {
        return 0;
    };
    let Some(item) = snapshot.targets().get(index) else {
        return 0;
    };
    // SAFETY: The caller guarantees a writable output pointer.
    unsafe {
        view_out.write(VinputFcitxAsrDisplaySnapshotItemView {
            provider_id: string_view(&item.provider_id),
            kind: string_view(&item.kind),
            item_id: string_view(&item.item_id),
            display_title: string_view(&item.display_title),
            model_value: string_view(&item.model_value),
            base_label: string_view(item.base_label()),
            is_loading: u8::from(snapshot.is_loading_target(item)),
        });
    }
    1
}

#[cfg(test)]
mod tests;

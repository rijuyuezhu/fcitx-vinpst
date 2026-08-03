//! Raw-pointer C ABI for Rust-owned daemon menu snapshots.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use vinput_fcitx_core::{
    AsrDisplaySnapshot, AsrDisplaySnapshotItem, SceneSnapshot, SceneSnapshotItem,
};

/// Opaque Rust-owned scene snapshot.
pub struct VinputFcitxSceneSnapshot {
    snapshot: SceneSnapshot,
}

/// Opaque Rust-owned ASR display-menu snapshot.
pub struct VinputFcitxAsrDisplaySnapshot {
    snapshot: AsrDisplaySnapshot,
}

unsafe fn text_input<'a>(data: *const u8, len: usize) -> Option<&'a str> {
    if data.is_null() {
        return (len == 0).then_some("");
    }
    // SAFETY: The caller guarantees that `data` points to `len` readable bytes.
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    std::str::from_utf8(bytes).ok()
}

fn string_data(value: &str) -> *const u8 {
    if value.is_empty() {
        ptr::null()
    } else {
        value.as_ptr()
    }
}

unsafe fn scene_ref<'a>(
    snapshot: *const VinputFcitxSceneSnapshot,
) -> Option<&'a VinputFcitxSceneSnapshot> {
    // SAFETY: Forwarded from each exported function's caller contract.
    unsafe { snapshot.as_ref() }
}

unsafe fn scene_mut<'a>(
    snapshot: *mut VinputFcitxSceneSnapshot,
) -> Option<&'a mut VinputFcitxSceneSnapshot> {
    // SAFETY: Forwarded from each exported function's caller contract.
    unsafe { snapshot.as_mut() }
}

unsafe fn asr_ref<'a>(
    snapshot: *const VinputFcitxAsrDisplaySnapshot,
) -> Option<&'a VinputFcitxAsrDisplaySnapshot> {
    // SAFETY: Forwarded from each exported function's caller contract.
    unsafe { snapshot.as_ref() }
}

unsafe fn asr_mut<'a>(
    snapshot: *mut VinputFcitxAsrDisplaySnapshot,
) -> Option<&'a mut VinputFcitxAsrDisplaySnapshot> {
    // SAFETY: Forwarded from each exported function's caller contract.
    unsafe { snapshot.as_mut() }
}

pub(crate) unsafe fn scene_core_ref<'a>(
    snapshot: *const VinputFcitxSceneSnapshot,
) -> Option<&'a SceneSnapshot> {
    // SAFETY: Forwarded from the internal caller's exported-function contract.
    unsafe { scene_ref(snapshot) }.map(|snapshot| &snapshot.snapshot)
}

pub(crate) unsafe fn asr_core_ref<'a>(
    snapshot: *const VinputFcitxAsrDisplaySnapshot,
) -> Option<&'a AsrDisplaySnapshot> {
    // SAFETY: Forwarded from the internal caller's exported-function contract.
    unsafe { asr_ref(snapshot) }.map(|snapshot| &snapshot.snapshot)
}

fn scene_item(snapshot: &VinputFcitxSceneSnapshot, index: usize) -> Option<&SceneSnapshotItem> {
    snapshot.snapshot.scenes().get(index)
}

fn asr_item(
    snapshot: &VinputFcitxAsrDisplaySnapshot,
    index: usize,
) -> Option<&AsrDisplaySnapshotItem> {
    snapshot.snapshot.targets().get(index)
}

fn scene_active_id(snapshot: &VinputFcitxSceneSnapshot) -> &str {
    snapshot.snapshot.active_scene_id()
}

fn scene_active_label(snapshot: &VinputFcitxSceneSnapshot) -> &str {
    snapshot.snapshot.active_label()
}

fn asr_target_provider(snapshot: &VinputFcitxAsrDisplaySnapshot) -> &str {
    snapshot.snapshot.target_provider_id()
}

fn asr_target_model(snapshot: &VinputFcitxAsrDisplaySnapshot) -> &str {
    snapshot.snapshot.target_model_id()
}

fn asr_effective_provider(snapshot: &VinputFcitxAsrDisplaySnapshot) -> &str {
    snapshot.snapshot.effective_provider_id()
}

fn asr_effective_model(snapshot: &VinputFcitxAsrDisplaySnapshot) -> &str {
    snapshot.snapshot.effective_model_id()
}

fn asr_last_error(snapshot: &VinputFcitxAsrDisplaySnapshot) -> &str {
    snapshot.snapshot.last_error()
}

fn asr_effective_base_label(snapshot: &VinputFcitxAsrDisplaySnapshot) -> &str {
    snapshot.snapshot.effective_base_label()
}

fn asr_target_base_label(snapshot: &VinputFcitxAsrDisplaySnapshot) -> &str {
    snapshot.snapshot.target_base_label()
}

fn asr_item_provider(item: &AsrDisplaySnapshotItem) -> &str {
    &item.provider_id
}

fn asr_item_kind(item: &AsrDisplaySnapshotItem) -> &str {
    &item.kind
}

fn asr_item_id(item: &AsrDisplaySnapshotItem) -> &str {
    &item.item_id
}

fn asr_item_display_title(item: &AsrDisplaySnapshotItem) -> &str {
    &item.display_title
}

fn asr_item_model_value(item: &AsrDisplaySnapshotItem) -> &str {
    &item.model_value
}

/// Creates a scene snapshot with no rows.
///
/// Invalid UTF-8 returns null.
///
/// # Safety
///
/// `active_scene_data` must point to `active_scene_len` readable bytes unless
/// both are null/zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_snapshot_new(
    active_scene_data: *const u8,
    active_scene_len: usize,
) -> *mut VinputFcitxSceneSnapshot {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let active_scene = unsafe { text_input(active_scene_data, active_scene_len) }?;
        Some(Box::into_raw(Box::new(VinputFcitxSceneSnapshot {
            snapshot: SceneSnapshot::new(active_scene.to_owned()),
        })))
    }))
    .ok()
    .flatten()
    .unwrap_or(ptr::null_mut())
}

/// Releases a scene snapshot handle.
///
/// # Safety
///
/// A non-null pointer must be a live handle returned by this crate and must not
/// be freed more than once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_snapshot_free(snapshot: *mut VinputFcitxSceneSnapshot) {
    if snapshot.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        drop(unsafe { Box::from_raw(snapshot) });
    }));
}

/// Appends one scene row.
///
/// Invalid UTF-8 returns zero without mutating the snapshot.
///
/// # Safety
///
/// Input pointers must refer to their declared readable byte lengths.
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
            let Some(id) = (unsafe { text_input(id_data, id_len) }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(label) = (unsafe { text_input(label_data, label_len) }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(snapshot) = (unsafe { scene_mut(snapshot) }) else {
                return false;
            };
            snapshot.snapshot.push(id.to_owned(), label.to_owned());
            true
        }))
        .unwrap_or(false),
    )
}

/// Updates the active scene id.
///
/// Invalid UTF-8 returns zero without mutating the snapshot.
///
/// # Safety
///
/// `active_scene_data` must point to `active_scene_len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_snapshot_set_active(
    snapshot: *mut VinputFcitxSceneSnapshot,
    active_scene_data: *const u8,
    active_scene_len: usize,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(active_scene) = (unsafe { text_input(active_scene_data, active_scene_len) })
            else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(snapshot) = (unsafe { scene_mut(snapshot) }) else {
                return false;
            };
            snapshot
                .snapshot
                .set_active_scene_id(active_scene.to_owned());
            true
        }))
        .unwrap_or(false),
    )
}

macro_rules! scene_string_view {
    ($data_name:ident, $len_name:ident, $value:path) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $data_name(
            snapshot: *const VinputFcitxSceneSnapshot,
        ) -> *const u8 {
            // SAFETY: Forwarded from this function's caller contract.
            unsafe { scene_ref(snapshot) }
                .map_or(ptr::null(), |snapshot| string_data($value(snapshot)))
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $len_name(snapshot: *const VinputFcitxSceneSnapshot) -> usize {
            // SAFETY: Forwarded from this function's caller contract.
            unsafe { scene_ref(snapshot) }.map_or(0, |snapshot| $value(snapshot).len())
        }
    };
}

scene_string_view!(
    vinput_fcitx_scene_snapshot_active_id_data,
    vinput_fcitx_scene_snapshot_active_id_len,
    scene_active_id
);
scene_string_view!(
    vinput_fcitx_scene_snapshot_active_label_data,
    vinput_fcitx_scene_snapshot_active_label_len,
    scene_active_label
);

/// Returns the number of scene rows.
///
/// # Safety
///
/// `snapshot` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_snapshot_item_count(
    snapshot: *const VinputFcitxSceneSnapshot,
) -> usize {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { scene_ref(snapshot) }.map_or(0, |snapshot| snapshot.snapshot.scenes().len())
}

macro_rules! scene_item_string_view {
    ($data_name:ident, $len_name:ident, $field:ident) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $data_name(
            snapshot: *const VinputFcitxSceneSnapshot,
            index: usize,
        ) -> *const u8 {
            // SAFETY: Forwarded from this function's caller contract.
            unsafe { scene_ref(snapshot) }
                .and_then(|snapshot| scene_item(snapshot, index))
                .map_or(ptr::null(), |item| string_data(&item.$field))
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $len_name(
            snapshot: *const VinputFcitxSceneSnapshot,
            index: usize,
        ) -> usize {
            // SAFETY: Forwarded from this function's caller contract.
            unsafe { scene_ref(snapshot) }
                .and_then(|snapshot| scene_item(snapshot, index))
                .map_or(0, |item| item.$field.len())
        }
    };
}

scene_item_string_view!(
    vinput_fcitx_scene_snapshot_item_id_data,
    vinput_fcitx_scene_snapshot_item_id_len,
    id
);
scene_item_string_view!(
    vinput_fcitx_scene_snapshot_item_label_data,
    vinput_fcitx_scene_snapshot_item_label_len,
    label
);

/// Creates an ASR display-menu snapshot with no rows.
///
/// Invalid UTF-8 returns null.
///
/// # Safety
///
/// Input pointers must refer to their declared readable byte lengths.
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
        let target_provider = unsafe { text_input(target_provider_data, target_provider_len) }?;
        // SAFETY: Forwarded from this function's caller contract.
        let target_model = unsafe { text_input(target_model_data, target_model_len) }?;
        // SAFETY: Forwarded from this function's caller contract.
        let effective_provider =
            unsafe { text_input(effective_provider_data, effective_provider_len) }?;
        // SAFETY: Forwarded from this function's caller contract.
        let effective_model = unsafe { text_input(effective_model_data, effective_model_len) }?;
        // SAFETY: Forwarded from this function's caller contract.
        let last_error = unsafe { text_input(last_error_data, last_error_len) }?;
        Some(Box::into_raw(Box::new(VinputFcitxAsrDisplaySnapshot {
            snapshot: AsrDisplaySnapshot::new(
                target_provider.to_owned(),
                target_model.to_owned(),
                effective_provider.to_owned(),
                effective_model.to_owned(),
                reload_in_progress != 0,
                last_error.to_owned(),
            ),
        })))
    }))
    .ok()
    .flatten()
    .unwrap_or(ptr::null_mut())
}

/// Releases an ASR display snapshot handle.
///
/// # Safety
///
/// A non-null pointer must be a live handle returned by this crate and must not
/// be freed more than once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_display_snapshot_free(
    snapshot: *mut VinputFcitxAsrDisplaySnapshot,
) {
    if snapshot.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        drop(unsafe { Box::from_raw(snapshot) });
    }));
}

/// Appends one ASR display-menu row.
///
/// Invalid UTF-8 returns zero without mutating the snapshot.
///
/// # Safety
///
/// Input pointers must refer to their declared readable byte lengths.
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
            // SAFETY: Forwarded from this function's caller contract.
            let Some(snapshot) = (unsafe { asr_mut(snapshot) }) else {
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

macro_rules! asr_string_view {
    ($data_name:ident, $len_name:ident, $value:path) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $data_name(
            snapshot: *const VinputFcitxAsrDisplaySnapshot,
        ) -> *const u8 {
            // SAFETY: Forwarded from this function's caller contract.
            unsafe { asr_ref(snapshot) }
                .map_or(ptr::null(), |snapshot| string_data($value(snapshot)))
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $len_name(
            snapshot: *const VinputFcitxAsrDisplaySnapshot,
        ) -> usize {
            // SAFETY: Forwarded from this function's caller contract.
            unsafe { asr_ref(snapshot) }.map_or(0, |snapshot| $value(snapshot).len())
        }
    };
}

asr_string_view!(
    vinput_fcitx_asr_display_snapshot_target_provider_data,
    vinput_fcitx_asr_display_snapshot_target_provider_len,
    asr_target_provider
);
asr_string_view!(
    vinput_fcitx_asr_display_snapshot_target_model_data,
    vinput_fcitx_asr_display_snapshot_target_model_len,
    asr_target_model
);
asr_string_view!(
    vinput_fcitx_asr_display_snapshot_effective_provider_data,
    vinput_fcitx_asr_display_snapshot_effective_provider_len,
    asr_effective_provider
);
asr_string_view!(
    vinput_fcitx_asr_display_snapshot_effective_model_data,
    vinput_fcitx_asr_display_snapshot_effective_model_len,
    asr_effective_model
);
asr_string_view!(
    vinput_fcitx_asr_display_snapshot_last_error_data,
    vinput_fcitx_asr_display_snapshot_last_error_len,
    asr_last_error
);
asr_string_view!(
    vinput_fcitx_asr_display_snapshot_effective_base_label_data,
    vinput_fcitx_asr_display_snapshot_effective_base_label_len,
    asr_effective_base_label
);
asr_string_view!(
    vinput_fcitx_asr_display_snapshot_target_base_label_data,
    vinput_fcitx_asr_display_snapshot_target_base_label_len,
    asr_target_base_label
);

/// Returns one when backend reload is in progress.
///
/// # Safety
///
/// `snapshot` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_display_snapshot_reload_in_progress(
    snapshot: *const VinputFcitxAsrDisplaySnapshot,
) -> u8 {
    // SAFETY: Forwarded from this function's caller contract.
    u8::from(
        unsafe { asr_ref(snapshot) }.is_some_and(|snapshot| snapshot.snapshot.reload_in_progress()),
    )
}

/// Returns the number of ASR target rows.
///
/// # Safety
///
/// `snapshot` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_display_snapshot_item_count(
    snapshot: *const VinputFcitxAsrDisplaySnapshot,
) -> usize {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { asr_ref(snapshot) }.map_or(0, |snapshot| snapshot.snapshot.targets().len())
}

macro_rules! asr_item_string_view {
    ($data_name:ident, $len_name:ident, $value:path) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $data_name(
            snapshot: *const VinputFcitxAsrDisplaySnapshot,
            index: usize,
        ) -> *const u8 {
            // SAFETY: Forwarded from this function's caller contract.
            unsafe { asr_ref(snapshot) }
                .and_then(|snapshot| asr_item(snapshot, index))
                .map_or(ptr::null(), |item| string_data($value(item)))
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $len_name(
            snapshot: *const VinputFcitxAsrDisplaySnapshot,
            index: usize,
        ) -> usize {
            // SAFETY: Forwarded from this function's caller contract.
            unsafe { asr_ref(snapshot) }
                .and_then(|snapshot| asr_item(snapshot, index))
                .map_or(0, |item| $value(item).len())
        }
    };
}

asr_item_string_view!(
    vinput_fcitx_asr_display_snapshot_item_provider_data,
    vinput_fcitx_asr_display_snapshot_item_provider_len,
    asr_item_provider
);
asr_item_string_view!(
    vinput_fcitx_asr_display_snapshot_item_kind_data,
    vinput_fcitx_asr_display_snapshot_item_kind_len,
    asr_item_kind
);
asr_item_string_view!(
    vinput_fcitx_asr_display_snapshot_item_id_data,
    vinput_fcitx_asr_display_snapshot_item_id_len,
    asr_item_id
);
asr_item_string_view!(
    vinput_fcitx_asr_display_snapshot_item_display_title_data,
    vinput_fcitx_asr_display_snapshot_item_display_title_len,
    asr_item_display_title
);
asr_item_string_view!(
    vinput_fcitx_asr_display_snapshot_item_model_value_data,
    vinput_fcitx_asr_display_snapshot_item_model_value_len,
    asr_item_model_value
);
asr_item_string_view!(
    vinput_fcitx_asr_display_snapshot_item_base_label_data,
    vinput_fcitx_asr_display_snapshot_item_base_label_len,
    AsrDisplaySnapshotItem::base_label
);

/// Returns one when the indexed row is the requested target currently loading.
///
/// # Safety
///
/// `snapshot` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_display_snapshot_item_is_loading(
    snapshot: *const VinputFcitxAsrDisplaySnapshot,
    index: usize,
) -> u8 {
    // SAFETY: Forwarded from this function's caller contract.
    u8::from(unsafe { asr_ref(snapshot) }.is_some_and(|snapshot| {
        asr_item(snapshot, index).is_some_and(|item| snapshot.snapshot.is_loading_target(item))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn bytes_from_view<'a>(data: *const u8, len: usize) -> &'a [u8] {
        if data.is_null() {
            return &[];
        }
        // SAFETY: Tests keep the owning snapshot alive for each view.
        unsafe { std::slice::from_raw_parts(data, len) }
    }

    #[test]
    fn exposes_scene_snapshot_rows_and_active_fallback() {
        // SAFETY: All input slices are live, and the handle is freed exactly once.
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
            assert_eq!(vinput_fcitx_scene_snapshot_item_count(snapshot), 1);
            assert_eq!(
                bytes_from_view(
                    vinput_fcitx_scene_snapshot_active_label_data(snapshot),
                    vinput_fcitx_scene_snapshot_active_label_len(snapshot),
                ),
                b"Meeting Notes"
            );
            assert_eq!(
                vinput_fcitx_scene_snapshot_set_active(snapshot, b"missing".as_ptr(), 7),
                1
            );
            assert_eq!(
                bytes_from_view(
                    vinput_fcitx_scene_snapshot_active_label_data(snapshot),
                    vinput_fcitx_scene_snapshot_active_label_len(snapshot),
                ),
                b"missing"
            );
            vinput_fcitx_scene_snapshot_free(snapshot);
        }
    }

    #[test]
    fn exposes_asr_snapshot_labels_and_loading_state() {
        // SAFETY: All input slices are live, and the handle is freed exactly once.
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
                    b"effective".as_ptr(),
                    9,
                    b"Effective Model".as_ptr(),
                    15,
                    b"effective".as_ptr(),
                    9,
                ),
                1
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
                    ptr::null(),
                    0,
                    b"requested".as_ptr(),
                    9,
                ),
                1
            );
            assert_eq!(vinput_fcitx_asr_display_snapshot_item_count(snapshot), 2);
            assert_eq!(
                bytes_from_view(
                    vinput_fcitx_asr_display_snapshot_effective_base_label_data(snapshot),
                    vinput_fcitx_asr_display_snapshot_effective_base_label_len(snapshot),
                ),
                b"Effective Model"
            );
            assert_eq!(
                vinput_fcitx_asr_display_snapshot_item_is_loading(snapshot, 0),
                0
            );
            assert_eq!(
                vinput_fcitx_asr_display_snapshot_item_is_loading(snapshot, 1),
                1
            );
            assert_eq!(
                bytes_from_view(
                    vinput_fcitx_asr_display_snapshot_item_base_label_data(snapshot, 1),
                    vinput_fcitx_asr_display_snapshot_item_base_label_len(snapshot, 1),
                ),
                b"requested"
            );
            vinput_fcitx_asr_display_snapshot_free(snapshot);
        }
    }

    #[test]
    fn invalid_row_utf8_does_not_mutate_snapshots() {
        let invalid = [0xff];
        // SAFETY: All input slices are live, and handles are freed exactly once.
        unsafe {
            let scene = vinput_fcitx_scene_snapshot_new(ptr::null(), 0);
            assert_eq!(
                vinput_fcitx_scene_snapshot_add(
                    scene,
                    invalid.as_ptr(),
                    invalid.len(),
                    b"label".as_ptr(),
                    5,
                ),
                0
            );
            assert_eq!(vinput_fcitx_scene_snapshot_item_count(scene), 0);
            vinput_fcitx_scene_snapshot_free(scene);

            let asr = vinput_fcitx_asr_display_snapshot_new(
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
                    asr,
                    invalid.as_ptr(),
                    invalid.len(),
                    ptr::null(),
                    0,
                    ptr::null(),
                    0,
                    ptr::null(),
                    0,
                    ptr::null(),
                    0,
                ),
                0
            );
            assert_eq!(vinput_fcitx_asr_display_snapshot_item_count(asr), 0);
            vinput_fcitx_asr_display_snapshot_free(asr);
        }
    }
}

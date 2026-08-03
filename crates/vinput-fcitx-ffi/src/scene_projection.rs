//! Raw-pointer C ABI for scene menu snapshot projection.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use vinput_fcitx_core::{MenuFilterState, SceneMenuItem, SceneMenuProjection, project_scene_menu};

use crate::menu_snapshot::{VinputFcitxSceneSnapshot, scene_core_ref};

/// Opaque scene snapshot builder and projection result owned by Rust.
pub struct VinputFcitxSceneProjection {
    active_scene_id: String,
    filter: MenuFilterState,
    scenes: Vec<SceneMenuItem>,
    projection: Option<SceneMenuProjection>,
}

unsafe fn text_input<'a>(data: *const u8, len: usize) -> Option<&'a str> {
    if data.is_null() {
        return (len == 0).then_some("");
    }

    // SAFETY: The caller guarantees that `data` points to `len` readable bytes
    // for the duration of this call.
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    std::str::from_utf8(bytes).ok()
}

unsafe fn projection_ref<'a>(
    projection: *const VinputFcitxSceneProjection,
) -> Option<&'a VinputFcitxSceneProjection> {
    // SAFETY: The caller guarantees a live handle returned by this crate.
    unsafe { projection.as_ref() }
}

unsafe fn projection_mut<'a>(
    projection: *mut VinputFcitxSceneProjection,
) -> Option<&'a mut VinputFcitxSceneProjection> {
    // SAFETY: The caller guarantees exclusive access to a live handle.
    unsafe { projection.as_mut() }
}

fn string_data(value: &str) -> *const u8 {
    if value.is_empty() {
        ptr::null()
    } else {
        value.as_ptr()
    }
}

fn filter_from_query(query: &str) -> MenuFilterState {
    let mut filter = MenuFilterState::default();
    filter.activate();
    filter.append_text(query);
    filter
}

/// Creates a scene projection builder for an active scene id and query.
///
/// Invalid pointers, invalid UTF-8, or caught Rust panics return null.
///
/// # Safety
///
/// Each non-null data pointer must reference its byte length for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_projection_new(
    active_scene_data: *const u8,
    active_scene_len: usize,
    query_data: *const u8,
    query_len: usize,
) -> *mut VinputFcitxSceneProjection {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(active_scene_id) = (unsafe { text_input(active_scene_data, active_scene_len) })
        else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(query) = (unsafe { text_input(query_data, query_len) }) else {
            return ptr::null_mut();
        };
        Box::into_raw(Box::new(VinputFcitxSceneProjection {
            active_scene_id: active_scene_id.to_owned(),
            filter: filter_from_query(query),
            scenes: Vec::new(),
            projection: None,
        }))
    }))
    .unwrap_or(ptr::null_mut())
}

/// Projects an existing Rust-owned scene snapshot directly.
///
/// Invalid handles, invalid UTF-8, or caught Rust panics return null.
///
/// # Safety
///
/// `snapshot` must be null or a live scene snapshot handle. `query_data` must
/// reference `query_len` readable bytes unless both are null/zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_projection_from_snapshot(
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
        let filter = filter_from_query(query);
        let scenes = snapshot
            .scenes()
            .iter()
            .enumerate()
            .map(|(source_index, scene)| SceneMenuItem {
                source_index,
                id: scene.id.clone(),
                label: scene.label.clone(),
            })
            .collect::<Vec<_>>();
        let projection = project_scene_menu(snapshot.active_scene_id(), &scenes, &filter);
        Box::into_raw(Box::new(VinputFcitxSceneProjection {
            active_scene_id: snapshot.active_scene_id().to_owned(),
            filter,
            scenes,
            projection: Some(projection),
        }))
    }))
    .unwrap_or(ptr::null_mut())
}

/// Releases a scene projection handle.
///
/// A null handle is ignored.
///
/// # Safety
///
/// A non-null handle must be live and freed no more than once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_projection_free(
    projection: *mut VinputFcitxSceneProjection,
) {
    if projection.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        drop(unsafe { Box::from_raw(projection) });
    }));
}

/// Adds one row from the daemon scene snapshot.
///
/// Returns zero for invalid handles, pointers, UTF-8, or a finalized builder.
///
/// # Safety
///
/// Each non-null data pointer must reference its byte length for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_projection_add(
    projection: *mut VinputFcitxSceneProjection,
    source_index: usize,
    id_data: *const u8,
    id_len: usize,
    label_data: *const u8,
    label_len: usize,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(projection) = (unsafe { projection_mut(projection) }) else {
                return false;
            };
            if projection.projection.is_some() {
                return false;
            }
            // SAFETY: Forwarded from this function's caller contract.
            let Some(id) = (unsafe { text_input(id_data, id_len) }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(label) = (unsafe { text_input(label_data, label_len) }) else {
                return false;
            };
            let scene = SceneMenuItem {
                source_index,
                id: id.to_owned(),
                label: label.to_owned(),
            };
            projection.scenes.push(scene);
            true
        }))
        .unwrap_or(false),
    )
}

/// Finalizes the scene projection.
///
/// Returns zero for invalid handles or caught Rust panics.
///
/// # Safety
///
/// `projection` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_projection_finish(
    projection: *mut VinputFcitxSceneProjection,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(projection) = (unsafe { projection_mut(projection) }) else {
                return false;
            };
            if projection.projection.is_none() {
                projection.projection = Some(project_scene_menu(
                    &projection.active_scene_id,
                    &projection.scenes,
                    &projection.filter,
                ));
            }
            true
        }))
        .unwrap_or(false),
    )
}

/// Returns the active scene label byte pointer after finalization.
///
/// # Safety
///
/// `projection` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_projection_active_label_data(
    projection: *const VinputFcitxSceneProjection,
) -> *const u8 {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { projection_ref(projection) }
        .and_then(|projection| projection.projection.as_ref())
        .map_or(ptr::null(), |projection| {
            string_data(&projection.active_label)
        })
}

/// Returns the active scene label length after finalization.
///
/// # Safety
///
/// `projection` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_projection_active_label_len(
    projection: *const VinputFcitxSceneProjection,
) -> usize {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { projection_ref(projection) }
        .and_then(|projection| projection.projection.as_ref())
        .map_or(0, |projection| projection.active_label.len())
}

/// Returns the visible row count after finalization.
///
/// # Safety
///
/// `projection` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_projection_item_count(
    projection: *const VinputFcitxSceneProjection,
) -> usize {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { projection_ref(projection) }
        .and_then(|projection| projection.projection.as_ref())
        .map_or(0, |projection| projection.items.len())
}

/// Returns a visible row's original daemon snapshot index.
///
/// Out-of-range or invalid accesses return `usize::MAX`.
///
/// # Safety
///
/// `projection` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_projection_item_source_index(
    projection: *const VinputFcitxSceneProjection,
    index: usize,
) -> usize {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { projection_ref(projection) }
        .and_then(|projection| projection.projection.as_ref())
        .and_then(|projection| projection.items.get(index))
        .map_or(usize::MAX, |item| item.source_index)
}

/// Returns a visible row label byte pointer.
///
/// # Safety
///
/// `projection` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_projection_item_label_data(
    projection: *const VinputFcitxSceneProjection,
    index: usize,
) -> *const u8 {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { projection_ref(projection) }
        .and_then(|projection| projection.projection.as_ref())
        .and_then(|projection| projection.items.get(index))
        .map_or(ptr::null(), |item| string_data(&item.label))
}

/// Returns a visible row label length.
///
/// # Safety
///
/// `projection` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_projection_item_label_len(
    projection: *const VinputFcitxSceneProjection,
    index: usize,
) -> usize {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { projection_ref(projection) }
        .and_then(|projection| projection.projection.as_ref())
        .and_then(|projection| projection.items.get(index))
        .map_or(0, |item| item.label.len())
}

#[cfg(test)]
mod tests {
    use super::{
        vinput_fcitx_scene_projection_active_label_data,
        vinput_fcitx_scene_projection_active_label_len, vinput_fcitx_scene_projection_add,
        vinput_fcitx_scene_projection_finish, vinput_fcitx_scene_projection_free,
        vinput_fcitx_scene_projection_from_snapshot, vinput_fcitx_scene_projection_item_count,
        vinput_fcitx_scene_projection_item_label_data,
        vinput_fcitx_scene_projection_item_label_len,
        vinput_fcitx_scene_projection_item_source_index, vinput_fcitx_scene_projection_new,
    };
    use crate::menu_snapshot::{
        vinput_fcitx_scene_snapshot_add, vinput_fcitx_scene_snapshot_free,
        vinput_fcitx_scene_snapshot_new,
    };

    unsafe fn bytes_from_view<'a>(data: *const u8, len: usize) -> &'a [u8] {
        if data.is_null() {
            return &[];
        }
        // SAFETY: Test callers keep the projection handle alive.
        unsafe { std::slice::from_raw_parts(data, len) }
    }

    #[test]
    fn projects_visible_scene_rows_through_stable_views() {
        let active = b"meeting";
        let query = b"code";
        let rows: [(&[u8], &[u8]); 3] = [
            (b"__raw__", b"Raw Dictation"),
            (b"meeting", b"Meeting Notes"),
            (b"code", b"Code Review"),
        ];

        // SAFETY: All views point to live local byte slices and the handle is
        // released exactly once after its final use.
        unsafe {
            let projection = vinput_fcitx_scene_projection_new(
                active.as_ptr(),
                active.len(),
                query.as_ptr(),
                query.len(),
            );
            assert!(!projection.is_null());
            for (index, (id, label)) in rows.iter().enumerate() {
                assert_eq!(
                    vinput_fcitx_scene_projection_add(
                        projection,
                        index,
                        id.as_ptr(),
                        id.len(),
                        label.as_ptr(),
                        label.len(),
                    ),
                    1
                );
            }
            assert_eq!(vinput_fcitx_scene_projection_finish(projection), 1);
            assert_eq!(
                bytes_from_view(
                    vinput_fcitx_scene_projection_active_label_data(projection),
                    vinput_fcitx_scene_projection_active_label_len(projection),
                ),
                b"Meeting Notes"
            );
            assert_eq!(vinput_fcitx_scene_projection_item_count(projection), 1);
            assert_eq!(
                vinput_fcitx_scene_projection_item_source_index(projection, 0),
                2
            );
            assert_eq!(
                bytes_from_view(
                    vinput_fcitx_scene_projection_item_label_data(projection, 0),
                    vinput_fcitx_scene_projection_item_label_len(projection, 0),
                ),
                b"Code Review"
            );
            assert_eq!(
                vinput_fcitx_scene_projection_item_source_index(projection, 1),
                usize::MAX
            );
            vinput_fcitx_scene_projection_free(projection);
        }
    }

    #[test]
    fn rejects_rows_after_finalization() {
        let empty = b"";
        let id = b"scene";
        let label = b"Scene";

        // SAFETY: All views point to live local slices and the handle is freed once.
        unsafe {
            let projection = vinput_fcitx_scene_projection_new(
                empty.as_ptr(),
                empty.len(),
                empty.as_ptr(),
                empty.len(),
            );
            assert!(!projection.is_null());
            assert_eq!(vinput_fcitx_scene_projection_finish(projection), 1);
            assert_eq!(
                vinput_fcitx_scene_projection_add(
                    projection,
                    0,
                    id.as_ptr(),
                    id.len(),
                    label.as_ptr(),
                    label.len(),
                ),
                0
            );
            vinput_fcitx_scene_projection_free(projection);
        }
    }

    #[test]
    fn projects_directly_from_rust_snapshot() {
        // SAFETY: All byte slices are live and both handles are freed exactly once.
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
            assert_eq!(
                vinput_fcitx_scene_snapshot_add(
                    snapshot,
                    b"code".as_ptr(),
                    4,
                    b"Code Review".as_ptr(),
                    11,
                ),
                1
            );
            let projection =
                vinput_fcitx_scene_projection_from_snapshot(snapshot, b"code".as_ptr(), 4);
            assert!(!projection.is_null());
            assert_eq!(vinput_fcitx_scene_projection_item_count(projection), 1);
            assert_eq!(
                vinput_fcitx_scene_projection_item_source_index(projection, 0),
                1
            );
            assert_eq!(
                bytes_from_view(
                    vinput_fcitx_scene_projection_active_label_data(projection),
                    vinput_fcitx_scene_projection_active_label_len(projection),
                ),
                b"Meeting Notes"
            );
            vinput_fcitx_scene_projection_free(projection);
            vinput_fcitx_scene_snapshot_free(snapshot);
        }
    }
}

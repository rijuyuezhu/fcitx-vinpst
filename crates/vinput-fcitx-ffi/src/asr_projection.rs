//! Raw-pointer C ABI for ASR menu snapshot projection.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use vinput_fcitx_core::{
    AsrMenuItem, AsrMenuProjectionState, MenuFilterState, ProjectedMenuItem, project_asr_menu,
};

use crate::menu_snapshot::{VinputFcitxAsrDisplaySnapshot, asr_core_ref};

/// Opaque ASR snapshot builder and projection result owned by Rust.
pub struct VinputFcitxAsrProjection {
    state: AsrMenuProjectionState,
    filter: MenuFilterState,
    targets: Vec<AsrMenuItem>,
    projection: Option<Vec<ProjectedMenuItem>>,
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
    projection: *const VinputFcitxAsrProjection,
) -> Option<&'a VinputFcitxAsrProjection> {
    // SAFETY: The caller guarantees a live handle returned by this crate.
    unsafe { projection.as_ref() }
}

unsafe fn projection_mut<'a>(
    projection: *mut VinputFcitxAsrProjection,
) -> Option<&'a mut VinputFcitxAsrProjection> {
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

/// Creates an ASR projection builder from target/effective state and a query.
///
/// Invalid pointers, invalid UTF-8, or caught Rust panics return null.
///
/// # Safety
///
/// Each non-null data pointer must reference its byte length for this call.
#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_projection_new(
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
    query_data: *const u8,
    query_len: usize,
) -> *mut VinputFcitxAsrProjection {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(target_provider_id) =
            (unsafe { text_input(target_provider_data, target_provider_len) })
        else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(target_model_id) = (unsafe { text_input(target_model_data, target_model_len) })
        else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(effective_provider_id) =
            (unsafe { text_input(effective_provider_data, effective_provider_len) })
        else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(effective_model_id) =
            (unsafe { text_input(effective_model_data, effective_model_len) })
        else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(last_error) = (unsafe { text_input(last_error_data, last_error_len) }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(query) = (unsafe { text_input(query_data, query_len) }) else {
            return ptr::null_mut();
        };

        Box::into_raw(Box::new(VinputFcitxAsrProjection {
            state: AsrMenuProjectionState {
                target_provider_id: target_provider_id.to_owned(),
                target_model_id: target_model_id.to_owned(),
                effective_provider_id: effective_provider_id.to_owned(),
                effective_model_id: effective_model_id.to_owned(),
                reload_in_progress: reload_in_progress != 0,
                last_error: last_error.to_owned(),
            },
            filter: filter_from_query(query),
            targets: Vec::new(),
            projection: None,
        }))
    }))
    .unwrap_or(ptr::null_mut())
}

/// Creates an ASR projection builder from an existing Rust-owned snapshot.
///
/// Invalid handles, invalid UTF-8, or caught Rust panics return null.
///
/// # Safety
///
/// `snapshot` must be null or a live ASR display snapshot handle. `query_data`
/// must reference `query_len` readable bytes unless both are null/zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_projection_new_from_snapshot(
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
            state: AsrMenuProjectionState {
                target_provider_id: snapshot.target_provider_id().to_owned(),
                target_model_id: snapshot.target_model_id().to_owned(),
                effective_provider_id: snapshot.effective_provider_id().to_owned(),
                effective_model_id: snapshot.effective_model_id().to_owned(),
                reload_in_progress: snapshot.reload_in_progress(),
                last_error: snapshot.last_error().to_owned(),
            },
            filter: filter_from_query(query),
            targets: Vec::new(),
            projection: None,
        }))
    }))
    .unwrap_or(ptr::null_mut())
}

/// Releases an ASR projection handle.
///
/// A null handle is ignored.
///
/// # Safety
///
/// A non-null handle must be live and freed no more than once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_projection_free(
    projection: *mut VinputFcitxAsrProjection,
) {
    if projection.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        drop(unsafe { Box::from_raw(projection) });
    }));
}

/// Adds one localized row from the daemon ASR snapshot.
///
/// Returns zero for invalid handles, pointers, UTF-8, or a finalized builder.
///
/// # Safety
///
/// Each non-null data pointer must reference its byte length for this call.
#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_projection_add(
    projection: *mut VinputFcitxAsrProjection,
    source_index: usize,
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
    rendered_label_data: *const u8,
    rendered_label_len: usize,
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
            let Some(rendered_label) =
                (unsafe { text_input(rendered_label_data, rendered_label_len) })
            else {
                return false;
            };

            projection.targets.push(AsrMenuItem {
                source_index,
                provider_id: provider_id.to_owned(),
                kind: kind.to_owned(),
                item_id: item_id.to_owned(),
                display_title: display_title.to_owned(),
                model_value: model_value.to_owned(),
                rendered_label: rendered_label.to_owned(),
            });
            true
        }))
        .unwrap_or(false),
    )
}

/// Adds one localized row by index from a Rust-owned ASR snapshot.
///
/// Returns zero for invalid handles, out-of-range rows, invalid UTF-8, or a
/// finalized builder. Failure does not mutate the projection.
///
/// # Safety
///
/// Both handles must be null or live handles returned by this crate.
/// `rendered_label_data` must reference `rendered_label_len` readable bytes
/// unless both are null/zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_projection_add_snapshot_item(
    projection: *mut VinputFcitxAsrProjection,
    snapshot: *const VinputFcitxAsrDisplaySnapshot,
    source_index: usize,
    rendered_label_data: *const u8,
    rendered_label_len: usize,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(snapshot) = (unsafe { asr_core_ref(snapshot) }) else {
                return false;
            };
            let Some(target) = snapshot.targets().get(source_index) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(rendered_label) =
                (unsafe { text_input(rendered_label_data, rendered_label_len) })
            else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(projection) = (unsafe { projection_mut(projection) }) else {
                return false;
            };
            if projection.projection.is_some() {
                return false;
            }
            projection.targets.push(AsrMenuItem {
                source_index,
                provider_id: target.provider_id.clone(),
                kind: target.kind.clone(),
                item_id: target.item_id.clone(),
                display_title: target.display_title.clone(),
                model_value: target.model_value.clone(),
                rendered_label: rendered_label.to_owned(),
            });
            true
        }))
        .unwrap_or(false),
    )
}

/// Finalizes the ASR projection.
///
/// Returns zero for invalid handles or caught Rust panics.
///
/// # Safety
///
/// `projection` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_projection_finish(
    projection: *mut VinputFcitxAsrProjection,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(projection) = (unsafe { projection_mut(projection) }) else {
                return false;
            };
            if projection.projection.is_none() {
                projection.projection = Some(project_asr_menu(
                    &projection.state,
                    &projection.targets,
                    &projection.filter,
                ));
            }
            true
        }))
        .unwrap_or(false),
    )
}

/// Returns the visible row count after finalization.
///
/// # Safety
///
/// `projection` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_projection_item_count(
    projection: *const VinputFcitxAsrProjection,
) -> usize {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { projection_ref(projection) }
        .and_then(|projection| projection.projection.as_ref())
        .map_or(0, Vec::len)
}

/// Returns a visible row's original daemon snapshot index.
///
/// Out-of-range or invalid accesses return `usize::MAX`.
///
/// # Safety
///
/// `projection` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_projection_item_source_index(
    projection: *const VinputFcitxAsrProjection,
    index: usize,
) -> usize {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { projection_ref(projection) }
        .and_then(|projection| projection.projection.as_ref())
        .and_then(|projection| projection.get(index))
        .map_or(usize::MAX, |item| item.source_index)
}

/// Returns a visible row label byte pointer.
///
/// # Safety
///
/// `projection` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_projection_item_label_data(
    projection: *const VinputFcitxAsrProjection,
    index: usize,
) -> *const u8 {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { projection_ref(projection) }
        .and_then(|projection| projection.projection.as_ref())
        .and_then(|projection| projection.get(index))
        .map_or(ptr::null(), |item| string_data(&item.label))
}

/// Returns a visible row label length.
///
/// # Safety
///
/// `projection` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_projection_item_label_len(
    projection: *const VinputFcitxAsrProjection,
    index: usize,
) -> usize {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { projection_ref(projection) }
        .and_then(|projection| projection.projection.as_ref())
        .and_then(|projection| projection.get(index))
        .map_or(0, |item| item.label.len())
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use super::{
        vinput_fcitx_asr_projection_add, vinput_fcitx_asr_projection_add_snapshot_item,
        vinput_fcitx_asr_projection_finish, vinput_fcitx_asr_projection_free,
        vinput_fcitx_asr_projection_item_count, vinput_fcitx_asr_projection_item_label_data,
        vinput_fcitx_asr_projection_item_label_len, vinput_fcitx_asr_projection_item_source_index,
        vinput_fcitx_asr_projection_new, vinput_fcitx_asr_projection_new_from_snapshot,
    };
    use crate::menu_snapshot::{
        vinput_fcitx_asr_display_snapshot_add, vinput_fcitx_asr_display_snapshot_free,
        vinput_fcitx_asr_display_snapshot_new,
    };

    unsafe fn bytes_from_view<'a>(data: *const u8, len: usize) -> &'a [u8] {
        if data.is_null() {
            return &[];
        }
        // SAFETY: Test callers keep the projection handle alive.
        unsafe { std::slice::from_raw_parts(data, len) }
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn add_row(
        projection: *mut super::VinputFcitxAsrProjection,
        source_index: usize,
        provider: &[u8],
        kind: &[u8],
        item_id: &[u8],
        display_title: &[u8],
        model_value: &[u8],
        label: &[u8],
    ) -> u8 {
        // SAFETY: Forwarded from this test helper's caller contract.
        unsafe {
            vinput_fcitx_asr_projection_add(
                projection,
                source_index,
                provider.as_ptr(),
                provider.len(),
                kind.as_ptr(),
                kind.len(),
                item_id.as_ptr(),
                item_id.len(),
                display_title.as_ptr(),
                display_title.len(),
                model_value.as_ptr(),
                model_value.len(),
                label.as_ptr(),
                label.len(),
            )
        }
    }

    #[test]
    fn excludes_effective_row_and_filters_localized_rows() {
        let target_provider = b"sherpa";
        let target_model = b"moonshine-en";
        let effective_provider = b"sherpa";
        let effective_model = b"moonshine-en";
        let empty = b"";
        let query = b"chinese local";

        // SAFETY: All views point to live local slices and the handle is freed once.
        unsafe {
            let projection = vinput_fcitx_asr_projection_new(
                target_provider.as_ptr(),
                target_provider.len(),
                target_model.as_ptr(),
                target_model.len(),
                effective_provider.as_ptr(),
                effective_provider.len(),
                effective_model.as_ptr(),
                effective_model.len(),
                0,
                empty.as_ptr(),
                empty.len(),
                query.as_ptr(),
                query.len(),
            );
            assert!(!projection.is_null());
            assert_eq!(
                add_row(
                    projection,
                    0,
                    b"sherpa",
                    b"local",
                    b"moonshine-en",
                    b"Moonshine English",
                    b"moonshine-en",
                    b"Moonshine English [Local]",
                ),
                1
            );
            assert_eq!(
                add_row(
                    projection,
                    1,
                    b"sherpa",
                    b"local",
                    b"paraformer-zh",
                    b"Paraformer Chinese",
                    b"paraformer-zh",
                    b"Paraformer Chinese [Local]",
                ),
                1
            );
            assert_eq!(vinput_fcitx_asr_projection_finish(projection), 1);
            assert_eq!(vinput_fcitx_asr_projection_item_count(projection), 1);
            assert_eq!(
                vinput_fcitx_asr_projection_item_source_index(projection, 0),
                1
            );
            assert_eq!(
                bytes_from_view(
                    vinput_fcitx_asr_projection_item_label_data(projection, 0),
                    vinput_fcitx_asr_projection_item_label_len(projection, 0),
                ),
                b"Paraformer Chinese [Local]"
            );
            vinput_fcitx_asr_projection_free(projection);
        }
    }

    #[test]
    fn keeps_requested_row_visible_during_reload() {
        let provider = b"sherpa";
        let target_model = b"requested";
        let effective_model = b"legacy";
        let empty = b"";

        // SAFETY: All views point to live local slices and the handle is freed once.
        unsafe {
            let projection = vinput_fcitx_asr_projection_new(
                provider.as_ptr(),
                provider.len(),
                target_model.as_ptr(),
                target_model.len(),
                provider.as_ptr(),
                provider.len(),
                effective_model.as_ptr(),
                effective_model.len(),
                1,
                empty.as_ptr(),
                empty.len(),
                empty.as_ptr(),
                empty.len(),
            );
            assert!(!projection.is_null());
            assert_eq!(
                add_row(
                    projection,
                    4,
                    provider,
                    b"local",
                    target_model,
                    b"Requested",
                    target_model,
                    b"Requested [Local] (loading)",
                ),
                1
            );
            assert_eq!(vinput_fcitx_asr_projection_finish(projection), 1);
            assert_eq!(vinput_fcitx_asr_projection_item_count(projection), 1);
            assert_eq!(
                vinput_fcitx_asr_projection_item_source_index(projection, 0),
                4
            );
            vinput_fcitx_asr_projection_free(projection);
        }
    }

    #[test]
    fn projects_localized_rows_directly_from_rust_snapshot() {
        // SAFETY: All byte slices are live and both handles are freed exactly once.
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
                1
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
                1
            );
            let projection = vinput_fcitx_asr_projection_new_from_snapshot(
                snapshot,
                b"chinese local".as_ptr(),
                13,
            );
            assert!(!projection.is_null());
            assert_eq!(
                vinput_fcitx_asr_projection_add_snapshot_item(
                    projection,
                    snapshot,
                    0,
                    b"Moonshine English [Local]".as_ptr(),
                    25,
                ),
                1
            );
            assert_eq!(
                vinput_fcitx_asr_projection_add_snapshot_item(
                    projection,
                    snapshot,
                    1,
                    b"Paraformer Chinese [Local]".as_ptr(),
                    26,
                ),
                1
            );
            assert_eq!(vinput_fcitx_asr_projection_finish(projection), 1);
            assert_eq!(vinput_fcitx_asr_projection_item_count(projection), 1);
            assert_eq!(
                vinput_fcitx_asr_projection_item_source_index(projection, 0),
                1
            );
            vinput_fcitx_asr_projection_free(projection);
            vinput_fcitx_asr_display_snapshot_free(snapshot);
        }
    }
}

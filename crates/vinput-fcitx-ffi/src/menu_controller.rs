//! Opaque Rust menu controllers that own daemon snapshots and finalize projections.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

#[cfg(test)]
use vinput_fcitx_core::{AsrDisplaySnapshot, SceneSnapshot};
use vinput_fcitx_core::{AsrDisplayText, AsrMenuController, SceneMenuController};

use crate::{
    menu::{VinputFcitxMenuSession, menu_session_filter_ref},
    menu_projection::VinputFcitxMenuProjection,
};

/// Opaque Rust-owned scene menu controller.
pub struct VinputFcitxSceneMenuController {
    pub(crate) controller: SceneMenuController,
}

/// Opaque Rust-owned ASR menu controller.
pub struct VinputFcitxAsrMenuController {
    pub(crate) controller: AsrMenuController,
}

#[cfg(test)]
pub(crate) fn boxed_scene_controller(
    snapshot: Option<SceneSnapshot>,
) -> *mut VinputFcitxSceneMenuController {
    let mut controller = SceneMenuController::default();
    if let Some(snapshot) = snapshot {
        controller.replace_snapshot(snapshot);
    }
    Box::into_raw(Box::new(VinputFcitxSceneMenuController { controller }))
}

#[cfg(test)]
pub(crate) fn boxed_asr_controller(
    snapshot: Option<AsrDisplaySnapshot>,
) -> *mut VinputFcitxAsrMenuController {
    let mut controller = AsrMenuController::default();
    if let Some(snapshot) = snapshot {
        controller.replace_snapshot(snapshot);
    }
    Box::into_raw(Box::new(VinputFcitxAsrMenuController { controller }))
}

unsafe fn text_input<'a>(data: *const u8, len: usize) -> Option<&'a str> {
    if data.is_null() {
        return (len == 0).then_some("");
    }
    // SAFETY: Forwarded from each exported function's caller contract.
    std::str::from_utf8(unsafe { std::slice::from_raw_parts(data, len) }).ok()
}

pub(crate) unsafe fn scene_controller_ref<'a>(
    controller: *const VinputFcitxSceneMenuController,
) -> Option<&'a SceneMenuController> {
    // SAFETY: Forwarded from the caller contract.
    unsafe { controller.as_ref() }.map(|value| &value.controller)
}

pub(crate) unsafe fn scene_controller_mut<'a>(
    controller: *mut VinputFcitxSceneMenuController,
) -> Option<&'a mut SceneMenuController> {
    // SAFETY: Forwarded from the caller contract.
    unsafe { controller.as_mut() }.map(|value| &mut value.controller)
}

pub(crate) unsafe fn asr_controller_mut<'a>(
    controller: *mut VinputFcitxAsrMenuController,
) -> Option<&'a mut AsrMenuController> {
    // SAFETY: Forwarded from the caller contract.
    unsafe { controller.as_mut() }.map(|value| &mut value.controller)
}

/// Creates an empty scene menu controller.
#[unsafe(no_mangle)]
pub extern "C" fn vinput_fcitx_scene_menu_controller_new() -> *mut VinputFcitxSceneMenuController {
    catch_unwind(|| {
        Box::into_raw(Box::new(VinputFcitxSceneMenuController {
            controller: SceneMenuController::default(),
        }))
    })
    .unwrap_or(ptr::null_mut())
}

/// Releases a scene menu controller.
///
/// # Safety
///
/// A non-null pointer must be a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_menu_controller_free(
    controller: *mut VinputFcitxSceneMenuController,
) {
    if !controller.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            drop(unsafe { Box::from_raw(controller) });
        }));
    }
}

/// Finalizes a scene projection from the controller's latest snapshot.
///
/// # Safety
///
/// Both pointers must be live handles.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_scene_menu_controller_projection_new(
    controller: *const VinputFcitxSceneMenuController,
    session: *const VinputFcitxMenuSession,
) -> *mut VinputFcitxMenuProjection {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(controller) = (unsafe { scene_controller_ref(controller) }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(filter) = (unsafe { menu_session_filter_ref(session) }) else {
            return ptr::null_mut();
        };
        let Some(projection) = controller.project(filter) else {
            return ptr::null_mut();
        };
        Box::into_raw(Box::new(VinputFcitxMenuProjection {
            summary: projection.summary,
            items: projection.items,
        }))
    }))
    .unwrap_or(ptr::null_mut())
}

/// Creates an empty ASR menu controller.
#[unsafe(no_mangle)]
pub extern "C" fn vinput_fcitx_asr_menu_controller_new() -> *mut VinputFcitxAsrMenuController {
    catch_unwind(|| {
        Box::into_raw(Box::new(VinputFcitxAsrMenuController {
            controller: AsrMenuController::default(),
        }))
    })
    .unwrap_or(ptr::null_mut())
}

/// Releases an ASR menu controller.
///
/// # Safety
///
/// A non-null pointer must be a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_menu_controller_free(
    controller: *mut VinputFcitxAsrMenuController,
) {
    if !controller.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            drop(unsafe { Box::from_raw(controller) });
        }));
    }
}

/// Finalizes a localized ASR projection from the controller's latest snapshot.
///
/// # Safety
///
/// Both handles must be live and every text pointer must match its declared length.
#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_menu_controller_projection_new(
    controller: *const VinputFcitxAsrMenuController,
    session: *const VinputFcitxMenuSession,
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
) -> *mut VinputFcitxMenuProjection {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(controller) = (unsafe { controller.as_ref() }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(filter) = (unsafe { menu_session_filter_ref(session) }) else {
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
        let Some(projection) = controller.controller.project(filter, &text) else {
            return ptr::null_mut();
        };
        Box::into_raw(Box::new(VinputFcitxMenuProjection {
            summary: projection.summary,
            items: projection.items,
        }))
    }))
    .unwrap_or(ptr::null_mut())
}

#[cfg(test)]
mod tests;

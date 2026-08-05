//! Opaque Rust menu controllers that own daemon snapshots and finalize projections.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

#[cfg(test)]
use vinpst_fcitx_core::{AsrDisplaySnapshot, SceneSnapshot};
use vinpst_fcitx_core::{AsrDisplayText, AsrMenuController, SceneMenuController};

use crate::{
    ffi_string::{VinpstFcitxStringView, text_view_input},
    menu::{VinpstFcitxMenuSession, menu_session_filter_ref},
    menu_projection::VinpstFcitxMenuProjection,
};

/// Opaque Rust-owned scene menu controller.
pub struct VinpstFcitxSceneMenuController {
    pub(crate) controller: SceneMenuController,
}

/// Opaque Rust-owned ASR menu controller.
pub struct VinpstFcitxAsrMenuController {
    pub(crate) controller: AsrMenuController,
}

/// Borrowed localized fragments used to render an ASR menu projection.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinpstFcitxAsrMenuTextView {
    pub local: VinpstFcitxStringView,
    pub remote: VinpstFcitxStringView,
    pub command: VinpstFcitxStringView,
    pub loading_suffix: VinpstFcitxStringView,
    pub unavailable: VinpstFcitxStringView,
    pub loading_prefix: VinpstFcitxStringView,
    pub error_prefix: VinpstFcitxStringView,
}

impl VinpstFcitxAsrMenuTextView {
    unsafe fn borrow<'a>(&self) -> Option<AsrDisplayText<'a>> {
        let borrow = |view| {
            // SAFETY: Forwarded from this method's caller contract.
            unsafe { text_view_input(view) }
        };
        Some(AsrDisplayText {
            local: borrow(self.local)?,
            remote: borrow(self.remote)?,
            command: borrow(self.command)?,
            loading_suffix: borrow(self.loading_suffix)?,
            unavailable: borrow(self.unavailable)?,
            loading_prefix: borrow(self.loading_prefix)?,
            error_prefix: borrow(self.error_prefix)?,
        })
    }
}

#[cfg(test)]
pub(crate) fn boxed_scene_controller(
    snapshot: Option<SceneSnapshot>,
) -> *mut VinpstFcitxSceneMenuController {
    let mut controller = SceneMenuController::default();
    if let Some(snapshot) = snapshot {
        controller.replace_snapshot(snapshot);
    }
    Box::into_raw(Box::new(VinpstFcitxSceneMenuController { controller }))
}

#[cfg(test)]
pub(crate) fn boxed_asr_controller(
    snapshot: Option<AsrDisplaySnapshot>,
) -> *mut VinpstFcitxAsrMenuController {
    let mut controller = AsrMenuController::default();
    if let Some(snapshot) = snapshot {
        controller.replace_snapshot(snapshot);
    }
    Box::into_raw(Box::new(VinpstFcitxAsrMenuController { controller }))
}

pub(crate) unsafe fn scene_controller_ref<'a>(
    controller: *const VinpstFcitxSceneMenuController,
) -> Option<&'a SceneMenuController> {
    // SAFETY: Forwarded from the caller contract.
    unsafe { controller.as_ref() }.map(|value| &value.controller)
}

pub(crate) unsafe fn scene_controller_mut<'a>(
    controller: *mut VinpstFcitxSceneMenuController,
) -> Option<&'a mut SceneMenuController> {
    // SAFETY: Forwarded from the caller contract.
    unsafe { controller.as_mut() }.map(|value| &mut value.controller)
}

pub(crate) unsafe fn asr_controller_mut<'a>(
    controller: *mut VinpstFcitxAsrMenuController,
) -> Option<&'a mut AsrMenuController> {
    // SAFETY: Forwarded from the caller contract.
    unsafe { controller.as_mut() }.map(|value| &mut value.controller)
}

/// Creates an empty scene menu controller.
#[unsafe(no_mangle)]
pub extern "C" fn vinpst_fcitx_scene_menu_controller_new() -> *mut VinpstFcitxSceneMenuController {
    catch_unwind(|| {
        Box::into_raw(Box::new(VinpstFcitxSceneMenuController {
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
pub unsafe extern "C" fn vinpst_fcitx_scene_menu_controller_free(
    controller: *mut VinpstFcitxSceneMenuController,
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
pub unsafe extern "C" fn vinpst_fcitx_scene_menu_controller_projection_new(
    controller: *const VinpstFcitxSceneMenuController,
    session: *const VinpstFcitxMenuSession,
) -> *mut VinpstFcitxMenuProjection {
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
        Box::into_raw(Box::new(VinpstFcitxMenuProjection {
            summary: projection.summary,
            items: projection.items,
        }))
    }))
    .unwrap_or(ptr::null_mut())
}

/// Creates an empty ASR menu controller.
#[unsafe(no_mangle)]
pub extern "C" fn vinpst_fcitx_asr_menu_controller_new() -> *mut VinpstFcitxAsrMenuController {
    catch_unwind(|| {
        Box::into_raw(Box::new(VinpstFcitxAsrMenuController {
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
pub unsafe extern "C" fn vinpst_fcitx_asr_menu_controller_free(
    controller: *mut VinpstFcitxAsrMenuController,
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
/// Both handles and `text` must be live, and every borrowed string must match its
/// declared length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_asr_menu_controller_projection_new(
    controller: *const VinpstFcitxAsrMenuController,
    session: *const VinpstFcitxMenuSession,
    text: *const VinpstFcitxAsrMenuTextView,
) -> *mut VinpstFcitxMenuProjection {
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
        let Some(text) = (unsafe { text.as_ref() }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(text) = (unsafe { text.borrow() }) else {
            return ptr::null_mut();
        };
        let Some(projection) = controller.controller.project(filter, &text) else {
            return ptr::null_mut();
        };
        Box::into_raw(Box::new(VinpstFcitxMenuProjection {
            summary: projection.summary,
            items: projection.items,
        }))
    }))
    .unwrap_or(ptr::null_mut())
}

#[cfg(test)]
mod tests;

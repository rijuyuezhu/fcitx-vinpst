//! Opaque Rust menu controllers that own daemon snapshots and finalize projections.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

#[cfg(test)]
use vinput_fcitx_core::{AsrDisplaySnapshot, SceneSnapshot};
use vinput_fcitx_core::{AsrDisplayText, AsrMenuController, SceneMenuController};

use crate::{
    ffi_string::{VinputFcitxStringView, text_view_input},
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

/// Borrowed localized fragments used to render an ASR menu projection.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxAsrMenuTextView {
    pub local: VinputFcitxStringView,
    pub remote: VinputFcitxStringView,
    pub command: VinputFcitxStringView,
    pub loading_suffix: VinputFcitxStringView,
    pub unavailable: VinputFcitxStringView,
    pub loading_prefix: VinputFcitxStringView,
    pub error_prefix: VinputFcitxStringView,
}

impl VinputFcitxAsrMenuTextView {
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
/// Both handles and `text` must be live, and every borrowed string must match its
/// declared length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_asr_menu_controller_projection_new(
    controller: *const VinputFcitxAsrMenuController,
    session: *const VinputFcitxMenuSession,
    text: *const VinputFcitxAsrMenuTextView,
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
        Box::into_raw(Box::new(VinputFcitxMenuProjection {
            summary: projection.summary,
            items: projection.items,
        }))
    }))
    .unwrap_or(ptr::null_mut())
}

#[cfg(test)]
mod tests;

//! Opaque Rust-owned Scene and ASR display snapshots.

use std::panic::{AssertUnwindSafe, catch_unwind};

use vinput_fcitx_core::{AsrDisplaySnapshot, SceneSnapshot};

/// Opaque Rust-owned scene snapshot.
pub struct VinputFcitxSceneSnapshot {
    snapshot: SceneSnapshot,
}

/// Opaque Rust-owned ASR display snapshot.
pub struct VinputFcitxAsrDisplaySnapshot {
    snapshot: AsrDisplaySnapshot,
}

pub(crate) fn boxed_scene_snapshot(snapshot: SceneSnapshot) -> *mut VinputFcitxSceneSnapshot {
    Box::into_raw(Box::new(VinputFcitxSceneSnapshot { snapshot }))
}

pub(crate) fn boxed_asr_display_snapshot(
    snapshot: AsrDisplaySnapshot,
) -> *mut VinputFcitxAsrDisplaySnapshot {
    Box::into_raw(Box::new(VinputFcitxAsrDisplaySnapshot { snapshot }))
}

pub(crate) unsafe fn scene_core_ref<'a>(
    snapshot: *const VinputFcitxSceneSnapshot,
) -> Option<&'a SceneSnapshot> {
    // SAFETY: Forwarded from the caller contract.
    unsafe { snapshot.as_ref() }.map(|value| &value.snapshot)
}

pub(crate) unsafe fn scene_core_mut<'a>(
    snapshot: *mut VinputFcitxSceneSnapshot,
) -> Option<&'a mut SceneSnapshot> {
    // SAFETY: Forwarded from the caller contract.
    unsafe { snapshot.as_mut() }.map(|value| &mut value.snapshot)
}

pub(crate) unsafe fn asr_core_ref<'a>(
    snapshot: *const VinputFcitxAsrDisplaySnapshot,
) -> Option<&'a AsrDisplaySnapshot> {
    // SAFETY: Forwarded from the caller contract.
    unsafe { snapshot.as_ref() }.map(|value| &value.snapshot)
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

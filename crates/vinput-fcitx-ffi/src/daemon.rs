//! Typed C ABI over the safe Rust D-Bus runtime.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use vinput_fcitx_core::{AsrDisplaySnapshot, SceneSnapshot};
use vinput_fcitx_dbus::{DaemonClient, DaemonOperation, DaemonResponse};

use crate::{
    ffi_string::{VinputFcitxStringView, string_view, text_input},
    menu_controller::{
        VinputFcitxAsrMenuController, VinputFcitxSceneMenuController, asr_controller_mut,
        scene_controller_mut,
    },
};

/// Opaque blocking daemon client.
pub struct VinputFcitxDaemonClient {
    pub(crate) client: DaemonClient,
}

/// Opaque Rust-owned UTF-8 string.
pub struct VinputFcitxOwnedString {
    value: String,
}

/// Borrowed provider/model pair identifying one ASR target.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxAsrTargetView {
    /// ASR provider id.
    pub provider: VinputFcitxStringView,
    /// Provider-specific model value.
    pub model: VinputFcitxStringView,
}

impl VinputFcitxAsrTargetView {
    unsafe fn borrow(&self) -> Option<(&str, &str)> {
        // SAFETY: Forwarded from the exported function's caller contract.
        let provider = unsafe { text_input(self.provider.data, self.provider.len) }?;
        // SAFETY: Forwarded from the exported function's caller contract.
        let model = unsafe { text_input(self.model.data, self.model.len) }?;
        Some((provider, model))
    }
}

struct ErrorOut(*mut *mut VinputFcitxOwnedString);

impl ErrorOut {
    unsafe fn new(output: *mut *mut VinputFcitxOwnedString) -> Self {
        if !output.is_null() {
            // SAFETY: The caller guarantees a writable output pointer when non-null.
            unsafe { output.write(ptr::null_mut()) };
        }
        Self(output)
    }

    fn write(&self, message: impl Into<String>) {
        if !self.0.is_null() {
            // SAFETY: Construction requires a writable output pointer when non-null.
            unsafe { self.0.write(boxed_string(message.into())) };
        }
    }
}

fn boxed_string(value: String) -> *mut VinputFcitxOwnedString {
    Box::into_raw(Box::new(VinputFcitxOwnedString { value }))
}

fn call(
    client: &VinputFcitxDaemonClient,
    operation: DaemonOperation,
    first: &str,
    second: &str,
) -> Result<DaemonResponse, String> {
    client
        .client
        .call(operation, first, second)
        .map_err(|error| error.to_string())
}

fn expect<T>(
    result: Result<DaemonResponse, String>,
    expected: &str,
    extract: impl FnOnce(DaemonResponse) -> Option<T>,
) -> Result<T, String> {
    match result {
        Ok(response) => extract(response).ok_or_else(|| {
            format!("voice input daemon returned an unexpected response; expected {expected}")
        }),
        Err(error) => Err(error),
    }
}

fn take_text(response: DaemonResponse) -> Option<String> {
    match response {
        DaemonResponse::Text(value) => Some(value),
        _ => None,
    }
}

fn take_scene(response: DaemonResponse) -> Option<SceneSnapshot> {
    match response {
        DaemonResponse::SceneSnapshot(value) => Some(value),
        _ => None,
    }
}

fn take_asr_display(response: DaemonResponse) -> Option<AsrDisplaySnapshot> {
    match response {
        DaemonResponse::AsrDisplaySnapshot(value) => Some(value),
        _ => None,
    }
}

unsafe fn bool_call(
    client: *const VinputFcitxDaemonClient,
    operation: DaemonOperation,
    first: &str,
    second: &str,
    persisted_out: *mut u8,
    errors: &ErrorOut,
) -> bool {
    if persisted_out.is_null() {
        errors.write("missing persisted output");
        return false;
    }
    // SAFETY: The output pointer was validated above.
    unsafe { persisted_out.write(0) };
    // SAFETY: Forwarded from the exported function's caller contract.
    let Some(client) = (unsafe { client.as_ref() }) else {
        errors.write("invalid daemon client");
        return false;
    };
    match expect(
        call(client, operation, first, second),
        "boolean",
        |response| match response {
            DaemonResponse::Bool(value) => Some(value),
            _ => None,
        },
    ) {
        Ok(persisted) => {
            // SAFETY: The output pointer was validated above.
            unsafe { persisted_out.write(u8::from(persisted)) };
            true
        }
        Err(error) => {
            errors.write(error);
            false
        }
    }
}

/// Connects to the user session bus.
///
/// On failure, returns null and writes an owned error response to `error_out`.
///
/// # Safety
///
/// `error_out` must be writable when non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_daemon_client_connect(
    error_out: *mut *mut VinputFcitxOwnedString,
) -> *mut VinputFcitxDaemonClient {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let errors = unsafe { ErrorOut::new(error_out) };
        match DaemonClient::connect_session() {
            Ok(client) => Box::into_raw(Box::new(VinputFcitxDaemonClient { client })),
            Err(error) => {
                errors.write(error.to_string());
                ptr::null_mut()
            }
        }
    }))
    .unwrap_or(ptr::null_mut())
}

/// Releases a daemon client.
///
/// # Safety
///
/// A non-null pointer must be a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_daemon_client_free(client: *mut VinputFcitxDaemonClient) {
    if !client.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            drop(unsafe { Box::from_raw(client) });
        }));
    }
}

/// Reads the current daemon status as owned text.
///
/// # Safety
///
/// `client` must be a live handle and `error_out` writable when non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_daemon_client_get_status(
    client: *const VinputFcitxDaemonClient,
    error_out: *mut *mut VinputFcitxOwnedString,
) -> *mut VinputFcitxOwnedString {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let errors = unsafe { ErrorOut::new(error_out) };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(client) = (unsafe { client.as_ref() }) else {
            errors.write("invalid daemon client");
            return ptr::null_mut();
        };
        match expect(
            call(client, DaemonOperation::GetStatus, "", ""),
            "text",
            take_text,
        ) {
            Ok(status) => boxed_string(status),
            Err(error) => {
                errors.write(error);
                ptr::null_mut()
            }
        }
    }))
    .unwrap_or(ptr::null_mut())
}

/// Refreshes a Rust-owned scene menu controller directly from the daemon.
///
/// # Safety
///
/// Both handles must be live and `error_out` writable when non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_daemon_client_refresh_scene_menu_controller(
    client: *const VinputFcitxDaemonClient,
    controller: *mut VinputFcitxSceneMenuController,
    error_out: *mut *mut VinputFcitxOwnedString,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let errors = unsafe { ErrorOut::new(error_out) };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(client) = (unsafe { client.as_ref() }) else {
                errors.write("invalid daemon client");
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(controller) = (unsafe { scene_controller_mut(controller) }) else {
                errors.write("invalid scene menu controller");
                return false;
            };
            match expect(
                call(client, DaemonOperation::GetSceneState, "", ""),
                "scene snapshot",
                take_scene,
            ) {
                Ok(snapshot) => {
                    controller.replace_snapshot(snapshot);
                    true
                }
                Err(error) => {
                    errors.write(error);
                    false
                }
            }
        }))
        .unwrap_or(false),
    )
}

/// Persists or applies an active scene and updates the controller snapshot.
///
/// # Safety
///
/// All non-null pointers must satisfy the declared readable/writable lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_daemon_client_set_active_scene(
    client: *const VinputFcitxDaemonClient,
    controller: *mut VinputFcitxSceneMenuController,
    scene_data: *const u8,
    scene_len: usize,
    persisted_out: *mut u8,
    error_out: *mut *mut VinputFcitxOwnedString,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let errors = unsafe { ErrorOut::new(error_out) };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(controller) = (unsafe { scene_controller_mut(controller) }) else {
                errors.write("invalid scene menu controller");
                return false;
            };
            let Some(snapshot) = controller.snapshot_mut() else {
                errors.write("scene menu controller has no snapshot");
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(scene) = (unsafe { text_input(scene_data, scene_len) }) else {
                errors.write("scene id is not valid UTF-8");
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let succeeded = unsafe {
                bool_call(
                    client,
                    DaemonOperation::SetActiveScene,
                    scene,
                    "",
                    persisted_out,
                    &errors,
                )
            };
            if succeeded {
                snapshot.set_active_scene_id(scene.to_owned());
            }
            succeeded
        }))
        .unwrap_or(false),
    )
}

/// Refreshes a Rust-owned ASR menu controller directly from the daemon.
///
/// # Safety
///
/// Both handles must be live and `error_out` writable when non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_daemon_client_refresh_asr_menu_controller(
    client: *const VinputFcitxDaemonClient,
    controller: *mut VinputFcitxAsrMenuController,
    error_out: *mut *mut VinputFcitxOwnedString,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let errors = unsafe { ErrorOut::new(error_out) };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(client) = (unsafe { client.as_ref() }) else {
                errors.write("invalid daemon client");
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(controller) = (unsafe { asr_controller_mut(controller) }) else {
                errors.write("invalid ASR menu controller");
                return false;
            };
            match expect(
                call(client, DaemonOperation::GetAsrDisplayMenuState, "", ""),
                "ASR display snapshot",
                take_asr_display,
            ) {
                Ok(snapshot) => {
                    controller.replace_snapshot(snapshot);
                    true
                }
                Err(error) => {
                    errors.write(error);
                    false
                }
            }
        }))
        .unwrap_or(false),
    )
}

/// Persists or applies the active ASR provider/model target.
///
/// # Safety
///
/// All non-null pointers must satisfy the declared readable/writable lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_daemon_client_set_active_asr_target(
    client: *const VinputFcitxDaemonClient,
    target: *const VinputFcitxAsrTargetView,
    persisted_out: *mut u8,
    error_out: *mut *mut VinputFcitxOwnedString,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let errors = unsafe { ErrorOut::new(error_out) };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(target) = (unsafe { target.as_ref() }) else {
                errors.write("invalid ASR target");
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some((provider, model)) = (unsafe { target.borrow() }) else {
                errors.write("ASR target is not valid UTF-8");
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            unsafe {
                bool_call(
                    client,
                    DaemonOperation::SetActiveAsrTarget,
                    provider,
                    model,
                    persisted_out,
                    &errors,
                )
            }
        }))
        .unwrap_or(false),
    )
}

/// Releases a Rust-owned UTF-8 string.
///
/// # Safety
///
/// A non-null pointer must be a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_owned_string_free(value: *mut VinputFcitxOwnedString) {
    if !value.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            drop(unsafe { Box::from_raw(value) });
        }));
    }
}

/// Borrows a Rust-owned UTF-8 string.
///
/// # Safety
///
/// `value` must be live and `view_out` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_owned_string_view(
    value: *const VinputFcitxOwnedString,
    view_out: *mut VinputFcitxStringView,
) -> u8 {
    if view_out.is_null() {
        return 0;
    }
    // SAFETY: Forwarded from this function's caller contract.
    let Some(value) = (unsafe { value.as_ref() }) else {
        return 0;
    };
    // SAFETY: The caller guarantees a writable output pointer.
    unsafe {
        view_out.write(string_view(&value.value));
    }
    1
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use vinput_fcitx_core::{AsrDisplaySnapshot, SceneSnapshot};
    use vinput_fcitx_dbus::DaemonResponse;

    use super::{
        boxed_string, expect, take_asr_display, take_scene, take_text,
        vinput_fcitx_owned_string_free, vinput_fcitx_owned_string_view,
    };
    use crate::ffi_string::VinputFcitxStringView;

    unsafe fn bytes(view: VinputFcitxStringView) -> &'static [u8] {
        if view.data.is_null() {
            return &[];
        }
        // SAFETY: Test callers keep the response alive.
        unsafe { std::slice::from_raw_parts(view.data, view.len) }
    }

    #[test]
    fn exposes_owned_strings() {
        // SAFETY: Each string is live for all accesses and freed exactly once.
        unsafe {
            for text in ["recording", "broken"] {
                let value = boxed_string(text.to_owned());
                let mut view = VinputFcitxStringView {
                    data: ptr::null(),
                    len: 0,
                };
                assert_eq!(vinput_fcitx_owned_string_view(value, &raw mut view), 1);
                assert_eq!(bytes(view), text.as_bytes());
                vinput_fcitx_owned_string_free(value);
            }
        }
    }

    #[test]
    fn validates_typed_daemon_responses() {
        assert_eq!(
            expect(
                Ok(DaemonResponse::Text("idle".to_owned())),
                "text",
                take_text,
            )
            .as_deref(),
            Ok("idle")
        );
        assert!(expect(Ok(DaemonResponse::Bool(true)), "text", take_text).is_err());
        assert_eq!(
            expect(Ok(DaemonResponse::Bool(true)), "boolean", |response| {
                match response {
                    DaemonResponse::Bool(value) => Some(value),
                    _ => None,
                }
            }),
            Ok(true)
        );

        let scene = SceneSnapshot::new("scene".to_owned());
        assert_eq!(
            expect(
                Ok(DaemonResponse::SceneSnapshot(scene.clone())),
                "scene snapshot",
                take_scene,
            ),
            Ok(scene)
        );
        assert!(expect(Ok(DaemonResponse::None), "scene snapshot", take_scene).is_err());

        let asr = AsrDisplaySnapshot::default();
        assert_eq!(
            expect(
                Ok(DaemonResponse::AsrDisplaySnapshot(asr.clone())),
                "ASR display snapshot",
                take_asr_display,
            ),
            Ok(asr)
        );
        assert!(expect(Err("failed".to_owned()), "text", take_text).is_err());
    }
}

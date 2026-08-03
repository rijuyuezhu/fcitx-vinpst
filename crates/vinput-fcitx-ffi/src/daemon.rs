//! Typed C ABI over the safe Rust D-Bus runtime.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use vinput_fcitx_core::{AsrDisplaySnapshot, SceneSnapshot};
use vinput_fcitx_dbus::{DaemonClient, DaemonOperation, DaemonResponse};

use crate::{
    frontend::VinputFcitxStringView,
    menu_snapshot::{
        VinputFcitxAsrDisplaySnapshot, VinputFcitxSceneSnapshot, boxed_asr_display_snapshot,
        boxed_scene_snapshot, scene_core_mut,
    },
};

/// Opaque blocking daemon client.
pub struct VinputFcitxDaemonClient {
    pub(crate) client: DaemonClient,
}

/// Owned daemon text or transport/type error.
pub struct VinputFcitxDaemonResponse {
    is_error: bool,
    text: String,
}

/// Borrowed daemon text/error response.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxDaemonResponseView {
    /// Whether `text` is an error message instead of a successful text result.
    pub is_error: u8,
    /// Borrowed text valid while the response handle remains alive.
    pub text: VinputFcitxStringView,
}

struct ErrorOut(*mut *mut VinputFcitxDaemonResponse);

impl ErrorOut {
    unsafe fn new(output: *mut *mut VinputFcitxDaemonResponse) -> Self {
        if !output.is_null() {
            // SAFETY: The caller guarantees a writable output pointer when non-null.
            unsafe { output.write(ptr::null_mut()) };
        }
        Self(output)
    }

    fn write(&self, message: impl Into<String>) {
        if !self.0.is_null() {
            // SAFETY: Construction requires a writable output pointer when non-null.
            unsafe { self.0.write(boxed_response(true, message.into())) };
        }
    }
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

fn boxed_response(is_error: bool, text: String) -> *mut VinputFcitxDaemonResponse {
    Box::into_raw(Box::new(VinputFcitxDaemonResponse { is_error, text }))
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

unsafe fn snapshot_call<T, H>(
    client: *const VinputFcitxDaemonClient,
    error_out: *mut *mut VinputFcitxDaemonResponse,
    operation: DaemonOperation,
    expected: &str,
    extract: fn(DaemonResponse) -> Option<T>,
    boxed: fn(T) -> *mut H,
) -> *mut H {
    // SAFETY: Forwarded from the exported function's caller contract.
    let errors = unsafe { ErrorOut::new(error_out) };
    // SAFETY: Forwarded from the exported function's caller contract.
    let Some(client) = (unsafe { client.as_ref() }) else {
        errors.write("invalid daemon client");
        return ptr::null_mut();
    };
    match expect(call(client, operation, "", ""), expected, extract) {
        Ok(value) => boxed(value),
        Err(error) => {
            errors.write(error);
            ptr::null_mut()
        }
    }
}

unsafe fn bool_call(
    client: *const VinputFcitxDaemonClient,
    operation: DaemonOperation,
    first: (*const u8, usize, &str),
    second: (*const u8, usize, &str),
    persisted_out: *mut u8,
    error_out: *mut *mut VinputFcitxDaemonResponse,
) -> bool {
    // SAFETY: Forwarded from the exported function's caller contract.
    let errors = unsafe { ErrorOut::new(error_out) };
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
    // SAFETY: Forwarded from the exported function's caller contract.
    let Some(first_value) = (unsafe { text_input(first.0, first.1) }) else {
        errors.write(format!("{} is not valid UTF-8", first.2));
        return false;
    };
    // SAFETY: Forwarded from the exported function's caller contract.
    let Some(second_value) = (unsafe { text_input(second.0, second.1) }) else {
        errors.write(format!("{} is not valid UTF-8", second.2));
        return false;
    };
    match expect(
        call(client, operation, first_value, second_value),
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
    error_out: *mut *mut VinputFcitxDaemonResponse,
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

/// Reads the current daemon status as owned text or an owned error.
///
/// # Safety
///
/// `client` must be a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_daemon_client_get_status(
    client: *const VinputFcitxDaemonClient,
) -> *mut VinputFcitxDaemonResponse {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(client) = (unsafe { client.as_ref() }) else {
            return ptr::null_mut();
        };
        match expect(
            call(client, DaemonOperation::GetStatus, "", ""),
            "text",
            take_text,
        ) {
            Ok(status) => boxed_response(false, status),
            Err(error) => boxed_response(true, error),
        }
    }))
    .unwrap_or(ptr::null_mut())
}

/// Reads the Rust-owned Scene snapshot.
///
/// # Safety
///
/// `client` must be live and `error_out` writable when non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_daemon_client_get_scene_state(
    client: *const VinputFcitxDaemonClient,
    error_out: *mut *mut VinputFcitxDaemonResponse,
) -> *mut VinputFcitxSceneSnapshot {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        unsafe {
            snapshot_call(
                client,
                error_out,
                DaemonOperation::GetSceneState,
                "scene snapshot",
                take_scene,
                boxed_scene_snapshot,
            )
        }
    }))
    .unwrap_or(ptr::null_mut())
}

/// Persists or applies the active Scene id.
///
/// # Safety
///
/// All non-null pointers must satisfy the declared readable/writable lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_daemon_client_set_active_scene(
    client: *const VinputFcitxDaemonClient,
    snapshot: *mut VinputFcitxSceneSnapshot,
    scene_data: *const u8,
    scene_len: usize,
    persisted_out: *mut u8,
    error_out: *mut *mut VinputFcitxDaemonResponse,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let errors = unsafe { ErrorOut::new(error_out) };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(snapshot) = (unsafe { scene_core_mut(snapshot) }) else {
                errors.write("invalid scene snapshot");
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
                    (scene_data, scene_len, "scene id"),
                    (ptr::null(), 0, "unused argument"),
                    persisted_out,
                    error_out,
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

/// Reads the Rust-owned ASR display snapshot.
///
/// # Safety
///
/// `client` must be live and `error_out` writable when non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_daemon_client_get_asr_display_state(
    client: *const VinputFcitxDaemonClient,
    error_out: *mut *mut VinputFcitxDaemonResponse,
) -> *mut VinputFcitxAsrDisplaySnapshot {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        unsafe {
            snapshot_call(
                client,
                error_out,
                DaemonOperation::GetAsrDisplayMenuState,
                "ASR display snapshot",
                take_asr_display,
                boxed_asr_display_snapshot,
            )
        }
    }))
    .unwrap_or(ptr::null_mut())
}

/// Persists or applies the active ASR provider/model target.
///
/// # Safety
///
/// All non-null pointers must satisfy the declared readable/writable lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_daemon_client_set_active_asr_target(
    client: *const VinputFcitxDaemonClient,
    provider_data: *const u8,
    provider_len: usize,
    model_data: *const u8,
    model_len: usize,
    persisted_out: *mut u8,
    error_out: *mut *mut VinputFcitxDaemonResponse,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            unsafe {
                bool_call(
                    client,
                    DaemonOperation::SetActiveAsrTarget,
                    (provider_data, provider_len, "provider id"),
                    (model_data, model_len, "model value"),
                    persisted_out,
                    error_out,
                )
            }
        }))
        .unwrap_or(false),
    )
}

/// Releases a daemon text/error response.
///
/// # Safety
///
/// A non-null pointer must be a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_daemon_response_free(
    response: *mut VinputFcitxDaemonResponse,
) {
    if !response.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            drop(unsafe { Box::from_raw(response) });
        }));
    }
}

/// Borrows a daemon text/error response.
///
/// # Safety
///
/// `response` must be live and `view_out` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_daemon_response_view(
    response: *const VinputFcitxDaemonResponse,
    view_out: *mut VinputFcitxDaemonResponseView,
) -> u8 {
    if view_out.is_null() {
        return 0;
    }
    // SAFETY: Forwarded from this function's caller contract.
    let Some(response) = (unsafe { response.as_ref() }) else {
        return 0;
    };
    // SAFETY: The caller guarantees a writable output pointer.
    unsafe {
        view_out.write(VinputFcitxDaemonResponseView {
            is_error: u8::from(response.is_error),
            text: string_view(&response.text),
        });
    }
    1
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use vinput_fcitx_core::{AsrDisplaySnapshot, SceneSnapshot};
    use vinput_fcitx_dbus::DaemonResponse;

    use super::{
        VinputFcitxDaemonResponseView, boxed_response, expect, take_asr_display, take_scene,
        take_text, vinput_fcitx_daemon_response_free, vinput_fcitx_daemon_response_view,
    };
    use crate::frontend::VinputFcitxStringView;

    unsafe fn bytes(view: VinputFcitxStringView) -> &'static [u8] {
        if view.data.is_null() {
            return &[];
        }
        // SAFETY: Test callers keep the response alive.
        unsafe { std::slice::from_raw_parts(view.data, view.len) }
    }

    #[test]
    fn exposes_owned_text_and_error_responses() {
        // SAFETY: Each response is live for all accesses and freed exactly once.
        unsafe {
            for (is_error, text) in [(false, "recording"), (true, "broken")] {
                let response = boxed_response(is_error, text.to_owned());
                let mut view = VinputFcitxDaemonResponseView {
                    is_error: u8::MAX,
                    text: VinputFcitxStringView {
                        data: ptr::null(),
                        len: 0,
                    },
                };
                assert_eq!(
                    vinput_fcitx_daemon_response_view(response, &raw mut view),
                    1
                );
                assert_eq!(view.is_error, u8::from(is_error));
                assert_eq!(bytes(view.text), text.as_bytes());
                vinput_fcitx_daemon_response_free(response);
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

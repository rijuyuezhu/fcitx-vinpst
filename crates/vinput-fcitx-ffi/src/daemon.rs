//! Thin operation/response C ABI over the safe Rust D-Bus runtime.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use vinput_fcitx_dbus::{DaemonClient, DaemonOperation, DaemonResponse};

use crate::{
    frontend::VinputFcitxStringView,
    menu_snapshot::{
        VinputFcitxAsrDisplaySnapshot, VinputFcitxSceneSnapshot, boxed_asr_display_snapshot,
        boxed_scene_snapshot,
    },
};

/// Opaque blocking daemon client.
pub struct VinputFcitxDaemonClient {
    client: DaemonClient,
}

/// Opaque daemon response or transport error.
pub struct VinputFcitxDaemonResponse {
    value: Option<StoredResponse>,
}

enum StoredResponse {
    Error(String),
    Value(DaemonResponse),
}

/// Borrowed daemon response summary.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VinputFcitxDaemonResponseView {
    /// Stable `VINPUT_FCITX_DAEMON_RESPONSE_*` value.
    pub kind: u8,
    /// Boolean return value when `kind` is bool.
    pub bool_value: u8,
    /// Text return value or error message.
    pub text: VinputFcitxStringView,
}

const DAEMON_RESPONSE_ERROR: u8 = 0;
const DAEMON_RESPONSE_NONE: u8 = 1;
const DAEMON_RESPONSE_TEXT: u8 = 2;
const DAEMON_RESPONSE_BOOL: u8 = 3;
const DAEMON_RESPONSE_SCENE_SNAPSHOT: u8 = 4;
const DAEMON_RESPONSE_ASR_DISPLAY_SNAPSHOT: u8 = 5;

const DAEMON_OPERATION_START_RECORDING: u8 = 0;
const DAEMON_OPERATION_START_COMMAND_RECORDING: u8 = 1;
const DAEMON_OPERATION_STOP_RECORDING: u8 = 2;
const DAEMON_OPERATION_GET_STATUS: u8 = 3;
const DAEMON_OPERATION_GET_SCENE_STATE: u8 = 4;
const DAEMON_OPERATION_SET_ACTIVE_SCENE: u8 = 5;
const DAEMON_OPERATION_GET_ASR_DISPLAY_MENU_STATE: u8 = 6;
const DAEMON_OPERATION_SET_ACTIVE_ASR_PROVIDER: u8 = 7;
const DAEMON_OPERATION_SET_ACTIVE_ASR_TARGET: u8 = 8;
const DAEMON_OPERATION_GET_TEXT_ADAPTER_STATE: u8 = 9;
const DAEMON_OPERATION_START_ADAPTER: u8 = 10;
const DAEMON_OPERATION_STOP_ADAPTER: u8 = 11;
const DAEMON_OPERATION_GET_RUNTIME_STATUS: u8 = 12;

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

fn operation(value: u8) -> Option<DaemonOperation> {
    match value {
        DAEMON_OPERATION_START_RECORDING => Some(DaemonOperation::StartRecording),
        DAEMON_OPERATION_START_COMMAND_RECORDING => Some(DaemonOperation::StartCommandRecording),
        DAEMON_OPERATION_STOP_RECORDING => Some(DaemonOperation::StopRecording),
        DAEMON_OPERATION_GET_STATUS => Some(DaemonOperation::GetStatus),
        DAEMON_OPERATION_GET_SCENE_STATE => Some(DaemonOperation::GetSceneState),
        DAEMON_OPERATION_SET_ACTIVE_SCENE => Some(DaemonOperation::SetActiveScene),
        DAEMON_OPERATION_GET_ASR_DISPLAY_MENU_STATE => {
            Some(DaemonOperation::GetAsrDisplayMenuState)
        }
        DAEMON_OPERATION_SET_ACTIVE_ASR_PROVIDER => Some(DaemonOperation::SetActiveAsrProvider),
        DAEMON_OPERATION_SET_ACTIVE_ASR_TARGET => Some(DaemonOperation::SetActiveAsrTarget),
        DAEMON_OPERATION_GET_TEXT_ADAPTER_STATE => Some(DaemonOperation::GetTextAdapterState),
        DAEMON_OPERATION_START_ADAPTER => Some(DaemonOperation::StartAdapter),
        DAEMON_OPERATION_STOP_ADAPTER => Some(DaemonOperation::StopAdapter),
        DAEMON_OPERATION_GET_RUNTIME_STATUS => Some(DaemonOperation::GetRuntimeStatus),
        _ => None,
    }
}

fn boxed_response(value: StoredResponse) -> *mut VinputFcitxDaemonResponse {
    Box::into_raw(Box::new(VinputFcitxDaemonResponse { value: Some(value) }))
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
        if !error_out.is_null() {
            // SAFETY: Forwarded from this function's caller contract.
            unsafe { error_out.write(ptr::null_mut()) };
        }
        match DaemonClient::connect_session() {
            Ok(client) => Box::into_raw(Box::new(VinputFcitxDaemonClient { client })),
            Err(error) => {
                if !error_out.is_null() {
                    // SAFETY: Forwarded from this function's caller contract.
                    unsafe {
                        error_out.write(boxed_response(StoredResponse::Error(error.to_string())));
                    };
                }
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

/// Executes one daemon operation.
///
/// D-Bus failures return an owned error response. Invalid ABI inputs return null.
///
/// # Safety
///
/// `client` must be live and input byte pointers must reference their lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_daemon_client_call(
    client: *const VinputFcitxDaemonClient,
    operation_value: u8,
    first_data: *const u8,
    first_len: usize,
    second_data: *const u8,
    second_len: usize,
) -> *mut VinputFcitxDaemonResponse {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(client) = (unsafe { client.as_ref() }) else {
            return ptr::null_mut();
        };
        let Some(operation) = operation(operation_value) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(first) = (unsafe { text_input(first_data, first_len) }) else {
            return ptr::null_mut();
        };
        // SAFETY: Forwarded from this function's caller contract.
        let Some(second) = (unsafe { text_input(second_data, second_len) }) else {
            return ptr::null_mut();
        };
        let value = match client.client.call(operation, first, second) {
            Ok(response) => StoredResponse::Value(response),
            Err(error) => StoredResponse::Error(error.to_string()),
        };
        boxed_response(value)
    }))
    .unwrap_or(ptr::null_mut())
}

/// Releases a daemon response.
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

/// Borrows the response kind and scalar value.
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
    let Some(value) = (unsafe { response.as_ref() }).and_then(|response| response.value.as_ref())
    else {
        return 0;
    };
    let view = match value {
        StoredResponse::Error(error) => VinputFcitxDaemonResponseView {
            kind: DAEMON_RESPONSE_ERROR,
            bool_value: 0,
            text: string_view(error),
        },
        StoredResponse::Value(DaemonResponse::None) => VinputFcitxDaemonResponseView {
            kind: DAEMON_RESPONSE_NONE,
            bool_value: 0,
            text: string_view(""),
        },
        StoredResponse::Value(DaemonResponse::Text(text)) => VinputFcitxDaemonResponseView {
            kind: DAEMON_RESPONSE_TEXT,
            bool_value: 0,
            text: string_view(text),
        },
        StoredResponse::Value(DaemonResponse::Bool(value)) => VinputFcitxDaemonResponseView {
            kind: DAEMON_RESPONSE_BOOL,
            bool_value: u8::from(*value),
            text: string_view(""),
        },
        StoredResponse::Value(DaemonResponse::SceneSnapshot(_)) => VinputFcitxDaemonResponseView {
            kind: DAEMON_RESPONSE_SCENE_SNAPSHOT,
            bool_value: 0,
            text: string_view(""),
        },
        StoredResponse::Value(DaemonResponse::AsrDisplaySnapshot(_)) => {
            VinputFcitxDaemonResponseView {
                kind: DAEMON_RESPONSE_ASR_DISPLAY_SNAPSHOT,
                bool_value: 0,
                text: string_view(""),
            }
        }
    };
    // SAFETY: The caller guarantees a writable output pointer.
    unsafe { view_out.write(view) };
    1
}

/// Transfers a scene snapshot out of a response exactly once.
///
/// # Safety
///
/// `response` must be live and exclusively borrowed for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_daemon_response_take_scene_snapshot(
    response: *mut VinputFcitxDaemonResponse,
) -> *mut VinputFcitxSceneSnapshot {
    // SAFETY: Forwarded from this function's caller contract.
    let Some(response) = (unsafe { response.as_mut() }) else {
        return ptr::null_mut();
    };
    if !matches!(
        response.value,
        Some(StoredResponse::Value(DaemonResponse::SceneSnapshot(_)))
    ) {
        return ptr::null_mut();
    }
    let Some(StoredResponse::Value(DaemonResponse::SceneSnapshot(snapshot))) =
        response.value.take()
    else {
        return ptr::null_mut();
    };
    boxed_scene_snapshot(snapshot)
}

/// Transfers an ASR display snapshot out of a response exactly once.
///
/// # Safety
///
/// `response` must be live and exclusively borrowed for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_daemon_response_take_asr_display_snapshot(
    response: *mut VinputFcitxDaemonResponse,
) -> *mut VinputFcitxAsrDisplaySnapshot {
    // SAFETY: Forwarded from this function's caller contract.
    let Some(response) = (unsafe { response.as_mut() }) else {
        return ptr::null_mut();
    };
    if !matches!(
        response.value,
        Some(StoredResponse::Value(DaemonResponse::AsrDisplaySnapshot(_)))
    ) {
        return ptr::null_mut();
    }
    let Some(StoredResponse::Value(DaemonResponse::AsrDisplaySnapshot(snapshot))) =
        response.value.take()
    else {
        return ptr::null_mut();
    };
    boxed_asr_display_snapshot(snapshot)
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use vinput_fcitx_core::{AsrDisplaySnapshot, SceneSnapshot};
    use vinput_fcitx_dbus::DaemonResponse;

    use super::{
        DAEMON_RESPONSE_ASR_DISPLAY_SNAPSHOT, DAEMON_RESPONSE_BOOL, DAEMON_RESPONSE_ERROR,
        DAEMON_RESPONSE_SCENE_SNAPSHOT, DAEMON_RESPONSE_TEXT, StoredResponse,
        VinputFcitxDaemonResponseView, boxed_response, vinput_fcitx_daemon_response_free,
        vinput_fcitx_daemon_response_take_asr_display_snapshot,
        vinput_fcitx_daemon_response_take_scene_snapshot, vinput_fcitx_daemon_response_view,
    };
    use crate::{
        frontend::VinputFcitxStringView,
        menu_snapshot::{vinput_fcitx_asr_display_snapshot_free, vinput_fcitx_scene_snapshot_free},
    };

    unsafe fn bytes(view: VinputFcitxStringView) -> &'static [u8] {
        if view.data.is_null() {
            return &[];
        }
        // SAFETY: Test callers keep the response alive.
        unsafe { std::slice::from_raw_parts(view.data, view.len) }
    }

    unsafe fn view(
        response: *const super::VinputFcitxDaemonResponse,
    ) -> VinputFcitxDaemonResponseView {
        let mut view = VinputFcitxDaemonResponseView {
            kind: u8::MAX,
            bool_value: u8::MAX,
            text: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
        };
        // SAFETY: Test callers pass a live response and writable local output.
        assert_eq!(
            unsafe { vinput_fcitx_daemon_response_view(response, &raw mut view) },
            1
        );
        view
    }

    #[test]
    fn exposes_scalar_and_error_responses() {
        // SAFETY: Each response is live for all accesses and freed once.
        unsafe {
            let error = boxed_response(StoredResponse::Error("broken".to_owned()));
            let error_view = view(error);
            assert_eq!(error_view.kind, DAEMON_RESPONSE_ERROR);
            assert_eq!(bytes(error_view.text), b"broken");
            vinput_fcitx_daemon_response_free(error);

            let text = boxed_response(StoredResponse::Value(DaemonResponse::Text(
                "recording".to_owned(),
            )));
            let text_view = view(text);
            assert_eq!(text_view.kind, DAEMON_RESPONSE_TEXT);
            assert_eq!(bytes(text_view.text), b"recording");
            vinput_fcitx_daemon_response_free(text);

            let boolean = boxed_response(StoredResponse::Value(DaemonResponse::Bool(true)));
            let bool_view = view(boolean);
            assert_eq!(bool_view.kind, DAEMON_RESPONSE_BOOL);
            assert_eq!(bool_view.bool_value, 1);
            vinput_fcitx_daemon_response_free(boolean);
        }
    }

    #[test]
    fn transfers_snapshots_exactly_once() {
        // SAFETY: Responses and transferred handles are freed exactly once.
        unsafe {
            let scene = boxed_response(StoredResponse::Value(DaemonResponse::SceneSnapshot(
                SceneSnapshot::new("scene".to_owned()),
            )));
            assert_eq!(view(scene).kind, DAEMON_RESPONSE_SCENE_SNAPSHOT);
            let scene_snapshot = vinput_fcitx_daemon_response_take_scene_snapshot(scene);
            assert!(!scene_snapshot.is_null());
            assert!(vinput_fcitx_daemon_response_take_scene_snapshot(scene).is_null());
            assert_eq!(vinput_fcitx_daemon_response_view(scene, ptr::null_mut()), 0);
            vinput_fcitx_scene_snapshot_free(scene_snapshot);
            vinput_fcitx_daemon_response_free(scene);

            let asr = boxed_response(StoredResponse::Value(DaemonResponse::AsrDisplaySnapshot(
                AsrDisplaySnapshot::default(),
            )));
            assert_eq!(view(asr).kind, DAEMON_RESPONSE_ASR_DISPLAY_SNAPSHOT);
            let asr_snapshot = vinput_fcitx_daemon_response_take_asr_display_snapshot(asr);
            assert!(!asr_snapshot.is_null());
            assert!(vinput_fcitx_daemon_response_take_asr_display_snapshot(asr).is_null());
            vinput_fcitx_asr_display_snapshot_free(asr_snapshot);
            vinput_fcitx_daemon_response_free(asr);
        }
    }
}

//! Raw-pointer C ABI implementation.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use vinput_fcitx_core::{CandidateSource, CommitPlan, FrontendState, make_commit_plan};

/// Opaque recognition commit plan owned by Rust.
pub struct VinputFcitxCommitPlan {
    plan: CommitPlan,
}

/// Opaque frontend session state owned by Rust.
pub struct VinputFcitxFrontendState {
    state: FrontendState,
}

const CANDIDATE_SOURCE_RAW: u8 = 0;
const CANDIDATE_SOURCE_LLM: u8 = 1;
const CANDIDATE_SOURCE_ASR: u8 = 2;
const CANDIDATE_SOURCE_CANCEL: u8 = 3;

unsafe fn json_input<'a>(data: *const u8, len: usize) -> Option<&'a str> {
    if data.is_null() {
        return (len == 0).then_some("");
    }

    // SAFETY: The caller guarantees that `data` points to `len` readable bytes
    // for the duration of this call.
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    Some(std::str::from_utf8(bytes).unwrap_or_default())
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

unsafe fn optional_text_input<'a>(
    data: *const u8,
    len: usize,
    present: u8,
) -> Result<Option<&'a str>, ()> {
    if present == 0 {
        return Ok(None);
    }

    // SAFETY: Forwarded from the caller contract.
    unsafe { text_input(data, len) }.map(Some).ok_or(())
}

unsafe fn state_ref<'a>(
    state: *const VinputFcitxFrontendState,
) -> Option<&'a VinputFcitxFrontendState> {
    // SAFETY: The caller guarantees that a non-null pointer was returned by
    // `vinput_fcitx_frontend_state_new` and has not been freed.
    unsafe { state.as_ref() }
}

unsafe fn state_mut<'a>(
    state: *mut VinputFcitxFrontendState,
) -> Option<&'a mut VinputFcitxFrontendState> {
    // SAFETY: The caller guarantees exclusive access to a live state handle.
    unsafe { state.as_mut() }
}
unsafe fn plan_ref<'a>(plan: *const VinputFcitxCommitPlan) -> Option<&'a VinputFcitxCommitPlan> {
    // SAFETY: The caller guarantees that a non-null pointer was returned by
    // `vinput_fcitx_commit_plan_new` and has not been freed.
    unsafe { plan.as_ref() }
}

fn string_data(value: &str) -> *const u8 {
    if value.is_empty() {
        ptr::null()
    } else {
        value.as_ptr()
    }
}

fn source_code(source: CandidateSource) -> u8 {
    match source {
        CandidateSource::Raw => CANDIDATE_SOURCE_RAW,
        CandidateSource::Llm => CANDIDATE_SOURCE_LLM,
        CandidateSource::Asr => CANDIDATE_SOURCE_ASR,
        CandidateSource::Cancel => CANDIDATE_SOURCE_CANCEL,
    }
}

/// Creates an idle frontend session-state handle.
#[unsafe(no_mangle)]
pub extern "C" fn vinput_fcitx_frontend_state_new() -> *mut VinputFcitxFrontendState {
    catch_unwind(|| {
        Box::into_raw(Box::new(VinputFcitxFrontendState {
            state: FrontendState::default(),
        }))
    })
    .unwrap_or(ptr::null_mut())
}

/// Releases a frontend session-state handle.
///
/// A null handle is ignored.
///
/// # Safety
///
/// A non-null `state` must be a live handle returned by this crate and must not
/// be freed more than once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_state_free(state: *mut VinputFcitxFrontendState) {
    if state.is_null() {
        return;
    }

    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        drop(unsafe { Box::from_raw(state) });
    }));
}

/// Returns one when a frontend recording session is active.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_state_recording(
    state: *const VinputFcitxFrontendState,
) -> u8 {
    // SAFETY: Forwarded from this function's caller contract.
    u8::from(unsafe { state_ref(state) }.is_some_and(|state| state.state.recording()))
}

/// Returns one when the active frontend session is command mode.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_state_command_mode(
    state: *const VinputFcitxFrontendState,
) -> u8 {
    // SAFETY: Forwarded from this function's caller contract.
    u8::from(unsafe { state_ref(state) }.is_some_and(|state| state.state.command_mode()))
}

/// Returns one when the current session captured an explicit scene id.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_state_has_active_scene(
    state: *const VinputFcitxFrontendState,
) -> u8 {
    // SAFETY: Forwarded from this function's caller contract.
    u8::from(
        unsafe { state_ref(state) }.is_some_and(|state| state.state.active_scene_id().is_some()),
    )
}

/// Returns the active scene-id byte pointer owned by `state`.
///
/// The pointer remains valid until the state is mutated or freed. An absent or
/// empty scene id returns null.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_state_active_scene_data(
    state: *const VinputFcitxFrontendState,
) -> *const u8 {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { state_ref(state) }
        .and_then(|state| state.state.active_scene_id())
        .map_or(ptr::null(), string_data)
}

/// Returns the active scene-id length in bytes.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_state_active_scene_len(
    state: *const VinputFcitxFrontendState,
) -> usize {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { state_ref(state) }
        .and_then(|state| state.state.active_scene_id())
        .map_or(0, str::len)
}

/// Records a successful normal-dictation start.
///
/// Returns zero for an invalid handle, invalid UTF-8, or a caught Rust panic.
///
/// # Safety
///
/// When `has_scene` is nonzero, `scene_data` must point to `scene_len` readable
/// bytes for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_state_start_normal(
    state: *mut VinputFcitxFrontendState,
    scene_data: *const u8,
    scene_len: usize,
    has_scene: u8,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(state) = (unsafe { state_mut(state) }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Ok(scene_id) = (unsafe { optional_text_input(scene_data, scene_len, has_scene) })
            else {
                return false;
            };
            state.state.start_normal(scene_id);
            true
        }))
        .unwrap_or(false),
    )
}

/// Records a successful command-mode start.
///
/// Returns zero for an invalid handle, invalid UTF-8, or a caught Rust panic.
///
/// # Safety
///
/// When `has_scene` is nonzero, `scene_data` must point to `scene_len` readable
/// bytes for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_state_start_command(
    state: *mut VinputFcitxFrontendState,
    scene_data: *const u8,
    scene_len: usize,
    has_scene: u8,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(state) = (unsafe { state_mut(state) }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Ok(scene_id) = (unsafe { optional_text_input(scene_data, scene_len, has_scene) })
            else {
                return false;
            };
            state.state.start_command(scene_id);
            true
        }))
        .unwrap_or(false),
    )
}

/// Adopts a recording session already active in the daemon.
///
/// Returns zero for an invalid handle, invalid UTF-8, or a caught Rust panic.
///
/// # Safety
///
/// `scene_data` must point to `scene_len` readable bytes, unless both are
/// null/zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_state_adopt(
    state: *mut VinputFcitxFrontendState,
    command_mode: u8,
    scene_data: *const u8,
    scene_len: usize,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(state) = (unsafe { state_mut(state) }) else {
                return false;
            };
            // SAFETY: Forwarded from this function's caller contract.
            let Some(scene_id) = (unsafe { text_input(scene_data, scene_len) }) else {
                return false;
            };
            state.state.adopt_recording(command_mode != 0, scene_id);
            true
        }))
        .unwrap_or(false),
    )
}

/// Resets a frontend session-state handle to idle.
///
/// Returns zero for an invalid handle or a caught Rust panic.
///
/// # Safety
///
/// `state` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_frontend_state_reset(
    state: *mut VinputFcitxFrontendState,
) -> u8 {
    u8::from(
        catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forwarded from this function's caller contract.
            let Some(state) = (unsafe { state_mut(state) }) else {
                return false;
            };
            state.state.reset();
            true
        }))
        .unwrap_or(false),
    )
}
/// Parses recognition JSON and returns an owned Rust commit-plan handle.
///
/// Returns null when the input pointer contract is invalid or Rust panics while
/// constructing the plan. Invalid JSON itself remains a valid empty plan for
/// compatibility with the retained frontend behavior.
///
/// # Safety
///
/// `json_data` must point to `json_len` readable bytes, unless both are null/zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_commit_plan_new(
    json_data: *const u8,
    json_len: usize,
    command_mode: u8,
) -> *mut VinputFcitxCommitPlan {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        let Some(json) = (unsafe { json_input(json_data, json_len) }) else {
            return ptr::null_mut();
        };
        let plan = make_commit_plan(json, command_mode != 0);
        Box::into_raw(Box::new(VinputFcitxCommitPlan { plan }))
    }))
    .unwrap_or(ptr::null_mut())
}

/// Releases a commit-plan handle returned by `vinput_fcitx_commit_plan_new`.
///
/// A null handle is ignored.
///
/// # Safety
///
/// A non-null `plan` must be a live handle returned by this crate and must not
/// be freed more than once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_commit_plan_free(plan: *mut VinputFcitxCommitPlan) {
    if plan.is_null() {
        return;
    }

    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Forwarded from this function's caller contract.
        drop(unsafe { Box::from_raw(plan) });
    }));
}

/// Returns one when the frontend should show a candidate menu.
///
/// # Safety
///
/// `plan` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_commit_plan_show_candidate_menu(
    plan: *const VinputFcitxCommitPlan,
) -> u8 {
    // SAFETY: Forwarded from this function's caller contract.
    u8::from(unsafe { plan_ref(plan) }.is_some_and(|plan| plan.plan.show_candidate_menu))
}

/// Returns the recognition commit-text byte pointer owned by `plan`.
///
/// The pointer remains valid until the plan is freed. Empty text returns null.
///
/// # Safety
///
/// `plan` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_commit_plan_text_data(
    plan: *const VinputFcitxCommitPlan,
) -> *const u8 {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { plan_ref(plan) }.map_or(ptr::null(), |plan| {
        string_data(&plan.plan.payload.commit_text)
    })
}

/// Returns the recognition commit-text length in bytes.
///
/// # Safety
///
/// `plan` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_commit_plan_text_len(
    plan: *const VinputFcitxCommitPlan,
) -> usize {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { plan_ref(plan) }.map_or(0, |plan| plan.plan.payload.commit_text.len())
}

/// Returns the number of recognition candidates in `plan`.
///
/// # Safety
///
/// `plan` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_commit_plan_candidate_count(
    plan: *const VinputFcitxCommitPlan,
) -> usize {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { plan_ref(plan) }.map_or(0, |plan| plan.plan.payload.candidates.len())
}

/// Returns the candidate text byte pointer for `index`.
///
/// The pointer remains valid until the plan is freed. An invalid index or empty
/// text returns null.
///
/// # Safety
///
/// `plan` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_commit_plan_candidate_text_data(
    plan: *const VinputFcitxCommitPlan,
    index: usize,
) -> *const u8 {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { plan_ref(plan) }
        .and_then(|plan| plan.plan.payload.candidates.get(index))
        .map_or(ptr::null(), |candidate| string_data(&candidate.text))
}

/// Returns the candidate text length in bytes for `index`.
///
/// # Safety
///
/// `plan` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_commit_plan_candidate_text_len(
    plan: *const VinputFcitxCommitPlan,
    index: usize,
) -> usize {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { plan_ref(plan) }
        .and_then(|plan| plan.plan.payload.candidates.get(index))
        .map_or(0, |candidate| candidate.text.len())
}

/// Returns the candidate source code for `index`.
///
/// Unknown handles or indexes return the raw source code.
///
/// # Safety
///
/// `plan` must be null or a live handle returned by this crate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinput_fcitx_commit_plan_candidate_source(
    plan: *const VinputFcitxCommitPlan,
    index: usize,
) -> u8 {
    // SAFETY: Forwarded from this function's caller contract.
    unsafe { plan_ref(plan) }
        .and_then(|plan| plan.plan.payload.candidates.get(index))
        .map_or(CANDIDATE_SOURCE_RAW, |candidate| {
            source_code(candidate.source)
        })
}

#[cfg(test)]
mod tests {
    use super::{
        CANDIDATE_SOURCE_ASR, vinput_fcitx_commit_plan_candidate_count,
        vinput_fcitx_commit_plan_candidate_source, vinput_fcitx_commit_plan_candidate_text_data,
        vinput_fcitx_commit_plan_candidate_text_len, vinput_fcitx_commit_plan_free,
        vinput_fcitx_commit_plan_new, vinput_fcitx_commit_plan_show_candidate_menu,
        vinput_fcitx_commit_plan_text_data, vinput_fcitx_commit_plan_text_len,
        vinput_fcitx_frontend_state_active_scene_data,
        vinput_fcitx_frontend_state_active_scene_len, vinput_fcitx_frontend_state_adopt,
        vinput_fcitx_frontend_state_command_mode, vinput_fcitx_frontend_state_free,
        vinput_fcitx_frontend_state_has_active_scene, vinput_fcitx_frontend_state_new,
        vinput_fcitx_frontend_state_recording, vinput_fcitx_frontend_state_reset,
        vinput_fcitx_frontend_state_start_command, vinput_fcitx_frontend_state_start_normal,
    };

    unsafe fn bytes_from_view<'a>(data: *const u8, len: usize) -> &'a [u8] {
        if data.is_null() {
            return &[];
        }
        // SAFETY: Test callers keep the owning plan alive for the view lifetime.
        unsafe { std::slice::from_raw_parts(data, len) }
    }

    #[test]
    fn exposes_owned_plan_through_stable_views() {
        let json = br#"{"commit_text":"selected","candidates":[{"text":"selected","source":"raw"},{"text":"command","source":"asr"}]}"#;

        // SAFETY: The input points to the live `json` byte slice, and the handle
        // is used until exactly one matching free call.
        unsafe {
            let plan = vinput_fcitx_commit_plan_new(json.as_ptr(), json.len(), 1);
            assert!(!plan.is_null());
            assert_eq!(vinput_fcitx_commit_plan_show_candidate_menu(plan), 1);
            assert_eq!(vinput_fcitx_commit_plan_candidate_count(plan), 2);
            assert_eq!(
                vinput_fcitx_commit_plan_candidate_source(plan, 1),
                CANDIDATE_SOURCE_ASR
            );

            let text = bytes_from_view(
                vinput_fcitx_commit_plan_text_data(plan),
                vinput_fcitx_commit_plan_text_len(plan),
            );
            assert_eq!(text, b"selected");

            let candidate = bytes_from_view(
                vinput_fcitx_commit_plan_candidate_text_data(plan, 1),
                vinput_fcitx_commit_plan_candidate_text_len(plan, 1),
            );
            assert_eq!(candidate, b"command");

            vinput_fcitx_commit_plan_free(plan);
        }
    }

    #[test]
    fn null_input_with_nonzero_length_fails_closed() {
        // SAFETY: Null is deliberately passed to exercise the guarded invalid
        // pointer contract; the implementation does not dereference it.
        let plan = unsafe { vinput_fcitx_commit_plan_new(std::ptr::null(), 1, 0) };
        assert!(plan.is_null());
    }

    #[test]
    fn exposes_frontend_session_state_through_stable_views() {
        let normal_scene = b"scene-a";
        let adopted_scene = b"remote-scene";

        // SAFETY: Every byte view points to a live local slice, and the state
        // handle is released exactly once after its final use.
        unsafe {
            let state = vinput_fcitx_frontend_state_new();
            assert!(!state.is_null());
            assert_eq!(vinput_fcitx_frontend_state_recording(state), 0);
            assert_eq!(vinput_fcitx_frontend_state_command_mode(state), 0);
            assert_eq!(vinput_fcitx_frontend_state_has_active_scene(state), 0);

            assert_eq!(
                vinput_fcitx_frontend_state_start_normal(
                    state,
                    normal_scene.as_ptr(),
                    normal_scene.len(),
                    1,
                ),
                1
            );
            assert_eq!(vinput_fcitx_frontend_state_recording(state), 1);
            assert_eq!(vinput_fcitx_frontend_state_command_mode(state), 0);
            assert_eq!(vinput_fcitx_frontend_state_has_active_scene(state), 1);
            assert_eq!(
                bytes_from_view(
                    vinput_fcitx_frontend_state_active_scene_data(state),
                    vinput_fcitx_frontend_state_active_scene_len(state),
                ),
                normal_scene
            );

            assert_eq!(
                vinput_fcitx_frontend_state_start_command(state, std::ptr::null(), 0, 0,),
                1
            );
            assert_eq!(vinput_fcitx_frontend_state_command_mode(state), 1);
            assert_eq!(vinput_fcitx_frontend_state_has_active_scene(state), 0);

            assert_eq!(
                vinput_fcitx_frontend_state_adopt(
                    state,
                    1,
                    adopted_scene.as_ptr(),
                    adopted_scene.len(),
                ),
                1
            );
            assert_eq!(vinput_fcitx_frontend_state_recording(state), 1);
            assert_eq!(vinput_fcitx_frontend_state_command_mode(state), 1);
            assert_eq!(
                bytes_from_view(
                    vinput_fcitx_frontend_state_active_scene_data(state),
                    vinput_fcitx_frontend_state_active_scene_len(state),
                ),
                adopted_scene
            );

            assert_eq!(vinput_fcitx_frontend_state_reset(state), 1);
            assert_eq!(vinput_fcitx_frontend_state_recording(state), 0);
            assert_eq!(vinput_fcitx_frontend_state_command_mode(state), 0);
            assert_eq!(vinput_fcitx_frontend_state_has_active_scene(state), 0);

            vinput_fcitx_frontend_state_free(state);
        }
    }

    #[test]
    fn invalid_utf8_does_not_update_frontend_state() {
        let invalid_scene = [0xff];

        // SAFETY: The invalid byte slice is live for the call and deliberately
        // exercises UTF-8 validation; the state handle is freed exactly once.
        unsafe {
            let state = vinput_fcitx_frontend_state_new();
            assert!(!state.is_null());
            assert_eq!(
                vinput_fcitx_frontend_state_start_normal(
                    state,
                    invalid_scene.as_ptr(),
                    invalid_scene.len(),
                    1,
                ),
                0
            );
            assert_eq!(vinput_fcitx_frontend_state_recording(state), 0);
            assert_eq!(vinput_fcitx_frontend_state_has_active_scene(state), 0);
            vinput_fcitx_frontend_state_free(state);
        }
    }
}

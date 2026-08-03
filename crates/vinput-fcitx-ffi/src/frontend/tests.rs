use std::ptr;

use super::{
    FRONTEND_CALL_START_COMMAND, FRONTEND_CALL_START_NORMAL, FRONTEND_CALL_STOP,
    FRONTEND_STEP_CALL_READY, FRONTEND_STEP_OUTCOME_READY, VinputFcitxCandidateView,
    VinputFcitxFrontendOutcome, VinputFcitxFrontendOutcomeView, VinputFcitxStringView,
    vinput_fcitx_frontend_controller_adopt, vinput_fcitx_frontend_controller_command_mode,
    vinput_fcitx_frontend_controller_complete, vinput_fcitx_frontend_controller_free,
    vinput_fcitx_frontend_controller_new, vinput_fcitx_frontend_controller_pending_call,
    vinput_fcitx_frontend_controller_recording, vinput_fcitx_frontend_controller_start_command,
    vinput_fcitx_frontend_controller_start_normal, vinput_fcitx_frontend_controller_stop,
    vinput_fcitx_frontend_outcome_candidate, vinput_fcitx_frontend_outcome_free,
    vinput_fcitx_frontend_outcome_from_payload, vinput_fcitx_frontend_outcome_view,
};

unsafe fn bytes(view: VinputFcitxStringView) -> &'static [u8] {
    if view.data.is_null() {
        return &[];
    }
    // SAFETY: Test callers keep the owning handle alive.
    unsafe { std::slice::from_raw_parts(view.data, view.len) }
}

unsafe fn outcome_view(outcome: *mut VinputFcitxFrontendOutcome) -> VinputFcitxFrontendOutcomeView {
    let mut view = VinputFcitxFrontendOutcomeView {
        kind: 0,
        command_mode: 0,
        text: VinputFcitxStringView {
            data: ptr::null(),
            len: 0,
        },
        commit_text: VinputFcitxStringView {
            data: ptr::null(),
            len: 0,
        },
        candidate_count: 0,
    };
    // SAFETY: Test callers pass live handles and writable local output.
    assert_eq!(
        unsafe { vinput_fcitx_frontend_outcome_view(outcome, &raw mut view) },
        1
    );
    view
}

#[test]
fn drives_normal_start_and_stop_through_compact_views() {
    // SAFETY: All byte views remain alive and handles are freed exactly once.
    unsafe {
        let controller = vinput_fcitx_frontend_controller_new();
        assert!(!controller.is_null());
        let mut immediate = ptr::null_mut();
        assert_eq!(
            vinput_fcitx_frontend_controller_start_normal(
                controller,
                b"started".as_ptr(),
                7,
                1,
                &raw mut immediate,
            ),
            FRONTEND_STEP_CALL_READY
        );
        assert!(immediate.is_null());

        let mut call_kind = 0;
        let mut argument = VinputFcitxStringView {
            data: ptr::null(),
            len: 0,
        };
        assert_eq!(
            vinput_fcitx_frontend_controller_pending_call(
                controller,
                &raw mut call_kind,
                &raw mut argument,
            ),
            1
        );
        assert_eq!(call_kind, FRONTEND_CALL_START_NORMAL);
        assert!(bytes(argument).is_empty());

        let start = vinput_fcitx_frontend_controller_complete(controller, 1, ptr::null(), 0);
        assert!(!start.is_null());
        let view = outcome_view(start);
        assert_eq!(view.kind, 1);
        assert_eq!(bytes(view.text), b"... Recording ...");
        vinput_fcitx_frontend_outcome_free(start);
        assert_eq!(vinput_fcitx_frontend_controller_recording(controller), 1);

        assert_eq!(
            vinput_fcitx_frontend_controller_stop(
                controller,
                b"fallback".as_ptr(),
                8,
                &raw mut immediate,
            ),
            FRONTEND_STEP_CALL_READY
        );
        assert_eq!(
            vinput_fcitx_frontend_controller_pending_call(
                controller,
                &raw mut call_kind,
                &raw mut argument,
            ),
            1
        );
        assert_eq!(call_kind, FRONTEND_CALL_STOP);
        assert_eq!(bytes(argument), b"started");

        let payload = br#"{"commit_text":"done","candidates":[{"text":"done","source":"asr"}]}"#;
        let stop = vinput_fcitx_frontend_controller_complete(
            controller,
            1,
            payload.as_ptr(),
            payload.len(),
        );
        let view = outcome_view(stop);
        assert_eq!(view.kind, 3);
        assert_eq!(bytes(view.text), b"done");
        assert_eq!(bytes(view.commit_text), b"done");
        assert_eq!(view.candidate_count, 1);
        let mut candidate = VinputFcitxCandidateView {
            text: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
            source: 0,
        };
        assert_eq!(
            vinput_fcitx_frontend_outcome_candidate(stop, 0, &raw mut candidate),
            1
        );
        assert_eq!(bytes(candidate.text), b"done");
        assert_eq!(candidate.source, 2);
        vinput_fcitx_frontend_outcome_free(stop);
        assert_eq!(vinput_fcitx_frontend_controller_recording(controller), 0);
        vinput_fcitx_frontend_controller_free(controller);
    }
}

#[test]
fn handles_immediate_command_error_and_daemon_failure() {
    // SAFETY: All byte views remain alive and handles are freed exactly once.
    unsafe {
        let controller = vinput_fcitx_frontend_controller_new();
        let mut outcome = ptr::null_mut();
        assert_eq!(
            vinput_fcitx_frontend_controller_start_command(
                controller,
                ptr::null(),
                0,
                ptr::null(),
                0,
                0,
                &raw mut outcome,
            ),
            FRONTEND_STEP_OUTCOME_READY
        );
        let view = outcome_view(outcome);
        assert_eq!(view.kind, 5);
        assert_eq!(bytes(view.text), b"Please select text first.");
        vinput_fcitx_frontend_outcome_free(outcome);

        assert_eq!(
            vinput_fcitx_frontend_controller_start_command(
                controller,
                b"selected".as_ptr(),
                8,
                ptr::null(),
                0,
                0,
                &raw mut outcome,
            ),
            FRONTEND_STEP_CALL_READY
        );
        let mut call_kind = 0;
        let mut argument = VinputFcitxStringView {
            data: ptr::null(),
            len: 0,
        };
        assert_eq!(
            vinput_fcitx_frontend_controller_pending_call(
                controller,
                &raw mut call_kind,
                &raw mut argument,
            ),
            1
        );
        assert_eq!(call_kind, FRONTEND_CALL_START_COMMAND);
        assert_eq!(bytes(argument), b"selected");
        let failed = vinput_fcitx_frontend_controller_complete(controller, 0, ptr::null(), 0);
        let view = outcome_view(failed);
        assert_eq!(view.kind, 5);
        assert_eq!(bytes(view.text), b"Voice input daemon is unavailable.");
        assert_eq!(vinput_fcitx_frontend_controller_recording(controller), 0);
        vinput_fcitx_frontend_outcome_free(failed);
        vinput_fcitx_frontend_controller_free(controller);
    }
}

#[test]
fn adopts_command_session_and_builds_candidate_outcome() {
    // SAFETY: All byte views remain alive and handles are freed exactly once.
    unsafe {
        let controller = vinput_fcitx_frontend_controller_new();
        assert_eq!(
            vinput_fcitx_frontend_controller_adopt(controller, 1, b"remote".as_ptr(), 6,),
            1
        );
        assert_eq!(vinput_fcitx_frontend_controller_command_mode(controller), 1);

        let payload = br#"{"commit_text":"selected","candidates":[{"text":"selected","source":"raw"},{"text":"changed","source":"asr"}]}"#;
        let outcome =
            vinput_fcitx_frontend_outcome_from_payload(payload.as_ptr(), payload.len(), 1);
        let view = outcome_view(outcome);
        assert_eq!(view.kind, 4);
        assert_eq!(view.command_mode, 1);
        assert_eq!(view.candidate_count, 2);
        vinput_fcitx_frontend_outcome_free(outcome);
        vinput_fcitx_frontend_controller_free(controller);
    }
}

#[test]
fn rejects_invalid_utf8_without_mutating_controller() {
    // SAFETY: All byte views remain alive and handles are freed exactly once.
    unsafe {
        let controller = vinput_fcitx_frontend_controller_new();
        let mut outcome = ptr::null_mut();
        let invalid = [0xff];
        assert_eq!(
            vinput_fcitx_frontend_controller_start_normal(
                controller,
                invalid.as_ptr(),
                invalid.len(),
                1,
                &raw mut outcome,
            ),
            0
        );
        assert!(outcome.is_null());
        assert_eq!(vinput_fcitx_frontend_controller_recording(controller), 0);
        vinput_fcitx_frontend_controller_free(controller);
    }
}

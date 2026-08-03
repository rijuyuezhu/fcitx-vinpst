use std::ptr;

use vinput_fcitx_core::{FrontendController, FrontendOutcomeKind};
use vinput_fcitx_dbus::{DaemonOperation, DaemonResponse};

use super::{
    FRONTEND_TRIGGER_INTENT_NONE, FRONTEND_TRIGGER_INTENT_SHOW_ASR_MENU,
    FRONTEND_TRIGGER_INTENT_SHOW_SCENE_MENU, FRONTEND_TRIGGER_INTENT_START_NORMAL,
    FRONTEND_TRIGGER_INTENT_STOP_COMMAND, FRONTEND_TRIGGER_INTENT_STOP_NORMAL,
    FRONTEND_TRIGGER_REQUEST_SHOW_ASR_MENU, FRONTEND_TRIGGER_REQUEST_SHOW_SCENE_MENU,
    FRONTEND_TRIGGER_REQUEST_START_NORMAL, FRONTEND_TRIGGER_REQUEST_STOP_COMMAND,
    FRONTEND_TRIGGER_REQUEST_STOP_NORMAL, VinputFcitxCandidateView, VinputFcitxFrontendOutcome,
    VinputFcitxFrontendOutcomeView, VinputFcitxStringView, execute_step_with,
    vinput_fcitx_frontend_controller_adopt_and_stop_with_daemon,
    vinput_fcitx_frontend_controller_command_mode, vinput_fcitx_frontend_controller_free,
    vinput_fcitx_frontend_controller_new, vinput_fcitx_frontend_controller_plan_trigger,
    vinput_fcitx_frontend_controller_recording,
    vinput_fcitx_frontend_controller_start_command_with_daemon,
    vinput_fcitx_frontend_controller_start_normal_with_daemon,
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
fn builds_candidate_outcome_views() {
    // SAFETY: Input bytes remain alive and the outcome handle is freed exactly once.
    unsafe {
        let payload = br#"{"commit_text":"selected","candidates":[{"text":"selected","source":"raw"},{"text":"changed","source":"asr"}]}"#;
        let outcome =
            vinput_fcitx_frontend_outcome_from_payload(payload.as_ptr(), payload.len(), 1);
        assert!(!outcome.is_null());
        let view = outcome_view(outcome);
        assert_eq!(view.kind, 4);
        assert_eq!(view.command_mode, 1);
        assert_eq!(bytes(view.commit_text), b"selected");
        assert_eq!(view.candidate_count, 2);

        let mut candidate = VinputFcitxCandidateView {
            text: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
            source: 0,
        };
        assert_eq!(
            vinput_fcitx_frontend_outcome_candidate(outcome, 1, &raw mut candidate),
            1
        );
        assert_eq!(bytes(candidate.text), b"changed");
        assert_eq!(candidate.source, 2);
        vinput_fcitx_frontend_outcome_free(outcome);
    }
}

#[test]
fn direct_exports_validate_inputs_and_immediate_errors() {
    // SAFETY: The controller is live and all returned outcomes are freed exactly once.
    unsafe {
        let controller = vinput_fcitx_frontend_controller_new();
        assert!(!controller.is_null());

        let invalid = [0xff];
        let invalid_outcome = vinput_fcitx_frontend_controller_start_normal_with_daemon(
            controller,
            ptr::null(),
            invalid.as_ptr(),
            invalid.len(),
        );
        assert!(invalid_outcome.is_null());
        assert_eq!(vinput_fcitx_frontend_controller_recording(controller), 0);

        let immediate = vinput_fcitx_frontend_controller_start_command_with_daemon(
            controller,
            ptr::null(),
            ptr::null(),
            0,
            b"command".as_ptr(),
            7,
        );
        assert!(!immediate.is_null());
        let view = outcome_view(immediate);
        assert_eq!(view.kind, 5);
        assert_eq!(bytes(view.text), b"Please select text first.");
        vinput_fcitx_frontend_outcome_free(immediate);

        let adopted = vinput_fcitx_frontend_controller_adopt_and_stop_with_daemon(
            controller,
            ptr::null(),
            1,
            b"remote".as_ptr(),
            6,
        );
        assert!(!adopted.is_null());
        let view = outcome_view(adopted);
        assert_eq!(view.kind, 5);
        assert_eq!(bytes(view.text), b"Voice input daemon is unavailable.");
        assert_eq!(vinput_fcitx_frontend_controller_recording(controller), 0);
        assert_eq!(vinput_fcitx_frontend_controller_command_mode(controller), 0);
        vinput_fcitx_frontend_outcome_free(adopted);
        vinput_fcitx_frontend_controller_free(controller);
    }
}

#[test]
fn gates_semantic_trigger_requests_through_controller_state() {
    // SAFETY: The controller is live for all calls and freed exactly once.
    unsafe {
        let controller = vinput_fcitx_frontend_controller_new();
        let mut intent = u8::MAX;
        assert_eq!(
            vinput_fcitx_frontend_controller_plan_trigger(
                controller,
                FRONTEND_TRIGGER_REQUEST_START_NORMAL,
                &raw mut intent,
            ),
            1,
        );
        assert_eq!(intent, FRONTEND_TRIGGER_INTENT_START_NORMAL);
        assert_eq!(
            vinput_fcitx_frontend_controller_plan_trigger(
                controller,
                FRONTEND_TRIGGER_REQUEST_SHOW_SCENE_MENU,
                &raw mut intent,
            ),
            1,
        );
        assert_eq!(intent, FRONTEND_TRIGGER_INTENT_SHOW_SCENE_MENU);
        assert_eq!(
            vinput_fcitx_frontend_controller_plan_trigger(
                controller,
                FRONTEND_TRIGGER_REQUEST_SHOW_ASR_MENU,
                &raw mut intent,
            ),
            1,
        );
        assert_eq!(intent, FRONTEND_TRIGGER_INTENT_SHOW_ASR_MENU);

        (*controller).controller.adopt_recording(false, "normal");
        assert_eq!(
            vinput_fcitx_frontend_controller_plan_trigger(
                controller,
                FRONTEND_TRIGGER_REQUEST_STOP_NORMAL,
                &raw mut intent,
            ),
            1,
        );
        assert_eq!(intent, FRONTEND_TRIGGER_INTENT_STOP_NORMAL);
        assert_eq!(
            vinput_fcitx_frontend_controller_plan_trigger(
                controller,
                FRONTEND_TRIGGER_REQUEST_SHOW_SCENE_MENU,
                &raw mut intent,
            ),
            1,
        );
        assert_eq!(intent, FRONTEND_TRIGGER_INTENT_NONE);

        (*controller).controller.adopt_recording(true, "command");
        assert_eq!(
            vinput_fcitx_frontend_controller_plan_trigger(
                controller,
                FRONTEND_TRIGGER_REQUEST_STOP_COMMAND,
                &raw mut intent,
            ),
            1,
        );
        assert_eq!(intent, FRONTEND_TRIGGER_INTENT_STOP_COMMAND);
        assert_eq!(
            vinput_fcitx_frontend_controller_plan_trigger(controller, 99, &raw mut intent),
            0,
        );
        assert_eq!(intent, FRONTEND_TRIGGER_INTENT_STOP_COMMAND);
        vinput_fcitx_frontend_controller_free(controller);
    }
}

#[test]
fn executes_frontend_daemon_calls_without_leaving_rust() {
    let mut controller = FrontendController::default();
    let start_step = controller.start_normal(Some("started-scene"));
    let start = execute_step_with(&mut controller, start_step, |operation, argument| {
        assert_eq!(operation, DaemonOperation::StartRecording);
        assert!(argument.is_empty());
        Ok(DaemonResponse::None)
    });
    assert_eq!(start.kind(), FrontendOutcomeKind::Preedit);
    assert_eq!(start.text(), "... Recording ...");
    assert!(controller.recording());

    let stop_step = controller.stop("fallback-scene");
    let payload = r#"{"commit_text":"done","candidates":[{"text":"done","source":"asr"}]}"#;
    let stop = execute_step_with(&mut controller, stop_step, |operation, argument| {
        assert_eq!(operation, DaemonOperation::StopRecording);
        assert_eq!(argument, "started-scene");
        Ok(DaemonResponse::Text(payload.to_owned()))
    });
    assert_eq!(stop.kind(), FrontendOutcomeKind::Commit);
    assert_eq!(stop.text(), "done");
    assert!(!controller.recording());
}

#[test]
fn direct_execution_preserves_immediate_daemon_and_adoption_outcomes() {
    let mut controller = FrontendController::default();
    let immediate_step = controller.start_command("", Some("command-scene"));
    let immediate = execute_step_with(&mut controller, immediate_step, |_, _| {
        panic!("immediate command validation must not call the daemon")
    });
    assert_eq!(immediate.kind(), FrontendOutcomeKind::Error);
    assert_eq!(immediate.text(), "Please select text first.");

    let start_step = controller.start_command("selected", Some("command-scene"));
    let failed = execute_step_with(&mut controller, start_step, |operation, argument| {
        assert_eq!(operation, DaemonOperation::StartCommandRecording);
        assert_eq!(argument, "selected");
        Err("daemon failed".to_owned())
    });
    assert_eq!(failed.kind(), FrontendOutcomeKind::Error);
    assert_eq!(failed.text(), "daemon failed");
    assert!(!controller.recording());

    controller.adopt_recording(false, "remote-scene");
    let stop_step = controller.stop("fallback-scene");
    let adopted = execute_step_with(&mut controller, stop_step, |operation, argument| {
        assert_eq!(operation, DaemonOperation::StopRecording);
        assert_eq!(argument, "remote-scene");
        Ok(DaemonResponse::Text(
            r#"{"commit_text":"remote","candidates":[]}"#.to_owned(),
        ))
    });
    assert_eq!(adopted.kind(), FrontendOutcomeKind::Commit);
    assert_eq!(adopted.text(), "remote");
    assert!(!controller.recording());
}

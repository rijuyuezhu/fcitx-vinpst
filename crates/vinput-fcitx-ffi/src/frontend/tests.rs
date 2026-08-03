use std::ptr;

use vinput_fcitx_core::{FrontendController, FrontendOutcome, FrontendOutcomeKind, SceneSnapshot};
use vinput_fcitx_dbus::{DaemonOperation, DaemonResponse};

use super::{
    FRONTEND_TRIGGER_INTENT_NONE, FRONTEND_TRIGGER_INTENT_SHOW_ASR_MENU,
    FRONTEND_TRIGGER_INTENT_SHOW_SCENE_MENU, FRONTEND_TRIGGER_INTENT_START_NORMAL,
    FRONTEND_TRIGGER_INTENT_STOP_COMMAND, FRONTEND_TRIGGER_INTENT_STOP_NORMAL,
    FRONTEND_TRIGGER_REQUEST_SHOW_ASR_MENU, FRONTEND_TRIGGER_REQUEST_SHOW_SCENE_MENU,
    FRONTEND_TRIGGER_REQUEST_START_NORMAL, FRONTEND_TRIGGER_REQUEST_STOP_COMMAND,
    FRONTEND_TRIGGER_REQUEST_STOP_NORMAL, VinputFcitxFrontendOutcome,
    VinputFcitxFrontendPresentation, VinputFcitxFrontendPresentationView,
    VinputFcitxPresentedCandidateView, VinputFcitxStringView, boxed_outcome, execute_step_with,
    vinput_fcitx_frontend_controller_adopt_and_stop_with_daemon,
    vinput_fcitx_frontend_controller_command_mode, vinput_fcitx_frontend_controller_free,
    vinput_fcitx_frontend_controller_new, vinput_fcitx_frontend_controller_plan_trigger,
    vinput_fcitx_frontend_controller_recording,
    vinput_fcitx_frontend_controller_start_command_with_daemon,
    vinput_fcitx_frontend_controller_start_normal_with_daemon,
    vinput_fcitx_frontend_controller_stop_with_daemon, vinput_fcitx_frontend_outcome_free,
    vinput_fcitx_frontend_presentation_candidate, vinput_fcitx_frontend_presentation_free,
    vinput_fcitx_frontend_presentation_new, vinput_fcitx_frontend_presentation_view,
};
use crate::menu_controller::{boxed_scene_controller, vinput_fcitx_scene_menu_controller_free};

unsafe fn bytes(view: VinputFcitxStringView) -> &'static [u8] {
    if view.data.is_null() {
        return &[];
    }
    // SAFETY: Test callers keep the owning handle alive.
    unsafe { std::slice::from_raw_parts(view.data, view.len) }
}

unsafe fn presentation_view(
    outcome: *mut VinputFcitxFrontendOutcome,
) -> (
    *mut VinputFcitxFrontendPresentation,
    VinputFcitxFrontendPresentationView,
) {
    let original = b"Original";
    let voice_command = b"Voice Command";
    let cancel = b"Cancel";
    // SAFETY: Test byte slices and the outcome remain live for the call.
    let presentation = unsafe {
        vinput_fcitx_frontend_presentation_new(
            outcome,
            original.as_ptr(),
            original.len(),
            voice_command.as_ptr(),
            voice_command.len(),
            cancel.as_ptr(),
            cancel.len(),
        )
    };
    assert!(!presentation.is_null());
    let mut view = VinputFcitxFrontendPresentationView {
        kind: 0,
        replace_selection: 0,
        text: VinputFcitxStringView {
            data: ptr::null(),
            len: 0,
        },
        candidate_count: 0,
        cursor_index: 0,
    };
    // SAFETY: Test callers pass live handles and writable local output.
    assert_eq!(
        unsafe { vinput_fcitx_frontend_presentation_view(presentation, &raw mut view) },
        1
    );
    (presentation, view)
}

#[test]
fn builds_projected_frontend_presentation_views() {
    // SAFETY: Input bytes remain alive and the outcome handle is freed exactly once.
    unsafe {
        let payload = r#"{"commit_text":"selected","candidates":[{"text":"selected","source":"raw"},{"text":"changed","source":"asr"}]}"#;
        let outcome = boxed_outcome(FrontendOutcome::from_payload(payload, true));
        assert!(!outcome.is_null());
        let (presentation, view) = presentation_view(outcome);
        assert_eq!(view.kind, 4);
        assert_eq!(view.replace_selection, 1);
        assert_eq!(bytes(view.text), b"selected");
        assert_eq!(view.candidate_count, 2);
        assert_eq!(view.cursor_index, 0);

        let mut candidate = VinputFcitxPresentedCandidateView {
            text: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
            comment: VinputFcitxStringView {
                data: ptr::null(),
                len: 0,
            },
            commit: 0,
        };
        assert_eq!(
            vinput_fcitx_frontend_presentation_candidate(presentation, 1, &raw mut candidate,),
            1
        );
        assert_eq!(bytes(candidate.text), b"changed");
        assert_eq!(bytes(candidate.comment), b"Voice Command");
        assert_eq!(candidate.commit, 1);
        vinput_fcitx_frontend_presentation_free(presentation);
        vinput_fcitx_frontend_outcome_free(outcome);
    }
}

#[test]
fn direct_exports_validate_inputs_and_immediate_errors() {
    // SAFETY: The controller is live and all returned outcomes are freed exactly once.
    unsafe {
        let controller = vinput_fcitx_frontend_controller_new();
        assert!(!controller.is_null());

        let invalid_outcome = vinput_fcitx_frontend_controller_start_normal_with_daemon(
            controller,
            ptr::null(),
            ptr::null(),
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
        let (presentation, view) = presentation_view(immediate);
        assert_eq!(view.kind, 5);
        assert_eq!(bytes(view.text), b"Please select text first.");
        vinput_fcitx_frontend_presentation_free(presentation);
        vinput_fcitx_frontend_outcome_free(immediate);

        let scenes = boxed_scene_controller(Some(SceneSnapshot::new("remote".to_owned())));
        let adopted = vinput_fcitx_frontend_controller_adopt_and_stop_with_daemon(
            controller,
            ptr::null(),
            1,
            scenes,
        );
        assert!(!adopted.is_null());
        let (presentation, view) = presentation_view(adopted);
        assert_eq!(view.kind, 5);
        assert_eq!(bytes(view.text), b"Voice input daemon is unavailable.");
        assert_eq!(vinput_fcitx_frontend_controller_recording(controller), 0);
        assert_eq!(vinput_fcitx_frontend_controller_command_mode(controller), 0);
        vinput_fcitx_frontend_presentation_free(presentation);
        vinput_fcitx_frontend_outcome_free(adopted);
        vinput_fcitx_scene_menu_controller_free(scenes);
        vinput_fcitx_frontend_controller_free(controller);
    }
}

#[test]
fn scene_controller_exports_share_one_snapshot_with_frontend_calls() {
    // SAFETY: Every handle is live for each call and released exactly once.
    unsafe {
        let controller = vinput_fcitx_frontend_controller_new();
        let scenes = boxed_scene_controller(None);
        assert!(!controller.is_null());
        assert!(!scenes.is_null());

        assert!(
            vinput_fcitx_frontend_controller_start_normal_with_daemon(
                controller,
                ptr::null(),
                scenes,
            )
            .is_null()
        );
        assert_eq!(vinput_fcitx_frontend_controller_recording(controller), 0);

        vinput_fcitx_scene_menu_controller_free(scenes);
        let scenes = boxed_scene_controller(Some(SceneSnapshot::new("remote".to_owned())));
        let start = vinput_fcitx_frontend_controller_start_normal_with_daemon(
            controller,
            ptr::null(),
            scenes,
        );
        assert!(!start.is_null());
        let (presentation, view) = presentation_view(start);
        assert_eq!(view.kind, 5);
        assert_eq!(bytes(view.text), b"Voice input daemon is unavailable.");
        assert_eq!(vinput_fcitx_frontend_controller_recording(controller), 0);
        vinput_fcitx_frontend_presentation_free(presentation);
        vinput_fcitx_frontend_outcome_free(start);

        let adopted = vinput_fcitx_frontend_controller_adopt_and_stop_with_daemon(
            controller,
            ptr::null(),
            1,
            scenes,
        );
        assert!(!adopted.is_null());
        let (presentation, view) = presentation_view(adopted);
        assert_eq!(view.kind, 5);
        assert_eq!(bytes(view.text), b"Voice input daemon is unavailable.");
        assert_eq!(vinput_fcitx_frontend_controller_recording(controller), 0);
        assert_eq!(vinput_fcitx_frontend_controller_command_mode(controller), 0);
        vinput_fcitx_frontend_presentation_free(presentation);
        vinput_fcitx_frontend_outcome_free(adopted);

        (*controller).controller.adopt_recording(false, "started");
        let stopped =
            vinput_fcitx_frontend_controller_stop_with_daemon(controller, ptr::null(), ptr::null());
        assert!(!stopped.is_null());
        assert_eq!(vinput_fcitx_frontend_controller_recording(controller), 0);
        vinput_fcitx_frontend_outcome_free(stopped);

        vinput_fcitx_scene_menu_controller_free(scenes);
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

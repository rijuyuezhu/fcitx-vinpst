//! Blocking D-Bus confirmation for asynchronous ASR backend reloads.

use std::time::{Duration, Instant};

use vinpst_protocol::{AsrBackendState, RequestedAsrBackendStatus, dbus};

use crate::{daemon_client::query_asr_backend_state, daemon_proxy};

const ASR_RELOAD_CONFIRM_TIMEOUT: Duration = Duration::from_secs(60);
const ASR_RELOAD_POLL_INTERVAL: Duration = Duration::from_millis(25);

pub(crate) fn reload_asr_backend() -> Result<(), String> {
    let connection = zbus::blocking::Connection::session().map_err(|error| error.to_string())?;
    let proxy = daemon_proxy(&connection)?;
    proxy
        .call::<_, _, ()>(dbus::method::RELOAD_ASR_BACKEND, &())
        .map_err(|error| error.to_string())
}

pub(crate) fn reload_asr_backend_and_wait(expected_provider_id: &str) -> Result<String, String> {
    reload_asr_backend()?;
    wait_for_requested_asr_backend(expected_provider_id)
}

pub(crate) fn wait_for_requested_asr_backend(expected_provider_id: &str) -> Result<String, String> {
    let connection = zbus::blocking::Connection::session().map_err(|error| error.to_string())?;
    let proxy = daemon_proxy(&connection)?;
    let deadline = Instant::now() + ASR_RELOAD_CONFIRM_TIMEOUT;
    loop {
        let state = query_asr_backend_state(&proxy)?;
        if let Some(result) = classify_completed_asr_reload(&state, expected_provider_id) {
            return result.map(|()| "daemon ASR backend applied".to_owned());
        }
        if Instant::now() >= deadline {
            return Err(
                "Timed out waiting for the daemon to apply the saved hotword update; the file was preserved."
                    .to_owned(),
            );
        }
        std::thread::sleep(ASR_RELOAD_POLL_INTERVAL);
    }
}

fn classify_completed_asr_reload(
    state: &AsrBackendState,
    expected_provider_id: &str,
) -> Option<Result<(), String>> {
    if state.reload_in_progress {
        return None;
    }
    if state.target_provider_id != expected_provider_id {
        return Some(Err(
            "The daemon ASR target changed while applying the hotword update; the file was preserved but this update was not confirmed."
                .to_owned(),
        ));
    }
    if !state.last_error.is_empty() {
        return Some(Err(
            "The daemon could not apply the saved hotword update; the file was preserved and the previous backend may still be active."
                .to_owned(),
        ));
    }
    let status =
        state.classify_requested_backend(&state.target_provider_id, &state.target_model_id);
    Some(match status {
        RequestedAsrBackendStatus::Applied => Ok(()),
        RequestedAsrBackendStatus::FailedStillUsingPrevious
        | RequestedAsrBackendStatus::FailedNoUsableBackend => Err(
            "The daemon could not apply the saved hotword update; the file was preserved and the previous backend may still be active."
                .to_owned(),
        ),
        RequestedAsrBackendStatus::ConfigSaved
        | RequestedAsrBackendStatus::Unknown
        | RequestedAsrBackendStatus::ReloadInProgress => Err(
            "The daemon finished reloading without confirming the saved hotword update; the file was preserved."
                .to_owned(),
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_reload_classification_waits_applies_and_rejects_failures() {
        let mut state = AsrBackendState::ready("local", "model");
        state.reload_in_progress = true;
        assert_eq!(classify_completed_asr_reload(&state, "local"), None);

        state.reload_in_progress = false;
        assert_eq!(classify_completed_asr_reload(&state, "local"), Some(Ok(())));

        state.last_error = "same-backend fixture failure".to_owned();
        assert!(
            classify_completed_asr_reload(&state, "local")
                .expect("same-backend failure")
                .expect_err("same backend identifiers must not hide reload failure")
                .contains("could not apply")
        );
        state.last_error.clear();

        state.target_provider_id = "local".to_owned();
        state.target_model_id = "new-model".to_owned();
        state.effective_provider_id = "previous".to_owned();
        state.effective_model_id = "old-model".to_owned();
        state.last_error = "redacted fixture failure".to_owned();
        assert!(
            classify_completed_asr_reload(&state, "local")
                .expect("completed failure")
                .expect_err("reload should fail")
                .contains("could not apply")
        );

        state.target_provider_id = "other".to_owned();
        assert!(
            classify_completed_asr_reload(&state, "local")
                .expect("superseded reload")
                .expect_err("superseded reload should fail")
                .contains("target changed")
        );
    }
}

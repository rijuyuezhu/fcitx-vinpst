//! GUI state and task ownership for cancellable provider and adapter installation.

use std::sync::{Arc, Mutex};

use iced::{
    Element, Length, Task,
    widget::{button, column, progress_bar, row, text, text_input},
};
use vinput_registry::{LiveScriptKind, RegistryOperationControl, RegistryOperationProgress};

use crate::{
    App, ConfigDocument, Message, OperationState, load_config_document,
    script_management::install_registry_script_controlled, script_management::resource_label,
};

/// Final typed outcome of a GUI provider or adapter installation worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptInstallOutcome {
    /// The script and its validated configuration entry were installed.
    Installed(String),
    /// The user or application shutdown requested cancellation.
    Cancelled,
    /// The operation failed.
    Failed(String),
}

#[derive(Debug, Default)]
pub(crate) enum ScriptInstallState {
    #[default]
    Idle,
    Active(ActiveScriptInstall),
    Succeeded(String),
    Cancelled {
        kind: LiveScriptKind,
        selector: String,
    },
    Failed {
        kind: LiveScriptKind,
        selector: String,
        error: String,
    },
}

#[derive(Debug)]
pub(crate) struct ActiveScriptInstall {
    operation_id: u64,
    kind: LiveScriptKind,
    selector: String,
    control: RegistryOperationControl,
    shared_progress: Arc<Mutex<RegistryOperationProgress>>,
    progress: RegistryOperationProgress,
    cancelling: bool,
}

impl Drop for ActiveScriptInstall {
    fn drop(&mut self) {
        self.control.cancel();
    }
}

impl ScriptInstallState {
    pub(crate) fn is_active(&self) -> bool {
        matches!(self, Self::Active(_))
    }

    pub(crate) fn start(
        document: ConfigDocument,
        kind: LiveScriptKind,
        selector: String,
        operation_id: u64,
    ) -> (Self, Task<Message>) {
        let initial_progress = RegistryOperationProgress::ResolvingRegistry;
        let shared_progress = Arc::new(Mutex::new(initial_progress.clone()));
        let reported = Arc::clone(&shared_progress);
        let control = RegistryOperationControl::new(move |progress| {
            *reported
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = progress;
        });
        let worker_control = control.clone();
        let worker_selector = selector.clone();
        let task = Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    install_registry_script_controlled(
                        &document,
                        kind,
                        &worker_selector,
                        &worker_control,
                    )
                })
                .await
                .unwrap_or_else(|_| {
                    ScriptInstallOutcome::Failed(
                        "Script installation worker stopped unexpectedly.".to_owned(),
                    )
                })
            },
            move |outcome| Message::ScriptInstalled {
                operation_id,
                outcome,
            },
        );
        (
            Self::Active(ActiveScriptInstall {
                operation_id,
                kind,
                selector,
                control,
                shared_progress,
                progress: initial_progress,
                cancelling: false,
            }),
            task,
        )
    }

    pub(crate) fn cancel(&mut self) {
        if let Self::Active(active) = self {
            active.control.cancel();
            active.cancelling = true;
        }
    }

    pub(crate) fn refresh_progress(&mut self) {
        if let Self::Active(active) = self {
            active.progress = active
                .shared_progress
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
        }
    }

    pub(crate) fn finish(&mut self, operation_id: u64, outcome: ScriptInstallOutcome) -> bool {
        let (kind, selector) = match self {
            Self::Active(active) if active.operation_id == operation_id => {
                (active.kind, active.selector.clone())
            }
            _ => return false,
        };
        *self = match outcome {
            ScriptInstallOutcome::Installed(summary) => Self::Succeeded(summary),
            ScriptInstallOutcome::Cancelled => Self::Cancelled { kind, selector },
            ScriptInstallOutcome::Failed(error) => Self::Failed {
                kind,
                selector,
                error,
            },
        };
        true
    }

    pub(crate) fn retry_request(&self) -> Option<(LiveScriptKind, String)> {
        match self {
            Self::Cancelled { kind, selector } | Self::Failed { kind, selector, .. } => {
                Some((*kind, selector.clone()))
            }
            Self::Idle | Self::Active(_) | Self::Succeeded(_) => None,
        }
    }

    pub(crate) fn view(&self) -> Option<Element<'_, Message>> {
        match self {
            Self::Idle => None,
            Self::Active(active) => Some(active.view()),
            Self::Succeeded(summary) => Some(text(format!("Success: {summary}")).into()),
            Self::Cancelled { kind, .. } => Some(
                row![
                    text(format!("{} installation cancelled.", resource_label(*kind))),
                    button("Retry").on_press(Message::RetryScriptInstall),
                ]
                .spacing(10)
                .into(),
            ),
            Self::Failed { error, .. } => Some(
                column![
                    text(format!("Error: {error}")),
                    button("Retry").on_press(Message::RetryScriptInstall),
                ]
                .spacing(8)
                .into(),
            ),
        }
    }
}

impl App {
    pub(crate) fn begin_script_install(&mut self, kind: LiveScriptKind) -> Task<Message> {
        let selector = match kind {
            LiveScriptKind::AsrProvider => self.provider_selector.trim(),
            LiveScriptKind::LlmAdapter => self.adapter_selector.trim(),
        }
        .to_owned();
        self.begin_script_install_for(kind, selector)
    }

    pub(crate) fn retry_script_install(&mut self) -> Task<Message> {
        let Some((kind, selector)) = self.script_install.retry_request() else {
            return Task::none();
        };
        match kind {
            LiveScriptKind::AsrProvider => self.provider_selector.clone_from(&selector),
            LiveScriptKind::LlmAdapter => self.adapter_selector.clone_from(&selector),
        }
        self.begin_script_install_for(kind, selector)
    }

    fn begin_script_install_for(
        &mut self,
        kind: LiveScriptKind,
        selector: String,
    ) -> Task<Message> {
        if selector.is_empty() {
            self.operation = OperationState::Failed(format!(
                "Enter a {} registry id or short id before installing.",
                resource_label(kind)
            ));
            return Task::none();
        }
        let Ok(document) = &self.config else {
            self.operation = OperationState::Failed("No valid config is loaded.".to_owned());
            return Task::none();
        };
        if self.is_busy() {
            return Task::none();
        }
        let operation_id = self.next_script_install_id;
        self.next_script_install_id = self.next_script_install_id.wrapping_add(1).max(1);
        let (state, task) =
            ScriptInstallState::start(document.clone(), kind, selector, operation_id);
        self.operation = OperationState::Idle;
        self.script_install = state;
        task
    }

    pub(crate) fn finish_script_install(
        &mut self,
        operation_id: u64,
        outcome: ScriptInstallOutcome,
    ) -> Task<Message> {
        if !self.script_install.finish(operation_id, outcome) {
            return Task::none();
        }
        if matches!(self.script_install, ScriptInstallState::Succeeded(_)) {
            let path = self
                .config
                .as_ref()
                .ok()
                .map(|document| document.path.clone());
            self.replace_config(load_config_document(path.as_deref()));
        }
        self.begin_daemon_refresh(false)
    }

    pub(crate) fn provider_install_controls(&self, busy: bool) -> Element<'_, Message> {
        row![
            text_input("Registry provider id or short id", &self.provider_selector)
                .on_input(Message::ProviderSelectorChanged)
                .width(Length::Fill),
            button("Install or update").on_press_maybe(
                (!busy && !self.provider_selector.trim().is_empty())
                    .then_some(Message::InstallProvider),
            ),
        ]
        .spacing(10)
        .into()
    }

    pub(crate) fn adapter_install_controls(&self, busy: bool) -> Element<'_, Message> {
        row![
            text_input("Registry adapter id or short id", &self.adapter_selector)
                .on_input(Message::AdapterSelectorChanged)
                .width(Length::Fill),
            button("Install or update").on_press_maybe(
                (!busy && !self.adapter_selector.trim().is_empty())
                    .then_some(Message::InstallAdapter),
            ),
        ]
        .spacing(10)
        .into()
    }
}

impl ActiveScriptInstall {
    fn view(&self) -> Element<'_, Message> {
        let mut body = column![text(progress_label(self.kind, &self.progress))].spacing(8);
        if let RegistryOperationProgress::Downloading {
            downloaded_bytes,
            total_bytes: Some(total_bytes),
        } = self.progress
            && total_bytes > 0
        {
            let permille = downloaded_bytes
                .saturating_mul(1000)
                .checked_div(total_bytes)
                .unwrap_or(0)
                .min(1000);
            let permille = u16::try_from(permille).unwrap_or(1000);
            body = body.push(progress_bar(0.0..=1.0, f32::from(permille) / 1000.0));
        }
        let commit_started = matches!(
            self.progress,
            RegistryOperationProgress::UpdatingConfiguration | RegistryOperationProgress::Completed
        );
        body = body.push(
            button(if self.cancelling {
                "Cancelling…"
            } else if commit_started {
                "Finishing…"
            } else {
                "Cancel"
            })
            .on_press_maybe(
                (!self.cancelling && !commit_started).then_some(Message::CancelScriptInstall),
            ),
        );
        body.into()
    }
}

fn progress_label(kind: LiveScriptKind, progress: &RegistryOperationProgress) -> String {
    let label = resource_label(kind);
    match progress {
        RegistryOperationProgress::Preparing => format!("Preparing {label} installation…"),
        RegistryOperationProgress::ResolvingRegistry => format!("Resolving {label} catalog…"),
        RegistryOperationProgress::Downloading {
            downloaded_bytes,
            total_bytes,
        } => total_bytes.map_or_else(
            || format!("Downloading {label} script… {downloaded_bytes} bytes received"),
            |total| format!("Downloading {label} script… {downloaded_bytes} of {total} bytes"),
        ),
        RegistryOperationProgress::VerifyingChecksum => {
            format!("Verifying {label} script…")
        }
        RegistryOperationProgress::Extracting { .. } => {
            format!("Extracting {label} resources…")
        }
        RegistryOperationProgress::WritingMetadata => {
            format!("Writing {label} metadata…")
        }
        RegistryOperationProgress::Publishing => format!("Publishing {label} script…"),
        RegistryOperationProgress::UpdatingConfiguration => {
            format!("Updating configuration for {label}…")
        }
        RegistryOperationProgress::Completed => format!("{label} installation completed."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_completion_does_not_replace_active_script_operation() {
        let document = ConfigDocument {
            path: "/tmp/config.json".into(),
            from_disk: false,
            config: vinput_config::VinputConfig::bundled_default().expect("bundled config"),
        };
        let (mut state, _) = ScriptInstallState::start(
            document,
            LiveScriptKind::AsrProvider,
            "fixture".to_owned(),
            12,
        );

        assert!(!state.finish(11, ScriptInstallOutcome::Cancelled));
        assert!(state.is_active());
        assert!(state.finish(12, ScriptInstallOutcome::Cancelled));
        assert_eq!(
            state.retry_request(),
            Some((LiveScriptKind::AsrProvider, "fixture".to_owned()))
        );
    }

    #[test]
    fn active_script_state_cancels_when_dropped() {
        let document = ConfigDocument {
            path: "/tmp/config.json".into(),
            from_disk: false,
            config: vinput_config::VinputConfig::bundled_default().expect("bundled config"),
        };
        let (state, _) = ScriptInstallState::start(
            document,
            LiveScriptKind::LlmAdapter,
            "fixture".to_owned(),
            13,
        );
        let control = match &state {
            ScriptInstallState::Active(active) => active.control.clone(),
            _ => panic!("active script install state"),
        };

        drop(state);

        assert!(control.is_cancelled());
    }
}

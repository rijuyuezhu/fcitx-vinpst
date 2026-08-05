//! GUI state and task ownership for cancellable model installation.

use crate::keyboard_action::keyboard_button;

use std::sync::{Arc, Mutex};

use iced::{
    Element, Task,
    widget::{column, progress_bar, row, text},
};
use vinput_config::VinputConfig;
use vinput_registry::{RegistryOperationControl, RegistryOperationProgress};

use crate::{GuiLocale, GuiText, Message, model_management::install_registry_model_controlled};

/// Final typed outcome of a GUI model installation worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelInstallOutcome {
    /// The model was installed or updated successfully.
    Installed(String),
    /// The user or application shutdown requested cancellation.
    Cancelled,
    /// The operation failed without publishing an incomplete model.
    Failed(String),
}

#[derive(Debug, Default)]
pub(crate) enum ModelInstallState {
    #[default]
    Idle,
    Active(ActiveModelInstall),
    Succeeded(String),
    Cancelled {
        selector: String,
    },
    Failed {
        selector: String,
        error: String,
    },
}

#[derive(Debug)]
pub(crate) struct ActiveModelInstall {
    operation_id: u64,
    selector: String,
    control: RegistryOperationControl,
    shared_progress: Arc<Mutex<RegistryOperationProgress>>,
    progress: RegistryOperationProgress,
    cancelling: bool,
}

impl Drop for ActiveModelInstall {
    fn drop(&mut self) {
        self.control.cancel();
    }
}

impl ModelInstallState {
    pub(crate) fn is_active(&self) -> bool {
        matches!(self, Self::Active(_))
    }

    pub(crate) fn start(
        config: VinputConfig,
        selector: String,
        operation_id: u64,
        locale: GuiLocale,
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
        let task = crate::blocking_task::perform(
            "vinput-gui-model-install",
            move || {
                install_registry_model_controlled(
                    &config,
                    &worker_selector,
                    &worker_control,
                    locale,
                )
            },
            move |result| Message::ModelInstalled {
                operation_id,
                outcome: result.unwrap_or_else(|_| {
                    ModelInstallOutcome::Failed(
                        "Model installation worker stopped unexpectedly.".to_owned(),
                    )
                }),
            },
        );
        (
            Self::Active(ActiveModelInstall {
                operation_id,
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

    pub(crate) fn finish(&mut self, operation_id: u64, outcome: ModelInstallOutcome) -> bool {
        let selector = match self {
            Self::Active(active) if active.operation_id == operation_id => active.selector.clone(),
            _ => return false,
        };
        *self = match outcome {
            ModelInstallOutcome::Installed(summary) => Self::Succeeded(summary),
            ModelInstallOutcome::Cancelled => Self::Cancelled { selector },
            ModelInstallOutcome::Failed(error) => Self::Failed { selector, error },
        };
        true
    }

    pub(crate) fn retry_selector(&self) -> Option<String> {
        match self {
            Self::Cancelled { selector } | Self::Failed { selector, .. } => Some(selector.clone()),
            Self::Idle | Self::Active(_) | Self::Succeeded(_) => None,
        }
    }

    pub(crate) fn view(&self, locale: GuiLocale) -> Option<Element<'_, Message>> {
        match self {
            Self::Idle => None,
            Self::Active(active) => Some(active.view(locale)),
            Self::Succeeded(summary) => Some(text(locale.operation_success(summary)).into()),
            Self::Cancelled { .. } => Some(
                row![
                    text(locale.text(GuiText::ModelInstallationCancelled)),
                    keyboard_button(locale.text(GuiText::Retry))
                        .on_press(Message::RetryModelInstall),
                ]
                .spacing(10)
                .into(),
            ),
            Self::Failed { error, .. } => Some(
                column![
                    text(locale.operation_error(error)),
                    keyboard_button(locale.text(GuiText::Retry))
                        .on_press(Message::RetryModelInstall),
                ]
                .spacing(8)
                .into(),
            ),
        }
    }
}

impl ActiveModelInstall {
    fn view(&self, locale: GuiLocale) -> Element<'_, Message> {
        let mut body = column![text(progress_label(locale, &self.progress))].spacing(8);
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
        body = body.push(
            keyboard_button(locale.text(if self.cancelling {
                GuiText::Cancelling
            } else {
                GuiText::Cancel
            }))
            .on_press_maybe((!self.cancelling).then_some(Message::CancelModelInstall)),
        );
        body.into()
    }
}

fn progress_label(locale: GuiLocale, progress: &RegistryOperationProgress) -> String {
    match progress {
        RegistryOperationProgress::Preparing => {
            locale.text(GuiText::PreparingModelInstallation).to_owned()
        }
        RegistryOperationProgress::ResolvingRegistry => {
            locale.text(GuiText::ResolvingModelCatalog).to_owned()
        }
        RegistryOperationProgress::Downloading {
            downloaded_bytes,
            total_bytes,
        } => {
            let downloaded = format_bytes(*downloaded_bytes);
            let total = total_bytes.map(format_bytes);
            locale.model_download_progress(&downloaded, total.as_deref())
        }
        RegistryOperationProgress::VerifyingChecksum => {
            locale.text(GuiText::VerifyingModelChecksum).to_owned()
        }
        RegistryOperationProgress::Extracting {
            processed_entries,
            extracted_bytes,
        } => locale.model_extraction_progress(*processed_entries, &format_bytes(*extracted_bytes)),
        RegistryOperationProgress::WritingMetadata => {
            locale.text(GuiText::WritingModelMetadata).to_owned()
        }
        RegistryOperationProgress::Publishing => {
            locale.text(GuiText::PublishingModelAtomically).to_owned()
        }
        RegistryOperationProgress::UpdatingConfiguration => locale
            .text(GuiText::UpdatingConfigurationProgress)
            .to_owned(),
        RegistryOperationProgress::Completed => {
            locale.text(GuiText::ModelInstallationCompleted).to_owned()
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;
    if bytes >= GIB {
        format_binary_unit(bytes, GIB, "GiB")
    } else if bytes >= MIB {
        format_binary_unit(bytes, MIB, "MiB")
    } else if bytes >= KIB {
        format_binary_unit(bytes, KIB, "KiB")
    } else {
        format!("{bytes} B")
    }
}

fn format_binary_unit(bytes: u64, unit: u64, suffix: &str) -> String {
    let tenths = bytes.saturating_mul(10) / unit;
    format!("{}.{} {suffix}", tenths / 10, tenths % 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_state_cancels_when_dropped() {
        let config = VinputConfig::bundled_default().expect("bundled config");
        let (state, task) =
            ModelInstallState::start(config, "fixture".to_owned(), 7, GuiLocale::EnUs);
        assert_eq!(task.units(), 1);
        let control = match &state {
            ModelInstallState::Active(active) => active.control.clone(),
            _ => panic!("active install state"),
        };

        drop(state);

        assert!(control.is_cancelled());
    }

    #[test]
    fn cancel_requests_cooperative_worker_shutdown() {
        let config = VinputConfig::bundled_default().expect("bundled config");
        let (mut state, _) =
            ModelInstallState::start(config, "fixture".to_owned(), 8, GuiLocale::EnUs);
        let control = match &state {
            ModelInstallState::Active(active) => active.control.clone(),
            _ => panic!("active install state"),
        };

        state.cancel();

        assert!(control.is_cancelled());
        assert!(matches!(
            state,
            ModelInstallState::Active(ActiveModelInstall {
                cancelling: true,
                ..
            })
        ));
    }

    #[test]
    fn stale_completion_does_not_replace_active_operation() {
        let config = VinputConfig::bundled_default().expect("bundled config");
        let (mut state, _) =
            ModelInstallState::start(config, "fixture".to_owned(), 9, GuiLocale::EnUs);

        assert!(!state.finish(8, ModelInstallOutcome::Cancelled));
        assert!(state.is_active());
        assert!(state.finish(9, ModelInstallOutcome::Cancelled));
        assert_eq!(state.retry_selector().as_deref(), Some("fixture"));
    }

    #[test]
    fn byte_formatting_uses_compact_binary_units() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_bytes(2 * 1024 * 1024), "2.0 MiB");
    }
}

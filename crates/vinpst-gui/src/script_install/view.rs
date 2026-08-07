//! Localized provider and adapter installation rendering.

use crate::keyboard_action::keyboard_button;

use iced::{
    Element, Length,
    widget::{column, progress_bar, row, text, text_input},
};
use vinpst_registry::{LiveScriptKind, RegistryOperationProgress};

use super::{
    ActiveScriptInstall, ActiveScriptPreparation, ScriptInstallPlan, ScriptInstallState,
    adapter_selector_id, provider_selector_id, script_primary_action_id,
};
use crate::{App, GuiLocale, GuiText, Message, SecretInput};

impl ScriptInstallPlan {
    fn view(&self, locale: GuiLocale) -> Element<'_, Message> {
        let resource = localized_resource_label(locale, self.kind);
        let mut body = column![
            text(locale.configure_script_before_install(resource, &self.entry.id)).size(18),
            text(locale.text(GuiText::ValuesStoredHidden)),
        ]
        .spacing(8);

        for environment in &self.environment {
            let name = environment.name.clone();
            body = body.push(
                column![
                    text(locale.environment_requirement(&environment.name, environment.required)),
                    text_input(
                        locale.text(GuiText::EnterEnvironmentValue),
                        &environment.value
                    )
                    .secure(true)
                    .on_input(move |value| Message::ScriptEnvironmentChanged {
                        name: name.clone(),
                        value: SecretInput::new(value),
                    })
                    .width(Length::Fill),
                ]
                .spacing(4),
            );
        }

        let can_install = self.missing_required_environment().is_none();
        body.push(
            row![
                keyboard_button(locale.text(GuiText::InstallOrUpdate))
                    .id(script_primary_action_id())
                    .on_press_maybe(can_install.then_some(Message::ConfirmScriptInstall)),
                keyboard_button(locale.text(GuiText::Cancel))
                    .on_press(Message::CancelScriptInstall),
            ]
            .spacing(10),
        )
        .into()
    }
}

impl ScriptInstallState {
    pub(crate) fn view(&self, locale: GuiLocale) -> Option<Element<'_, Message>> {
        match self {
            Self::Idle | Self::Failed { .. } => None,
            Self::Preparing(active) => Some(active.view(locale)),
            Self::AwaitingEnvironment(plan) => Some(plan.view(locale)),
            Self::Active(active) => Some(active.view(locale)),
            Self::Recovering(active) => {
                let resource = localized_resource_label(locale, active.plan.kind);
                Some(
                    column![
                        text(
                            locale.retrying_script_configuration(resource, &active.plan.entry.id,)
                        ),
                        text(locale.text(GuiText::ReusingPublishedScript)),
                    ]
                    .spacing(6)
                    .into(),
                )
            }
            Self::RecoveryRequired { plan, error } => {
                let resource = localized_resource_label(locale, plan.kind);
                let path = plan.script_path.display().to_string();
                Some(
                    column![
                        text(locale.text(GuiText::ScriptPublishedConfigurationIncomplete)).size(18),
                        text(locale.script_published_at(resource, &plan.entry.id, &path)),
                        text(locale.configuration_error(error)),
                        text(locale.text(GuiText::RecoveryInstructions)),
                        row![
                            keyboard_button(locale.text(GuiText::ReloadConfig))
                                .on_press(Message::ReloadConfig),
                            keyboard_button(locale.text(GuiText::RetryConfigurationUpdate))
                                .id(script_primary_action_id())
                                .on_press(Message::RetryScriptConfigUpdate),
                            keyboard_button(locale.text(GuiText::DismissKeepScript))
                                .on_press(Message::DismissScriptRecovery),
                        ]
                        .spacing(10),
                    ]
                    .spacing(8)
                    .into(),
                )
            }
            Self::Succeeded(summary) => Some(text(locale.operation_success(summary)).into()),
            Self::Cancelled { .. } => Some(
                row![
                    text(locale.text(GuiText::ScriptInstallationCancelled)),
                    keyboard_button(locale.text(GuiText::Retry))
                        .id(script_primary_action_id())
                        .on_press(Message::RetryScriptInstall),
                ]
                .spacing(10)
                .into(),
            ),
        }
    }
}

impl App {
    pub(crate) fn provider_install_controls(&self, busy: bool) -> Element<'_, Message> {
        let input = text_input(
            self.locale.text(GuiText::RegistryProviderSelector),
            &self.provider_selector,
        )
        .id(provider_selector_id())
        .width(Length::Fill);
        let input = if busy {
            input
        } else {
            input.on_input(Message::ProviderSelectorChanged)
        };
        row![
            input,
            keyboard_button(self.locale.text(GuiText::InstallOrUpdate)).on_press_maybe(
                (!busy && !self.provider_selector.trim().is_empty())
                    .then_some(Message::InstallProvider),
            ),
        ]
        .spacing(10)
        .into()
    }

    pub(crate) fn adapter_install_controls(&self, busy: bool) -> Element<'_, Message> {
        let input = text_input(
            self.locale.text(GuiText::RegistryAdapterSelector),
            &self.adapter_selector,
        )
        .id(adapter_selector_id())
        .width(Length::Fill);
        let input = if busy {
            input
        } else {
            input.on_input(Message::AdapterSelectorChanged)
        };
        row![
            input,
            keyboard_button(self.locale.text(GuiText::InstallOrUpdate)).on_press_maybe(
                (!busy && !self.adapter_selector.trim().is_empty())
                    .then_some(Message::InstallAdapter),
            ),
        ]
        .spacing(10)
        .into()
    }
}

impl ActiveScriptPreparation {
    fn view(&self, locale: GuiLocale) -> Element<'_, Message> {
        let resource = localized_resource_label(locale, self.kind);
        row![
            text(locale.resolving_script_catalog(resource, &self.selector)),
            keyboard_button(locale.text(if self.cancelling {
                GuiText::Cancelling
            } else {
                GuiText::Cancel
            }))
            .on_press_maybe((!self.cancelling).then_some(Message::CancelScriptInstall)),
        ]
        .spacing(10)
        .into()
    }
}

impl ActiveScriptInstall {
    fn view(&self, locale: GuiLocale) -> Element<'_, Message> {
        let mut body =
            column![text(progress_label(locale, self.plan.kind, &self.progress))].spacing(8);
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
            keyboard_button(locale.text(if self.cancelling {
                GuiText::Cancelling
            } else if commit_started {
                GuiText::Finishing
            } else {
                GuiText::Cancel
            }))
            .on_press_maybe(
                (!self.cancelling && !commit_started).then_some(Message::CancelScriptInstall),
            ),
        );
        body.into()
    }
}

fn localized_resource_label(locale: GuiLocale, kind: LiveScriptKind) -> &'static str {
    locale.text(match kind {
        LiveScriptKind::AsrProvider => GuiText::AsrProviderResource,
        LiveScriptKind::LlmAdapter => GuiText::TextAdapterResource,
    })
}

fn progress_label(
    locale: GuiLocale,
    kind: LiveScriptKind,
    progress: &RegistryOperationProgress,
) -> String {
    let resource = localized_resource_label(locale, kind);
    match progress {
        RegistryOperationProgress::Preparing => {
            locale.script_progress("preparing", resource, None, None)
        }
        RegistryOperationProgress::ResolvingRegistry => {
            locale.script_progress("resolving", resource, None, None)
        }
        RegistryOperationProgress::Downloading {
            downloaded_bytes,
            total_bytes,
        } => locale.script_progress(
            "downloading",
            resource,
            Some(*downloaded_bytes),
            *total_bytes,
        ),
        RegistryOperationProgress::VerifyingChecksum => {
            locale.script_progress("verifying", resource, None, None)
        }
        RegistryOperationProgress::Extracting { .. } => {
            locale.script_progress("extracting", resource, None, None)
        }
        RegistryOperationProgress::WritingMetadata => {
            locale.script_progress("metadata", resource, None, None)
        }
        RegistryOperationProgress::Publishing => {
            locale.script_progress("publishing", resource, None, None)
        }
        RegistryOperationProgress::UpdatingConfiguration => {
            locale.script_progress("configuration", resource, None, None)
        }
        RegistryOperationProgress::Completed => {
            locale.script_progress("completed", resource, None, None)
        }
    }
}

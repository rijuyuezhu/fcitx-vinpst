//! Control-page typed config draft updates and editor rendering.

use crate::keyboard_action::{adjacent_values, keyboard_action, keyboard_button, keyboard_select};

use iced::{
    Element, Length,
    widget::{checkbox, column, pick_list, row, slider, text, text_input},
};

use crate::{App, ConfigDocument, ConfigDraft, ConfigDraftMessage, GuiText, Message};

impl App {
    pub(super) fn update_config_draft(&mut self, message: ConfigDraftMessage) {
        match message {
            ConfigDraftMessage::DefaultLanguage(value) => {
                self.update_draft(|draft| draft.default_language = value);
            }
            ConfigDraftMessage::CaptureDevice(value) => {
                self.update_draft(|draft| draft.capture_device = value);
            }
            ConfigDraftMessage::DuckOutput(value) => {
                self.update_draft(|draft| draft.duck_output_while_recording = value);
            }
            ConfigDraftMessage::DuckVolume(value) => {
                self.update_draft(|draft| draft.duck_output_volume = value);
            }
            ConfigDraftMessage::VadEnabled(value) => {
                self.update_draft(|draft| draft.vad_enabled = value);
            }
            ConfigDraftMessage::VadThreshold(value) => {
                self.update_draft(|draft| draft.vad_threshold = value);
            }
            ConfigDraftMessage::ActiveProvider(value) => {
                self.update_draft(|draft| draft.active_provider = value);
            }
            ConfigDraftMessage::ActiveScene(value) => {
                self.update_draft(|draft| draft.active_scene = value);
            }
        }
    }

    fn update_draft(&mut self, update: impl FnOnce(&mut ConfigDraft)) {
        if let Some(draft) = &mut self.draft {
            update(draft);
        }
    }

    pub(super) fn config_editor(&self, busy: bool) -> Element<'_, Message> {
        match (&self.config, &self.draft) {
            (Ok(document), Some(draft)) => self.loaded_config_editor(document, draft, busy),
            (Err(error), _) => text(self.locale.config_error(error)).into(),
            (Ok(_), None) => text(self.locale.text(GuiText::ConfigDraftUnavailable)).into(),
        }
    }

    fn loaded_config_editor<'a>(
        &'a self,
        document: &'a ConfigDocument,
        draft: &'a ConfigDraft,
        busy: bool,
    ) -> Element<'a, Message> {
        let source = if document.from_disk {
            self.locale.text(GuiText::SourceUserFile)
        } else {
            self.locale.text(GuiText::SourceBundledDefault)
        };
        column![
            text(self.locale.config_path(document.path.display())),
            text(match self.locale {
                crate::GuiLocale::EnUs => format!("Source: {source}"),
                crate::GuiLocale::ZhCn => format!("来源：{source}"),
            }),
            text(self.locale.text(GuiText::General)).size(22),
            self.general_config_editor(document, draft, busy),
            text(self.locale.text(GuiText::AudioAndVad)).size(22),
            self.audio_vad_editor(draft, busy),
            self.config_save_controls(document, draft, busy),
        ]
        .spacing(12)
        .into()
    }

    fn general_config_editor<'a>(
        &'a self,
        document: &'a ConfigDocument,
        draft: &'a ConfigDraft,
        busy: bool,
    ) -> Element<'a, Message> {
        let provider_options = document
            .config
            .asr
            .providers
            .iter()
            .map(|provider| provider.id.clone())
            .collect::<Vec<_>>();
        let scene_options = document
            .config
            .scenes
            .definitions
            .iter()
            .map(|scene| scene.id.clone())
            .collect::<Vec<_>>();
        let submit = (draft.is_dirty(&document.config) && !busy).then_some(Message::SaveConfig);
        let provider_control: Element<'a, Message> = if busy {
            text(&draft.active_provider).width(Length::Fill).into()
        } else {
            let (previous, next) = adjacent_values(&provider_options, Some(&draft.active_provider));
            keyboard_select(
                pick_list(
                    provider_options,
                    Some(draft.active_provider.clone()),
                    |value| Message::ConfigDraft(ConfigDraftMessage::ActiveProvider(value)),
                )
                .width(Length::Fill),
                previous
                    .map(|value| Message::ConfigDraft(ConfigDraftMessage::ActiveProvider(value))),
                next.map(|value| Message::ConfigDraft(ConfigDraftMessage::ActiveProvider(value))),
            )
        };
        let scene_control: Element<'a, Message> = if busy {
            text(&draft.active_scene).width(Length::Fill).into()
        } else {
            let (previous, next) = adjacent_values(&scene_options, Some(&draft.active_scene));
            keyboard_select(
                pick_list(scene_options, Some(draft.active_scene.clone()), |value| {
                    Message::ConfigDraft(ConfigDraftMessage::ActiveScene(value))
                })
                .width(Length::Fill),
                previous.map(|value| Message::ConfigDraft(ConfigDraftMessage::ActiveScene(value))),
                next.map(|value| Message::ConfigDraft(ConfigDraftMessage::ActiveScene(value))),
            )
        };
        column![
            row![
                text(self.locale.text(GuiText::DefaultLanguage)).width(180),
                text_input(
                    self.locale.text(GuiText::DefaultLanguagePlaceholder),
                    &draft.default_language
                )
                .on_input_maybe((!busy).then_some(|value| Message::ConfigDraft(
                    ConfigDraftMessage::DefaultLanguage(value)
                )))
                .on_submit_maybe(submit.clone())
                .width(Length::Fill),
            ]
            .spacing(12),
            row![
                text(self.locale.text(GuiText::CaptureDevice)).width(180),
                text_input(
                    self.locale.text(GuiText::PipeWireTarget),
                    &draft.capture_device
                )
                .on_input_maybe((!busy).then_some(|value| Message::ConfigDraft(
                    ConfigDraftMessage::CaptureDevice(value)
                )))
                .on_submit_maybe(submit)
                .width(Length::Fill),
            ]
            .spacing(12),
            row![
                text(self.locale.text(GuiText::ActiveAsrProvider)).width(180),
                provider_control,
            ]
            .spacing(12),
            row![
                text(self.locale.text(GuiText::ActiveScene)).width(180),
                scene_control,
            ]
            .spacing(12),
        ]
        .spacing(12)
        .into()
    }

    fn audio_vad_editor(&self, draft: &ConfigDraft, busy: bool) -> Element<'_, Message> {
        let duck_volume_control: Element<'_, Message> = if busy {
            text(self.locale.text(GuiText::LockedWhileFinishing))
                .width(Length::Fill)
                .into()
        } else {
            let previous = (draft.duck_output_volume > 0.0).then(|| {
                Message::ConfigDraft(ConfigDraftMessage::DuckVolume(
                    (draft.duck_output_volume - 0.05).max(0.0),
                ))
            });
            let next = (draft.duck_output_volume < 1.0).then(|| {
                Message::ConfigDraft(ConfigDraftMessage::DuckVolume(
                    (draft.duck_output_volume + 0.05).min(1.0),
                ))
            });
            keyboard_select(
                slider(0.0_f32..=1.0_f32, draft.duck_output_volume, |value| {
                    Message::ConfigDraft(ConfigDraftMessage::DuckVolume(value))
                })
                .step(0.05_f32)
                .width(Length::Fill),
                previous,
                next,
            )
        };
        let vad_threshold_control: Element<'_, Message> = if busy {
            text(self.locale.text(GuiText::LockedWhileFinishing))
                .width(Length::Fill)
                .into()
        } else {
            let previous = (draft.vad_threshold > 0.05).then(|| {
                Message::ConfigDraft(ConfigDraftMessage::VadThreshold(
                    (draft.vad_threshold - 0.05).max(0.05),
                ))
            });
            let next = (draft.vad_threshold < 0.95).then(|| {
                Message::ConfigDraft(ConfigDraftMessage::VadThreshold(
                    (draft.vad_threshold + 0.05).min(0.95),
                ))
            });
            keyboard_select(
                slider(0.05_f32..=0.95_f32, draft.vad_threshold, |value| {
                    Message::ConfigDraft(ConfigDraftMessage::VadThreshold(value))
                })
                .step(0.05_f32)
                .width(Length::Fill),
                previous,
                next,
            )
        };
        let duck_action = (!busy).then_some(Message::ConfigDraft(ConfigDraftMessage::DuckOutput(
            !draft.duck_output_while_recording,
        )));
        let duck_checkbox = keyboard_action(
            checkbox(draft.duck_output_while_recording)
                .label(self.locale.text(GuiText::DuckOutput))
                .on_toggle_maybe((!busy).then_some(|value| {
                    Message::ConfigDraft(ConfigDraftMessage::DuckOutput(value))
                })),
            duck_action,
        );
        let vad_action = (!busy).then_some(Message::ConfigDraft(ConfigDraftMessage::VadEnabled(
            !draft.vad_enabled,
        )));
        let vad_checkbox = keyboard_action(
            checkbox(draft.vad_enabled)
                .label(self.locale.text(GuiText::EnableVad))
                .on_toggle_maybe((!busy).then_some(|value| {
                    Message::ConfigDraft(ConfigDraftMessage::VadEnabled(value))
                })),
            vad_action,
        );
        column![
            duck_checkbox,
            row![
                text(self.locale.duck_volume(draft.duck_output_volume * 100.0),).width(180),
                duck_volume_control,
            ]
            .spacing(12),
            vad_checkbox,
            row![
                text(self.locale.vad_threshold(draft.vad_threshold)).width(180),
                vad_threshold_control,
            ]
            .spacing(12),
        ]
        .spacing(12)
        .into()
    }

    fn config_save_controls<'a>(
        &'a self,
        document: &'a ConfigDocument,
        draft: &'a ConfigDraft,
        busy: bool,
    ) -> Element<'a, Message> {
        let dirty = draft.is_dirty(&document.config);
        row![
            keyboard_button(self.locale.text(GuiText::SaveConfiguration))
                .on_press_maybe((dirty && !busy).then_some(Message::SaveConfig)),
            keyboard_button(self.locale.text(GuiText::ResetChanges))
                .on_press_maybe((dirty && !busy).then_some(Message::ResetConfigDraft)),
            text(if dirty {
                self.locale.text(GuiText::UnsavedChanges)
            } else {
                self.locale.text(GuiText::ConfigurationUpToDate)
            }),
        ]
        .spacing(10)
        .into()
    }
}

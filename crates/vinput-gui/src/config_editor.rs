//! Control-page typed config draft updates and editor rendering.

use iced::{
    Element, Length,
    widget::{button, checkbox, column, pick_list, row, slider, text, text_input},
};

use crate::{App, ConfigDocument, ConfigDraft, ConfigDraftMessage, Message};

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
            (Ok(document), Some(draft)) => Self::loaded_config_editor(document, draft, busy),
            (Err(error), _) => text(format!("Config error: {error}")).into(),
            (Ok(_), None) => text("Config draft is unavailable.").into(),
        }
    }

    fn loaded_config_editor<'a>(
        document: &'a ConfigDocument,
        draft: &'a ConfigDraft,
        busy: bool,
    ) -> Element<'a, Message> {
        column![
            text(format!("Config: {}", document.path.display())),
            text(format!(
                "Source: {}",
                if document.from_disk {
                    "user file"
                } else {
                    "bundled default; Save creates the user file"
                }
            )),
            text("General").size(22),
            Self::general_config_editor(document, draft, busy),
            text("Audio and VAD").size(22),
            Self::audio_vad_editor(draft, busy),
            Self::config_save_controls(document, draft, busy),
        ]
        .spacing(12)
        .into()
    }

    fn general_config_editor<'a>(
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
        let provider_control: Element<'a, Message> = if busy {
            text(&draft.active_provider).width(Length::Fill).into()
        } else {
            pick_list(
                provider_options,
                Some(draft.active_provider.clone()),
                |value| Message::ConfigDraft(ConfigDraftMessage::ActiveProvider(value)),
            )
            .width(Length::Fill)
            .into()
        };
        let scene_control: Element<'a, Message> = if busy {
            text(&draft.active_scene).width(Length::Fill).into()
        } else {
            pick_list(scene_options, Some(draft.active_scene.clone()), |value| {
                Message::ConfigDraft(ConfigDraftMessage::ActiveScene(value))
            })
            .width(Length::Fill)
            .into()
        };
        column![
            row![
                text("Default language").width(180),
                text_input("for example en-US or zh-CN", &draft.default_language)
                    .on_input_maybe((!busy).then_some(|value| Message::ConfigDraft(
                        ConfigDraftMessage::DefaultLanguage(value)
                    )))
                    .width(Length::Fill),
            ]
            .spacing(12),
            row![
                text("Capture device").width(180),
                text_input("PipeWire target", &draft.capture_device)
                    .on_input_maybe((!busy).then_some(|value| Message::ConfigDraft(
                        ConfigDraftMessage::CaptureDevice(value)
                    )))
                    .width(Length::Fill),
            ]
            .spacing(12),
            row![text("Active ASR provider").width(180), provider_control,].spacing(12),
            row![text("Active scene").width(180), scene_control,].spacing(12),
        ]
        .spacing(12)
        .into()
    }

    fn audio_vad_editor(draft: &ConfigDraft, busy: bool) -> Element<'_, Message> {
        let duck_volume_control: Element<'_, Message> = if busy {
            text("Locked while operation finishes")
                .width(Length::Fill)
                .into()
        } else {
            slider(0.0_f32..=1.0_f32, draft.duck_output_volume, |value| {
                Message::ConfigDraft(ConfigDraftMessage::DuckVolume(value))
            })
            .step(0.05_f32)
            .width(Length::Fill)
            .into()
        };
        let vad_threshold_control: Element<'_, Message> = if busy {
            text("Locked while operation finishes")
                .width(Length::Fill)
                .into()
        } else {
            slider(0.05_f32..=0.95_f32, draft.vad_threshold, |value| {
                Message::ConfigDraft(ConfigDraftMessage::VadThreshold(value))
            })
            .step(0.05_f32)
            .width(Length::Fill)
            .into()
        };
        column![
            checkbox(draft.duck_output_while_recording)
                .label("Duck output while recording")
                .on_toggle_maybe((!busy).then_some(|value| Message::ConfigDraft(
                    ConfigDraftMessage::DuckOutput(value)
                ))),
            row![
                text(format!(
                    "Duck volume: {:.0}%",
                    draft.duck_output_volume * 100.0
                ))
                .width(180),
                duck_volume_control,
            ]
            .spacing(12),
            checkbox(draft.vad_enabled)
                .label("Enable voice activity detection")
                .on_toggle_maybe((!busy).then_some(|value| Message::ConfigDraft(
                    ConfigDraftMessage::VadEnabled(value)
                ))),
            row![
                text(format!("VAD threshold: {:.2}", draft.vad_threshold)).width(180),
                vad_threshold_control,
            ]
            .spacing(12),
        ]
        .spacing(12)
        .into()
    }

    fn config_save_controls<'a>(
        document: &'a ConfigDocument,
        draft: &'a ConfigDraft,
        busy: bool,
    ) -> Element<'a, Message> {
        let dirty = draft.is_dirty(&document.config);
        row![
            button("Save configuration")
                .on_press_maybe((dirty && !busy).then_some(Message::SaveConfig)),
            button("Reset changes")
                .on_press_maybe((dirty && !busy).then_some(Message::ResetConfigDraft)),
            text(if dirty {
                "Unsaved changes"
            } else {
                "Configuration is up to date"
            }),
        ]
        .spacing(10)
        .into()
    }
}

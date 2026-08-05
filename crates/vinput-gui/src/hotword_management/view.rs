//! Localized Hotwords page rendering.

use crate::keyboard_action::{adjacent_values, keyboard_button, keyboard_select};

use iced::{
    Element, Length,
    widget::{column, pick_list, row, scrollable, text, text_editor, text_input},
};

use super::{HotwordMessage, HotwordProviderSelection, hotword_provider_options};
use crate::{App, GuiText, Message, SecretInput};

impl App {
    pub(crate) fn hotwords_page(&self) -> Element<'_, Message> {
        let busy = self.is_busy();
        let mut body = column![text(self.locale.text(GuiText::Hotwords)).size(30)].spacing(12);
        if let Some(notice) = self.operation_notice() {
            body = body.push(notice);
        }
        let Ok(document) = &self.config else {
            return scrollable(body.push(text(self.locale.text(GuiText::NoValidConfig)))).into();
        };
        let provider_options = hotword_provider_options(&document.config);
        if provider_options.is_empty() {
            return scrollable(body.push(text(self.locale.text(GuiText::NoHotwordProvider))))
                .into();
        }
        body = body.push(self.hotword_provider_picker(&provider_options, busy));
        body = body.push(self.hotword_path_controls(busy));
        body = body.push(self.hotword_content_controls(busy));
        body = body.push(self.hotword_content_editor(busy));
        scrollable(body).into()
    }

    fn hotword_provider_picker<'a>(
        &'a self,
        provider_options: &[HotwordProviderSelection],
        busy: bool,
    ) -> Element<'a, Message> {
        let selected = self
            .hotword_editor
            .selected_provider
            .as_deref()
            .and_then(|id| {
                provider_options
                    .iter()
                    .find(|provider| provider.id() == id)
                    .cloned()
            });
        let provider_picker: Element<'_, Message> = if busy {
            text(selected.as_ref().map_or_else(
                || self.locale.text(GuiText::NoProviderSelected).to_owned(),
                ToString::to_string,
            ))
            .width(Length::Fill)
            .into()
        } else {
            let (previous, next) = adjacent_values(provider_options, selected.as_ref());
            keyboard_select(
                pick_list(provider_options.to_vec(), selected, |selection| {
                    Message::Hotword(HotwordMessage::ProviderSelected(selection))
                })
                .width(Length::Fill),
                previous
                    .map(|selection| Message::Hotword(HotwordMessage::ProviderSelected(selection))),
                next.map(|selection| Message::Hotword(HotwordMessage::ProviderSelected(selection))),
            )
        };
        row![
            text(self.locale.text(GuiText::AsrProvider)).width(160),
            provider_picker
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center)
        .into()
    }

    fn hotword_path_controls(&self, busy: bool) -> Element<'_, Message> {
        let path_dirty = self.hotword_editor.path_is_dirty();
        let content_dirty = self.hotword_editor.content_is_dirty();
        row![
            text(self.locale.text(GuiText::HotwordFile)).width(160),
            text_input(
                self.locale.text(GuiText::HotwordPathPlaceholder),
                &self.hotword_editor.path_input
            )
            .on_input_maybe((!busy).then_some(|value| {
                Message::Hotword(HotwordMessage::PathChanged(SecretInput::new(value)))
            }))
            .width(Length::Fill),
            keyboard_button(self.locale.text(GuiText::Browse)).on_press_maybe(
                (!busy && self.hotword_editor.selected_provider.is_some() && !content_dirty)
                    .then_some(Message::Hotword(HotwordMessage::BrowsePath)),
            ),
            keyboard_button(self.locale.text(GuiText::SetPath)).on_press_maybe(
                (!busy
                    && path_dirty
                    && !content_dirty
                    && !self.hotword_editor.path_input.trim().is_empty())
                .then_some(Message::Hotword(HotwordMessage::SetPath)),
            ),
            keyboard_button(self.locale.text(GuiText::ClearPath)).on_press_maybe(
                (!busy && self.hotword_editor.configured_path.is_some() && !content_dirty)
                    .then_some(Message::Hotword(HotwordMessage::ClearPath)),
            ),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center)
        .into()
    }

    fn hotword_content_controls(&self, busy: bool) -> Element<'_, Message> {
        let path_dirty = self.hotword_editor.path_is_dirty();
        let content_dirty = self.hotword_editor.content_is_dirty();
        row![
            keyboard_button(self.locale.text(GuiText::LoadContent)).on_press_maybe(
                (!busy
                    && self.hotword_editor.content_path.is_some()
                    && !path_dirty
                    && !content_dirty)
                    .then_some(Message::Hotword(HotwordMessage::LoadContent)),
            ),
            keyboard_button(self.locale.text(GuiText::SaveContent)).on_press_maybe(
                (!busy
                    && !path_dirty
                    && content_dirty
                    && self.hotword_editor.content_matches_target())
                .then_some(Message::Hotword(HotwordMessage::SaveContent)),
            ),
            keyboard_button(self.locale.text(GuiText::ResetChanges)).on_press_maybe(
                (!busy && self.hotword_editor.has_unsaved_changes())
                    .then_some(Message::Hotword(HotwordMessage::ResetChanges)),
            ),
            keyboard_button(self.locale.text(GuiText::RetryActivation)).on_press_maybe(
                (!busy
                    && !path_dirty
                    && !content_dirty
                    && self
                        .hotword_editor
                        .pending_activation_for_selected_provider())
                .then_some(Message::Hotword(HotwordMessage::RetryActivation)),
            ),
            text(self.hotword_content_status(content_dirty)),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center)
        .into()
    }

    fn hotword_content_editor(&self, busy: bool) -> Element<'_, Message> {
        if busy || self.hotword_editor.baseline.is_none() {
            text_editor::<Message, iced::Theme, iced::Renderer>(&self.hotword_editor.content)
                .placeholder(self.locale.text(GuiText::OneHotwordPerLine))
                .height(Length::Fixed(320.0))
                .into()
        } else {
            text_editor::<Message, iced::Theme, iced::Renderer>(&self.hotword_editor.content)
                .placeholder(self.locale.text(GuiText::OneHotwordPerLine))
                .height(Length::Fixed(320.0))
                .on_action(|action| Message::Hotword(HotwordMessage::ContentAction(action)))
                .into()
        }
    }

    fn hotword_content_status(&self, content_dirty: bool) -> String {
        if self
            .hotword_editor
            .pending_activation_for_selected_provider()
        {
            self.locale
                .text(GuiText::HotwordActivationRetryable)
                .to_owned()
        } else if let Some(error) = &self.hotword_editor.content_path_error {
            error.clone()
        } else if self.hotword_editor.baseline.is_some() {
            self.locale
                .text(if content_dirty {
                    GuiText::UnsavedHotwordContent
                } else {
                    GuiText::HotwordContentUnchanged
                })
                .to_owned()
        } else {
            self.locale
                .text(GuiText::LoadConfiguredHotwordFile)
                .to_owned()
        }
    }
}

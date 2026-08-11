//! Localized text-adapter configuration editor rendering.

use crate::keyboard_action::keyboard_button;

use iced::{
    Element, Length,
    widget::{column, row, text, text_input},
};

use super::{AdapterConfigEditorField, AdapterConfigEditorState, AdapterConfigMessage};
use crate::{App, GuiLocale, GuiText, Message, SecretInput};

impl App {
    pub(crate) fn adapter_config_editor_view(&self, busy: bool) -> Option<Element<'_, Message>> {
        self.adapter_config_editor
            .as_ref()
            .map(|editor| adapter_config_editor_view(self.locale, editor, busy))
    }
}

fn adapter_config_editor_view(
    locale: GuiLocale,
    editor: &AdapterConfigEditorState,
    busy: bool,
) -> Element<'_, Message> {
    let dirty = editor.is_dirty();
    let adding = editor.original.is_none();
    let mut body = column![
        text(locale.text(if adding {
            GuiText::AddCustomTextAdapter
        } else {
            GuiText::EditTextAdapter
        }))
        .size(22)
    ]
    .spacing(10);
    body = if adding {
        body.push(labeled_input(
            locale.text(GuiText::AdapterId),
            locale.text(GuiText::CustomAdapterPlaceholder),
            &editor.fields.id,
            AdapterConfigEditorField::Id,
            false,
        ))
    } else {
        body.push(text(locale.adapter_id_immutable(&editor.fields.id)))
    };
    body.push(labeled_input(
        locale.text(GuiText::CommandField),
        locale.text(GuiText::AdapterCommandPlaceholder),
        editor.fields.command.as_str(),
        AdapterConfigEditorField::Command,
        false,
    ))
    .push(labeled_input(
        locale.text(GuiText::Arguments),
        locale.text(GuiText::JsonStringArray),
        editor.fields.args.as_str(),
        AdapterConfigEditorField::Args,
        false,
    ))
    .push(labeled_input(
        locale.text(GuiText::Environment),
        locale.text(GuiText::JsonStringObject),
        editor.fields.environment.as_str(),
        AdapterConfigEditorField::Environment,
        true,
    ))
    .push(labeled_input(
        locale.text(GuiText::WorkingDirectory),
        locale.text(GuiText::OptionalWorkingDirectory),
        editor.fields.working_directory.as_str(),
        AdapterConfigEditorField::WorkingDirectory,
        false,
    ))
    .push(
        row![
            keyboard_button(locale.text(if adding {
                GuiText::AddAdapter
            } else {
                GuiText::UpdateAdapter
            }))
            .on_press_maybe(
                (dirty && !busy).then_some(Message::AdapterConfig(AdapterConfigMessage::Save)),
            ),
            keyboard_button(locale.text(GuiText::ResetForm)).on_press_maybe(
                (dirty && !busy).then_some(Message::AdapterConfig(AdapterConfigMessage::ResetEdit)),
            ),
            keyboard_button(locale.text(GuiText::Cancel)).on_press_maybe(
                (!busy).then_some(Message::AdapterConfig(AdapterConfigMessage::CancelEdit)),
            ),
            text(locale.text(if dirty {
                GuiText::UnsavedAdapterChanges
            } else {
                GuiText::AdapterFormUnchanged
            })),
        ]
        .spacing(10),
    )
    .into()
}

fn labeled_input<'a>(
    label: &'static str,
    placeholder: &'static str,
    value: &'a str,
    field: AdapterConfigEditorField,
    secure: bool,
) -> Element<'a, Message> {
    row![
        text(label).width(160),
        text_input(placeholder, value)
            .secure(secure)
            .on_input(move |value| {
                Message::AdapterConfig(AdapterConfigMessage::EditorChanged {
                    field,
                    value: SecretInput::new(value),
                })
            })
            .width(Length::Fill),
    ]
    .spacing(10)
    .into()
}

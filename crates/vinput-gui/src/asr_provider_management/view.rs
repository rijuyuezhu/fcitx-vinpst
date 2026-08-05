//! Localized ASR provider editor rendering.

use crate::keyboard_action::keyboard_button;

use iced::{
    Element, Length,
    widget::{column, row, text, text_input},
};
use vinput_config::AsrProviderKind;

use super::{
    AsrProviderEditorField, AsrProviderEditorState, AsrProviderEnvironmentEntry, AsrProviderMessage,
};
use crate::{App, GuiLocale, GuiText, Message, SecretInput};

impl App {
    pub(crate) fn asr_provider_editor_view(&self, busy: bool) -> Option<Element<'_, Message>> {
        self.asr_provider_editor
            .as_ref()
            .map(|editor| provider_editor_view(self.locale, editor, busy))
    }
}

fn provider_editor_view(
    locale: GuiLocale,
    editor: &AsrProviderEditorState,
    busy: bool,
) -> Element<'_, Message> {
    let dirty = editor.is_dirty();
    let adding = editor.original.is_none();
    column![
        text(locale.text(if adding {
            GuiText::AddCustomAsrProvider
        } else {
            GuiText::EditAsrProvider
        }))
        .size(22),
        provider_identity_view(locale, editor, busy),
        labeled_input(
            locale.text(GuiText::TimeoutMsLabel),
            locale.text(GuiText::BlankBackendDefault),
            &editor.fields.timeout_ms,
            AsrProviderEditorField::TimeoutMs,
            false,
        ),
        labeled_input(
            locale.text(GuiText::Model),
            locale.text(GuiText::OptionalModelId),
            &editor.fields.model,
            AsrProviderEditorField::Model,
            false,
        ),
        provider_kind_fields(locale, editor, busy),
        row![
            keyboard_button(locale.text(if adding {
                GuiText::AddProvider
            } else {
                GuiText::UpdateProvider
            }))
            .on_press_maybe(
                (dirty && !busy).then_some(Message::AsrProvider(AsrProviderMessage::Save)),
            ),
            keyboard_button(locale.text(GuiText::ResetForm)).on_press_maybe(
                (dirty && !busy).then_some(Message::AsrProvider(AsrProviderMessage::ResetEdit)),
            ),
            keyboard_button(locale.text(GuiText::Cancel)).on_press_maybe(
                (!busy).then_some(Message::AsrProvider(AsrProviderMessage::CancelEdit)),
            ),
            text(locale.text(if dirty {
                GuiText::UnsavedProviderChanges
            } else {
                GuiText::ProviderFormUnchanged
            })),
        ]
        .spacing(10),
    ]
    .spacing(10)
    .into()
}

fn provider_identity_view(
    locale: GuiLocale,
    editor: &AsrProviderEditorState,
    busy: bool,
) -> Element<'_, Message> {
    if editor.original.is_some() {
        return text(
            locale.provider_identity(&editor.fields.id, locale.text(kind_title(&editor.kind))),
        )
        .into();
    }
    column![
        labeled_input(
            locale.text(GuiText::ProviderId),
            locale.text(GuiText::CustomProviderPlaceholder),
            &editor.fields.id,
            AsrProviderEditorField::Id,
            false,
        ),
        row![
            text(locale.text(GuiText::ProviderType)).width(160),
            kind_button(locale, AsrProviderKind::Local, &editor.kind, busy),
            kind_button(locale, AsrProviderKind::Command, &editor.kind, busy),
            kind_button(locale, AsrProviderKind::Remote, &editor.kind, busy),
        ]
        .spacing(10),
    ]
    .spacing(10)
    .into()
}

fn provider_kind_fields(
    locale: GuiLocale,
    editor: &AsrProviderEditorState,
    busy: bool,
) -> Element<'_, Message> {
    match editor.kind {
        AsrProviderKind::Local => text(locale.text(GuiText::HotwordsManagedOnPage)).into(),
        AsrProviderKind::Command => column![
            labeled_input(
                locale.text(GuiText::CommandField),
                locale.text(GuiText::ProviderCommandPlaceholder),
                editor.fields.command.as_str(),
                AsrProviderEditorField::Command,
                false,
            ),
            labeled_input(
                locale.text(GuiText::Arguments),
                locale.text(GuiText::JsonStringArray),
                editor.fields.args.as_str(),
                AsrProviderEditorField::Args,
                true,
            ),
            environment_editor_view(locale, &editor.fields.environment, busy),
        ]
        .spacing(10)
        .into(),
        AsrProviderKind::Remote => labeled_input(
            locale.text(GuiText::Endpoint),
            "https://provider.example/v1/audio/transcriptions",
            editor.fields.endpoint.as_str(),
            AsrProviderEditorField::Endpoint,
            editor.endpoint_secure,
        ),
    }
}

fn environment_editor_view(
    locale: GuiLocale,
    entries: &[AsrProviderEnvironmentEntry],
    busy: bool,
) -> Element<'_, Message> {
    let mut body = column![
        row![
            text(locale.text(GuiText::Environment))
                .size(18)
                .width(Length::Fill),
            keyboard_button(locale.text(GuiText::AddVariable)).on_press_maybe(
                (!busy).then_some(Message::AsrProvider(AsrProviderMessage::AddEnvironment)),
            ),
        ]
        .spacing(10)
    ]
    .spacing(8);
    if entries.is_empty() {
        body = body.push(text(locale.text(GuiText::NoEnvironmentVariables)));
    }
    for (index, entry) in entries.iter().enumerate() {
        body = body.push(
            row![
                text_input(locale.text(GuiText::VariableName), &entry.key)
                    .on_input(move |key| Message::AsrProvider(
                        AsrProviderMessage::EnvironmentKeyChanged { index, key }
                    ))
                    .width(Length::FillPortion(2)),
                text_input(locale.text(GuiText::Value), entry.value.as_str())
                    .secure(true)
                    .on_input(move |value| Message::AsrProvider(
                        AsrProviderMessage::EnvironmentValueChanged {
                            index,
                            value: SecretInput::new(value),
                        }
                    ))
                    .width(Length::FillPortion(3)),
                keyboard_button(locale.text(GuiText::Remove)).on_press_maybe((!busy).then_some(
                    Message::AsrProvider(AsrProviderMessage::RemoveEnvironment(index),)
                ),),
            ]
            .spacing(10),
        );
    }
    body.into()
}

fn kind_button<'a>(
    locale: GuiLocale,
    kind: AsrProviderKind,
    selected: &AsrProviderKind,
    busy: bool,
) -> Element<'a, Message> {
    let label = locale.text(kind_title(&kind));
    keyboard_button(text(if &kind == selected {
        locale.selected_label(label)
    } else {
        label.to_owned()
    }))
    .on_press_maybe(
        (!busy && &kind != selected)
            .then_some(Message::AsrProvider(AsrProviderMessage::KindChanged(kind))),
    )
    .into()
}

fn labeled_input<'a>(
    label: &'static str,
    placeholder: &'static str,
    value: &'a str,
    field: AsrProviderEditorField,
    secure: bool,
) -> Element<'a, Message> {
    row![
        text(label).width(160),
        text_input(placeholder, value)
            .secure(secure)
            .on_input(move |value| {
                Message::AsrProvider(AsrProviderMessage::EditorChanged {
                    field,
                    value: SecretInput::new(value),
                })
            })
            .width(Length::Fill),
    ]
    .spacing(10)
    .into()
}

const fn kind_title(kind: &AsrProviderKind) -> GuiText {
    match kind {
        AsrProviderKind::Local => GuiText::LocalTitle,
        AsrProviderKind::Remote => GuiText::RemoteTitle,
        AsrProviderKind::Command => GuiText::CommandTitle,
    }
}

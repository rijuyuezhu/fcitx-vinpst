//! Top-level modal presentation for recoverable GUI failures.

use iced::{
    Element, Length,
    widget::{column, container, opaque, row, text},
};

use crate::{App, GuiText, Message, OperationState, keyboard_action::keyboard_button};

impl App {
    pub(super) fn error_dialog_view(&self) -> Option<Element<'_, Message>> {
        let (message, retry) = match &self.operation {
            OperationState::Failed(message) => (message.as_str(), None),
            OperationState::Idle | OperationState::Running(_) | OperationState::Succeeded(_) => {
                if let Some(message) = self.model_install.failure_message() {
                    (message, Some(Message::RetryModelInstall))
                } else {
                    let message = self.script_install.failure_message()?;
                    (message, Some(Message::RetryScriptInstall))
                }
            }
        };

        let mut actions = row![].spacing(10);
        if let Some(retry) = retry {
            actions =
                actions.push(keyboard_button(self.locale.text(GuiText::Retry)).on_press(retry));
        }
        actions = actions
            .push(keyboard_button(self.locale.text(GuiText::Ok)).on_press(Message::DismissError));

        let dialog = container(
            column![
                text(self.locale.text(GuiText::ErrorDialogTitle)).size(24),
                text(message),
                actions,
            ]
            .spacing(16),
        )
        .padding(24)
        .max_width(560)
        .style(container::rounded_box);

        Some(opaque(
            container(dialog)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        ))
    }
}

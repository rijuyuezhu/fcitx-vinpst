//! Keyboard interaction and audited desktop capability reporting.

use iced::{
    Subscription, Task,
    keyboard::{self, Key, Modifiers, key},
    widget::operation,
};
use serde_json::{Value, json};

use crate::{App, Message, Page};

/// Keyboard-only interactions owned by the application shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionMessage {
    /// Move focus to the next focusable text control.
    FocusNext,
    /// Move focus to the previous focusable text control.
    FocusPrevious,
    /// Select one top-level page through a stable command shortcut.
    SelectPage(Page),
}

/// Listens only to keyboard events ignored by the active widget.
pub(crate) fn subscription() -> Subscription<Message> {
    keyboard::listen().filter_map(keyboard_message)
}

pub(crate) fn capability_snapshot() -> Value {
    json!({
        "toolkit": "iced-0.14",
        "accessibility_tree": {
            "available": false,
            "status": "blocked-by-toolkit",
        },
        "keyboard": {
            "tab_focus_traversal": true,
            "focus_scope": "text-controls",
            "button_focus_traversal": false,
            "page_shortcuts": ["Command+1", "Command+2", "Command+3", "Command+4"],
        },
        "input_method": {
            "preedit_commit": true,
            "backends": ["wayland", "x11"],
        },
        "clipboard": {
            "standard_text_editing": true,
            "backends": ["wayland", "x11"],
        },
    })
}

impl App {
    pub(crate) fn handle_interaction_message(
        &mut self,
        message: InteractionMessage,
    ) -> Task<Message> {
        match message {
            InteractionMessage::FocusNext => operation::focus_next(),
            InteractionMessage::FocusPrevious => operation::focus_previous(),
            InteractionMessage::SelectPage(page) => {
                self.select_page(page);
                Task::none()
            }
        }
    }
}

fn keyboard_message(event: keyboard::Event) -> Option<Message> {
    let keyboard::Event::KeyPressed {
        key,
        modifiers,
        repeat,
        ..
    } = event
    else {
        return None;
    };
    interaction_for_key(&key, modifiers, repeat).map(Message::Interaction)
}

fn interaction_for_key(
    key: &Key,
    modifiers: Modifiers,
    repeat: bool,
) -> Option<InteractionMessage> {
    if repeat {
        return None;
    }
    match key.as_ref() {
        Key::Named(key::Named::Tab) if modifiers == Modifiers::NONE => {
            Some(InteractionMessage::FocusNext)
        }
        Key::Named(key::Named::Tab) if modifiers == Modifiers::SHIFT => {
            Some(InteractionMessage::FocusPrevious)
        }
        Key::Character("1") if modifiers == Modifiers::COMMAND => {
            Some(InteractionMessage::SelectPage(Page::Control))
        }
        Key::Character("2") if modifiers == Modifiers::COMMAND => {
            Some(InteractionMessage::SelectPage(Page::Resources))
        }
        Key::Character("3") if modifiers == Modifiers::COMMAND => {
            Some(InteractionMessage::SelectPage(Page::Llm))
        }
        Key::Character("4") if modifiers == Modifiers::COMMAND => {
            Some(InteractionMessage::SelectPage(Page::Hotwords))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignored_keyboard_shortcuts_map_without_stealing_text_editing_commands() {
        assert_eq!(
            interaction_for_key(&Key::Named(key::Named::Tab), Modifiers::NONE, false,),
            Some(InteractionMessage::FocusNext)
        );
        assert_eq!(
            interaction_for_key(&Key::Named(key::Named::Tab), Modifiers::SHIFT, false,),
            Some(InteractionMessage::FocusPrevious)
        );
        assert_eq!(
            interaction_for_key(&Key::Character("3".into()), Modifiers::COMMAND, false,),
            Some(InteractionMessage::SelectPage(Page::Llm))
        );
        assert_eq!(
            interaction_for_key(&Key::Character("c".into()), Modifiers::COMMAND, false,),
            None
        );
        assert_eq!(
            interaction_for_key(&Key::Named(key::Named::Tab), Modifiers::CTRL, false,),
            None
        );
        assert_eq!(
            interaction_for_key(&Key::Character("1".into()), Modifiers::COMMAND, true,),
            None
        );
    }

    #[test]
    fn page_shortcuts_obey_busy_guards_while_focus_traversal_remains_available() {
        assert!(
            Message::Interaction(InteractionMessage::SelectPage(Page::Resources))
                .blocked_while_busy()
        );
        assert!(!Message::Interaction(InteractionMessage::FocusNext).blocked_while_busy());
        assert!(!Message::Interaction(InteractionMessage::FocusPrevious).blocked_while_busy());
    }

    #[test]
    fn dynamic_title_distinguishes_pages_and_locales() {
        let (mut app, boot_task) = App::boot();
        drop(boot_task);
        let control_title = app.window_title();
        app.page = Page::Resources;
        let resources_title = app.window_title();
        assert_ne!(control_title, resources_title);
        app.locale = crate::GuiLocale::ZhCn;
        assert_ne!(resources_title, app.window_title());
    }

    #[test]
    fn capability_snapshot_reports_supported_and_blocked_boundaries() {
        let snapshot = capability_snapshot();
        assert_eq!(snapshot["accessibility_tree"]["available"], false);
        assert_eq!(snapshot["keyboard"]["tab_focus_traversal"], true);
        assert_eq!(snapshot["keyboard"]["button_focus_traversal"], false);
        assert_eq!(snapshot["input_method"]["preedit_commit"], true);
        assert_eq!(snapshot["clipboard"]["standard_text_editing"], true);
    }
}

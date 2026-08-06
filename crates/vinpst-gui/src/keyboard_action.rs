//! Keyboard focus and activation wrapper for non-text controls.

use iced::advanced::Renderer as _;
use iced::{
    Border, Color, Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
        widget::{Operation, Tree, operation, tree},
    },
    keyboard::{self, Key, key},
    widget::{Button, Id},
};

/// Wraps one existing pointer control with focus traversal and activation.
pub(crate) struct KeyboardAction<'a, Message> {
    content: Element<'a, Message>,
    id: Option<Id>,
    on_activate: Option<Message>,
    on_previous: Option<Message>,
    on_next: Option<Message>,
}

#[derive(Debug, Default)]
struct State {
    focused: bool,
}

impl operation::Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl<'a, Message> KeyboardAction<'a, Message> {
    fn new(
        content: Element<'a, Message>,
        on_activate: Option<Message>,
        on_previous: Option<Message>,
        on_next: Option<Message>,
    ) -> Self {
        Self {
            content,
            id: None,
            on_activate,
            on_previous,
            on_next,
        }
    }
}

impl<Message> Widget<Message, Theme, Renderer> for KeyboardAction<'_, Message>
where
    Message: Clone,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let state = tree.state.downcast_mut::<State>();
        operation.focusable(self.id.as_ref(), layout.bounds(), state);
        operation.traverse(&mut |operation| {
            self.content.as_widget_mut().operate(
                &mut tree.children[0],
                layout,
                renderer,
                operation,
            );
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        if shell.is_event_captured() || !tree.state.downcast_ref::<State>().focused {
            return;
        }
        let message = match key_command(event) {
            Some(KeyCommand::Activate) => self.on_activate.as_ref(),
            Some(KeyCommand::Previous) => self.on_previous.as_ref(),
            Some(KeyCommand::Next) => self.on_next.as_ref(),
            None => None,
        };
        if let Some(message) = message {
            shell.publish(message.clone());
            shell.capture_event();
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
        if tree.state.downcast_ref::<State>().focused {
            let mut color = style.text_color;
            color.a = 0.9;
            renderer.fill_quad(
                renderer::Quad {
                    bounds: layout.bounds(),
                    border: Border::default().color(color).rounded(5).width(2),
                    ..renderer::Quad::default()
                },
                Color::TRANSPARENT,
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message> From<KeyboardAction<'a, Message>> for Element<'a, Message>
where
    Message: Clone + 'a,
{
    fn from(action: KeyboardAction<'a, Message>) -> Self {
        Element::new(action)
    }
}

/// Button builder that preserves Iced pointer behavior and adds keyboard activation.
pub(crate) struct KeyboardButton<'a, Message> {
    button: Button<'a, Message>,
    id: Option<Id>,
    on_activate: Option<Message>,
}

/// Creates a keyboard-aware button using the existing Iced button style and pointer behavior.
pub(crate) fn keyboard_button<'a, Message>(
    content: impl Into<Element<'a, Message>>,
) -> KeyboardButton<'a, Message> {
    KeyboardButton {
        button: Button::new(content),
        id: None,
        on_activate: None,
    }
}

impl<Message> KeyboardButton<'_, Message> {
    pub(crate) fn id(mut self, id: impl Into<Id>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub(crate) fn width(mut self, width: impl Into<Length>) -> Self {
        self.button = self.button.width(width);
        self
    }
}

impl<Message> KeyboardButton<'_, Message>
where
    Message: Clone,
{
    pub(crate) fn on_press(mut self, message: Message) -> Self {
        self.button = self.button.on_press(message.clone());
        self.on_activate = Some(message);
        self
    }

    pub(crate) fn on_press_maybe(mut self, message: Option<Message>) -> Self {
        self.button = self.button.on_press_maybe(message.clone());
        self.on_activate = message;
        self
    }
}

impl<'a, Message> From<KeyboardButton<'a, Message>> for Element<'a, Message>
where
    Message: Clone + 'a,
{
    fn from(button: KeyboardButton<'a, Message>) -> Self {
        keyboard_bindings(button.button, button.on_activate, None, None, button.id)
    }
}

/// Adds keyboard focus only when the underlying action is enabled.
pub(crate) fn keyboard_action<'a, Message>(
    content: impl Into<Element<'a, Message>>,
    on_activate: Option<Message>,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    keyboard_bindings(content, on_activate, None, None, None)
}

/// Adds arrow-key previous/next selection while preserving the pointer control.
pub(crate) fn keyboard_select<'a, Message>(
    content: impl Into<Element<'a, Message>>,
    on_previous: Option<Message>,
    on_next: Option<Message>,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    keyboard_bindings(content, None, on_previous, on_next, None)
}

/// Returns bounded adjacent values around the selected entry.
pub(crate) fn adjacent_values<T>(options: &[T], selected: Option<&T>) -> (Option<T>, Option<T>)
where
    T: Clone + PartialEq,
{
    let Some(index) =
        selected.and_then(|selected| options.iter().position(|value| value == selected))
    else {
        return (None, options.first().cloned());
    };
    (
        index
            .checked_sub(1)
            .and_then(|index| options.get(index))
            .cloned(),
        options.get(index + 1).cloned(),
    )
}

fn keyboard_bindings<'a, Message>(
    content: impl Into<Element<'a, Message>>,
    on_activate: Option<Message>,
    on_previous: Option<Message>,
    on_next: Option<Message>,
    id: Option<Id>,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let content = content.into();
    if on_activate.is_none() && on_previous.is_none() && on_next.is_none() {
        content
    } else {
        let mut action = KeyboardAction::new(content, on_activate, on_previous, on_next);
        action.id = id;
        action.into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyCommand {
    Activate,
    Previous,
    Next,
}

fn key_command(event: &Event) -> Option<KeyCommand> {
    let Event::Keyboard(keyboard::Event::KeyPressed {
        key,
        modifiers,
        repeat: false,
        ..
    }) = event
    else {
        return None;
    };
    if !modifiers.is_empty() {
        return None;
    }
    match key {
        Key::Named(key::Named::Enter | key::Named::Space) => Some(KeyCommand::Activate),
        Key::Named(key::Named::ArrowUp | key::Named::ArrowLeft) => Some(KeyCommand::Previous),
        Key::Named(key::Named::ArrowDown | key::Named::ArrowRight) => Some(KeyCommand::Next),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use iced::keyboard::{
        Location, Modifiers,
        key::{NativeCode, Physical},
    };

    use super::*;

    fn key_pressed(key: Key, modifiers: Modifiers, repeat: bool) -> Event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            modified_key: key.clone(),
            key,
            physical_key: Physical::Unidentified(NativeCode::Unidentified),
            location: Location::Standard,
            modifiers,
            text: None,
            repeat,
        })
    }

    #[test]
    fn activation_and_selection_keys_require_no_modifiers_or_repeat() {
        assert_eq!(
            key_command(&key_pressed(
                Key::Named(key::Named::Enter),
                Modifiers::NONE,
                false
            )),
            Some(KeyCommand::Activate),
        );
        assert_eq!(
            key_command(&key_pressed(
                Key::Named(key::Named::Space),
                Modifiers::NONE,
                false
            )),
            Some(KeyCommand::Activate),
        );
        assert_eq!(
            key_command(&key_pressed(
                Key::Named(key::Named::ArrowLeft),
                Modifiers::NONE,
                false
            )),
            Some(KeyCommand::Previous),
        );
        assert_eq!(
            key_command(&key_pressed(
                Key::Named(key::Named::ArrowDown),
                Modifiers::NONE,
                false
            )),
            Some(KeyCommand::Next),
        );
        assert_eq!(
            key_command(&key_pressed(
                Key::Named(key::Named::Enter),
                Modifiers::CTRL,
                false
            )),
            None,
        );
        assert_eq!(
            key_command(&key_pressed(
                Key::Named(key::Named::Space),
                Modifiers::NONE,
                true
            )),
            None,
        );
    }

    #[test]
    fn adjacent_values_are_bounded_and_handle_missing_selection() {
        let options = ["one", "two", "three"];
        assert_eq!(
            adjacent_values(&options, Some(&"two")),
            (Some("one"), Some("three"))
        );
        assert_eq!(adjacent_values(&options, Some(&"one")), (None, Some("two")));
        assert_eq!(
            adjacent_values(&options, Some(&"missing")),
            (None, Some("one"))
        );
        assert_eq!(adjacent_values::<&str>(&[], None), (None, None));
    }
}

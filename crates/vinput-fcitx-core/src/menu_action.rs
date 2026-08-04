//! Pure menu key-action decisions for the retained Fcitx adapter.

use crate::{MenuFilterState, MenuSessionState};

/// Candidate page size used by both retained scene and ASR menus.
pub const MENU_PAGE_SIZE: i32 = 10;

/// One Fcitx key translated into a stable semantic menu input.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MenuSemanticKey<'a> {
    /// Trigger key or pure modifier; consume without changing the menu.
    Passive,
    /// Escape key.
    Escape,
    /// Slash filter-activation key.
    Slash,
    /// Backspace key.
    Backspace,
    /// Ctrl+W delete-word shortcut.
    DeleteWord,
    /// Ctrl+U clear-filter shortcut.
    ClearFilter,
    /// Printable UTF-8 accepted as filter input.
    Text(&'a str),
    /// Relative page movement, normally `-1` or `1`.
    Page(i32),
    /// Zero-based digit selection on the current page.
    Digit(usize),
    /// Up-arrow request.
    MovePrevious,
    /// Down-arrow request.
    MoveNext,
    /// Enter request.
    Enter,
    /// Any key not handled by the menu.
    #[default]
    Other,
}

impl MenuSemanticKey<'_> {
    const fn handled(self) -> bool {
        !matches!(self, Self::Other)
    }
}

/// Fcitx-specific menu context accompanying one semantic key.
///
/// The retained C++ adapter owns `fcitx::Key` matching and candidate-list
/// inspection. Rust owns action priority, filter mutation, paging targets, and
/// visible-row selection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MenuKeyInput<'a> {
    /// Whether this is a key-release event.
    pub release: bool,
    /// Semantic key translated by the retained C++ adapter.
    pub key: MenuSemanticKey<'a>,
    /// Whether the current Fcitx candidate list exposes cursor movement.
    pub cursor_available: bool,
    /// Current zero-based visible-row selection across all pages.
    pub current_selection: Option<usize>,
    /// Current zero-based page.
    pub current_page: i32,
    /// Number of currently visible menu rows.
    pub visible_item_count: usize,
}

/// Fcitx-specific key context for a Rust-owned menu session.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MenuSessionKeyInput<'a> {
    /// Whether this is a key-release event.
    pub release: bool,
    /// Semantic key translated by the retained C++ adapter.
    pub key: MenuSemanticKey<'a>,
    /// Whether the current Fcitx candidate list exposes cursor movement.
    pub cursor_available: bool,
    /// Current zero-based visible-row selection across all pages.
    pub current_selection: Option<usize>,
    /// Number of currently visible menu rows.
    pub visible_item_count: usize,
}

/// Action returned to the retained Fcitx adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKeyAction {
    /// Leave the menu open and let the event continue.
    Pass,
    /// Consume the event without changing the menu.
    Consume,
    /// Close the menu and let the event continue.
    CloseAndPass,
    /// Close the menu and consume the event.
    CloseAndConsume,
    /// Rebuild the menu at a requested zero-based page.
    Rebuild {
        /// Requested zero-based page before candidate-list clamping.
        page: i32,
    },
    /// Move the Fcitx candidate cursor to the previous row.
    MovePrevious,
    /// Move the Fcitx candidate cursor to the next row.
    MoveNext,
    /// Select one zero-based row from the visible projected menu.
    Select {
        /// Zero-based index in the currently visible projected rows.
        visible_index: usize,
    },
}

impl MenuFilterState {
    /// Applies one semantic key event and returns the corresponding frontend action.
    pub fn handle_key(&mut self, input: MenuKeyInput<'_>) -> MenuKeyAction {
        if input.release {
            return if input.key.handled() {
                MenuKeyAction::Consume
            } else {
                MenuKeyAction::Pass
            };
        }

        match input.key {
            MenuSemanticKey::Passive => MenuKeyAction::Consume,
            MenuSemanticKey::Escape => {
                if self.active() || !self.query().is_empty() {
                    self.clear_and_deactivate();
                    MenuKeyAction::Rebuild { page: 0 }
                } else {
                    MenuKeyAction::CloseAndConsume
                }
            }
            MenuSemanticKey::Slash => {
                self.activate();
                MenuKeyAction::Rebuild { page: 0 }
            }
            MenuSemanticKey::Backspace if self.active() => {
                self.backspace();
                MenuKeyAction::Rebuild { page: 0 }
            }
            MenuSemanticKey::DeleteWord if self.active() => {
                self.delete_last_word();
                MenuKeyAction::Rebuild { page: 0 }
            }
            MenuSemanticKey::ClearFilter if self.active() => {
                self.clear_and_deactivate();
                MenuKeyAction::Rebuild { page: 0 }
            }
            MenuSemanticKey::Text(text) if !text.is_empty() => {
                self.append_text(text);
                MenuKeyAction::Rebuild { page: 0 }
            }
            MenuSemanticKey::Page(delta) => MenuKeyAction::Rebuild {
                page: input.current_page.saturating_add(delta),
            },
            MenuSemanticKey::Digit(digit) => {
                visible_index(input.current_page, digit, input.visible_item_count)
                    .map_or(MenuKeyAction::Consume, |visible_index| {
                        MenuKeyAction::Select { visible_index }
                    })
            }
            MenuSemanticKey::MovePrevious if input.cursor_available => MenuKeyAction::MovePrevious,
            MenuSemanticKey::MoveNext if input.cursor_available => MenuKeyAction::MoveNext,
            MenuSemanticKey::Enter => {
                let visible_index = input.current_selection.or_else(|| {
                    (input.visible_item_count > 0)
                        .then(|| visible_index(input.current_page, 0, input.visible_item_count))
                        .flatten()
                });
                visible_index.map_or(MenuKeyAction::CloseAndConsume, |visible_index| {
                    MenuKeyAction::Select { visible_index }
                })
            }
            MenuSemanticKey::Other
            | MenuSemanticKey::Backspace
            | MenuSemanticKey::DeleteWord
            | MenuSemanticKey::ClearFilter
            | MenuSemanticKey::Text(_)
            | MenuSemanticKey::MovePrevious
            | MenuSemanticKey::MoveNext => MenuKeyAction::CloseAndPass,
        }
    }
}

impl MenuSessionState {
    /// Applies one key to an open menu and owns close/select lifecycle transitions.
    pub fn handle_key(&mut self, input: MenuSessionKeyInput<'_>) -> Option<MenuKeyAction> {
        if !self.is_open() {
            return None;
        }
        let current_page = self.page();
        let action = self.filter_mut().handle_key(MenuKeyInput {
            release: input.release,
            key: input.key,
            cursor_available: input.cursor_available,
            current_selection: input.current_selection,
            current_page,
            visible_item_count: input.visible_item_count,
        });
        if matches!(
            action,
            MenuKeyAction::CloseAndPass
                | MenuKeyAction::CloseAndConsume
                | MenuKeyAction::Select { .. }
        ) {
            self.close();
        }
        Some(action)
    }
}

fn visible_index(current_page: i32, offset: usize, visible_item_count: usize) -> Option<usize> {
    let page = usize::try_from(current_page).ok()?;
    let page_size = usize::try_from(MENU_PAGE_SIZE).expect("positive menu page size");
    let index = page.checked_mul(page_size)?.checked_add(offset)?;
    (index < visible_item_count).then_some(index)
}

#[cfg(test)]
mod tests {
    use super::{
        MENU_PAGE_SIZE, MenuKeyAction, MenuKeyInput, MenuSemanticKey, MenuSessionKeyInput,
    };
    use crate::{MenuFilterState, MenuSessionState};

    #[test]
    fn distinguishes_release_and_unhandled_press_behavior() {
        let mut filter = MenuFilterState::default();
        assert_eq!(
            filter.handle_key(MenuKeyInput {
                release: true,
                key: MenuSemanticKey::Slash,
                ..MenuKeyInput::default()
            }),
            MenuKeyAction::Consume
        );
        assert_eq!(
            filter.handle_key(MenuKeyInput {
                release: true,
                ..MenuKeyInput::default()
            }),
            MenuKeyAction::Pass
        );
        assert_eq!(
            filter.handle_key(MenuKeyInput::default()),
            MenuKeyAction::CloseAndPass
        );
        assert_eq!(
            filter.handle_key(MenuKeyInput {
                key: MenuSemanticKey::Passive,
                ..MenuKeyInput::default()
            }),
            MenuKeyAction::Consume
        );
    }

    #[test]
    fn mutates_filter_using_legacy_action_priority() {
        let mut filter = MenuFilterState::default();
        assert_eq!(
            filter.handle_key(MenuKeyInput {
                key: MenuSemanticKey::Slash,
                ..MenuKeyInput::default()
            }),
            MenuKeyAction::Rebuild { page: 0 }
        );
        assert!(filter.active());

        assert_eq!(
            filter.handle_key(MenuKeyInput {
                key: MenuSemanticKey::Text("MOON en 中a"),
                ..MenuKeyInput::default()
            }),
            MenuKeyAction::Rebuild { page: 0 }
        );
        assert_eq!(filter.query(), "MOON en 中a");

        assert_eq!(
            filter.handle_key(MenuKeyInput {
                key: MenuSemanticKey::Backspace,
                ..MenuKeyInput::default()
            }),
            MenuKeyAction::Rebuild { page: 0 }
        );
        assert_eq!(filter.query(), "MOON en 中");

        assert_eq!(
            filter.handle_key(MenuKeyInput {
                key: MenuSemanticKey::DeleteWord,
                ..MenuKeyInput::default()
            }),
            MenuKeyAction::Rebuild { page: 0 }
        );
        assert_eq!(filter.query(), "MOON en ");

        assert_eq!(
            filter.handle_key(MenuKeyInput {
                key: MenuSemanticKey::ClearFilter,
                ..MenuKeyInput::default()
            }),
            MenuKeyAction::Rebuild { page: 0 }
        );
        assert!(!filter.active());
        assert!(filter.query().is_empty());

        assert_eq!(
            filter.handle_key(MenuKeyInput {
                key: MenuSemanticKey::Backspace,
                ..MenuKeyInput::default()
            }),
            MenuKeyAction::CloseAndPass
        );
    }

    #[test]
    fn escape_first_clears_filter_then_closes_menu() {
        let mut filter = MenuFilterState::default();
        filter.activate();
        filter.append_text("moon");
        assert_eq!(
            filter.handle_key(MenuKeyInput {
                key: MenuSemanticKey::Escape,
                ..MenuKeyInput::default()
            }),
            MenuKeyAction::Rebuild { page: 0 }
        );
        assert!(!filter.active());
        assert!(filter.query().is_empty());
        assert_eq!(
            filter.handle_key(MenuKeyInput {
                key: MenuSemanticKey::Escape,
                ..MenuKeyInput::default()
            }),
            MenuKeyAction::CloseAndConsume
        );
    }

    #[test]
    fn resolves_page_digit_and_cursor_actions() {
        let mut filter = MenuFilterState::default();
        assert_eq!(MENU_PAGE_SIZE, 10);
        assert_eq!(
            filter.handle_key(MenuKeyInput {
                key: MenuSemanticKey::Page(-1),
                current_page: 0,
                ..MenuKeyInput::default()
            }),
            MenuKeyAction::Rebuild { page: -1 }
        );
        assert_eq!(
            filter.handle_key(MenuKeyInput {
                key: MenuSemanticKey::Digit(2),
                current_page: 1,
                visible_item_count: 13,
                ..MenuKeyInput::default()
            }),
            MenuKeyAction::Select { visible_index: 12 }
        );
        assert_eq!(
            filter.handle_key(MenuKeyInput {
                key: MenuSemanticKey::Digit(3),
                current_page: 1,
                visible_item_count: 13,
                ..MenuKeyInput::default()
            }),
            MenuKeyAction::Consume
        );
        assert_eq!(
            filter.handle_key(MenuKeyInput {
                key: MenuSemanticKey::MovePrevious,
                cursor_available: true,
                ..MenuKeyInput::default()
            }),
            MenuKeyAction::MovePrevious
        );
        assert_eq!(
            filter.handle_key(MenuKeyInput {
                key: MenuSemanticKey::MoveNext,
                cursor_available: true,
                ..MenuKeyInput::default()
            }),
            MenuKeyAction::MoveNext
        );
        assert_eq!(
            filter.handle_key(MenuKeyInput {
                key: MenuSemanticKey::MovePrevious,
                ..MenuKeyInput::default()
            }),
            MenuKeyAction::CloseAndPass
        );
    }

    #[test]
    fn enter_uses_cursor_then_current_page_fallback() {
        let mut filter = MenuFilterState::default();
        assert_eq!(
            filter.handle_key(MenuKeyInput {
                key: MenuSemanticKey::Enter,
                current_selection: Some(11),
                current_page: 1,
                visible_item_count: 13,
                ..MenuKeyInput::default()
            }),
            MenuKeyAction::Select { visible_index: 11 }
        );
        assert_eq!(
            filter.handle_key(MenuKeyInput {
                key: MenuSemanticKey::Enter,
                current_page: 1,
                visible_item_count: 13,
                ..MenuKeyInput::default()
            }),
            MenuKeyAction::Select { visible_index: 10 }
        );
        assert_eq!(
            filter.handle_key(MenuKeyInput {
                key: MenuSemanticKey::Enter,
                current_page: 2,
                visible_item_count: 13,
                ..MenuKeyInput::default()
            }),
            MenuKeyAction::CloseAndConsume
        );
        assert_eq!(
            filter.handle_key(MenuKeyInput {
                key: MenuSemanticKey::Enter,
                ..MenuKeyInput::default()
            }),
            MenuKeyAction::CloseAndConsume
        );
    }

    #[test]
    fn session_uses_owned_page_and_closes_on_terminal_actions() {
        let mut session = MenuSessionState::default();
        assert_eq!(session.handle_key(MenuSessionKeyInput::default()), None);

        session.open();
        assert!(session.set_page(2));
        assert_eq!(
            session.handle_key(MenuSessionKeyInput {
                key: MenuSemanticKey::Digit(3),
                visible_item_count: 30,
                ..MenuSessionKeyInput::default()
            }),
            Some(MenuKeyAction::Select { visible_index: 23 })
        );
        assert!(!session.is_open());

        session.open();
        assert_eq!(
            session.handle_key(MenuSessionKeyInput {
                key: MenuSemanticKey::Escape,
                ..MenuSessionKeyInput::default()
            }),
            Some(MenuKeyAction::CloseAndConsume)
        );
        assert!(!session.is_open());
    }
}

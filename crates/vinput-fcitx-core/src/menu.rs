//! Pure menu filter and paging behavior for the retained Fcitx adapter.

/// Mutable text filter used by the scene and ASR candidate menus.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MenuFilterState {
    active: bool,
    query: String,
}

impl MenuFilterState {
    /// Clears the query and leaves filter-entry mode.
    pub fn reset(&mut self) {
        self.active = false;
        self.query.clear();
    }

    /// Enters filter-entry mode without changing the current query.
    pub const fn activate(&mut self) {
        self.active = true;
    }

    /// Clears the query and leaves filter-entry mode.
    pub fn clear_and_deactivate(&mut self) {
        self.reset();
    }

    /// Removes one Unicode scalar value from the query.
    ///
    /// Backspace on an already-empty active query leaves filter-entry mode. For
    /// compatibility with the retained frontend, deleting the final character
    /// keeps the filter active until the next backspace or an explicit reset.
    pub fn backspace(&mut self) {
        if !self.active {
            return;
        }
        if self.query.is_empty() {
            self.active = false;
            return;
        }
        self.query.pop();
    }

    /// Deletes trailing ASCII whitespace and the preceding Unicode word.
    pub fn delete_last_word(&mut self) {
        if !self.active {
            return;
        }

        while self
            .query
            .chars()
            .next_back()
            .is_some_and(|character| character.is_ascii_whitespace())
        {
            self.query.pop();
        }
        while let Some(character) = self.query.chars().next_back() {
            if character.is_ascii_whitespace() {
                break;
            }
            self.query.pop();
        }
        if self.query.is_empty() {
            self.active = false;
        }
    }

    /// Appends valid UTF-8 while filter-entry mode is active.
    pub fn append_text(&mut self, text: &str) {
        if self.active {
            self.query.push_str(text);
        }
    }

    /// Returns whether filter-entry mode is active.
    #[must_use]
    pub const fn active(&self) -> bool {
        self.active
    }

    /// Returns the current query.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Matches every ASCII-whitespace-separated query term in `search_text`.
    ///
    /// Only ASCII letters are folded, matching the byte-oriented behavior of
    /// the retained C++ implementation while leaving non-ASCII UTF-8 unchanged.
    #[must_use]
    pub fn matches(&self, search_text: &str) -> bool {
        if self.query.is_empty() {
            return true;
        }

        let normalized_haystack = search_text.to_ascii_lowercase();
        self.query
            .to_ascii_lowercase()
            .split_ascii_whitespace()
            .all(|term| normalized_haystack.contains(term))
    }

    /// Appends the visible filter prompt and query to a menu title.
    #[must_use]
    pub fn decorate_title(&self, base_title: &str) -> String {
        if !self.active && self.query.is_empty() {
            return base_title.to_owned();
        }
        if base_title.ends_with(" /") {
            return format!("{base_title}{}", self.query);
        }
        format!("{base_title} / {}", self.query)
    }
}

/// Clamps a requested zero-based page to the available page range.
#[must_use]
pub fn clamp_menu_page(total_pages: i32, requested_page: i32) -> Option<i32> {
    (total_pages > 0).then(|| requested_page.clamp(0, total_pages - 1))
}

#[cfg(test)]
mod tests {
    use super::{MenuFilterState, clamp_menu_page};

    #[test]
    fn edits_utf8_query_with_legacy_activation_rules() {
        let mut filter = MenuFilterState::default();
        assert!(!filter.active());
        assert!(filter.query().is_empty());

        filter.activate();
        filter.append_text("MOON en 中a");
        filter.backspace();
        assert_eq!(filter.query(), "MOON en 中");
        filter.backspace();
        assert_eq!(filter.query(), "MOON en ");
        filter.delete_last_word();
        assert_eq!(filter.query(), "MOON ");
        filter.delete_last_word();
        assert!(filter.query().is_empty());
        assert!(!filter.active());

        filter.activate();
        filter.append_text("a");
        filter.backspace();
        assert!(filter.active());
        assert!(filter.query().is_empty());
        filter.backspace();
        assert!(!filter.active());
    }

    #[test]
    fn matches_all_ascii_folded_terms() {
        let mut filter = MenuFilterState::default();
        assert!(filter.matches("Moonshine English"));

        filter.activate();
        filter.append_text("MOON en");
        assert!(filter.matches("moonshine English provider"));
        assert!(!filter.matches("moonshine Chinese provider"));

        filter.clear_and_deactivate();
        filter.activate();
        filter.append_text("中文 A");
        assert!(filter.matches("中文 adapter"));
        assert!(!filter.matches("中 adapter"));
    }

    #[test]
    fn decorates_filter_title_compatibly() {
        let mut filter = MenuFilterState::default();
        assert_eq!(filter.decorate_title("Models /filter"), "Models /filter");

        filter.activate();
        assert_eq!(filter.decorate_title("Models /"), "Models /");
        assert_eq!(filter.decorate_title("Models"), "Models / ");
        filter.append_text("MOON en");
        assert_eq!(filter.decorate_title("Models /"), "Models /MOON en");
        assert_eq!(filter.decorate_title("Models"), "Models / MOON en");
    }

    #[test]
    fn clamps_requested_menu_page() {
        assert_eq!(clamp_menu_page(0, 0), None);
        assert_eq!(clamp_menu_page(-1, 0), None);
        assert_eq!(clamp_menu_page(2, -1), Some(0));
        assert_eq!(clamp_menu_page(2, 0), Some(0));
        assert_eq!(clamp_menu_page(2, 1), Some(1));
        assert_eq!(clamp_menu_page(2, 99), Some(1));
    }
}

//! Top-level management GUI page identifiers.

use iced::{
    Element, Length,
    widget::{button, column, text},
};

use crate::{APPLICATION_TITLE, App, Message};

/// Main GUI pages matching the legacy management surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// Daemon and audio controls.
    Control,
    /// ASR providers and scenes.
    Resources,
    /// LLM providers and adapters.
    Llm,
    /// Hotword file configuration.
    Hotwords,
}

impl Page {
    pub(crate) const ALL: [Self; 4] = [Self::Control, Self::Resources, Self::Llm, Self::Hotwords];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Control => "Control",
            Self::Resources => "Resources",
            Self::Llm => "LLM",
            Self::Hotwords => "Hotwords",
        }
    }
}

impl App {
    pub(super) fn navigation_view(&self, busy: bool) -> Element<'_, Message> {
        let navigation = Page::ALL.into_iter().fold(
            column![text(APPLICATION_TITLE).size(24)].spacing(10),
            |navigation, page| {
                navigation.push(
                    button(text(page.label()))
                        .width(Length::Fill)
                        .on_press_maybe((!busy).then_some(Message::SelectPage(page))),
                )
            },
        );
        navigation.push(self.desktop_action_button(busy)).into()
    }

    pub(super) fn select_page(&mut self, page: Page) {
        if self.page == page {
            return;
        }
        if !self.guard_hotword_changes("leaving the Hotwords page") {
            return;
        }
        self.page = page;
        self.selected_resource = None;
        self.scene_editor = None;
        self.asr_provider_editor = None;
        self.llm_provider_editor = None;
        self.adapter_config_editor = None;
    }
}

//! Top-level management GUI page identifiers.

use crate::App;

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
    }
}

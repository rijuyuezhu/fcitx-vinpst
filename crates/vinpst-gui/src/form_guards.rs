//! Cross-form guards for configuration mutations.

use crate::App;

impl App {
    pub(crate) fn ensure_no_open_llm_provider_editor(&self) -> Result<(), String> {
        if self.llm_provider_editor.is_some() {
            return Err(self.locale.open_llm_provider_form_guard());
        }
        Ok(())
    }

    pub(crate) fn ensure_no_open_asr_provider_editor(&self) -> Result<(), String> {
        if self.asr_provider_editor.is_some() {
            return Err(self.locale.open_asr_provider_form_guard());
        }
        Ok(())
    }
}

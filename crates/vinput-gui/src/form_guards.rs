//! Cross-form guards for configuration mutations.

use crate::App;

impl App {
    pub(crate) fn ensure_no_open_llm_provider_editor(&self) -> Result<(), String> {
        if self.llm_provider_editor.is_some() {
            return Err(
                "Save or cancel the open LLM provider form before modifying provider or adapter scripts."
                    .to_owned(),
            );
        }
        Ok(())
    }

    pub(crate) fn ensure_no_open_asr_provider_editor(&self) -> Result<(), String> {
        if self.asr_provider_editor.is_some() {
            return Err(
                "Save or cancel the open ASR provider form before modifying resources.".to_owned(),
            );
        }
        Ok(())
    }
}

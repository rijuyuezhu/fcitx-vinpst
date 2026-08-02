//! Guarded provider and adapter removal tasks for the GUI.

use iced::Task;
use vinput_registry::LiveScriptKind;

use crate::{
    App, Message, OperationState, load_config_document,
    script_management::{remove_managed_script_entry, resource_label},
};

impl App {
    pub(crate) fn begin_script_remove(
        &mut self,
        kind: LiveScriptKind,
        id: String,
    ) -> Task<Message> {
        let Ok(document) = &self.config else {
            self.operation = OperationState::Failed("No valid config is loaded.".to_owned());
            return Task::none();
        };
        if self.is_busy() {
            return Task::none();
        }
        self.operation = OperationState::Running(match kind {
            LiveScriptKind::AsrProvider => "Removing provider…",
            LiveScriptKind::LlmAdapter => "Removing adapter…",
        });
        let document = document.clone();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    remove_managed_script_entry(&document, kind, &id)
                })
                .await
                .unwrap_or_else(|_| {
                    Err(format!(
                        "{} removal worker stopped unexpectedly.",
                        resource_label(kind)
                    ))
                })
            },
            Message::ScriptRemoved,
        )
    }

    pub(crate) fn finish_script_remove(&mut self, result: Result<String, String>) -> Task<Message> {
        match result {
            Ok(summary) => {
                let path = self
                    .config
                    .as_ref()
                    .ok()
                    .map(|document| document.path.clone());
                self.replace_config(load_config_document(path.as_deref()));
                self.operation = OperationState::Succeeded(summary);
            }
            Err(error) => self.operation = OperationState::Failed(error),
        }
        self.begin_daemon_refresh(false)
    }
}

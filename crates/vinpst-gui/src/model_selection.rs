//! Managed-model selection workflow for the Resources page.

use std::path::PathBuf;

use iced::Task;

use crate::{
    App, GuiText, Message, OperationState, blocking_task, load_config_document,
    load_installed_models, save_updated_config_with_daemon, select_model_for_active_provider,
};

impl App {
    pub(crate) fn begin_model_select(&mut self, target_path: PathBuf) -> Task<Message> {
        if let Err(error) = self
            .ensure_no_unsaved_config_draft()
            .and_then(|()| self.ensure_no_open_scene_editor())
            .and_then(|()| self.ensure_no_open_asr_provider_editor())
        {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        let Ok(document) = &self.config else {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::NoValidConfigLoaded).to_owned());
            return Task::none();
        };
        let document = document.clone();
        let locale = self.locale;
        self.operation = OperationState::Running(self.locale.text(GuiText::SelectingModel));
        blocking_task::perform(
            "vinpst-gui-model-select",
            move || {
                let (updated, provider_id) =
                    select_model_for_active_provider(&document.config, &target_path)?;
                let outcome = save_updated_config_with_daemon(&document, &updated)?;
                let directory = target_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("managed-model");
                Ok(locale.model_selected(directory, &provider_id, &outcome.daemon_reload))
            },
            |result| {
                Message::ModelSelected(result.unwrap_or_else(|failure| Err(failure.to_string())))
            },
        )
    }

    pub(crate) fn finish_model_select(&mut self, result: Result<String, String>) -> Task<Message> {
        match result {
            Ok(summary) => {
                let path = self
                    .config
                    .as_ref()
                    .ok()
                    .map(|document| document.path.clone());
                match load_config_document(path.as_deref()) {
                    Ok(document) => {
                        self.replace_config(Ok(document));
                        self.installed_models = load_installed_models();
                        self.operation = OperationState::Succeeded(summary);
                    }
                    Err(error) => {
                        self.replace_config(Err(error.clone()));
                        self.operation = OperationState::Failed(format!(
                            "Model selection was saved, but the GUI could not reload the updated config: {error}"
                        ));
                    }
                }
            }
            Err(error) => self.operation = OperationState::Failed(error),
        }
        self.begin_daemon_refresh(false)
    }
}

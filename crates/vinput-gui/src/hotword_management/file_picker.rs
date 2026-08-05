//! Portal-backed hotword file selection without bypassing config validation.

use std::path::PathBuf;

use iced::Task;

use super::{HotwordMessage, normalized_hotword_path};
use crate::{App, GuiLocale, GuiText, Message, OperationState, SecretInput};

impl App {
    pub(super) fn begin_hotword_file_browse(&mut self) -> Task<Message> {
        if self.hotword_editor.content_is_dirty() {
            self.operation = OperationState::Failed(
                self.locale
                    .text(GuiText::SaveOrResetHotwordBeforeSelecting)
                    .to_owned(),
            );
            return Task::none();
        }
        if self.hotword_editor.selected_provider.is_none() {
            self.operation = OperationState::Failed(
                self.locale
                    .text(GuiText::NoHotwordProviderSelected)
                    .to_owned(),
            );
            return Task::none();
        }
        let starting_directory = hotword_file_dialog_directory(&self.hotword_editor.path_input);
        let locale = self.locale;
        self.operation = OperationState::Running(locale.text(GuiText::SelectingHotwordFile));
        Task::perform(pick_hotword_file(starting_directory, locale), |result| {
            Message::Hotword(HotwordMessage::PathPicked(result))
        })
    }

    pub(super) fn finish_hotword_file_browse(
        &mut self,
        result: Result<Option<SecretInput>, String>,
    ) {
        match result {
            Ok(Some(path)) => {
                let path = path.into_inner();
                if self.hotword_editor.loaded_path.as_ref()
                    != normalized_hotword_path(&path).as_ref()
                {
                    self.hotword_editor.clear_loaded_content();
                }
                self.hotword_editor.path_input = path;
                self.operation = OperationState::Succeeded(
                    self.locale.text(GuiText::SelectedHotwordFile).to_owned(),
                );
            }
            Ok(None) => self.operation = OperationState::Idle,
            Err(error) => self.operation = OperationState::Failed(error),
        }
    }
}

async fn pick_hotword_file(
    starting_directory: Option<PathBuf>,
    locale: GuiLocale,
) -> Result<Option<SecretInput>, String> {
    let mut dialog = rfd::AsyncFileDialog::new()
        .set_title(locale.text(GuiText::SelectHotwordsFile))
        .add_filter(locale.text(GuiText::TextFiles), &["txt"])
        .add_filter(locale.text(GuiText::AllFiles), &["*"]);
    if let Some(directory) = starting_directory {
        dialog = dialog.set_directory(directory);
    }
    let Some(file) = dialog.pick_file().await else {
        return Ok(None);
    };
    let path = file
        .path()
        .to_str()
        .ok_or_else(|| locale.text(GuiText::InvalidUtf8HotwordPath).to_owned())?;
    Ok(Some(SecretInput::new(path.to_owned())))
}

fn hotword_file_dialog_directory(path_input: &str) -> Option<PathBuf> {
    let path = normalized_hotword_path(path_input)?;
    let candidate = if path.is_dir() {
        path
    } else {
        path.parent()?.to_path_buf()
    };
    candidate.is_dir().then_some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_directory_uses_existing_path_or_parent() {
        let directory = tempfile::tempdir().expect("dialog directory fixture");
        let file = directory.path().join("hotwords.txt");
        std::fs::write(&file, "fixture").expect("hotword fixture");

        assert_eq!(
            hotword_file_dialog_directory(directory.path().to_str().expect("UTF-8 fixture")),
            Some(directory.path().to_path_buf())
        );
        assert_eq!(
            hotword_file_dialog_directory(file.to_str().expect("UTF-8 fixture")),
            Some(directory.path().to_path_buf())
        );
        assert!(hotword_file_dialog_directory("").is_none());
        assert!(hotword_file_dialog_directory("/definitely/missing/hotwords.txt").is_none());
    }
}

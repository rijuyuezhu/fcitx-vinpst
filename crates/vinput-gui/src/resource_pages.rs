//! Resources, LLM, and hotword page rendering.

use iced::{
    Element, Length,
    widget::{button, column, row, scrollable, text, text_input},
};
use vinput_config::redact_url_for_diagnostics;

use crate::{
    App, Message, filtered_asr_rows, filtered_scene_rows, llm_adapter_rows, model_is_active,
};

impl App {
    pub(super) fn resources_page(&self) -> Element<'_, Message> {
        let busy = self.is_busy();
        let mut body = column![
            text("Resources").size(30),
            text_input("Filter providers and scenes", &self.filter)
                .on_input(Message::FilterChanged),
            text("Managed ASR models").size(22),
            row![
                text_input("Registry model id or short id", &self.model_selector)
                    .on_input(Message::ModelSelectorChanged)
                    .width(Length::Fill),
                button("Install or update").on_press_maybe(
                    (!busy && !self.model_selector.trim().is_empty())
                        .then_some(Message::InstallModel),
                ),
            ]
            .spacing(10),
            text("Managed command ASR providers").size(22),
            self.provider_install_controls(busy),
        ]
        .spacing(12);

        if let Some(notice) = self.operation_notice() {
            body = body.push(notice);
        }

        match &self.installed_models {
            Ok(models) if models.is_empty() => {
                body = body.push(text("No managed ASR models installed."));
            }
            Ok(models) => {
                for model in models {
                    let active = self
                        .config
                        .as_ref()
                        .is_ok_and(|document| model_is_active(&document.config, &model.model_dir));
                    let title = model
                        .display_title(&[])
                        .unwrap_or_else(|| model.stable_model_id());
                    let directory = model
                        .model_dir
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("managed-model");
                    let marker = if active { "active" } else { "inactive" };
                    body = body.push(
                        row![
                            text(format!(
                                "{title} · {directory} · {} files · {marker}",
                                model.file_count
                            ))
                            .width(Length::Fill),
                            button("Remove").on_press_maybe((!busy && !active).then_some(
                                Message::RemoveInstalledModel(model.model_dir.clone(),)
                            ),),
                        ]
                        .spacing(10),
                    );
                }
            }
            Err(error) => {
                body = body.push(text(format!("Installed model scan failed: {error}")));
            }
        }

        match &self.config {
            Ok(document) => {
                body = body.push(text("ASR providers").size(22));
                for provider in filtered_asr_rows(&document.config, &self.filter) {
                    body = body.push(text(provider));
                }
                body = body.push(text("Scenes").size(22));
                for scene in filtered_scene_rows(&document.config, &self.filter) {
                    body = body.push(text(scene));
                }
            }
            Err(error) => body = body.push(text(format!("Config error: {error}"))),
        }

        scrollable(body).into()
    }

    pub(super) fn llm_page(&self) -> Element<'_, Message> {
        let busy = self.is_busy();
        let mut body = column![
            text("LLM").size(30),
            text("Managed text adapters").size(22),
            self.adapter_install_controls(busy),
        ]
        .spacing(12);
        if let Some(notice) = self.operation_notice() {
            body = body.push(notice);
        }
        match &self.config {
            Ok(document) => {
                body = body.push(text("Providers").size(22));
                for provider in &document.config.llm.providers {
                    let endpoint = if provider.base_url.is_empty() {
                        "adapter/local".to_owned()
                    } else {
                        redact_url_for_diagnostics(&provider.base_url)
                    };
                    body = body.push(text(format!(
                        "{} · {} · {}",
                        provider.id,
                        provider.model.as_deref().unwrap_or("default model"),
                        endpoint
                    )));
                }
                if document.config.llm.providers.is_empty() {
                    body = body.push(text("No LLM providers configured."));
                }

                body = body.push(text("Adapters").size(22));
                for adapter in llm_adapter_rows(&document.config) {
                    body = body.push(text(adapter));
                }
                if document.config.llm.adapters.is_empty() {
                    body = body.push(text("No text adapters configured."));
                }
            }
            Err(error) => body = body.push(text(format!("Config error: {error}"))),
        }
        scrollable(body).into()
    }

    pub(super) fn hotwords_page(&self) -> Element<'_, Message> {
        let mut body = column![text("Hotwords").size(30)].spacing(12);
        match &self.config {
            Ok(document) => {
                let mut count = 0;
                for provider in &document.config.asr.providers {
                    if let Some(path) = provider.hotwords_file.as_deref() {
                        count += 1;
                        body = body.push(text(format!("{} · {path}", provider.id)));
                    }
                }
                if count == 0 {
                    body = body.push(text("No hotword files configured."));
                }
            }
            Err(error) => body = body.push(text(format!("Config error: {error}"))),
        }
        scrollable(body).into()
    }
}

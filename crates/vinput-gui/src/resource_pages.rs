//! Resources and LLM page rendering.

use iced::{
    Element, Length,
    widget::{button, column, row, scrollable, text, text_input},
};
use vinput_config::AsrProviderKind;
use vinput_registry::InstalledModelInfo;

use crate::{
    App, Message, model_is_active,
    script_management::{managed_adapter_script_path, managed_provider_script_path},
};

impl App {
    pub(super) fn resources_page(&self) -> Element<'_, Message> {
        let busy = self.is_busy();
        let resource_controls_busy = busy || self.asr_provider_editor.is_some();
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
                    (!resource_controls_busy && !self.model_selector.trim().is_empty())
                        .then_some(Message::InstallModel),
                ),
            ]
            .spacing(10),
            text("Managed command ASR providers").size(22),
            self.provider_install_controls(resource_controls_busy),
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
                    body = body.push(installed_model_row(model, active, resource_controls_busy));
                }
            }
            Err(error) => {
                body = body.push(text(format!("Installed model scan failed: {error}")));
            }
        }

        match &self.config {
            Ok(document) => {
                body = body.push(text("ASR providers").size(22));
                let filter = self.filter.to_ascii_lowercase();
                for provider in &document.config.asr.providers {
                    let kind = match provider.kind {
                        AsrProviderKind::Local => "local",
                        AsrProviderKind::Remote => "remote",
                        AsrProviderKind::Command => "command",
                    };
                    let model = provider.model.as_deref().unwrap_or("unselected model");
                    let label = format!("{} · {kind} · {model}", provider.id);
                    if !label.to_ascii_lowercase().contains(&filter) {
                        continue;
                    }
                    let active = provider.id == document.config.asr.active_provider;
                    let managed = managed_provider_script_path(provider).is_some();
                    body = body.push(provider_row(
                        label,
                        &provider.id,
                        resource_controls_busy,
                        managed,
                        active,
                    ));
                }
                if let Some(editor) = self.asr_provider_editor_view(busy) {
                    body = body.push(editor);
                }
                body = body.push(self.scene_management_view(resource_controls_busy));
            }
            Err(error) => body = body.push(text(format!("Config error: {error}"))),
        }

        if let Some(detail) = self.resource_detail_view() {
            body = body.push(detail);
        }

        scrollable(body).into()
    }

    pub(super) fn llm_page(&self) -> Element<'_, Message> {
        let busy = self.is_busy();
        let adapter_controls_busy =
            busy || self.llm_provider_editor.is_some() || self.adapter_config_editor.is_some();
        let mut body = column![
            text("LLM").size(30),
            text("Managed text adapters").size(22),
            self.adapter_install_controls(adapter_controls_busy),
        ]
        .spacing(12);
        if let Some(notice) = self.operation_notice() {
            body = body.push(notice);
        }
        match &self.config {
            Ok(document) => {
                body = body.push(self.llm_provider_management_view(busy));

                body = body.push(
                    row![
                        text("Adapters").size(22).width(Length::Fill),
                        button("Add custom adapter").on_press_maybe(
                            (!adapter_controls_busy).then_some(Message::AdapterConfig(
                                crate::AdapterConfigMessage::BeginAdd,
                            )),
                        ),
                        button("Refresh runtime").on_press_maybe(
                            (!adapter_controls_busy).then_some(Message::RefreshDaemon),
                        ),
                    ]
                    .spacing(10),
                );
                for adapter in &document.config.llm.adapters {
                    let managed = managed_adapter_script_path(adapter).is_some();
                    body = body.push(adapter_row(
                        &adapter.id,
                        &self.adapter_runtime_view_state(&adapter.id),
                        adapter_controls_busy,
                        managed,
                    ));
                }
                if let Some(editor) = self.adapter_config_editor_view(busy) {
                    body = body.push(editor);
                }
                if document.config.llm.adapters.is_empty() {
                    body = body.push(text("No text adapters configured."));
                }
            }
            Err(error) => body = body.push(text(format!("Config error: {error}"))),
        }
        if let Some(detail) = self.resource_detail_view() {
            body = body.push(detail);
        }
        scrollable(body).into()
    }
}

fn installed_model_row(
    model: &InstalledModelInfo,
    active: bool,
    busy: bool,
) -> Element<'static, Message> {
    let title = model
        .display_title(&[])
        .unwrap_or_else(|| model.stable_model_id());
    let directory = model
        .model_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("managed-model");
    let marker = if active { "active" } else { "inactive" };
    row![
        text(format!(
            "{title} · {directory} · {} files · {marker}",
            model.file_count
        ))
        .width(Length::Fill),
        button("Details").on_press(Message::SelectInstalledModelDetail(model.model_dir.clone())),
        button("Remove").on_press_maybe(
            (!busy && !active).then_some(Message::RemoveInstalledModel(model.model_dir.clone())),
        ),
    ]
    .spacing(10)
    .into()
}

fn provider_row(
    label: String,
    provider_id: &str,
    busy: bool,
    managed: bool,
    active: bool,
) -> Element<'static, Message> {
    row![
        text(label).width(Length::Fill),
        button("Details").on_press(Message::SelectAsrProviderDetail(provider_id.to_owned())),
        button("Edit").on_press_maybe((!busy).then_some(Message::AsrProvider(
            crate::AsrProviderMessage::BeginEdit(provider_id.to_owned()),
        ))),
        button("Edit script").on_press_maybe(
            (!busy && managed).then_some(Message::EditProviderScript(provider_id.to_owned())),
        ),
        button("Remove").on_press_maybe(
            (!busy && managed && !active)
                .then_some(Message::RemoveProvider(provider_id.to_owned())),
        ),
    ]
    .spacing(10)
    .into()
}

fn adapter_row(
    adapter_id: &str,
    runtime: &crate::adapter_runtime::AdapterRuntimeViewState,
    busy: bool,
    managed: bool,
) -> Element<'static, Message> {
    let start_id = adapter_id.to_owned();
    let stop_id = adapter_id.to_owned();
    row![
        text(format!(
            "{adapter_id} · command adapter · {}",
            runtime.label
        ))
        .width(Length::Fill),
        button("Details").on_press(Message::SelectLlmAdapterDetail(adapter_id.to_owned())),
        button("Edit").on_press_maybe((!busy).then_some(Message::AdapterConfig(
            crate::AdapterConfigMessage::BeginEdit(adapter_id.to_owned()),
        ))),
        button("Start").on_press_maybe((!busy && runtime.can_start).then_some(
            Message::AdapterRuntime(crate::AdapterRuntimeMessage::Start(start_id),)
        ),),
        button("Stop").on_press_maybe((!busy && runtime.can_stop).then_some(
            Message::AdapterRuntime(crate::AdapterRuntimeMessage::Stop(stop_id),)
        ),),
        button("Remove").on_press_maybe((!busy).then(|| {
            if managed {
                Message::RemoveAdapter(adapter_id.to_owned())
            } else {
                Message::AdapterConfig(crate::AdapterConfigMessage::Remove(adapter_id.to_owned()))
            }
        })),
    ]
    .spacing(10)
    .into()
}

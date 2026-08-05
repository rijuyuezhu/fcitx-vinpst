//! Resources and LLM page rendering.

use iced::{
    Element, Length,
    widget::{button, column, row, scrollable, text, text_input},
};
use vinput_config::AsrProviderKind;
use vinput_registry::InstalledModelInfo;

use crate::{
    App, GuiLocale, GuiText, Message, model_is_active,
    script_management::{managed_adapter_script_path, managed_provider_script_path},
};

impl App {
    pub(super) fn resources_page(&self) -> Element<'_, Message> {
        let busy = self.is_busy();
        let resource_controls_busy = busy || self.asr_provider_editor.is_some();
        let mut body = column![
            text(self.locale.text(GuiText::Resources)).size(30),
            text_input(
                self.locale.text(GuiText::FilterProvidersAndScenes),
                &self.filter
            )
            .on_input(Message::FilterChanged),
            text(self.locale.text(GuiText::ManagedAsrModels)).size(22),
            row![
                text_input(
                    self.locale.text(GuiText::RegistryModelSelector),
                    &self.model_selector
                )
                .on_input(Message::ModelSelectorChanged)
                .width(Length::Fill),
                button(self.locale.text(GuiText::InstallOrUpdate)).on_press_maybe(
                    (!resource_controls_busy && !self.model_selector.trim().is_empty())
                        .then_some(Message::InstallModel),
                ),
            ]
            .spacing(10),
            text(self.locale.text(GuiText::ManagedCommandAsrProviders)).size(22),
            self.provider_install_controls(resource_controls_busy),
        ]
        .spacing(12);
        if let Some(notice) = self.operation_notice() {
            body = body.push(notice);
        }
        body = body.push(self.installed_models_view(resource_controls_busy));
        body = body.push(self.configured_asr_resources_view(busy, resource_controls_busy));
        if let Some(detail) = self.resource_detail_view() {
            body = body.push(detail);
        }
        scrollable(body).into()
    }

    fn installed_models_view(&self, busy: bool) -> Element<'_, Message> {
        let mut body = column![].spacing(12);
        match &self.installed_models {
            Ok(models) if models.is_empty() => {
                body = body.push(text(self.locale.text(GuiText::NoManagedModelsInstalled)));
            }
            Ok(models) => {
                for model in models {
                    let active = self
                        .config
                        .as_ref()
                        .is_ok_and(|document| model_is_active(&document.config, &model.model_dir));
                    body = body.push(installed_model_row(self.locale, model, active, busy));
                }
            }
            Err(error) => {
                body = body.push(text(self.locale.installed_model_scan_failed(error)));
            }
        }
        body.into()
    }

    fn configured_asr_resources_view(
        &self,
        busy: bool,
        resource_controls_busy: bool,
    ) -> Element<'_, Message> {
        let mut body = column![].spacing(12);
        match &self.config {
            Ok(document) => {
                body = body.push(
                    row![
                        text(self.locale.text(GuiText::AsrProviders))
                            .size(22)
                            .width(Length::Fill),
                        button(self.locale.text(GuiText::AddCustomProvider)).on_press_maybe(
                            (!resource_controls_busy).then_some(Message::AsrProvider(
                                crate::AsrProviderMessage::BeginAdd,
                            )),
                        ),
                    ]
                    .spacing(10),
                );
                let filter = self.filter.to_ascii_lowercase();
                for provider in &document.config.asr.providers {
                    let kind = self.locale.text(match provider.kind {
                        AsrProviderKind::Local => GuiText::Local,
                        AsrProviderKind::Remote => GuiText::Remote,
                        AsrProviderKind::Command => GuiText::Command,
                    });
                    let model = provider
                        .model
                        .as_deref()
                        .unwrap_or_else(|| self.locale.text(GuiText::UnselectedModel));
                    let label = format!("{} · {kind} · {model}", provider.id);
                    if !label.to_ascii_lowercase().contains(&filter) {
                        continue;
                    }
                    let active = provider.id == document.config.asr.active_provider;
                    let managed = managed_provider_script_path(provider).is_some();
                    body = body.push(provider_row(
                        self.locale,
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
            Err(error) => body = body.push(text(self.locale.config_error(error))),
        }
        body.into()
    }

    pub(super) fn llm_page(&self) -> Element<'_, Message> {
        let busy = self.is_busy();
        let adapter_controls_busy =
            busy || self.llm_provider_editor.is_some() || self.adapter_config_editor.is_some();
        let mut body = column![
            text(self.locale.text(GuiText::Llm)).size(30),
            text(self.locale.text(GuiText::ManagedTextAdapters)).size(22),
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
                        text(self.locale.text(GuiText::Adapters))
                            .size(22)
                            .width(Length::Fill),
                        button(self.locale.text(GuiText::AddCustomAdapter)).on_press_maybe(
                            (!adapter_controls_busy).then_some(Message::AdapterConfig(
                                crate::AdapterConfigMessage::BeginAdd,
                            )),
                        ),
                        button(self.locale.text(GuiText::RefreshRuntime)).on_press_maybe(
                            (!adapter_controls_busy).then_some(Message::RefreshDaemon),
                        ),
                    ]
                    .spacing(10),
                );
                for adapter in &document.config.llm.adapters {
                    let managed = managed_adapter_script_path(adapter).is_some();
                    body = body.push(adapter_row(
                        self.locale,
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
                    body = body.push(text(self.locale.text(GuiText::NoTextAdaptersConfigured)));
                }
            }
            Err(error) => body = body.push(text(self.locale.config_error(error))),
        }
        if let Some(detail) = self.resource_detail_view() {
            body = body.push(detail);
        }
        scrollable(body).into()
    }
}

fn installed_model_row(
    locale: GuiLocale,
    model: &InstalledModelInfo,
    active: bool,
    busy: bool,
) -> Element<'static, Message> {
    let locale_code = locale.code().to_owned();
    let title = model
        .display_title(&[locale_code])
        .unwrap_or_else(|| model.stable_model_id());
    let directory = model
        .model_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("managed-model");
    row![
        text(locale.installed_model_row(title, directory, model.file_count, active))
            .width(Length::Fill),
        button(locale.text(GuiText::Details))
            .on_press(Message::SelectInstalledModelDetail(model.model_dir.clone())),
        button(locale.text(GuiText::Remove)).on_press_maybe(
            (!busy && !active).then_some(Message::RemoveInstalledModel(model.model_dir.clone())),
        ),
    ]
    .spacing(10)
    .into()
}

fn provider_row(
    locale: GuiLocale,
    label: String,
    provider_id: &str,
    busy: bool,
    managed: bool,
    active: bool,
) -> Element<'static, Message> {
    row![
        text(label).width(Length::Fill),
        button(locale.text(GuiText::Details))
            .on_press(Message::SelectAsrProviderDetail(provider_id.to_owned())),
        button(locale.text(GuiText::Edit)).on_press_maybe((!busy).then_some(Message::AsrProvider(
            crate::AsrProviderMessage::BeginEdit(provider_id.to_owned()),
        ))),
        button(locale.text(GuiText::EditScript)).on_press_maybe(
            (!busy && managed).then_some(Message::EditProviderScript(provider_id.to_owned())),
        ),
        button(locale.text(GuiText::Remove)).on_press_maybe((!busy && !active).then(|| {
            if managed {
                Message::RemoveProvider(provider_id.to_owned())
            } else {
                Message::AsrProvider(crate::AsrProviderMessage::Remove(provider_id.to_owned()))
            }
        })),
    ]
    .spacing(10)
    .into()
}

fn adapter_row(
    locale: GuiLocale,
    adapter_id: &str,
    runtime: &crate::adapter_runtime::AdapterRuntimeViewState,
    busy: bool,
    managed: bool,
) -> Element<'static, Message> {
    let start_id = adapter_id.to_owned();
    let stop_id = adapter_id.to_owned();
    row![
        text(locale.adapter_row(adapter_id, &runtime.label)).width(Length::Fill),
        button(locale.text(GuiText::Details))
            .on_press(Message::SelectLlmAdapterDetail(adapter_id.to_owned())),
        button(locale.text(GuiText::Edit)).on_press_maybe((!busy).then_some(
            Message::AdapterConfig(crate::AdapterConfigMessage::BeginEdit(
                adapter_id.to_owned()
            ),)
        )),
        button(locale.text(GuiText::Start)).on_press_maybe((!busy && runtime.can_start).then_some(
            Message::AdapterRuntime(crate::AdapterRuntimeMessage::Start(start_id),)
        ),),
        button(locale.text(GuiText::Stop)).on_press_maybe((!busy && runtime.can_stop).then_some(
            Message::AdapterRuntime(crate::AdapterRuntimeMessage::Stop(stop_id),)
        ),),
        button(locale.text(GuiText::Remove)).on_press_maybe((!busy).then(|| {
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

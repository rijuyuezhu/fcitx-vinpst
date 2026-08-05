//! Secret-safe typed summaries for selectable GUI resources.

use std::path::PathBuf;

use iced::{
    Element, Length,
    widget::{button, column, row, text},
};
use vinput_config::{
    AsrProviderConfig, AsrProviderKind, LlmAdapterConfig, LlmProviderConfig, VinputConfig,
    redact_url_for_diagnostics,
};
use vinput_registry::InstalledModelInfo;

use crate::{
    App, GuiLocale, GuiText, Message, model_is_active,
    script_management::{managed_adapter_script_path, managed_provider_script_path},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResourceSelection {
    InstalledModel(PathBuf),
    AsrProvider(String),
    LlmProvider(String),
    LlmAdapter(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResourceDetail {
    title: String,
    fields: Vec<ResourceDetailField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResourceDetailField {
    label: &'static str,
    value: String,
}

impl ResourceDetail {
    fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            fields: Vec::new(),
        }
    }

    fn field(mut self, label: &'static str, value: impl Into<String>) -> Self {
        self.fields.push(ResourceDetailField {
            label,
            value: value.into(),
        });
        self
    }

    fn view(self, locale: GuiLocale) -> Element<'static, Message> {
        let mut body = column![
            row![
                text(self.title).size(20).width(Length::Fill),
                button(locale.text(GuiText::CloseDetails)).on_press(Message::ClearResourceDetail),
            ]
            .spacing(10),
        ]
        .spacing(7);
        for field in self.fields {
            body = body.push(
                row![
                    text(field.label).width(Length::Fixed(150.0)),
                    text(field.value).width(Length::Fill),
                ]
                .spacing(10),
            );
        }
        body.into()
    }
}

impl App {
    pub(crate) fn select_installed_model_detail(&mut self, path: PathBuf) {
        self.selected_resource = Some(ResourceSelection::InstalledModel(path));
    }

    pub(crate) fn select_asr_provider_detail(&mut self, id: String) {
        self.selected_resource = Some(ResourceSelection::AsrProvider(id));
    }

    pub(crate) fn select_llm_provider_detail(&mut self, id: String) {
        self.selected_resource = Some(ResourceSelection::LlmProvider(id));
    }

    pub(crate) fn select_llm_adapter_detail(&mut self, id: String) {
        self.selected_resource = Some(ResourceSelection::LlmAdapter(id));
    }

    pub(crate) fn clear_resource_detail(&mut self) {
        self.selected_resource = None;
    }

    pub(crate) fn resource_detail_view(&self) -> Option<Element<'static, Message>> {
        let selection = self.selected_resource.as_ref()?;
        Some(
            match resolve_resource_detail(
                self.locale,
                selection,
                self.config.as_ref().map(|document| &document.config),
                self.installed_models.as_ref().map(Vec::as_slice),
            ) {
                Ok(detail) => detail.view(self.locale),
                Err(error) => column![
                    row![
                        text(self.locale.text(GuiText::ResourceDetailsUnavailable))
                            .size(20)
                            .width(Length::Fill),
                        button(self.locale.text(GuiText::CloseDetails))
                            .on_press(Message::ClearResourceDetail),
                    ]
                    .spacing(10),
                    text(error),
                ]
                .spacing(7)
                .into(),
            },
        )
    }
}

fn resolve_resource_detail(
    locale: GuiLocale,
    selection: &ResourceSelection,
    config: Result<&VinputConfig, &String>,
    installed_models: Result<&[InstalledModelInfo], &String>,
) -> Result<ResourceDetail, String> {
    match selection {
        ResourceSelection::InstalledModel(path) => {
            let models = installed_models
                .map_err(|error| format!("Installed model scan is unavailable: {error}"))?;
            let model = models
                .iter()
                .find(|model| model.model_dir == *path)
                .ok_or_else(|| {
                    format!(
                        "The selected installed model at {} is no longer available.",
                        path.display()
                    )
                })?;
            let config = config.map_err(|error| format!("Config is unavailable: {error}"))?;
            Ok(installed_model_detail(
                locale,
                model,
                model_is_active(config, &model.model_dir),
            ))
        }
        ResourceSelection::AsrProvider(id) => {
            let config = config.map_err(|error| format!("Config is unavailable: {error}"))?;
            let provider = config
                .asr
                .providers
                .iter()
                .find(|provider| provider.id == *id)
                .ok_or_else(|| format!("ASR provider `{id}` is no longer configured."))?;
            Ok(asr_provider_detail(
                locale,
                provider,
                provider.id == config.asr.active_provider,
            ))
        }
        ResourceSelection::LlmProvider(id) => {
            let config = config.map_err(|error| format!("Config is unavailable: {error}"))?;
            let provider = config
                .llm
                .providers
                .iter()
                .find(|provider| provider.id == *id)
                .ok_or_else(|| format!("LLM provider `{id}` is no longer configured."))?;
            Ok(llm_provider_detail(locale, provider))
        }
        ResourceSelection::LlmAdapter(id) => {
            let config = config.map_err(|error| format!("Config is unavailable: {error}"))?;
            let adapter = config
                .llm
                .adapters
                .iter()
                .find(|adapter| adapter.id == *id)
                .ok_or_else(|| format!("Text adapter `{id}` is no longer configured."))?;
            Ok(llm_adapter_detail(locale, adapter))
        }
    }
}

fn installed_model_detail(
    locale: GuiLocale,
    model: &InstalledModelInfo,
    active: bool,
) -> ResourceDetail {
    let metadata = &model.metadata;
    let locale_code = locale.code().to_owned();
    let title = model
        .display_title(&[locale_code])
        .unwrap_or_else(|| model.stable_model_id());
    ResourceDetail::new(locale.model_detail_title(title))
        .field(locale.text(GuiText::StableId), model.stable_model_id())
        .field(
            locale.text(GuiText::Status),
            locale.text(if active {
                GuiText::Active
            } else {
                GuiText::Inactive
            }),
        )
        .field(
            locale.text(GuiText::Backend),
            optional_text(locale, metadata.backend.as_deref()),
        )
        .field(
            locale.text(GuiText::Runtime),
            optional_text(locale, metadata.runtime.as_deref()),
        )
        .field(
            locale.text(GuiText::Family),
            optional_text(locale, metadata.model_family()),
        )
        .field(
            locale.text(GuiText::Language),
            optional_text(locale, metadata.language.as_deref()),
        )
        .field(
            locale.text(GuiText::DeclaredSize),
            optional_size(locale, metadata.size_bytes),
        )
        .field(
            locale.text(GuiText::RegularFiles),
            model.file_count.to_string(),
        )
        .field(
            locale.text(GuiText::Hotwords),
            if metadata.supports_hotwords {
                locale.text(GuiText::Supported)
            } else {
                locale.text(GuiText::NotDeclared)
            },
        )
        .field(
            locale.text(GuiText::InstallDirectory),
            model.model_dir.display().to_string(),
        )
        .field(
            locale.text(GuiText::MetadataFile),
            model.metadata_path.display().to_string(),
        )
}

fn asr_provider_detail(
    locale: GuiLocale,
    provider: &AsrProviderConfig,
    active: bool,
) -> ResourceDetail {
    let kind = locale.text(match provider.kind {
        AsrProviderKind::Local => GuiText::Local,
        AsrProviderKind::Remote => GuiText::Remote,
        AsrProviderKind::Command => GuiText::Command,
    });
    let endpoint = provider.endpoint.as_deref().map_or_else(
        || locale.text(GuiText::NotConfigured).to_owned(),
        redact_url_for_diagnostics,
    );
    ResourceDetail::new(locale.asr_provider_detail_title(&provider.id))
        .field(locale.text(GuiText::Kind), kind)
        .field(
            locale.text(GuiText::Status),
            locale.text(if active {
                GuiText::Active
            } else {
                GuiText::Inactive
            }),
        )
        .field(
            locale.text(GuiText::Model),
            optional_text(locale, provider.model.as_deref()),
        )
        .field(
            locale.text(GuiText::Timeout),
            optional_timeout(locale, provider.timeout_ms),
        )
        .field(locale.text(GuiText::Endpoint), endpoint)
        .field(
            locale.text(GuiText::HotwordFile),
            optional_text(locale, provider.hotwords_file.as_deref()),
        )
        .field(
            locale.text(GuiText::ManagedScript),
            yes_no(locale, managed_provider_script_path(provider).is_some()),
        )
        .field(
            locale.text(GuiText::Arguments),
            provider.args.len().to_string(),
        )
        .field(
            locale.text(GuiText::Environment),
            locale.configured_count(provider.env.len()),
        )
}

fn llm_provider_detail(locale: GuiLocale, provider: &LlmProviderConfig) -> ResourceDetail {
    let endpoint = if provider.base_url.is_empty() {
        locale.text(GuiText::AdapterLocal).to_owned()
    } else {
        redact_url_for_diagnostics(&provider.base_url)
    };
    ResourceDetail::new(locale.llm_provider_detail_title(&provider.id))
        .field(
            locale.text(GuiText::Model),
            optional_text(locale, provider.model.as_deref()),
        )
        .field(locale.text(GuiText::Endpoint), endpoint)
        .field(
            locale.text(GuiText::Credential),
            configured(locale, !provider.api_key.is_empty()),
        )
        .field(
            locale.text(GuiText::ExtraBodyFields),
            provider
                .extra_body
                .as_object()
                .map_or(0, serde_json::Map::len)
                .to_string(),
        )
        .field(
            locale.text(GuiText::ExtensionFields),
            provider.extra.len().to_string(),
        )
}

fn llm_adapter_detail(locale: GuiLocale, adapter: &LlmAdapterConfig) -> ResourceDetail {
    ResourceDetail::new(locale.text_adapter_detail_title(&adapter.id))
        .field(
            locale.text(GuiText::ManagedScript),
            yes_no(locale, managed_adapter_script_path(adapter).is_some()),
        )
        .field(
            locale.text(GuiText::Arguments),
            adapter.args.len().to_string(),
        )
        .field(
            locale.text(GuiText::Environment),
            locale.configured_count(adapter.env.len()),
        )
        .field(
            locale.text(GuiText::WorkingDirectory),
            configured(locale, adapter.working_dir.is_some()),
        )
        .field(
            locale.text(GuiText::ExtensionFields),
            adapter.extra.len().to_string(),
        )
}

fn optional_text(locale: GuiLocale, value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| locale.text(GuiText::NotConfigured))
        .to_owned()
}

fn optional_timeout(locale: GuiLocale, value: Option<u64>) -> String {
    value.map_or_else(
        || locale.text(GuiText::NotConfigured).to_owned(),
        |milliseconds| format!("{milliseconds} ms"),
    )
}

fn optional_size(locale: GuiLocale, value: Option<u64>) -> String {
    value.map_or_else(
        || locale.text(GuiText::NotDeclared).to_owned(),
        format_binary_size,
    )
}

fn format_binary_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;
    if bytes >= GIB {
        format_unit(bytes, GIB, "GiB")
    } else if bytes >= MIB {
        format_unit(bytes, MIB, "MiB")
    } else if bytes >= KIB {
        format_unit(bytes, KIB, "KiB")
    } else {
        format!("{bytes} B")
    }
}

fn format_unit(bytes: u64, unit: u64, suffix: &str) -> String {
    let whole = bytes / unit;
    let decimal = (bytes % unit) * 10 / unit;
    format!("{whole}.{decimal} {suffix}")
}

fn configured(locale: GuiLocale, value: bool) -> &'static str {
    locale.text(if value {
        GuiText::Configured
    } else {
        GuiText::NotConfigured
    })
}

fn yes_no(locale: GuiLocale, value: bool) -> &'static str {
    locale.text(if value { GuiText::Yes } else { GuiText::No })
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use super::*;
    use serde_json::json;
    use vinput_registry::{InstalledModelDisplayMetadata, LiveVinputModelMetadata};

    #[test]
    fn model_detail_uses_typed_metadata_without_raw_backend_json() {
        let directory = tempfile::tempdir().expect("temp dir");
        let model = InstalledModelInfo {
            model_id: "fixture".to_owned(),
            model_dir: directory.path().join("fixture"),
            metadata_path: directory.path().join("fixture/vinput-model.json"),
            metadata: LiveVinputModelMetadata {
                backend: Some("sherpa-offline".to_owned()),
                language: Some("zh".to_owned()),
                size_bytes: Some(2 * 1024 * 1024),
                supports_hotwords: true,
                runtime: Some("offline".to_owned()),
                family: Some("sense_voice".to_owned()),
                model_type: None,
                recognizer: Some(json!({"token": "recognizer-secret"})),
                model: Some(json!({"password": "model-secret"})),
                display: Some(InstalledModelDisplayMetadata {
                    registry_id: Some("model.fixture".to_owned()),
                    fallback_title: Some("Fixture model".to_owned()),
                    localized_titles: BTreeMap::new(),
                }),
                extra: BTreeMap::from([("private".to_owned(), json!("metadata-secret"))]),
            },
            files: vec!["model.onnx".to_owned()],
            file_count: 1,
        };

        let detail = installed_model_detail(GuiLocale::EnUs, &model, true);
        let debug = format!("{detail:?}");

        assert!(debug.contains("sherpa-offline"));
        assert!(debug.contains("sense_voice"));
        assert!(debug.contains("2.0 MiB"));
        assert!(!debug.contains("recognizer-secret"));
        assert!(!debug.contains("model-secret"));
        assert!(!debug.contains("metadata-secret"));
    }

    #[test]
    fn provider_details_redact_credentials_and_process_contents() {
        let provider = AsrProviderConfig {
            id: "remote".to_owned(),
            kind: AsrProviderKind::Remote,
            timeout_ms: Some(4_000),
            model: Some("whisper".to_owned()),
            hotwords_file: None,
            command: Some("helper --secret command-secret".to_owned()),
            args: vec!["argument-secret".to_owned()],
            env: HashMap::from([("TOKEN".to_owned(), "environment-secret".to_owned())]),
            endpoint: Some(
                "https://user:password@example.test/v1?token=query-secret#fragment-secret"
                    .to_owned(),
            ),
        };

        let debug = format!(
            "{:?}",
            asr_provider_detail(GuiLocale::EnUs, &provider, false)
        );

        assert!(debug.contains("example.test"));
        assert!(debug.contains("REDACTED"));
        for secret in [
            "password",
            "query-secret",
            "fragment-secret",
            "command-secret",
            "argument-secret",
            "environment-secret",
        ] {
            assert!(!debug.contains(secret));
        }
    }

    #[test]
    fn llm_details_report_configuration_without_secret_values() {
        let provider = LlmProviderConfig {
            id: "cloud".to_owned(),
            base_url: "https://user:pass@example.test/v1?key=provider-secret".to_owned(),
            api_key: "api-secret".to_owned(),
            model: Some("fixture-model".to_owned()),
            extra_body: json!({"private": "body-secret"}),
            extra: HashMap::from([("header".to_owned(), json!("extra-secret"))]),
        };
        let adapter = LlmAdapterConfig {
            id: "adapter".to_owned(),
            command: "helper adapter-secret".to_owned(),
            args: vec!["argument-secret".to_owned()],
            env: HashMap::from([("TOKEN".to_owned(), "environment-secret".to_owned())]),
            working_dir: Some("/private/working-secret".to_owned()),
            extra: HashMap::from([("private".to_owned(), json!("extension-secret"))]),
        };

        let provider_detail = llm_provider_detail(GuiLocale::EnUs, &provider);
        let adapter_detail = llm_adapter_detail(GuiLocale::EnUs, &adapter);
        let debug = format!("{provider_detail:?}\n{adapter_detail:?}");

        assert!(debug.contains("example.test"));
        assert_eq!(provider_detail.fields.len(), 5);
        assert_eq!(adapter_detail.fields.len(), 5);
        for secret in [
            "pass",
            "provider-secret",
            "api-secret",
            "body-secret",
            "extra-secret",
            "adapter-secret",
            "argument-secret",
            "environment-secret",
            "working-secret",
            "extension-secret",
        ] {
            assert!(!debug.contains(secret));
        }
    }

    #[test]
    fn resource_detail_locale_preserves_identity_and_structural_values() {
        let config = VinputConfig::bundled_default().expect("bundled config");
        let provider = config.asr.providers.first().expect("bundled provider");
        let english = asr_provider_detail(GuiLocale::EnUs, provider, true);
        let chinese = asr_provider_detail(GuiLocale::ZhCn, provider, true);

        assert!(english.title.contains(&provider.id));
        assert!(chinese.title.contains(&provider.id));
        assert_eq!(english.fields.len(), chinese.fields.len());
        assert_eq!(english.fields[3].value, chinese.fields[3].value);
        assert_eq!(english.fields[7].value, chinese.fields[7].value);
        assert!(
            english
                .fields
                .iter()
                .zip(&chinese.fields)
                .any(|(left, right)| left.label != right.label)
        );
        assert!(
            english
                .fields
                .iter()
                .zip(&chinese.fields)
                .any(|(left, right)| left.value != right.value)
        );
    }

    #[test]
    fn stale_selection_returns_unavailable_detail() {
        let config = VinputConfig::bundled_default().expect("bundled config");
        let selection = ResourceSelection::AsrProvider("missing-provider".to_owned());

        let error = resolve_resource_detail(GuiLocale::EnUs, &selection, Ok(&config), Ok(&[]))
            .expect_err("missing selection should fail");

        assert!(error.contains("no longer configured"));
    }
}

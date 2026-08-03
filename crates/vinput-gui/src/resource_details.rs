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
    App, Message, model_is_active,
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

    fn view(self) -> Element<'static, Message> {
        let mut body = column![
            row![
                text(self.title).size(20).width(Length::Fill),
                button("Close details").on_press(Message::ClearResourceDetail),
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
                selection,
                self.config.as_ref().map(|document| &document.config),
                self.installed_models.as_ref().map(Vec::as_slice),
            ) {
                Ok(detail) => detail.view(),
                Err(error) => column![
                    row![
                        text("Resource details unavailable")
                            .size(20)
                            .width(Length::Fill),
                        button("Close details").on_press(Message::ClearResourceDetail),
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
            Ok(llm_provider_detail(provider))
        }
        ResourceSelection::LlmAdapter(id) => {
            let config = config.map_err(|error| format!("Config is unavailable: {error}"))?;
            let adapter = config
                .llm
                .adapters
                .iter()
                .find(|adapter| adapter.id == *id)
                .ok_or_else(|| format!("Text adapter `{id}` is no longer configured."))?;
            Ok(llm_adapter_detail(adapter))
        }
    }
}

fn installed_model_detail(model: &InstalledModelInfo, active: bool) -> ResourceDetail {
    let metadata = &model.metadata;
    let title = model
        .display_title(&[])
        .unwrap_or_else(|| model.stable_model_id());
    ResourceDetail::new(format!("Model · {title}"))
        .field("Stable id", model.stable_model_id())
        .field("Status", if active { "active" } else { "inactive" })
        .field("Backend", optional_text(metadata.backend.as_deref()))
        .field("Runtime", optional_text(metadata.runtime.as_deref()))
        .field("Family", optional_text(metadata.model_family()))
        .field("Language", optional_text(metadata.language.as_deref()))
        .field("Declared size", optional_size(metadata.size_bytes))
        .field("Regular files", model.file_count.to_string())
        .field(
            "Hotwords",
            if metadata.supports_hotwords {
                "supported"
            } else {
                "not declared"
            },
        )
        .field("Install directory", model.model_dir.display().to_string())
        .field("Metadata file", model.metadata_path.display().to_string())
}

fn asr_provider_detail(provider: &AsrProviderConfig, active: bool) -> ResourceDetail {
    let kind = match provider.kind {
        AsrProviderKind::Local => "local",
        AsrProviderKind::Remote => "remote",
        AsrProviderKind::Command => "command",
    };
    let endpoint = provider
        .endpoint
        .as_deref()
        .map_or_else(|| "not configured".to_owned(), redact_url_for_diagnostics);
    ResourceDetail::new(format!("ASR provider · {}", provider.id))
        .field("Kind", kind)
        .field("Status", if active { "active" } else { "inactive" })
        .field("Model", optional_text(provider.model.as_deref()))
        .field("Timeout", optional_timeout(provider.timeout_ms))
        .field("Endpoint", endpoint)
        .field(
            "Hotwords file",
            optional_text(provider.hotwords_file.as_deref()),
        )
        .field(
            "Managed script",
            yes_no(managed_provider_script_path(provider).is_some()),
        )
        .field("Arguments", provider.args.len().to_string())
        .field("Environment", configured_count(provider.env.len()))
}

fn llm_provider_detail(provider: &LlmProviderConfig) -> ResourceDetail {
    let endpoint = if provider.base_url.is_empty() {
        "adapter/local".to_owned()
    } else {
        redact_url_for_diagnostics(&provider.base_url)
    };
    ResourceDetail::new(format!("LLM provider · {}", provider.id))
        .field("Model", optional_text(provider.model.as_deref()))
        .field("Endpoint", endpoint)
        .field("Credential", configured(!provider.api_key.is_empty()))
        .field(
            "Extra body fields",
            provider
                .extra_body
                .as_object()
                .map_or(0, serde_json::Map::len)
                .to_string(),
        )
        .field("Extension fields", provider.extra.len().to_string())
}

fn llm_adapter_detail(adapter: &LlmAdapterConfig) -> ResourceDetail {
    ResourceDetail::new(format!("Text adapter · {}", adapter.id))
        .field(
            "Managed script",
            yes_no(managed_adapter_script_path(adapter).is_some()),
        )
        .field("Arguments", adapter.args.len().to_string())
        .field("Environment", configured_count(adapter.env.len()))
        .field(
            "Working directory",
            configured(adapter.working_dir.is_some()),
        )
        .field("Extension fields", adapter.extra.len().to_string())
}

fn optional_text(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("not configured")
        .to_owned()
}

fn optional_timeout(value: Option<u64>) -> String {
    value.map_or_else(
        || "not configured".to_owned(),
        |milliseconds| format!("{milliseconds} ms"),
    )
}

fn optional_size(value: Option<u64>) -> String {
    value.map_or_else(|| "not declared".to_owned(), format_binary_size)
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

fn configured(value: bool) -> &'static str {
    if value {
        "configured"
    } else {
        "not configured"
    }
}

fn configured_count(count: usize) -> String {
    format!("{count} configured")
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
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

        let detail = installed_model_detail(&model, true);
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

        let debug = format!("{:?}", asr_provider_detail(&provider, false));

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

        let debug = format!(
            "{:?}\n{:?}",
            llm_provider_detail(&provider),
            llm_adapter_detail(&adapter)
        );

        assert!(debug.contains("example.test"));
        assert!(debug.contains("configured"));
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
    fn stale_selection_returns_unavailable_detail() {
        let config = VinputConfig::bundled_default().expect("bundled config");
        let selection = ResourceSelection::AsrProvider("missing-provider".to_owned());

        let error = resolve_resource_detail(&selection, Ok(&config), Ok(&[]))
            .expect_err("missing selection should fail");

        assert!(error.contains("no longer configured"));
    }
}

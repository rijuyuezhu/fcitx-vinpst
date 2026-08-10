use vinpst_config::RegistryConfig;
use vinpst_registry::{LiveScriptRegistry, RegistryTextSource};

pub(crate) struct LoadedLiveScriptRegistry {
    pub(crate) registry: LiveScriptRegistry,
    pub(crate) source_json: serde_json::Value,
}

pub(crate) fn live_registry_urls(registry: &RegistryConfig, path: &str) -> Vec<String> {
    registry
        .base_urls
        .iter()
        .map(|base_url| join_url(base_url, path))
        .collect()
}

fn join_url(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

pub(crate) struct FetchedText {
    pub(crate) url: String,
    pub(crate) text: String,
}

pub(crate) fn fetch_text_from_mirrors(
    source: &impl RegistryTextSource,
    urls: &[String],
) -> anyhow::Result<FetchedText> {
    if urls.is_empty() {
        anyhow::bail!("no live registry mirrors configured");
    }

    let mut failures = Vec::new();
    for url in urls {
        match source.fetch_registry_text(url) {
            Ok(text) => {
                return Ok(FetchedText {
                    url: url.clone(),
                    text,
                });
            }
            Err(message) => failures.push(serde_json::json!({
                "url": url,
                "error": message,
            })),
        }
    }

    anyhow::bail!(
        "all live registry mirrors failed: {}",
        serde_json::to_string(&failures)?
    );
}

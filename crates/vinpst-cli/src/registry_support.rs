use std::path::Path;

use vinpst_config::RegistryConfig;
use vinpst_registry::{
    LiveScriptRegistry, RegistryTextCache, RegistryTextSource, fetch_registry_text_with_cache,
};

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
    pub(crate) resolved_source: String,
    pub(crate) text: String,
    pub(crate) used_cache: bool,
    pub(crate) fallback_error: Option<String>,
}

pub(crate) fn fetch_text_from_mirrors(
    source: &impl RegistryTextSource,
    urls: &[String],
    cache_path: &Path,
) -> anyhow::Result<FetchedText> {
    if urls.is_empty() {
        anyhow::bail!("no live registry mirrors configured");
    }
    let cache = RegistryTextCache::new(cache_path);
    let fetched = fetch_registry_text_with_cache(source, urls, &cache)?;
    let resolved_source = fetched
        .fresh_url
        .unwrap_or_else(|| cache_path.display().to_string());
    Ok(FetchedText {
        resolved_source,
        text: fetched.text,
        used_cache: fetched.used_cache,
        fallback_error: fetched.fallback_error,
    })
}

pub(crate) fn fetched_text_source_json(
    fetched: &FetchedText,
    cache_path: &Path,
    registry_urls: &[String],
) -> serde_json::Value {
    if fetched.used_cache {
        serde_json::json!({
            "kind": "cache",
            "path": cache_path,
            "used_cache": true,
            "fallback_error": fetched.fallback_error,
            "mirror_count": registry_urls.len(),
            "registry_urls": registry_urls,
        })
    } else {
        serde_json::json!({
            "kind": "http",
            "url": fetched.resolved_source,
            "used_cache": false,
            "fallback_error": null,
            "mirror_count": registry_urls.len(),
            "registry_urls": registry_urls,
        })
    }
}

pub(crate) fn fetched_text_source_label(fetched: &FetchedText) -> String {
    if fetched.used_cache {
        format!("cache:{}", fetched.resolved_source)
    } else {
        format!("url:{}", fetched.resolved_source)
    }
}

pub(crate) fn print_cache_fallback_warning(source: &serde_json::Value, name: &str) {
    if source["used_cache"] == true {
        eprintln!("Warning: using cached {name} because the live registry is unavailable.");
    }
}

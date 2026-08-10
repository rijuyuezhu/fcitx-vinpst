//! Layered live-registry localization loading.

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context;
use serde_json::{Value, json};
use vinpst_config::user_paths;
use vinpst_registry::{
    LiveRegistryI18n, RegistryTextCache, RegistryTextSource, ReqwestRegistryTextSource,
    fetch_registry_text_with_cache, normalize_registry_locale, registry_i18n_cache_path,
    registry_url_for_diagnostics, resolve_registry_url,
};

use crate::paths::default_cache_root;

const FALLBACK_LOCALE: &str = "en_US";

/// Live registry localization plus source diagnostics.
pub(crate) struct LoadedLiveI18n {
    pub(crate) i18n: Option<LiveRegistryI18n>,
    pub(crate) source_json: Value,
    pub(crate) source_label: String,
}

struct LoadedI18nLayer {
    i18n: Option<LiveRegistryI18n>,
    diagnostic: Value,
    label: String,
}

/// Loads registry localization using legacy-compatible layer priority.
pub(crate) fn load_live_i18n(
    i18n_path: Option<&Path>,
    remote_base_urls: &[String],
    locale: &str,
) -> anyhow::Result<LoadedLiveI18n> {
    let source = ReqwestRegistryTextSource::with_limits(Duration::from_secs(20), 1024 * 1024);
    let local_override_path = default_local_i18n_override_path();
    let cache_root = default_cache_root()?;
    load_live_i18n_with_source_and_cache(
        &source,
        i18n_path,
        remote_base_urls,
        locale,
        local_override_path.as_deref(),
        Some(&cache_root),
    )
}

#[cfg(test)]
fn load_live_i18n_with_source(
    source: &impl RegistryTextSource,
    i18n_path: Option<&Path>,
    remote_base_urls: &[String],
    locale: &str,
    local_override_path: Option<&Path>,
) -> anyhow::Result<LoadedLiveI18n> {
    load_live_i18n_with_source_and_cache(
        source,
        i18n_path,
        remote_base_urls,
        locale,
        local_override_path,
        None,
    )
}

fn load_live_i18n_with_source_and_cache(
    source: &impl RegistryTextSource,
    i18n_path: Option<&Path>,
    remote_base_urls: &[String],
    locale: &str,
    local_override_path: Option<&Path>,
    cache_root: Option<&Path>,
) -> anyhow::Result<LoadedLiveI18n> {
    let preferred_locale =
        normalize_registry_locale(locale).unwrap_or_else(|| FALLBACK_LOCALE.to_owned());
    let (fallback, preferred) = if let Some(path) = i18n_path {
        (
            skipped_layer("fallback", "explicit i18n file supplied"),
            load_required_file_layer(path)?,
        )
    } else if !remote_base_urls.is_empty() {
        if preferred_locale == FALLBACK_LOCALE {
            (
                skipped_layer("fallback", "preferred locale is en_US"),
                fetch_remote_layer(source, remote_base_urls, &preferred_locale, cache_root),
            )
        } else {
            let preferred =
                fetch_remote_layer(source, remote_base_urls, &preferred_locale, cache_root);
            let fallback =
                fetch_remote_layer(source, remote_base_urls, FALLBACK_LOCALE, cache_root);
            (fallback, preferred)
        }
    } else {
        (
            skipped_layer("fallback", "no remote registry base URL"),
            skipped_layer("preferred", "no i18n source configured"),
        )
    };
    let local = load_optional_file_layer(local_override_path);

    let merged = LiveRegistryI18n::merge_layers(
        [
            fallback.i18n.clone(),
            preferred.i18n.clone(),
            local.i18n.clone(),
        ]
        .into_iter()
        .flatten(),
    );
    let loaded = !merged.is_empty();
    let entry_count = merged.entries.len();
    let i18n = loaded.then_some(merged);

    let mut source_json = if preferred.i18n.is_some() {
        preferred.diagnostic.clone()
    } else if fallback.i18n.is_some() {
        fallback.diagnostic.clone()
    } else if local.i18n.is_some() {
        local.diagnostic.clone()
    } else {
        preferred.diagnostic.clone()
    };
    if let Some(object) = source_json.as_object_mut() {
        object.insert("loaded".to_owned(), json!(loaded));
        object.insert("entry_count".to_owned(), json!(entry_count));
        object.insert("preferred_locale".to_owned(), json!(preferred_locale));
        object.insert(
            "priority".to_owned(),
            json!(["local", "preferred", "fallback"]),
        );
        object.insert(
            "layers".to_owned(),
            json!({
                "fallback": fallback.diagnostic,
                "preferred": preferred.diagnostic,
                "local": local.diagnostic,
            }),
        );
    }

    let source_label = [
        local.i18n.as_ref().map(|_| local.label.as_str()),
        preferred.i18n.as_ref().map(|_| preferred.label.as_str()),
        fallback.i18n.as_ref().map(|_| fallback.label.as_str()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" > ");
    let source_label = if source_label.is_empty() {
        preferred.label
    } else {
        source_label
    };

    Ok(LoadedLiveI18n {
        i18n,
        source_json,
        source_label,
    })
}

fn load_required_file_layer(path: &Path) -> anyhow::Result<LoadedI18nLayer> {
    let input = fs::read_to_string(path)
        .with_context(|| format!("read live registry i18n `{}`", path.display()))?;
    let i18n = LiveRegistryI18n::from_json_str(&input)
        .with_context(|| format!("parse live registry i18n `{}`", path.display()))?;
    Ok(LoadedI18nLayer {
        i18n: Some(i18n),
        diagnostic: json!({
            "kind": "file",
            "path": path,
            "loaded": true,
            "error": null,
        }),
        label: format!("file:{}", path.display()),
    })
}

fn load_optional_file_layer(path: Option<&Path>) -> LoadedI18nLayer {
    let Some(path) = path else {
        return skipped_layer("local", "user config directory unavailable");
    };
    let label = format!("local:{}", path.display());
    match fs::read_to_string(path) {
        Ok(input) => match LiveRegistryI18n::from_json_str(&input) {
            Ok(i18n) => LoadedI18nLayer {
                i18n: Some(i18n),
                diagnostic: json!({
                    "kind": "local",
                    "path": path,
                    "loaded": true,
                    "error": null,
                }),
                label,
            },
            Err(error) => LoadedI18nLayer {
                i18n: None,
                diagnostic: json!({
                    "kind": "local",
                    "path": path,
                    "loaded": false,
                    "error": error.to_string(),
                }),
                label: format!("{label} (parse failed)"),
            },
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => LoadedI18nLayer {
            i18n: None,
            diagnostic: json!({
                "kind": "local",
                "path": path,
                "loaded": false,
                "error": null,
            }),
            label: "none".to_owned(),
        },
        Err(error) => LoadedI18nLayer {
            i18n: None,
            diagnostic: json!({
                "kind": "local",
                "path": path,
                "loaded": false,
                "error": error.to_string(),
            }),
            label: format!("{label} (unavailable)"),
        },
    }
}

fn fetch_remote_layer(
    source: &impl RegistryTextSource,
    remote_base_urls: &[String],
    locale: &str,
    cache_root: Option<&Path>,
) -> LoadedI18nLayer {
    let urls = remote_base_urls
        .iter()
        .map(|base| resolve_registry_url(base, &format!("i18n/{locale}.json")))
        .collect::<Vec<_>>();

    if let Some(cache_root) = cache_root {
        return fetch_cached_remote_layer(source, &urls, locale, cache_root);
    }

    fetch_uncached_remote_layer(source, &urls, locale)
}

fn fetch_cached_remote_layer(
    source: &impl RegistryTextSource,
    urls: &[String],
    locale: &str,
    cache_root: &Path,
) -> LoadedI18nLayer {
    let cache_path = registry_i18n_cache_path(cache_root, locale);
    let cache = RegistryTextCache::new(&cache_path);
    match fetch_registry_text_with_cache(source, urls, &cache) {
        Ok(fetched) => {
            let resolved_source = fetched.fresh_url.as_deref().map_or_else(
                || cache_path.display().to_string(),
                registry_url_for_diagnostics,
            );
            let label = if fetched.used_cache {
                format!("cache:{resolved_source}")
            } else {
                format!("url:{resolved_source}")
            };
            match LiveRegistryI18n::from_json_str(&fetched.text) {
                Ok(i18n) => LoadedI18nLayer {
                    i18n: Some(i18n),
                    diagnostic: json!({
                        "kind": if fetched.used_cache { "cache" } else { "http" },
                        "source": resolved_source,
                        "locale": locale,
                        "mirror_count": urls.len(),
                        "loaded": true,
                        "used_cache": fetched.used_cache,
                        "fallback_error": fetched.fallback_error,
                        "error": null,
                    }),
                    label,
                },
                Err(error) => LoadedI18nLayer {
                    i18n: None,
                    diagnostic: json!({
                        "kind": if fetched.used_cache { "cache" } else { "http" },
                        "source": resolved_source,
                        "locale": locale,
                        "mirror_count": urls.len(),
                        "loaded": false,
                        "used_cache": fetched.used_cache,
                        "fallback_error": fetched.fallback_error,
                        "error": error.to_string(),
                    }),
                    label: format!("{label} (parse failed)"),
                },
            }
        }
        Err(error) => LoadedI18nLayer {
            i18n: None,
            diagnostic: json!({
                "kind": "cache",
                "path": cache_path,
                "locale": locale,
                "mirror_count": urls.len(),
                "loaded": false,
                "used_cache": false,
                "fallback_error": null,
                "error": error.to_string(),
            }),
            label: format!("cache:{} (unavailable)", cache_path.display()),
        },
    }
}

fn fetch_uncached_remote_layer(
    source: &impl RegistryTextSource,
    urls: &[String],
    locale: &str,
) -> LoadedI18nLayer {
    let mut last_error = None;
    for url in urls {
        let diagnostic_url = registry_url_for_diagnostics(url);
        let label = format!("url:{diagnostic_url}");
        match source.fetch_registry_text(url) {
            Ok(input) => {
                return match LiveRegistryI18n::from_json_str(&input) {
                    Ok(i18n) => LoadedI18nLayer {
                        i18n: Some(i18n),
                        diagnostic: json!({
                            "kind": "http",
                            "url": diagnostic_url,
                            "locale": locale,
                            "mirror_count": urls.len(),
                            "loaded": true,
                            "error": null,
                        }),
                        label,
                    },
                    Err(error) => LoadedI18nLayer {
                        i18n: None,
                        diagnostic: json!({
                            "kind": "http",
                            "url": diagnostic_url,
                            "locale": locale,
                            "mirror_count": urls.len(),
                            "loaded": false,
                            "error": error.to_string(),
                        }),
                        label: format!("{label} (parse failed)"),
                    },
                };
            }
            Err(error) => last_error = Some(error),
        }
    }
    let url = urls
        .last()
        .map(|url| registry_url_for_diagnostics(url))
        .unwrap_or_default();
    let label = if url.is_empty() {
        "none".to_owned()
    } else {
        format!("url:{url} (unavailable)")
    };
    LoadedI18nLayer {
        i18n: None,
        diagnostic: json!({
            "kind": "http",
            "url": url,
            "locale": locale,
            "mirror_count": urls.len(),
            "loaded": false,
            "error": last_error,
        }),
        label,
    }
}

fn skipped_layer(role: &str, reason: &str) -> LoadedI18nLayer {
    LoadedI18nLayer {
        i18n: None,
        diagnostic: json!({
            "kind": "none",
            "role": role,
            "loaded": false,
            "error": reason,
        }),
        label: "none".to_owned(),
    }
}

fn default_local_i18n_override_path() -> Option<PathBuf> {
    Some(
        user_paths::user_config_home()?
            .join("vinpst")
            .join("i18n.local.json"),
    )
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Mutex};

    use super::*;

    #[derive(Default)]
    struct FakeTextSource {
        responses: BTreeMap<String, Result<String, String>>,
        requests: Mutex<Vec<String>>,
    }

    impl RegistryTextSource for FakeTextSource {
        fn fetch_registry_text(&self, url: &str) -> Result<String, String> {
            self.requests
                .lock()
                .expect("request log lock poisoned")
                .push(url.to_owned());
            self.responses
                .get(url)
                .cloned()
                .unwrap_or_else(|| Err("missing fixture response".to_owned()))
        }
    }

    #[test]
    fn merges_fallback_preferred_and_local_layers_in_legacy_priority() {
        let base = "https://registry.example/root";
        let bases = vec![base.to_owned()];
        let fallback_url = format!("{base}/i18n/en_US.json");
        let preferred_url = format!("{base}/i18n/zh_CN.json");
        let directory = tempfile::tempdir().expect("create local i18n directory");
        let local_path = directory.path().join("i18n.local.json");
        fs::write(
            &local_path,
            r#"{"shared":"local","local-only":"local value"}"#,
        )
        .expect("write local i18n override");
        let source = FakeTextSource {
            responses: BTreeMap::from([
                (
                    fallback_url.clone(),
                    Ok(r#"{"shared":"fallback","fallback-only":"fallback value"}"#.to_owned()),
                ),
                (
                    preferred_url.clone(),
                    Ok(r#"{"shared":"preferred","preferred-only":"preferred value"}"#.to_owned()),
                ),
            ]),
            requests: Mutex::new(Vec::new()),
        };

        let loaded = load_live_i18n_with_source(&source, None, &bases, "zh_CN", Some(&local_path))
            .expect("load layered i18n");
        let i18n = loaded.i18n.expect("merged i18n should be available");

        assert_eq!(i18n.get("shared"), Some("local"));
        assert_eq!(i18n.get("fallback-only"), Some("fallback value"));
        assert_eq!(i18n.get("preferred-only"), Some("preferred value"));
        assert_eq!(i18n.get("local-only"), Some("local value"));
        assert_eq!(
            *source.requests.lock().expect("request log lock poisoned"),
            vec![preferred_url, fallback_url]
        );
        assert_eq!(loaded.source_json["priority"][0], "local");
        assert_eq!(loaded.source_json["layers"]["local"]["loaded"], true);
        assert!(loaded.source_label.starts_with("local:"));
    }

    #[test]
    fn remote_i18n_keeps_request_credentials_but_redacts_source_diagnostics() {
        let base =
            "https://registry-user:registry-pass@example.test/root?token=registry-secret#private";
        let bases = vec![base.to_owned()];
        let preferred_url = resolve_registry_url(base, "i18n/zh_CN.json");
        let fallback_url = resolve_registry_url(base, "i18n/en_US.json");
        let source = FakeTextSource {
            responses: BTreeMap::from([
                (
                    preferred_url.clone(),
                    Ok(r#"{"shared":"preferred"}"#.to_owned()),
                ),
                (
                    fallback_url.clone(),
                    Ok(r#"{"shared":"fallback"}"#.to_owned()),
                ),
            ]),
            requests: Mutex::new(Vec::new()),
        };

        let loaded = load_live_i18n_with_source(&source, None, &bases, "zh_CN", None)
            .expect("load credentialed remote i18n");
        let rendered = loaded.source_json.to_string();

        assert_eq!(
            *source.requests.lock().expect("request log lock poisoned"),
            vec![preferred_url, fallback_url]
        );
        assert!(rendered.contains("token=REDACTED"));
        assert!(loaded.source_label.contains("token=REDACTED"));
        for secret in [
            "registry-user",
            "registry-pass",
            "registry-secret",
            "private",
        ] {
            assert!(!rendered.contains(secret));
            assert!(!loaded.source_label.contains(secret));
        }
    }

    #[test]
    fn falls_back_to_en_us_when_the_preferred_locale_is_unavailable() {
        let base = "https://registry.example/root";
        let bases = vec![base.to_owned()];
        let fallback_url = format!("{base}/i18n/en_US.json");
        let preferred_url = format!("{base}/i18n/fr_FR.json");
        let source = FakeTextSource {
            responses: BTreeMap::from([
                (
                    fallback_url,
                    Ok(r#"{"model.test.title":"English title"}"#.to_owned()),
                ),
                (preferred_url, Err("preferred unavailable".to_owned())),
            ]),
            requests: Mutex::new(Vec::new()),
        };

        let loaded = load_live_i18n_with_source(&source, None, &bases, "fr_FR", None)
            .expect("load fallback i18n");

        assert_eq!(
            loaded
                .i18n
                .as_ref()
                .and_then(|i18n| i18n.get("model.test.title")),
            Some("English title")
        );
        assert_eq!(loaded.source_json["loaded"], true);
        assert_eq!(loaded.source_json["layers"]["preferred"]["loaded"], false);
        assert_eq!(loaded.source_json["layers"]["fallback"]["loaded"], true);
    }

    #[test]
    fn remote_i18n_tries_all_mirrors_independently_of_registry_selection() {
        let first = "https://first.example/root".to_owned();
        let second = "https://second.example/root".to_owned();
        let bases = vec![first.clone(), second.clone()];
        let preferred_first = format!("{first}/i18n/zh_CN.json");
        let preferred_second = format!("{second}/i18n/zh_CN.json");
        let fallback_first = format!("{first}/i18n/en_US.json");
        let source = FakeTextSource {
            responses: BTreeMap::from([
                (
                    preferred_first.clone(),
                    Err("preferred unavailable".to_owned()),
                ),
                (
                    preferred_second.clone(),
                    Ok(r#"{"shared":"preferred","preferred-only":"zh"}"#.to_owned()),
                ),
                (
                    fallback_first.clone(),
                    Ok(r#"{"shared":"fallback","fallback-only":"en"}"#.to_owned()),
                ),
            ]),
            requests: Mutex::new(Vec::new()),
        };

        let loaded = load_live_i18n_with_source(&source, None, &bases, "zh_CN", None)
            .expect("load i18n through independent mirrors");
        let i18n = loaded.i18n.expect("merged i18n");

        assert_eq!(i18n.get("shared"), Some("preferred"));
        assert_eq!(i18n.get("preferred-only"), Some("zh"));
        assert_eq!(i18n.get("fallback-only"), Some("en"));
        assert_eq!(
            *source.requests.lock().expect("request log lock poisoned"),
            vec![preferred_first, preferred_second, fallback_first]
        );
    }

    #[test]
    fn malformed_local_override_is_nonfatal() {
        let directory = tempfile::tempdir().expect("create local i18n directory");
        let local_path = directory.path().join("i18n.local.json");
        fs::write(&local_path, "not-json").expect("write malformed local override");
        let explicit_path = directory.path().join("preferred.json");
        fs::write(&explicit_path, r#"{"key":"preferred"}"#).expect("write preferred i18n");

        let loaded = load_live_i18n_with_source(
            &FakeTextSource::default(),
            Some(&explicit_path),
            &[],
            "zh_CN",
            Some(&local_path),
        )
        .expect("malformed local override should not fail loading");

        assert_eq!(
            loaded.i18n.as_ref().and_then(|i18n| i18n.get("key")),
            Some("preferred")
        );
        assert_eq!(loaded.source_json["kind"], "file");
        assert_eq!(loaded.source_json["layers"]["local"]["loaded"], false);
        assert!(loaded.source_json["layers"]["local"]["error"].is_string());
    }
}

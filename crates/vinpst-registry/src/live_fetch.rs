//! High-level live-registry fetch side effects shared by CLI and GUI.
//!
//! Frozen registry fetches refresh the preferred and `en_US` i18n caches after
//! a fresh registry download. This module owns that best-effort policy while
//! leaving HTTP transport and stale-cache mechanics in their lower layers.

use std::{path::Path, time::Duration};

use crate::{
    CachedRegistryText, RegistryCachedFetchError, RegistryTextCache, RegistryTextSource,
    ReqwestRegistryTextSource, detect_preferred_registry_locale, fetch_registry_text_with_cache,
    registry_i18n_cache_path, resolve_registry_url,
};

const REGISTRY_TIMEOUT: Duration = Duration::from_secs(30);
const REGISTRY_MAX_BYTES: usize = 4 * 1024 * 1024;
const I18N_TIMEOUT: Duration = Duration::from_secs(20);
const I18N_MAX_BYTES: usize = 1024 * 1024;
const FALLBACK_LOCALE: &str = "en_US";

/// Raw live-registry fetch plus nonfatal cache warnings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveRegistryTextFetch {
    /// Fresh or stale registry text and source metadata.
    pub registry: CachedRegistryText,
    /// Sanitized cache fallback warnings.
    pub warnings: Vec<String>,
}

/// Fetches one live registry with frozen cache and i18n-refresh policy.
pub fn fetch_live_registry_text(
    registry_urls: &[String],
    registry_cache_path: &Path,
    base_urls: &[String],
    cache_root: &Path,
) -> Result<LiveRegistryTextFetch, RegistryCachedFetchError> {
    let registry_source =
        ReqwestRegistryTextSource::with_limits(REGISTRY_TIMEOUT, REGISTRY_MAX_BYTES);
    let i18n_source = ReqwestRegistryTextSource::with_limits(I18N_TIMEOUT, I18N_MAX_BYTES);
    let preferred_locale = detect_preferred_registry_locale();
    fetch_live_registry_text_with_sources(
        &registry_source,
        &i18n_source,
        registry_urls,
        registry_cache_path,
        base_urls,
        cache_root,
        &preferred_locale,
    )
}

/// Injectable form of [`fetch_live_registry_text`].
pub fn fetch_live_registry_text_with_sources(
    registry_source: &impl RegistryTextSource,
    i18n_source: &impl RegistryTextSource,
    registry_urls: &[String],
    registry_cache_path: &Path,
    base_urls: &[String],
    cache_root: &Path,
    preferred_locale: &str,
) -> Result<LiveRegistryTextFetch, RegistryCachedFetchError> {
    let cache = RegistryTextCache::new(registry_cache_path);
    let registry = fetch_registry_text_with_cache(registry_source, registry_urls, &cache)?;
    let warnings = if registry.used_cache {
        vec![cache_fallback_warning(
            registry_cache_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("registry"),
            registry.fallback_error.as_deref(),
        )]
    } else {
        refresh_registry_i18n_after_fresh_fetch_with_source(
            i18n_source,
            base_urls,
            cache_root,
            preferred_locale,
        )
    };
    Ok(LiveRegistryTextFetch { registry, warnings })
}

fn refresh_registry_i18n_after_fresh_fetch_with_source(
    source: &impl RegistryTextSource,
    base_urls: &[String],
    cache_root: &Path,
    preferred_locale: &str,
) -> Vec<String> {
    let mut warnings = Vec::new();
    refresh_locale(
        source,
        base_urls,
        cache_root,
        preferred_locale,
        &mut warnings,
    );
    if preferred_locale != FALLBACK_LOCALE {
        refresh_locale(
            source,
            base_urls,
            cache_root,
            FALLBACK_LOCALE,
            &mut warnings,
        );
    }
    warnings
}

fn refresh_locale(
    source: &impl RegistryTextSource,
    base_urls: &[String],
    cache_root: &Path,
    locale: &str,
    warnings: &mut Vec<String>,
) {
    let urls = base_urls
        .iter()
        .map(|base| resolve_registry_url(base, &format!("i18n/{locale}.json")))
        .collect::<Vec<_>>();
    if urls.is_empty() {
        return;
    }

    let cache = RegistryTextCache::new(registry_i18n_cache_path(cache_root, locale));
    let Ok(fetched) = fetch_registry_text_with_cache(source, &urls, &cache) else {
        return;
    };
    if !fetched.used_cache {
        return;
    }

    warnings.push(cache_fallback_warning(
        &format!("i18n {locale}"),
        fetched.fallback_error.as_deref(),
    ));
}

fn cache_fallback_warning(name: &str, fallback_error: Option<&str>) -> String {
    let mut warning = format!("using cached {name} because download failed");
    if let Some(error) = fallback_error.filter(|error| !error.is_empty()) {
        warning.push_str(": ");
        warning.push_str(error);
    }
    warning
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Mutex};

    use super::*;

    #[derive(Default)]
    struct FixtureSource {
        responses: BTreeMap<String, Result<String, String>>,
        requests: Mutex<Vec<String>>,
    }

    impl RegistryTextSource for FixtureSource {
        fn fetch_registry_text(&self, url: &str) -> Result<String, String> {
            self.requests
                .lock()
                .expect("request log lock poisoned")
                .push(url.to_owned());
            self.responses
                .get(url)
                .cloned()
                .unwrap_or_else(|| Err("fixture unavailable".to_owned()))
        }
    }

    #[test]
    fn fresh_registry_refreshes_preferred_then_fallback_locale_caches() {
        let root = tempfile::tempdir().expect("cache root");
        let base = "https://registry.example".to_owned();
        let preferred_url = format!("{base}/i18n/zh_CN.json");
        let fallback_url = format!("{base}/i18n/en_US.json");
        let source = FixtureSource {
            responses: BTreeMap::from([
                (preferred_url.clone(), Ok("preferred".to_owned())),
                (fallback_url.clone(), Ok("fallback".to_owned())),
            ]),
            requests: Mutex::new(Vec::new()),
        };

        let warnings = refresh_registry_i18n_after_fresh_fetch_with_source(
            &source,
            std::slice::from_ref(&base),
            root.path(),
            "zh_CN",
        );

        assert!(warnings.is_empty());
        assert_eq!(
            *source.requests.lock().expect("request log lock poisoned"),
            vec![preferred_url, fallback_url]
        );
        assert_eq!(
            std::fs::read_to_string(registry_i18n_cache_path(root.path(), "zh_CN"))
                .expect("preferred cache"),
            "preferred"
        );
        assert_eq!(
            std::fs::read_to_string(registry_i18n_cache_path(root.path(), "en_US"))
                .expect("fallback cache"),
            "fallback"
        );
    }

    #[test]
    fn stale_registry_skips_i18n_refresh_and_reports_registry_warning() {
        let root = tempfile::tempdir().expect("cache root");
        let base = "https://registry.example".to_owned();
        let registry_url = format!("{base}/registry/models.json");
        let registry_cache_path = root.path().join("registry/models.json");
        std::fs::create_dir_all(registry_cache_path.parent().expect("cache parent"))
            .expect("create cache parent");
        std::fs::write(&registry_cache_path, "stale registry").expect("seed registry cache");
        let registry_source = FixtureSource {
            responses: BTreeMap::from([(registry_url.clone(), Err("offline".to_owned()))]),
            requests: Mutex::new(Vec::new()),
        };
        let i18n_source = FixtureSource {
            responses: BTreeMap::from([(
                format!("{base}/i18n/en_US.json"),
                Ok("should not be fetched".to_owned()),
            )]),
            requests: Mutex::new(Vec::new()),
        };

        let fetched = fetch_live_registry_text_with_sources(
            &registry_source,
            &i18n_source,
            std::slice::from_ref(&registry_url),
            &registry_cache_path,
            std::slice::from_ref(&base),
            root.path(),
            FALLBACK_LOCALE,
        )
        .expect("stale registry fallback");

        assert!(fetched.registry.used_cache);
        assert_eq!(fetched.registry.text, "stale registry");
        assert_eq!(
            fetched.warnings,
            ["using cached models.json because download failed: all registry mirrors failed"]
        );
        assert!(
            i18n_source
                .requests
                .lock()
                .expect("request log lock poisoned")
                .is_empty()
        );
    }

    #[test]
    fn stale_locale_cache_is_nonfatal_and_reported() {
        let root = tempfile::tempdir().expect("cache root");
        let base = "https://registry.example".to_owned();
        let preferred_url = format!("{base}/i18n/zh_CN.json");
        let fallback_url = format!("{base}/i18n/en_US.json");
        let preferred_cache = registry_i18n_cache_path(root.path(), "zh_CN");
        std::fs::create_dir_all(preferred_cache.parent().expect("cache parent"))
            .expect("create cache parent");
        std::fs::write(&preferred_cache, "stale preferred").expect("seed stale cache");
        let source = FixtureSource {
            responses: BTreeMap::from([
                (preferred_url, Err("offline".to_owned())),
                (fallback_url, Ok("fresh fallback".to_owned())),
            ]),
            requests: Mutex::new(Vec::new()),
        };

        let warnings = refresh_registry_i18n_after_fresh_fetch_with_source(
            &source,
            std::slice::from_ref(&base),
            root.path(),
            "zh_CN",
        );

        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0],
            "using cached i18n zh_CN because download failed: all registry mirrors failed"
        );
        assert_eq!(
            std::fs::read_to_string(preferred_cache).expect("stale preferred cache"),
            "stale preferred"
        );
        assert_eq!(
            std::fs::read_to_string(registry_i18n_cache_path(root.path(), "en_US"))
                .expect("fresh fallback cache"),
            "fresh fallback"
        );
    }

    #[test]
    fn unavailable_i18n_without_cache_does_not_fail_registry_refresh() {
        let root = tempfile::tempdir().expect("cache root");
        let base = "https://registry.example".to_owned();
        let source = FixtureSource::default();

        let warnings = refresh_registry_i18n_after_fresh_fetch_with_source(
            &source,
            &[base],
            root.path(),
            FALLBACK_LOCALE,
        );

        assert!(warnings.is_empty());
        assert!(!registry_i18n_cache_path(root.path(), FALLBACK_LOCALE).exists());
    }
}

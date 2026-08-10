//! Registry index text cache boundary.
//!
//! This module only caches registry index JSON text. It does not download
//! registry assets, verify checksums, extract archives, install files, or mutate
//! configuration.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::{RegistryError, RegistryFetchError, RegistryIndex, RegistryTextSource};

/// Raw registry text returned from a fresh mirror or stale cache fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedRegistryText {
    /// Registry document text.
    pub text: String,
    /// Fresh mirror URL when the network fetch succeeded.
    pub fresh_url: Option<String>,
    /// Whether the returned text came from stale cache.
    pub used_cache: bool,
    /// Sanitized fresh-fetch failure when stale cache was used.
    pub fallback_error: Option<String>,
}

/// Registry text cache errors.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum RegistryCacheError {
    /// Cache text could not be read.
    #[error("failed to read registry cache `{path}`: {message}")]
    Read {
        /// Cache path.
        path: String,
        /// Sanitized I/O failure message.
        message: String,
    },
    /// Cache text could not be written atomically.
    #[error("failed to write registry cache `{path}`: {message}")]
    Write {
        /// Cache path.
        path: String,
        /// Sanitized I/O failure message.
        message: String,
    },
    /// Cache text was readable but did not parse or validate as a registry index.
    #[error("registry cache `{path}` is invalid: {error}")]
    InvalidIndex {
        /// Cache path.
        path: String,
        /// Registry parse or validation error.
        error: RegistryError,
    },
}

/// Registry fetch with stale-cache fallback errors.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum RegistryCachedFetchError {
    /// Fresh fetch produced an invalid registry body.
    #[error(transparent)]
    Fetch(#[from] RegistryFetchError),
    /// Fresh fetch failed and stale cache could not be loaded.
    #[error(
        "registry fetch failed and stale cache could not be loaded: fetch={fetch}; cache={cache}"
    )]
    StaleCacheUnavailable {
        /// Fresh fetch error.
        fetch: RegistryFetchError,
        /// Stale cache read/parse error.
        cache: RegistryCacheError,
    },
    /// Fresh fetch and parse succeeded, but writing cache failed.
    #[error("registry fetch succeeded but cache update failed: {0}")]
    CacheWrite(RegistryCacheError),
}

/// File-backed registry index text cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryTextCache {
    path: PathBuf,
}

impl RegistryTextCache {
    /// Creates a registry text cache at one concrete file path.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Returns the cache file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads cached registry text and parses it as a registry index.
    pub fn read_index(&self) -> Result<RegistryIndex, RegistryCacheError> {
        let text = self.read_text()?;
        RegistryIndex::from_json_str(&text).map_err(|error| RegistryCacheError::InvalidIndex {
            path: display_path(&self.path),
            error,
        })
    }

    /// Reads cached registry text without interpreting its schema.
    pub fn read_text(&self) -> Result<String, RegistryCacheError> {
        fs::read_to_string(&self.path).map_err(|error| RegistryCacheError::Read {
            path: display_path(&self.path),
            message: sanitize_io_error(&error),
        })
    }

    /// Writes registry text through a temporary file and same-directory rename.
    pub fn write_text_atomic(&self, text: &str) -> Result<(), RegistryCacheError> {
        if let Some(parent) = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|error| RegistryCacheError::Write {
                path: display_path(&self.path),
                message: sanitize_io_error(&error),
            })?;
        }

        let temp_path = self.temp_path();
        fs::write(&temp_path, text).map_err(|error| {
            let _ = fs::remove_file(&temp_path);
            RegistryCacheError::Write {
                path: display_path(&self.path),
                message: sanitize_io_error(&error),
            }
        })?;
        fs::rename(&temp_path, &self.path).map_err(|error| {
            let _ = fs::remove_file(&temp_path);
            RegistryCacheError::Write {
                path: display_path(&self.path),
                message: sanitize_io_error(&error),
            }
        })
    }

    fn temp_path(&self) -> PathBuf {
        let file_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("registry-index.json");
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        self.path
            .with_file_name(format!(".{file_name}.tmp.{}.{unique}", std::process::id()))
    }
}

/// Cache path for the live model registry under one Vinpst cache root.
#[must_use]
pub fn model_registry_cache_path(cache_root: &Path) -> PathBuf {
    cache_root.join("registry").join("models.json")
}

/// Cache path for the live ASR-provider registry under one Vinpst cache root.
#[must_use]
pub fn provider_registry_cache_path(cache_root: &Path) -> PathBuf {
    cache_root.join("registry").join("providers.json")
}

/// Cache path for the live LLM-adapter registry under one Vinpst cache root.
#[must_use]
pub fn adapter_registry_cache_path(cache_root: &Path) -> PathBuf {
    cache_root.join("registry").join("adapters.json")
}

/// Cache path for one normalized registry locale under one Vinpst cache root.
#[must_use]
pub fn registry_i18n_cache_path(cache_root: &Path, locale: &str) -> PathBuf {
    let safe_locale = locale
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    cache_root
        .join("registry")
        .join("i18n")
        .join(format!("{safe_locale}.json"))
}

/// Fetches raw registry text, updates cache after fresh success, and falls back
/// to stale cached text only when all fresh mirror attempts fail.
pub fn fetch_registry_text_with_cache(
    source: &impl RegistryTextSource,
    mirrors: &[String],
    cache: &RegistryTextCache,
) -> Result<CachedRegistryText, RegistryCachedFetchError> {
    match crate::fetch::fetch_registry_text_from_mirrors(source, mirrors) {
        Ok(fetched) => {
            cache
                .write_text_atomic(&fetched.text)
                .map_err(RegistryCachedFetchError::CacheWrite)?;
            Ok(CachedRegistryText {
                text: fetched.text,
                fresh_url: Some(fetched.url),
                used_cache: false,
                fallback_error: None,
            })
        }
        Err(fetch) => match cache.read_text() {
            Ok(text) => Ok(CachedRegistryText {
                text,
                fresh_url: None,
                used_cache: true,
                fallback_error: Some(fetch.to_string()),
            }),
            Err(cache) => Err(RegistryCachedFetchError::StaleCacheUnavailable { fetch, cache }),
        },
    }
}

/// Fetches a registry index, updates cache after fresh success, and falls back
/// to stale cache only when all fresh mirror attempts fail.
pub fn fetch_registry_index_with_cache(
    source: &impl RegistryTextSource,
    mirrors: &[String],
    cache: &RegistryTextCache,
) -> Result<RegistryIndex, RegistryCachedFetchError> {
    match crate::fetch::fetch_registry_text_from_mirrors(source, mirrors) {
        Ok(fetched) => {
            let index = RegistryIndex::from_json_str(&fetched.text).map_err(|error| {
                RegistryCachedFetchError::Fetch(RegistryFetchError::InvalidIndex {
                    url: fetched.url,
                    error,
                })
            })?;
            cache
                .write_text_atomic(&fetched.text)
                .map_err(RegistryCachedFetchError::CacheWrite)?;
            Ok(index)
        }
        Err(fetch) => cache
            .read_index()
            .map_err(|cache| RegistryCachedFetchError::StaleCacheUnavailable { fetch, cache }),
    }
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

fn sanitize_io_error(error: &io::Error) -> String {
    error.kind().to_string()
}

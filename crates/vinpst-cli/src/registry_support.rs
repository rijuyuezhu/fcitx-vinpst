use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context;
use vinpst_config::RegistryConfig;
use vinpst_registry::{
    LiveScriptRegistry, fetch_live_registry_text, registry_url_for_diagnostics,
    resolve_registry_url,
};

pub(crate) struct LoadedLiveScriptRegistry {
    pub(crate) registry: LiveScriptRegistry,
    pub(crate) source_json: serde_json::Value,
}

pub(crate) fn live_registry_urls(registry: &RegistryConfig, path: &str) -> Vec<String> {
    registry
        .base_urls
        .iter()
        .map(|base_url| resolve_registry_url(base_url, path))
        .collect()
}

pub(crate) struct FetchedText {
    pub(crate) resolved_source: String,
    pub(crate) text: String,
    pub(crate) used_cache: bool,
    pub(crate) fallback_error: Option<String>,
    pub(crate) warnings: Vec<String>,
}

pub(crate) fn fetch_text_from_mirrors(
    urls: &[String],
    cache_path: &Path,
    base_urls: &[String],
    cache_root: &Path,
) -> anyhow::Result<FetchedText> {
    if urls.is_empty() {
        anyhow::bail!("no live registry mirrors configured");
    }
    let fetched = fetch_live_registry_text(urls, cache_path, base_urls, cache_root)?;
    let resolved_source = fetched.registry.fresh_url.as_deref().map_or_else(
        || cache_path.display().to_string(),
        registry_url_for_diagnostics,
    );
    Ok(FetchedText {
        resolved_source,
        text: fetched.registry.text,
        used_cache: fetched.registry.used_cache,
        fallback_error: fetched.registry.fallback_error,
        warnings: fetched.warnings,
    })
}

pub(crate) fn registry_urls_for_diagnostics(urls: &[String]) -> Vec<String> {
    urls.iter()
        .map(|url| registry_url_for_diagnostics(url))
        .collect()
}

pub(crate) fn fetched_text_source_json(
    fetched: &FetchedText,
    cache_path: &Path,
    registry_urls: &[String],
) -> serde_json::Value {
    let registry_urls = registry_urls_for_diagnostics(registry_urls);
    let resolved_source = if fetched.used_cache {
        fetched.resolved_source.clone()
    } else {
        registry_url_for_diagnostics(&fetched.resolved_source)
    };
    if fetched.used_cache {
        serde_json::json!({
            "kind": "cache",
            "path": cache_path,
            "used_cache": true,
            "fallback_error": fetched.fallback_error,
            "warnings": fetched.warnings,
            "mirror_count": registry_urls.len(),
            "registry_urls": registry_urls,
        })
    } else {
        serde_json::json!({
            "kind": "http",
            "url": resolved_source,
            "used_cache": false,
            "fallback_error": null,
            "warnings": fetched.warnings,
            "mirror_count": registry_urls.len(),
            "registry_urls": registry_urls,
        })
    }
}

pub(crate) fn fetched_text_source_label(fetched: &FetchedText) -> String {
    if fetched.used_cache {
        format!("cache:{}", fetched.resolved_source)
    } else {
        format!(
            "url:{}",
            registry_url_for_diagnostics(&fetched.resolved_source)
        )
    }
}

pub(crate) fn print_cache_fallback_warning(source: &serde_json::Value, name: &str) {
    if let Some(warnings) = source["warnings"].as_array() {
        for warning in warnings.iter().filter_map(serde_json::Value::as_str) {
            eprintln!("Warning: {warning}");
        }
    } else if source["used_cache"] == true {
        eprintln!("Warning: using cached {name} because the live registry is unavailable.");
    }
}

pub(crate) fn with_managed_script_transaction<T>(
    script_path: &Path,
    install: impl FnOnce() -> anyhow::Result<T>,
    commit_config: impl FnOnce(&T) -> anyhow::Result<()>,
) -> anyhow::Result<T> {
    let rollback = ManagedScriptRollback::prepare(script_path)?;
    let installed = match install() {
        Ok(installed) => installed,
        Err(error) => {
            return match rollback.restore() {
                Ok(()) => Err(error),
                Err(restore_error) => Err(error.context(format!(
                    "restore managed script after install failure: {restore_error}"
                ))),
            };
        }
    };
    if let Err(error) = commit_config(&installed) {
        return match rollback.restore() {
            Ok(()) => Err(error),
            Err(restore_error) => Err(error.context(format!(
                "restore managed script after config write failure: {restore_error}"
            ))),
        };
    }
    rollback.discard();
    Ok(installed)
}

struct ManagedScriptRollback {
    script_path: PathBuf,
    backup_path: Option<PathBuf>,
    existed: bool,
}

impl ManagedScriptRollback {
    fn prepare(script_path: &Path) -> anyhow::Result<Self> {
        let metadata = match fs::symlink_metadata(script_path) {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "inspect managed script `{}` before update",
                        script_path.display()
                    )
                });
            }
        };
        let Some(metadata) = metadata else {
            return Ok(Self {
                script_path: script_path.to_path_buf(),
                backup_path: None,
                existed: false,
            });
        };
        if !metadata.file_type().is_file() {
            anyhow::bail!(
                "refusing to update managed script `{}` because it is not a regular file",
                script_path.display()
            );
        }
        let backup_path = transaction_backup_path(script_path);
        fs::copy(script_path, &backup_path).with_context(|| {
            format!(
                "preserve managed script `{}` before update",
                script_path.display()
            )
        })?;
        if let Err(error) = fs::File::open(&backup_path).and_then(|file| file.sync_all()) {
            let _ = fs::remove_file(&backup_path);
            return Err(error).with_context(|| {
                format!(
                    "sync managed script backup `{}` before update",
                    backup_path.display()
                )
            });
        }
        Ok(Self {
            script_path: script_path.to_path_buf(),
            backup_path: Some(backup_path),
            existed: true,
        })
    }

    fn restore(self) -> anyhow::Result<()> {
        if self.existed {
            let backup_path = self
                .backup_path
                .as_ref()
                .context("managed script rollback backup is missing")?;
            fs::rename(backup_path, &self.script_path).with_context(|| {
                format!(
                    "restore previous managed script `{}`",
                    self.script_path.display()
                )
            })?;
        } else {
            match fs::remove_file(&self.script_path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!(
                            "remove newly published managed script `{}`",
                            self.script_path.display()
                        )
                    });
                }
            }
        }
        Ok(())
    }

    fn discard(self) {
        if let Some(backup_path) = self.backup_path {
            let _ = fs::remove_file(backup_path);
        }
    }
}

fn transaction_backup_path(script_path: &Path) -> PathBuf {
    let file_name = script_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("managed-script");
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    script_path.with_file_name(format!(
        ".{file_name}.cli-rollback.{}.{unique}",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_registry_urls_append_paths_before_query_and_fragment() {
        let config = RegistryConfig {
            base_urls: vec![
                "https://user:password@example.test/root?token=secret#fragment".to_owned(),
                "mirror".to_owned(),
            ],
        };

        assert_eq!(
            live_registry_urls(&config, "registry/models.json"),
            [
                "https://user:password@example.test/root/registry/models.json?token=secret#fragment",
                "mirror/registry/models.json",
            ]
        );
    }

    #[test]
    fn registry_source_diagnostics_redact_network_urls() {
        let fetched = FetchedText {
            resolved_source:
                "https://user:password@example.test/root/registry/models.json?token=secret#fragment"
                    .to_owned(),
            text: "{}".to_owned(),
            used_cache: false,
            fallback_error: None,
            warnings: Vec::new(),
        };
        let urls = vec![
            "https://user:password@example.test/root/registry/models.json?token=secret#fragment"
                .to_owned(),
            "mirror/registry/models.json".to_owned(),
        ];
        let source = fetched_text_source_json(&fetched, Path::new("cache/models.json"), &urls);
        let rendered = source.to_string();

        assert_eq!(
            source["url"],
            "https://example.test/root/registry/models.json?token=REDACTED"
        );
        assert_eq!(source["registry_urls"][1], "mirror/registry/models.json");
        assert_eq!(
            fetched_text_source_label(&fetched),
            "url:https://example.test/root/registry/models.json?token=REDACTED"
        );
        for secret in ["user", "password", "secret", "fragment"] {
            assert!(!rendered.contains(secret));
            assert!(!fetched_text_source_label(&fetched).contains(secret));
        }
    }
}

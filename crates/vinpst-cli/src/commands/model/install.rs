use std::{
    io::{IsTerminal, Write},
    sync::{Arc, Mutex},
};

use vinpst_registry::{
    RegistryOperationControl, RegistryOperationProgress, install_live_model_controlled,
};

use super::{
    ArchiveFormat, Context, Duration, LiveModelEntry, LiveModelInstallRequest,
    LiveModelInstallResult, LiveRegistryI18n, LoadedLiveI18n, LoadedLiveModelRegistry,
    ModelInstallPlanRequest, Path, ReqwestRegistryAssetSource, default_model_install_staging_root,
    default_model_root,
};
use super::{
    catalog::{live_model_list_json, load_live_model_catalog},
    support::{format_size_bytes, managed_model_dir_name, optional_str},
};

pub(super) fn print_model_install_plan(request: ModelInstallPlanRequest<'_>) -> anyhow::Result<()> {
    let (loaded, i18n) = load_live_model_catalog(
        request.registry_path,
        request.i18n_path,
        request.config_path,
        request.locale,
    )?;
    let model = loaded
        .registry
        .model_by_id_or_short_id(request.id_or_short_id)
        .with_context(|| format!("unknown model id or short_id `{}`", request.id_or_short_id))?;
    let model_root = match request.model_root {
        Some(path) => path.to_path_buf(),
        None => default_model_root()?,
    };
    let staging_root = match request.staging_root {
        Some(path) => path.to_path_buf(),
        None => default_model_install_staging_root()?,
    };

    if request.dry_run {
        let output = live_model_install_plan_json(
            model,
            i18n.i18n.as_ref(),
            &loaded,
            &i18n,
            &model_root,
            &staging_root,
        )?;
        if request.json_output {
            vinpst_terminal::print_json(&output)?;
        } else {
            print_model_install_plan_text(model, i18n.i18n.as_ref(), &model_root, &staging_root)?;
        }
        return Ok(());
    }

    let model_dir = model_root.join(managed_model_dir_name(model));
    let staging_dir = staging_root.join(managed_model_dir_name(model));
    let source = ReqwestRegistryAssetSource::with_timeout(Duration::from_secs(600));
    let progress = Arc::new(Mutex::new(ModelInstallProgress::new(
        request.id_or_short_id,
        std::io::stdout().is_terminal(),
    )));
    let reporter = Arc::clone(&progress);
    let control = RegistryOperationControl::new(move |event| {
        reporter
            .lock()
            .expect("model install progress lock poisoned")
            .report(&event);
    });
    let install_result = install_live_model_controlled(
        &source,
        &LiveModelInstallRequest {
            model,
            model_dir: model_dir.clone(),
            staging_dir: staging_dir.clone(),
            display: Some(model.installed_display_metadata(request.locale, i18n.i18n.as_ref())),
        },
        &control,
    );
    progress
        .lock()
        .expect("model install progress lock poisoned")
        .finish(install_result.is_ok());
    let installed = install_result.with_context(|| format!("install live model `{}`", model.id))?;

    if request.json_output {
        let output =
            live_model_install_result_json(model, i18n.i18n.as_ref(), &loaded, &i18n, &installed)?;
        vinpst_terminal::print_json(&output)?;
    } else {
        print_model_install_result_text(model, i18n.i18n.as_ref(), &installed);
    }
    Ok(())
}

struct ModelInstallProgress {
    label: String,
    is_tty: bool,
    last_bucket: Option<u64>,
    tty_line_open: bool,
}

impl ModelInstallProgress {
    fn new(label: &str, is_tty: bool) -> Self {
        Self {
            label: label.to_owned(),
            is_tty,
            last_bucket: None,
            tty_line_open: false,
        }
    }

    fn report(&mut self, event: &RegistryOperationProgress) {
        let RegistryOperationProgress::Downloading {
            downloaded_bytes,
            total_bytes: Some(total_bytes),
        } = event
        else {
            return;
        };
        if *total_bytes == 0 {
            return;
        }
        let percent = (downloaded_bytes.saturating_mul(100) / total_bytes).min(100);
        if self.is_tty {
            eprint!("\rDownloading {}... {percent}%   ", self.label);
            let _ = std::io::stderr().flush();
            self.tty_line_open = true;
            self.last_bucket = Some(percent);
            return;
        }
        let bucket = (percent / 10) * 10;
        if self.last_bucket.is_none_or(|last| bucket > last) {
            eprintln!("Downloading {}...: {bucket}%", self.label);
            self.last_bucket = Some(bucket);
        }
    }

    fn finish(&mut self, success: bool) {
        if self.is_tty {
            if self.tty_line_open {
                eprintln!();
            }
            return;
        }
        if success && self.last_bucket.unwrap_or_default() < 100 {
            eprintln!("Downloading {}...: 100%", self.label);
            self.last_bucket = Some(100);
        }
    }
}

fn live_model_install_result_json(
    model: &LiveModelEntry,
    i18n: Option<&LiveRegistryI18n>,
    loaded: &LoadedLiveModelRegistry,
    loaded_i18n: &LoadedLiveI18n,
    installed: &LiveModelInstallResult,
) -> anyhow::Result<serde_json::Value> {
    let mut model_json = live_model_list_json(model, i18n);
    model_json["vinpst_model"] =
        model
            .vinpst_model
            .as_ref()
            .map_or(Ok(serde_json::Value::Null), |metadata| {
                metadata
                    .to_raw_value()
                    .context("serialize vinpst_model metadata")
            })?;
    Ok(serde_json::json!({
        "ok": true,
        "dry_run": false,
        "source": loaded.source_json,
        "i18n": loaded_i18n.source_json,
        "model": model_json,
        "install": {
            "model_dir": installed.materialized.target_path,
            "metadata_path": installed.metadata_path,
            "archive_path": installed.staged_asset.path,
            "extract_path": installed.staged_archive.path,
            "materialize_source_path": installed.materialize_source_path,
            "replaced_existing": installed.materialized.replaced_existing,
            "checksum_verified": installed.checksum_verified(),
            "file_count": installed.staged_archive.file_count,
            "directory_count": installed.staged_archive.directory_count,
        },
        "will_write_config": false,
        "next_steps": [
            "run vinpst model use to update config",
            "run vinpst asr-state to verify native runtime loading"
        ],
    }))
}

fn live_model_install_plan_json(
    model: &LiveModelEntry,
    i18n: Option<&LiveRegistryI18n>,
    loaded: &LoadedLiveModelRegistry,
    loaded_i18n: &LoadedLiveI18n,
    model_root: &Path,
    staging_root: &Path,
) -> anyhow::Result<serde_json::Value> {
    let archive_file_name = model_archive_file_name(model)?;
    let archive_format = archive_format_label(archive_file_name);
    let archive_supported = ArchiveFormat::from_path(archive_file_name).is_some();
    let model_dir_name = managed_model_dir_name(model);
    let model_dir = model_root.join(&model_dir_name);
    let staging_dir = staging_root.join(&model_dir_name);
    let archive_path = staging_dir.join("archives").join(archive_file_name);
    let extract_dir = staging_dir.join("extract");
    let metadata_path = model_dir.join("vinpst-model.json");

    let mut model_json = live_model_list_json(model, i18n);
    model_json["vinpst_model"] =
        model
            .vinpst_model
            .as_ref()
            .map_or(Ok(serde_json::Value::Null), |metadata| {
                metadata
                    .to_raw_value()
                    .context("serialize vinpst_model metadata")
            })?;

    Ok(serde_json::json!({
        "ok": true,
        "dry_run": true,
        "source": loaded.source_json,
        "i18n": loaded_i18n.source_json,
        "model": model_json,
        "archive": {
            "file_name": archive_file_name,
            "format": archive_format,
            "supported": archive_supported,
            "supported_formats": ["tar", "tar_zst", "tar_bz2"],
            "urls": model.urls,
            "sha256": model.sha256,
            "size_bytes": model.size_bytes,
        },
        "target": {
            "model_root": model_root,
            "model_dir_name": model_dir_name,
            "model_dir": model_dir,
            "metadata_path": metadata_path,
            "config_model_value": model_dir,
        },
        "staging": {
            "staging_root": staging_root,
            "staging_dir": staging_dir,
            "archive_path": archive_path,
            "extract_dir": extract_dir,
        },
        "will_download": false,
        "will_extract": false,
        "will_write_config": false,
        "next_steps": [
            "download archive with mirror fallback",
            "verify sha256 before extraction",
            "extract with safe archive policy",
            "materialize model directory",
            "write vinpst-model.json metadata",
            "run vinpst model use to update config"
        ],
    }))
}

fn print_model_install_result_text(
    model: &LiveModelEntry,
    i18n: Option<&LiveRegistryI18n>,
    installed: &LiveModelInstallResult,
) {
    println!(
        "Installed model `{}` ({}).",
        model.resolved_title(i18n),
        model.id
    );
    println!("Location: {}", installed.materialized.target_path.display());
    println!(
        "Next: vinpst model use {}",
        optional_str(model.short_id.as_deref().or(Some(model.id.as_str())))
    );
}

fn print_model_install_plan_text(
    model: &LiveModelEntry,
    i18n: Option<&LiveRegistryI18n>,
    model_root: &Path,
    _staging_root: &Path,
) -> anyhow::Result<()> {
    model_archive_file_name(model)?;
    let model_dir_name = managed_model_dir_name(model);
    let model_dir = model_root.join(&model_dir_name);
    println!(
        "Would install model `{}` ({}, {}).",
        model.resolved_title(i18n),
        model.id,
        format_size_bytes(model.size_bytes)
    );
    println!("Location: {}", model_dir.display());
    Ok(())
}

fn model_archive_file_name(model: &LiveModelEntry) -> anyhow::Result<&str> {
    let first_url = model
        .urls
        .first()
        .context("live model has no download URLs")?;
    let file_name = first_url
        .rsplit('/')
        .next()
        .unwrap_or(first_url)
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    if file_name.is_empty() {
        anyhow::bail!(
            "live model `{}` has no archive file name in first URL",
            model.id
        );
    }
    Ok(file_name)
}

fn archive_format_label(file_name: &str) -> &'static str {
    if ascii_suffix_eq(file_name, ".tar.zst") {
        "tar_zst"
    } else if ascii_suffix_eq(file_name, ".tar.bz2") || ascii_suffix_eq(file_name, ".tbz2") {
        "tar_bz2"
    } else if ascii_suffix_eq(file_name, ".tar.gz") || ascii_suffix_eq(file_name, ".tgz") {
        "tar_gz"
    } else if ascii_suffix_eq(file_name, ".tar") {
        "tar"
    } else {
        "unsupported"
    }
}

fn ascii_suffix_eq(value: &str, suffix: &str) -> bool {
    value
        .as_bytes()
        .get(value.len().saturating_sub(suffix.len())..)
        .is_some_and(|tail| tail.eq_ignore_ascii_case(suffix.as_bytes()))
}

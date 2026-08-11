use crate::{
    AssetEntry, AssetPlanSummary, Context, Path, PathBuf, PlannedAsset, RegistryConfig,
    RegistryIndex, VinpstConfig, fs, load_config_file,
    registry_support::registry_urls_for_diagnostics,
};

pub(crate) fn print_registry_summary() -> anyhow::Result<()> {
    let config = VinpstConfig::bundled_default().context("parse bundled config")?;
    let index_asset = AssetEntry {
        path: "index.json".to_owned(),
        sha256: None,
        size_bytes: None,
    };
    let summary = serde_json::json!({
        "base_url_count": config.registry.base_urls.len(),
        "index_urls": registry_urls_for_diagnostics(&index_asset.resolved_urls(&config.registry)),
    });
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

pub(crate) fn validate_registry_index(path: &PathBuf) -> anyhow::Result<()> {
    let input = fs::read_to_string(path)
        .with_context(|| format!("read registry index `{}`", path.display()))?;
    let index = RegistryIndex::from_json_str(&input)
        .with_context(|| format!("validate registry index `{}`", path.display()))?;
    let index_summary = index.summary();
    let summary = serde_json::json!({
        "ok": true,
        "version": index_summary.version,
        "model_count": index_summary.model_count,
        "adapter_count": index_summary.adapter_count,
        "asset_count": index_summary.asset_count,
    });
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

pub(crate) fn print_registry_plan(
    path: &PathBuf,
    config_path: Option<&PathBuf>,
    model_id: Option<&str>,
    adapter_id: Option<&str>,
    summary_only: bool,
) -> anyhow::Result<()> {
    let input = fs::read_to_string(path)
        .with_context(|| format!("read registry index `{}`", path.display()))?;
    let index = RegistryIndex::from_json_str(&input)
        .with_context(|| format!("validate registry index `{}`", path.display()))?;
    let config = match config_path {
        Some(config_path) => load_config_file(config_path)?,
        None => VinpstConfig::bundled_default().context("parse bundled config")?,
    };
    let planned_assets = selected_registry_assets(&index, &config.registry, model_id, adapter_id)?;
    let plan_summary = AssetPlanSummary::from_assets(&planned_assets);
    let summary = if summary_only {
        serde_json::json!({
            "ok": true,
            "asset_count": plan_summary.asset_count,
            "known_size_bytes": plan_summary.known_size_bytes,
            "unknown_size_count": plan_summary.unknown_size_count,
        })
    } else {
        serde_json::json!({
            "ok": true,
            "asset_count": plan_summary.asset_count,
            "known_size_bytes": plan_summary.known_size_bytes,
            "unknown_size_count": plan_summary.unknown_size_count,
            "assets": planned_assets,
        })
    };
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

pub(crate) fn print_registry_install_plan(
    path: &PathBuf,
    target_root: &Path,
    config_path: Option<&PathBuf>,
    model_id: Option<&str>,
    adapter_id: Option<&str>,
    summary_only: bool,
) -> anyhow::Result<()> {
    let input = fs::read_to_string(path)
        .with_context(|| format!("read registry index `{}`", path.display()))?;
    let index = RegistryIndex::from_json_str(&input)
        .with_context(|| format!("validate registry index `{}`", path.display()))?;
    let config = match config_path {
        Some(config_path) => load_config_file(config_path)?,
        None => VinpstConfig::bundled_default().context("parse bundled config")?,
    };
    let target_root = target_root.to_string_lossy();
    let plan = match (model_id, adapter_id) {
        (Some(model_id), None) => {
            index.install_model_plan(model_id, &config.registry, &target_root)?
        }
        (None, Some(adapter_id)) => {
            index.install_adapter_plan(adapter_id, &config.registry, &target_root)?
        }
        (None, None) => index.install_plan(&config.registry, &target_root),
        (Some(_), Some(_)) => unreachable!("clap prevents model and adapter together"),
    };
    let summary = if summary_only {
        serde_json::json!({
            "ok": true,
            "target_root": plan.target_root,
            "asset_count": plan.summary.asset_count,
            "known_size_bytes": plan.summary.known_size_bytes,
            "missing_checksum_count": plan.summary.missing_checksum_count,
        })
    } else {
        serde_json::json!({
            "ok": true,
            "target_root": plan.target_root,
            "asset_count": plan.summary.asset_count,
            "known_size_bytes": plan.summary.known_size_bytes,
            "missing_checksum_count": plan.summary.missing_checksum_count,
            "assets": plan.assets,
        })
    };
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn selected_registry_assets(
    index: &RegistryIndex,
    registry: &RegistryConfig,
    model_id: Option<&str>,
    adapter_id: Option<&str>,
) -> anyhow::Result<Vec<PlannedAsset>> {
    Ok(match (model_id, adapter_id) {
        (Some(model_id), None) => index.planned_model_assets(model_id, registry)?,
        (None, Some(adapter_id)) => index.planned_adapter_assets(adapter_id, registry)?,
        (None, None) => index.planned_assets(registry),
        (Some(_), Some(_)) => unreachable!("clap prevents model and adapter together"),
    })
}

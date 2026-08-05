use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context;
use vinpst_config::{
    VinpstConfig, config_backup_path as shared_config_backup_path, write_config_file,
};

use crate::{ConfigExample, config_example_contents, paths::default_config_path};

pub(crate) fn config_backup_path(config_path: &Path) -> PathBuf {
    shared_config_backup_path(config_path)
}

pub(crate) fn write_config_output(config: &VinpstConfig, output_path: &Path) -> anyhow::Result<()> {
    write_config_file(config, output_path, None)
        .map(|_| ())
        .with_context(|| format!("write updated config `{}`", output_path.display()))
}

pub(crate) fn write_config_in_place(
    config: &VinpstConfig,
    config_path: &Path,
    backup_path: &Path,
) -> anyhow::Result<()> {
    write_config_file(config, config_path, Some(backup_path))
        .map(|_| ())
        .with_context(|| format!("write updated config `{}`", config_path.display()))
}

pub(crate) fn write_file_atomically(path: &Path, contents: &str) -> anyhow::Result<()> {
    let temp_path = atomic_temp_path(path);
    fs::write(&temp_path, contents)
        .with_context(|| format!("write temporary config `{}`", temp_path.display()))?;
    fs::rename(&temp_path, path).with_context(|| {
        format!(
            "rename temporary config `{}` to `{}`",
            temp_path.display(),
            path.display()
        )
    })
}

pub(crate) fn atomic_temp_path(path: &Path) -> PathBuf {
    let mut temp = path.as_os_str().to_os_string();
    temp.push(format!(".tmp-{}", std::process::id()));
    PathBuf::from(temp)
}

pub(crate) fn same_path_text(left: &Path, right: &Path) -> bool {
    left == right
}

pub(crate) fn write_config_json_value(
    document: &serde_json::Value,
    output_path: &Path,
) -> anyhow::Result<()> {
    if let Some(parent) = output_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("create config output directory `{}`", parent.display()))?;
    }
    let contents = serde_json::to_string_pretty(document).context("serialize updated config")?;
    write_file_atomically(output_path, &format!("{contents}\n"))
        .with_context(|| format!("write updated config `{}`", output_path.display()))
}

pub(crate) fn load_config_file(path: &PathBuf) -> anyhow::Result<VinpstConfig> {
    let config = VinpstConfig::from_json_file(path)
        .with_context(|| format!("load config `{}`", path.display()))?;
    config
        .validate()
        .with_context(|| format!("validate config `{}`", path.display()))?;
    Ok(config)
}

pub(crate) struct LoadedConfigJson {
    pub(crate) path: Option<PathBuf>,
    pub(crate) source: &'static str,
    pub(crate) document: serde_json::Value,
}

#[derive(Clone)]
pub(crate) enum ConfigSetWriteTarget {
    DryRun,
    Output(PathBuf),
    InPlace {
        config_path: PathBuf,
        backup_path: Option<PathBuf>,
    },
}

impl ConfigSetWriteTarget {
    pub(crate) fn output_path(&self) -> Option<PathBuf> {
        match self {
            Self::DryRun => None,
            Self::Output(path) => Some(path.clone()),
            Self::InPlace { config_path, .. } => Some(config_path.clone()),
        }
    }

    pub(crate) fn backup_path(&self) -> Option<PathBuf> {
        match self {
            Self::InPlace { backup_path, .. } => backup_path.clone(),
            Self::DryRun | Self::Output(_) => None,
        }
    }

    pub(crate) fn in_place(&self) -> bool {
        matches!(self, Self::InPlace { .. })
    }
}

pub(crate) fn config_set_write_target(
    output_path: Option<&Path>,
    in_place: bool,
    dry_run: bool,
    input_path: Option<&PathBuf>,
    default_path: &Path,
) -> anyhow::Result<ConfigSetWriteTarget> {
    if output_path.is_some() && in_place {
        anyhow::bail!("config set cannot combine --output and --in-place");
    }
    if dry_run {
        return Ok(ConfigSetWriteTarget::DryRun);
    }
    if in_place {
        let target = input_path
            .cloned()
            .unwrap_or_else(|| default_path.to_path_buf());
        let backup_path = target.exists().then(|| config_backup_path(&target));
        return Ok(ConfigSetWriteTarget::InPlace {
            config_path: target,
            backup_path,
        });
    }
    let output_path = output_path.with_context(|| {
        "config set writes require --output <path> or --in-place; rerun with --dry-run to inspect the config patch"
    })?;
    if let Some(input_path) = input_path
        && same_path_text(input_path, output_path)
    {
        anyhow::bail!(
            "refusing to overwrite input config `{}` with --output; use --in-place to create a backup",
            input_path.display()
        );
    }
    Ok(ConfigSetWriteTarget::Output(output_path.to_path_buf()))
}

pub(crate) fn write_config_set_document(
    document: &serde_json::Value,
    target: &ConfigSetWriteTarget,
) -> anyhow::Result<()> {
    match target {
        ConfigSetWriteTarget::DryRun => Ok(()),
        ConfigSetWriteTarget::Output(output_path) => write_config_json_value(document, output_path),
        ConfigSetWriteTarget::InPlace {
            config_path,
            backup_path,
        } => {
            if let Some(backup_path) = backup_path {
                fs::copy(config_path, backup_path).with_context(|| {
                    format!(
                        "backup config `{}` to `{}`",
                        config_path.display(),
                        backup_path.display()
                    )
                })?;
            }
            write_config_json_value(document, config_path)
        }
    }
}

pub(crate) fn load_config_json(config_path: Option<&PathBuf>) -> anyhow::Result<LoadedConfigJson> {
    let path = if let Some(path) = config_path {
        Some(path.clone())
    } else {
        let default_path = default_config_path()?;
        default_path.exists().then_some(default_path)
    };
    let (source, contents) = match &path {
        Some(path) => (
            "file",
            fs::read_to_string(path)
                .with_context(|| format!("read config `{}`", path.display()))?,
        ),
        None => (
            "bundled-default",
            config_example_contents(ConfigExample::Default).to_owned(),
        ),
    };
    let document = serde_json::from_str::<serde_json::Value>(&contents)
        .with_context(|| format!("parse {source} config as JSON"))?;
    validate_config_json_value(&document, "validate config")?;
    Ok(LoadedConfigJson {
        path,
        source,
        document,
    })
}

pub(crate) fn validate_config_json_value(
    document: &serde_json::Value,
    context: &str,
) -> anyhow::Result<()> {
    let contents = serde_json::to_string(document).context("serialize config for validation")?;
    let config = VinpstConfig::from_json_str(&contents).context("parse config")?;
    config.validate().with_context(|| context.to_owned())
}

pub(crate) fn split_editor_argv(editor: &str) -> Vec<String> {
    editor
        .split_whitespace()
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(crate) fn config_summary_json(config: &VinpstConfig) -> serde_json::Value {
    let summary = config.summary();
    serde_json::json!({
        "ok": true,
        "version": summary.version,
        "active_scene": summary.active_scene,
        "active_provider": summary.active_provider,
        "scene_count": summary.scene_count,
        "provider_count": summary.provider_count,
        "registry_mirror_count": summary.registry_mirror_count,
    })
}

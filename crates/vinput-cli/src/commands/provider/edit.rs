use super::{
    AsrProviderConfig, AsrProviderKind, Context, InstalledProviderResolution, LiveScriptKind, Path,
    PathBuf, ProcessCommand, ProviderEditOutcome, ProviderEditRequest, ProviderEditScriptOutcome,
    ProviderEditScriptRequest, VinputConfig, asr_provider_kind_label, config_set_write_target,
    default_config_path, fs, load_config_json, normalize_provider_id, split_editor_argv, user_home,
    validate_config_json_value, write_config_set_document,
};
use super::{
    catalog::{load_live_provider_registry, load_provider_list_context},
    mutation::{normalize_provider_kind, parse_provider_env},
};

pub(super) fn print_provider_edit(request: ProviderEditRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_provider_edit(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&provider_edit_outcome_json(&outcome))?
        );
    } else {
        print_provider_edit_text(&outcome);
    }
    Ok(())
}

fn run_provider_edit(request: &ProviderEditRequest<'_>) -> anyhow::Result<ProviderEditOutcome> {
    let id = normalize_provider_id(request.id)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for provider edit")?;
    let config =
        VinputConfig::from_json_str(&contents).context("parse config for provider edit")?;
    let provider_index = config
        .asr
        .providers
        .iter()
        .position(|provider| provider.id == id)
        .with_context(|| format!("ASR provider `{id}` not found"))?;
    let before_provider = &config.asr.providers[provider_index];
    let before_provider_type = asr_provider_kind_label(&before_provider.kind);

    let providers = loaded
        .document
        .pointer_mut("/asr/providers")
        .and_then(serde_json::Value::as_array_mut)
        .with_context(|| "config pointer `/asr/providers` not found or not an array")?;
    let provider_object = providers
        .get_mut(provider_index)
        .and_then(serde_json::Value::as_object_mut)
        .with_context(|| format!("ASR provider `{id}` is not a JSON object"))?;
    let changed_fields = apply_provider_edit(provider_object, request)?;
    if changed_fields.is_empty() {
        anyhow::bail!("provider edit requires at least one field change");
    }

    validate_config_json_value(&loaded.document, "validate updated provider config")?;
    let updated_contents =
        serde_json::to_string(&loaded.document).context("serialize updated provider config")?;
    let updated_config =
        VinputConfig::from_json_str(&updated_contents).context("parse updated provider config")?;
    let after_provider = &updated_config.asr.providers[provider_index];
    let after_provider_type = asr_provider_kind_label(&after_provider.kind);

    let write_target = config_set_write_target(
        request.output_path,
        request.in_place,
        request.dry_run,
        loaded.path.as_ref(),
        &default_path,
    )?;

    let mut wrote_config = false;
    if !request.dry_run {
        write_config_set_document(&loaded.document, &write_target)?;
        wrote_config = true;
    }

    Ok(ProviderEditOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        provider_id: id,
        before_provider_type,
        after_provider_type,
        active_provider: config.asr.active_provider,
        changed_fields,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn provider_edit_outcome_json(outcome: &ProviderEditOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "provider_id": outcome.provider_id,
        "before_provider_type": outcome.before_provider_type,
        "after_provider_type": outcome.after_provider_type,
        "active_provider": outcome.active_provider,
        "changed_fields": outcome.changed_fields,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinput provider list to verify configured ASR providers",
            "run vinput asr-state to inspect provider runtime readiness",
            "run vinput doctor to inspect full local diagnostics"
        ],
    })
}

fn print_provider_edit_text(outcome: &ProviderEditOutcome) {
    println!("dry_run: {}", outcome.dry_run);
    println!("source: {}", outcome.source);
    if let Some(config_path) = &outcome.config_path {
        println!("config_path: {}", config_path.display());
    }
    println!("provider_id: {}", outcome.provider_id);
    println!("before_provider_type: {}", outcome.before_provider_type);
    println!("after_provider_type: {}", outcome.after_provider_type);
    println!("active_provider: {}", outcome.active_provider);
    println!("changed_fields: {}", outcome.changed_fields.join(","));
    println!("in_place: {}", outcome.in_place);
    if let Some(output_path) = &outcome.output_path {
        println!("output_path: {}", output_path.display());
    }
    if let Some(backup_path) = &outcome.backup_path {
        println!("backup_path: {}", backup_path.display());
    }
    println!("will_write_config: {}", !outcome.dry_run);
    println!("wrote_config: {}", outcome.wrote_config);
}

pub(super) fn print_provider_edit_script(
    request: ProviderEditScriptRequest<'_>,
) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_provider_edit_script(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&provider_edit_script_outcome_json(&outcome))?
        );
    } else {
        print_provider_edit_script_text(&outcome);
    }
    Ok(())
}

fn run_provider_edit_script(
    request: &ProviderEditScriptRequest<'_>,
) -> anyhow::Result<ProviderEditScriptOutcome> {
    let resolution = resolve_installed_provider_selector(
        request.selector,
        request.registry_path,
        request.config_path,
    )?;
    if resolution.provider.kind != AsrProviderKind::Command {
        anyhow::bail!(
            "ASR provider `{}` is not a command provider and has no editable script",
            resolution.provider.id
        );
    }
    let script_path =
        resolve_editable_provider_script(&resolution.provider)?.with_context(|| {
            format!(
                "ASR provider `{}` does not reference an existing editable script file",
                resolution.provider.id
            )
        })?;
    let editor_argv = resolve_provider_editor(request.editor)?;
    let mut edited = false;
    let mut exit_status = None;
    if !request.dry_run {
        let status = run_provider_editor(&editor_argv, &script_path)?;
        exit_status = status.code();
        if !status.success() {
            anyhow::bail!(
                "provider editor `{}` exited with status {}",
                editor_argv.join(" "),
                exit_status.map_or_else(|| "signal".to_owned(), |code| code.to_string())
            );
        }
        edited = true;
    }
    Ok(ProviderEditScriptOutcome {
        selector: resolution.selector,
        provider_id: resolution.provider.id,
        config_path: resolution.config_path,
        source: resolution.source,
        registry_source: resolution.registry_source,
        script_path,
        editor_argv,
        dry_run: request.dry_run,
        edited,
        exit_status,
    })
}

fn resolve_installed_provider_selector(
    selector: &str,
    registry_path: Option<&Path>,
    config_path: Option<&PathBuf>,
) -> anyhow::Result<InstalledProviderResolution> {
    let selector = normalize_provider_id(selector)?;
    let context = load_provider_list_context(config_path)?;
    let (provider, registry_source) = if let Some(provider) = context
        .config
        .asr
        .providers
        .iter()
        .find(|provider| provider.id == selector)
    {
        (provider.clone(), None)
    } else if let Some(registry_path) = registry_path {
        let registry = load_live_provider_registry(Some(registry_path), &context.config.registry)?;
        let entry = registry
            .registry
            .entry_by_id_or_short_id(&selector, LiveScriptKind::AsrProvider)
            .with_context(|| format!("ASR provider selector `{selector}` not found"))?;
        let provider = context
            .config
            .asr
            .providers
            .iter()
            .find(|provider| provider.id == entry.id)
            .with_context(|| {
                format!(
                    "ASR provider `{}` resolved from `{selector}` is not installed",
                    entry.id
                )
            })?;
        (provider.clone(), Some(registry.source_json))
    } else {
        anyhow::bail!(
            "ASR provider `{selector}` not found; pass --registry <providers.json> to resolve a short id"
        );
    };
    Ok(InstalledProviderResolution {
        selector,
        provider,
        config_path: context.config_path,
        source: context.source,
        registry_source,
    })
}

fn resolve_editable_provider_script(
    provider: &AsrProviderConfig,
) -> anyhow::Result<Option<PathBuf>> {
    if let Some(command) = provider.command.as_deref()
        && is_path_like_command(command)
        && let Some(path) = resolve_existing_regular_file(command)?
    {
        return Ok(Some(path));
    }
    for argument in &provider.args {
        if let Some(path) = resolve_existing_regular_file(argument)? {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn is_path_like_command(command: &str) -> bool {
    command.contains('/') || command.starts_with('.') || command.starts_with('~')
}

fn resolve_existing_regular_file(candidate: &str) -> anyhow::Result<Option<PathBuf>> {
    let candidate = candidate.trim();
    if candidate.is_empty() {
        return Ok(None);
    }
    let path = if candidate == "~" {
        user_home()?
    } else if let Some(relative) = candidate.strip_prefix("~/") {
        user_home()?.join(relative)
    } else if candidate.starts_with('~') {
        return Ok(None);
    } else {
        PathBuf::from(candidate)
    };
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .context("resolve current directory for provider script")?
            .join(path)
    };
    match fs::metadata(&path) {
        Ok(metadata) if metadata.is_file() => Ok(Some(path)),
        Ok(_) => Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error)
            .with_context(|| format!("inspect provider script candidate `{}`", path.display())),
    }
}

fn resolve_provider_editor(editor: Option<&str>) -> anyhow::Result<Vec<String>> {
    let editor = editor
        .map(str::to_owned)
        .or_else(|| std::env::var("VINPUT_PROVIDER_EDITOR").ok())
        .or_else(|| std::env::var("VISUAL").ok())
        .or_else(|| std::env::var("EDITOR").ok())
        .unwrap_or_else(|| "vi".to_owned());
    let argv = split_editor_argv(&editor);
    if argv.is_empty() {
        anyhow::bail!("provider editor command is empty");
    }
    Ok(argv)
}

fn run_provider_editor(
    editor_argv: &[String],
    path: &Path,
) -> anyhow::Result<std::process::ExitStatus> {
    let (program, args) = editor_argv
        .split_first()
        .with_context(|| "provider editor command is empty")?;
    ProcessCommand::new(program)
        .args(args)
        .arg(path)
        .status()
        .with_context(|| format!("run provider editor `{}`", editor_argv.join(" ")))
}

fn provider_edit_script_outcome_json(outcome: &ProviderEditScriptOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "selector": outcome.selector,
        "provider_id": outcome.provider_id,
        "config_path": outcome.config_path,
        "source": outcome.source,
        "registry_source": outcome.registry_source,
        "script_path": outcome.script_path,
        "editor": outcome.editor_argv.join(" "),
        "editor_argv": outcome.editor_argv,
        "edited": outcome.edited,
        "exit_status": outcome.exit_status,
        "next_steps": [
            "run vinput provider list to verify the installed provider",
            "run vinput asr-state to inspect provider runtime readiness"
        ],
    })
}

fn print_provider_edit_script_text(outcome: &ProviderEditScriptOutcome) {
    println!("dry_run: {}", outcome.dry_run);
    println!("selector: {}", outcome.selector);
    println!("provider_id: {}", outcome.provider_id);
    println!("source: {}", outcome.source);
    if let Some(config_path) = &outcome.config_path {
        println!("config_path: {}", config_path.display());
    }
    println!("script_path: {}", outcome.script_path.display());
    println!("editor: {}", outcome.editor_argv.join(" "));
    println!("edited: {}", outcome.edited);
    if let Some(exit_status) = outcome.exit_status {
        println!("exit_status: {exit_status}");
    }
}

fn apply_provider_edit(
    provider_object: &mut serde_json::Map<String, serde_json::Value>,
    request: &ProviderEditRequest<'_>,
) -> anyhow::Result<Vec<String>> {
    let mut changed = Vec::new();
    if let Some(kind) = request.kind {
        provider_object.insert(
            "type".to_owned(),
            serde_json::Value::String(normalize_provider_kind(kind)?.to_owned()),
        );
        changed.push("type".to_owned());
    }
    apply_optional_provider_string_edit(
        provider_object,
        "model",
        "model",
        request.model,
        request.clear_model,
        &mut changed,
    )?;
    apply_optional_provider_string_edit(
        provider_object,
        "hotwords_file",
        "hotwords-file",
        request.hotwords_file,
        request.clear_hotwords_file,
        &mut changed,
    )?;
    apply_optional_provider_string_edit(
        provider_object,
        "command",
        "command",
        request.command,
        request.clear_command,
        &mut changed,
    )?;
    if !request.args.is_empty() && request.clear_args {
        anyhow::bail!("provider edit cannot combine --arg and --clear-args");
    }
    if !request.args.is_empty() {
        provider_object.insert("args".to_owned(), serde_json::json!(request.args));
        changed.push("args".to_owned());
    } else if request.clear_args {
        provider_object.remove("args");
        changed.push("args".to_owned());
    }
    if !request.env.is_empty() && request.clear_env {
        anyhow::bail!("provider edit cannot combine --env and --clear-env");
    }
    if !request.env.is_empty() {
        provider_object.insert(
            "env".to_owned(),
            serde_json::json!(parse_provider_env(request.env)?),
        );
        changed.push("env".to_owned());
    } else if request.clear_env {
        provider_object.remove("env");
        changed.push("env".to_owned());
    }
    apply_optional_provider_string_edit(
        provider_object,
        "endpoint",
        "endpoint",
        request.endpoint,
        request.clear_endpoint,
        &mut changed,
    )?;
    if request.timeout_ms.is_some() && request.clear_timeout {
        anyhow::bail!("provider edit cannot combine --timeout-ms and --clear-timeout");
    }
    if let Some(timeout_ms) = request.timeout_ms {
        provider_object.insert("timeout_ms".to_owned(), serde_json::json!(timeout_ms));
        changed.push("timeout_ms".to_owned());
    } else if request.clear_timeout {
        provider_object.remove("timeout_ms");
        changed.push("timeout_ms".to_owned());
    }
    Ok(changed)
}

fn apply_optional_provider_string_edit(
    provider_object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    option_name: &str,
    value: Option<&str>,
    clear: bool,
    changed: &mut Vec<String>,
) -> anyhow::Result<()> {
    if value.is_some() && clear {
        anyhow::bail!("provider edit cannot combine --{option_name} and --clear-{option_name}");
    }
    if let Some(value) = value {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            anyhow::bail!("provider field `{key}` cannot be empty");
        }
        provider_object.insert(
            key.to_owned(),
            serde_json::Value::String(trimmed.to_owned()),
        );
        changed.push(key.to_owned());
    } else if clear {
        provider_object.remove(key);
        changed.push(key.to_owned());
    }
    Ok(())
}

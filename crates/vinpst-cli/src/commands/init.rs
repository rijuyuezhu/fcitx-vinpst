use crate::{
    ConfigExample, Context, Path, PathBuf, VinpstConfig, config_example_contents,
    default_cache_root, default_config_path, default_model_root, fs, user_activation_service_path,
    write_private_file_atomically,
};

#[derive(Clone, Copy)]
pub(crate) struct InitRequest<'a> {
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) model_root: Option<&'a Path>,
    pub(crate) cache_root: Option<&'a Path>,
    pub(crate) force: bool,
    pub(crate) dry_run: bool,
    pub(crate) json_output: bool,
}

#[allow(clippy::struct_excessive_bools)]
struct InitOutcome {
    dry_run: bool,
    force: bool,
    config_path: PathBuf,
    config_existed: bool,
    wrote_config: bool,
    model_root: PathBuf,
    model_root_existed: bool,
    created_model_root: bool,
    cache_root: PathBuf,
    cache_root_existed: bool,
    created_cache_root: bool,
    activation_service_path: Option<PathBuf>,
    activation_command_argv: Vec<String>,
}

pub(crate) fn handle_init(request: InitRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_init(&request)?;
    if json_output {
        vinpst_terminal::print_json(&init_outcome_json(&outcome))?;
    } else {
        print_init_outcome_text(&outcome);
    }
    Ok(())
}

fn run_init(request: &InitRequest<'_>) -> anyhow::Result<InitOutcome> {
    let config_path = match request.config_path {
        Some(path) => path.to_path_buf(),
        None => default_config_path()?,
    };
    let model_root = match request.model_root {
        Some(path) => path.to_path_buf(),
        None => default_model_root()?,
    };
    let cache_root = match request.cache_root {
        Some(path) => path.to_path_buf(),
        None => default_cache_root()?,
    };
    let activation_service_path = user_activation_service_path().ok();
    let activation_command_argv = init_activation_command_argv(&config_path);

    let config_existed = config_path.exists();
    let model_root_existed = model_root.exists();
    let cache_root_existed = cache_root.exists();
    let mut wrote_config = false;
    let mut created_model_root = false;
    let mut created_cache_root = false;

    let bundled_config = VinpstConfig::bundled_default().context("parse bundled init config")?;
    bundled_config
        .validate()
        .context("validate bundled init config")?;

    if !request.dry_run {
        if !config_existed || request.force {
            if let Some(parent) = config_path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create config directory `{}`", parent.display()))?;
            }
            let contents = config_example_contents(ConfigExample::Default);
            write_private_file_atomically(&config_path, contents)
                .with_context(|| format!("write default config `{}`", config_path.display()))?;
            wrote_config = true;
        }
        if !model_root_existed {
            fs::create_dir_all(&model_root)
                .with_context(|| format!("create model root `{}`", model_root.display()))?;
            created_model_root = true;
        }
        if !cache_root_existed {
            fs::create_dir_all(&cache_root)
                .with_context(|| format!("create cache root `{}`", cache_root.display()))?;
            created_cache_root = true;
        }
    }

    Ok(InitOutcome {
        dry_run: request.dry_run,
        force: request.force,
        config_path,
        config_existed,
        wrote_config,
        model_root,
        model_root_existed,
        created_model_root,
        cache_root,
        cache_root_existed,
        created_cache_root,
        activation_service_path,
        activation_command_argv,
    })
}

fn init_outcome_json(outcome: &InitOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "force": outcome.force,
        "config": {
            "path": outcome.config_path,
            "existed": outcome.config_existed,
            "will_write": outcome.dry_run && (!outcome.config_existed || outcome.force),
            "wrote": outcome.wrote_config,
        },
        "directories": {
            "model_root": {
                "path": outcome.model_root,
                "existed": outcome.model_root_existed,
                "will_create": outcome.dry_run && !outcome.model_root_existed,
                "created": outcome.created_model_root,
            },
            "cache_root": {
                "path": outcome.cache_root,
                "existed": outcome.cache_root_existed,
                "will_create": outcome.dry_run && !outcome.cache_root_existed,
                "created": outcome.created_cache_root,
            },
        },
        "activation_service": {
            "user_service_path": outcome.activation_service_path,
            "command": outcome.activation_command_argv.join(" "),
            "command_argv": outcome.activation_command_argv,
        },
        "next_steps": [
            "browse available models with vinpst model list --available",
            "install a model with vinpst model install <id-or-short-id>",
            "select it with vinpst model use <id-or-short-id> --in-place",
            "check setup with vinpst doctor"
        ],
    })
}

fn print_init_outcome_text(outcome: &InitOutcome) {
    if outcome.dry_run {
        println!("Initialization preview");
    } else {
        println!("Vinpst is initialized.");
    }
    println!();
    println!(
        "Config: {} ({})",
        outcome.config_path.display(),
        init_config_state(outcome)
    );
    println!(
        "Models: {} ({})",
        outcome.model_root.display(),
        init_directory_state(
            outcome.dry_run,
            outcome.model_root_existed,
            outcome.created_model_root
        )
    );
    println!(
        "Cache:  {} ({})",
        outcome.cache_root.display(),
        init_directory_state(
            outcome.dry_run,
            outcome.cache_root_existed,
            outcome.created_cache_root
        )
    );
    if outcome.dry_run {
        println!();
        println!("No files were changed.");
        return;
    }
    println!();
    println!("Next:");
    println!("  1. Browse models:  vinpst model list --available");
    println!("  2. Install a model: vinpst model install <id-or-short-id>");
    println!("  3. Select it:       vinpst model use <id-or-short-id> --in-place");
    println!("  4. Check setup:     vinpst doctor");
}

fn init_config_state(outcome: &InitOutcome) -> &'static str {
    if outcome.dry_run {
        if outcome.config_existed && outcome.force {
            "would replace"
        } else if outcome.config_existed {
            "kept"
        } else {
            "would create"
        }
    } else if outcome.wrote_config {
        if outcome.config_existed {
            "replaced"
        } else {
            "created"
        }
    } else {
        "kept"
    }
}

const fn init_directory_state(dry_run: bool, existed: bool, created: bool) -> &'static str {
    if dry_run {
        if existed { "ready" } else { "would create" }
    } else if created {
        "created"
    } else {
        "ready"
    }
}

fn init_activation_command_argv(config_path: &Path) -> Vec<String> {
    vec![
        "vinpst".to_owned(),
        "activation-service".to_owned(),
        "--daemon".to_owned(),
        default_daemon_path_hint().to_string_lossy().into_owned(),
        "--config".to_owned(),
        config_path.to_string_lossy().into_owned(),
        "--configured-backends".to_owned(),
        "--user".to_owned(),
    ]
}

fn default_daemon_path_hint() -> PathBuf {
    if let Ok(current_exe) = std::env::current_exe()
        && let Some(parent) = current_exe.parent()
    {
        let sibling = parent.join("vinpst-daemon");
        if sibling.exists() {
            return sibling;
        }
    }
    PathBuf::from("vinpst-daemon")
}

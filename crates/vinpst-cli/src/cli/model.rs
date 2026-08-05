use std::path::PathBuf;

use clap::Subcommand;
use vinpst_registry::detect_preferred_registry_locale;

/// Model-related commands backed by the live registry catalog.
#[derive(Debug, Subcommand)]
pub(crate) enum ModelCommand {
    /// List models from live registry/models.json metadata.
    #[command(alias = "ls")]
    List {
        /// Legacy-compatible flag for listing remote/available models.
        #[arg(short = 'a', long)]
        available: bool,
        /// List installed models from the managed model root instead of the live registry.
        #[arg(long)]
        installed: bool,
        /// Managed model root used by --installed. Defaults to $XDG_DATA_HOME/fcitx-vinpst/models.
        #[arg(long)]
        model_root: Option<PathBuf>,
        /// Optional local live registry/models.json file. Omitted to fetch configured mirrors.
        #[arg(long)]
        registry: Option<PathBuf>,
        /// Optional local live i18n JSON file used for title/description fallback.
        #[arg(long)]
        i18n: Option<PathBuf>,
        /// Optional config JSON file that provides registry mirrors.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Live registry i18n locale to fetch when reading remote mirrors.
        #[arg(long, default_value_t = detect_preferred_registry_locale())]
        locale: String,
        /// Print machine-readable JSON instead of text table output.
        #[arg(long)]
        json: bool,
    },
    /// Show one live registry model by full id or short id.
    Info {
        /// Full model id, `short_id`, installed path, or managed model directory name with --installed.
        id: String,
        /// Read installed model metadata from the managed model root instead of the live registry.
        #[arg(long)]
        installed: bool,
        /// Managed model root used by --installed. Defaults to $XDG_DATA_HOME/fcitx-vinpst/models.
        #[arg(long)]
        model_root: Option<PathBuf>,
        /// Optional local live registry/models.json file. Omitted to fetch configured mirrors.
        #[arg(long)]
        registry: Option<PathBuf>,
        /// Optional local live i18n JSON file used for title/description fallback.
        #[arg(long)]
        i18n: Option<PathBuf>,
        /// Optional config JSON file that provides registry mirrors.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Live registry i18n locale to fetch when reading remote mirrors.
        #[arg(long, default_value_t = detect_preferred_registry_locale())]
        locale: String,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Install a model from the live registry, or inspect the plan with --dry-run.
    #[command(alias = "add")]
    Install {
        /// Full model id or `short_id`.
        id: String,
        /// Optional local live registry/models.json file. Omitted to fetch configured mirrors.
        #[arg(long)]
        registry: Option<PathBuf>,
        /// Optional local live i18n JSON file used for title/description fallback.
        #[arg(long)]
        i18n: Option<PathBuf>,
        /// Optional config JSON file that provides registry mirrors.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Live registry i18n locale to fetch when reading remote mirrors.
        #[arg(long, default_value_t = detect_preferred_registry_locale())]
        locale: String,
        /// Managed model root. Defaults to $XDG_DATA_HOME/fcitx-vinpst/models.
        #[arg(long)]
        model_root: Option<PathBuf>,
        /// Temporary staging root. Defaults to $XDG_CACHE_HOME/fcitx-vinpst/model-install.
        #[arg(long)]
        staging_root: Option<PathBuf>,
        /// Print the install plan without downloading, extracting, or writing config.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Preview selecting the active local ASR model in config.
    Use {
        /// Full model id, `short_id`, installed model path, or managed model dir name.
        selector: String,
        /// Treat selector as an installed managed model directory name instead of a live registry id.
        #[arg(long)]
        installed: bool,
        /// Optional local live registry/models.json file for resolving `id`/`short_id`.
        #[arg(long)]
        registry: Option<PathBuf>,
        /// Optional local live i18n JSON file used for title/description output.
        #[arg(long)]
        i18n: Option<PathBuf>,
        /// Optional config JSON file to preview changing.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Live registry i18n locale to fetch when reading remote mirrors.
        #[arg(long, default_value_t = detect_preferred_registry_locale())]
        locale: String,
        /// ASR provider id to update. Defaults to the config active provider.
        #[arg(long)]
        provider: Option<String>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update --config in place and write a <config>.bak backup.
        #[arg(long)]
        in_place: bool,
        /// Managed model root. Defaults to $XDG_DATA_HOME/fcitx-vinpst/models.
        #[arg(long)]
        model_root: Option<PathBuf>,
        /// Reload the running daemon ASR backend after writing config. Dry-run prints the planned call.
        #[arg(long)]
        reload_daemon: bool,
        /// Preview config changes without writing. Required until config mutation is implemented.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Preview removing a managed installed model directory.
    #[command(alias = "rm")]
    Remove {
        /// Full model id, `short_id`, managed model dir name, or installed path under model root.
        selector: String,
        /// Treat selector as an installed managed model directory name instead of a live registry id.
        #[arg(long)]
        installed: bool,
        /// Optional local live registry/models.json file for resolving `id`/`short_id`.
        #[arg(long)]
        registry: Option<PathBuf>,
        /// Optional local live i18n JSON file used for title/description output.
        #[arg(long)]
        i18n: Option<PathBuf>,
        /// Optional config JSON file that provides registry mirrors.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Live registry i18n locale to fetch when reading remote mirrors.
        #[arg(long, default_value_t = detect_preferred_registry_locale())]
        locale: String,
        /// Managed model root. Defaults to $XDG_DATA_HOME/fcitx-vinpst/models.
        #[arg(long)]
        model_root: Option<PathBuf>,
        /// Print the removal plan without deleting anything.
        #[arg(long)]
        dry_run: bool,
        /// Confirm removal. Required for deleting the managed model directory.
        #[arg(long)]
        yes: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
}

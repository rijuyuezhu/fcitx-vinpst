use std::path::PathBuf;

use clap::Subcommand;

/// Registry-related commands.
#[derive(Debug, Subcommand)]
pub(crate) enum RegistryCommand {
    /// Validate a local registry index JSON file and print a summary.
    Validate {
        /// Path to a registry index JSON file.
        path: PathBuf,
    },
    /// Print planned registry assets using configured mirrors.
    Plan {
        /// Path to a registry index JSON file.
        path: PathBuf,
        /// Optional config JSON file that provides registry mirrors.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Only plan assets for this model id.
        #[arg(long, conflicts_with = "adapter")]
        model: Option<String>,
        /// Only plan assets for this adapter id.
        #[arg(long, conflicts_with = "model")]
        adapter: Option<String>,
        /// Print only the plan summary without per-asset rows.
        #[arg(long)]
        summary_only: bool,
    },
    /// Print a dry-run install plan without downloading assets.
    InstallPlan {
        /// Path to a registry index JSON file.
        path: PathBuf,
        /// Target root directory for planned asset installation.
        #[arg(long)]
        target_root: PathBuf,
        /// Optional config JSON file that provides registry mirrors.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Only plan assets for this model id.
        #[arg(long, conflicts_with = "adapter")]
        model: Option<String>,
        /// Only plan assets for this adapter id.
        #[arg(long, conflicts_with = "model")]
        adapter: Option<String>,
        /// Print only the install-plan summary without per-asset rows.
        #[arg(long)]
        summary_only: bool,
    },
}

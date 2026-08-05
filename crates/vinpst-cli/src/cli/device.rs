use std::path::PathBuf;

use clap::Subcommand;

/// Audio device selection commands.
#[derive(Debug, Subcommand)]
pub(crate) enum DeviceCommand {
    /// List configured and live capture devices.
    #[command(alias = "ls")]
    List {
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Select the capture device in config.
    Use {
        /// Capture device value, such as default or a `PipeWire` object/name from device list.
        target: String,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
}

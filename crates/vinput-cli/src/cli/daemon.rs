use clap::Subcommand;

/// Daemon-related commands backed by the D-Bus service contract.
#[derive(Debug, Subcommand)]
pub(crate) enum DaemonCommand {
    /// Trigger daemon D-Bus activation by querying status.
    Start {
        /// Print the D-Bus activation plan without contacting the daemon.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Query daemon status and runtime diagnostics over D-Bus.
    Status {
        /// Print the D-Bus call plan without contacting the daemon.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Restart the user service only when daemon status reports a stale owner.
    Handoff {
        /// Print the conditional restart plan without contacting the daemon or systemd.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Stop and disable the running daemon before package removal.
    PrepareRemove {
        /// Print the guarded removal plan without contacting D-Bus or systemd.
        #[arg(long)]
        dry_run: bool,
        /// Probe the live session and removal guards without stopping or signalling anything.
        #[arg(long, conflicts_with = "dry_run")]
        preflight: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Reload the selected ASR backend on the running daemon.
    ReloadAsr {
        /// Print the D-Bus call plan without contacting the daemon.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Stop the user daemon service.
    Stop {
        /// Print the stop plan without mutating user services.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Restart the user daemon service.
    Restart {
        /// Print the restart plan without mutating user services.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Print daemon logs from the user service journal.
    Log {
        /// Limit journal output to the last N lines.
        #[arg(short = 'n', long)]
        lines: Option<u16>,
        /// Print the log retrieval plan without invoking external tools.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
}

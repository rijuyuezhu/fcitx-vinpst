mod asr;
mod handoff;
mod install_service;
mod removal;
mod service;
mod status;
#[cfg(test)]
mod tests;

use asr::print_daemon_reload_asr_plan;
use handoff::print_daemon_handoff;
use install_service::print_daemon_install_service;
use removal::print_daemon_prepare_remove;
use service::{print_daemon_start, print_daemon_user_service_plan};
use status::print_daemon_status;

use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
    thread,
    time::Duration,
};

use anyhow::Context;
use vinpst_protocol::dbus;

use crate::DaemonCommand;

pub(crate) fn handle_daemon_command(command: &DaemonCommand) -> anyhow::Result<()> {
    match command {
        DaemonCommand::Start { dry_run, json } => print_daemon_start(*dry_run, *json),
        DaemonCommand::Status { dry_run, json } => print_daemon_status(*dry_run, *json),
        DaemonCommand::Handoff { dry_run, json } => print_daemon_handoff(*dry_run, *json),
        DaemonCommand::PrepareRemove {
            dry_run,
            preflight,
            json,
        } => print_daemon_prepare_remove(*dry_run, *preflight, *json),
        DaemonCommand::ReloadAsr { dry_run, json } => print_daemon_reload_asr_plan(*dry_run, *json),
        DaemonCommand::InstallService {
            template,
            output,
            dry_run,
            json,
        } => print_daemon_install_service(template.as_deref(), output.as_deref(), *dry_run, *json),
        DaemonCommand::Stop { dry_run, json } => {
            print_daemon_user_service_plan("stop", None, *dry_run, *json)
        }
        DaemonCommand::Restart { dry_run, json } => {
            print_daemon_user_service_plan("restart", None, *dry_run, *json)
        }
        DaemonCommand::Log {
            lines,
            dry_run,
            json,
        } => print_daemon_user_service_plan("log", *lines, *dry_run, *json),
    }
}

const HANDOFF_VERIFY_ATTEMPTS: u32 = 100;
const HANDOFF_VERIFY_INTERVAL: Duration = Duration::from_millis(50);

pub(crate) use asr::{
    AsrReloadAfterWrite, reload_asr_backend_after_canonical_write, reload_asr_backend_via_dbus,
};
pub(crate) use status::{
    daemon_name_has_owner, daemon_owner_probe_plan_json, daemon_service_proxy, optional_json_str,
};

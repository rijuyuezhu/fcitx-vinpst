use super::status::DaemonAsrBackendStateTuple;
use super::{Context, Path, daemon_owner_probe_plan_json, daemon_service_proxy, dbus};
use crate::same_path_text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AsrReloadAfterWrite {
    NotCanonical,
    DaemonNotRunning,
    Reloaded,
    Warning(String),
}

impl AsrReloadAfterWrite {
    pub(crate) fn reloaded(&self) -> bool {
        matches!(self, Self::Reloaded)
    }

    pub(crate) fn warning(&self) -> Option<&str> {
        match self {
            Self::Warning(message) => Some(message),
            Self::NotCanonical | Self::DaemonNotRunning | Self::Reloaded => None,
        }
    }
}

pub(super) fn print_daemon_reload_asr_plan(dry_run: bool, json_output: bool) -> anyhow::Result<()> {
    let asr_state = if dry_run {
        None
    } else {
        Some(request_asr_reload_via_dbus()?)
    };
    let output = daemon_reload_asr_output(dry_run, asr_state.as_ref());
    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if dry_run {
        println!("Would reload the selected ASR backend.");
        println!("No daemon will be contacted.");
    } else if let Some(asr) = asr_state {
        if asr.6 {
            println!("ASR backend reloaded: {}", display_value(&asr.2));
            if !asr.3.is_empty() {
                println!("Model: {}", asr.3);
            }
        } else {
            println!("ASR backend reload completed, but no backend is ready.");
        }
        if !asr.4.is_empty() {
            println!("ASR error: {}", asr.4);
        }
    }
    Ok(())
}

fn display_value(value: &str) -> &str {
    if value.is_empty() {
        "not selected"
    } else {
        value
    }
}

fn daemon_reload_asr_output(
    dry_run: bool,
    asr_state: Option<&DaemonAsrBackendStateTuple>,
) -> serde_json::Value {
    let asr_backend = asr_state.map(|asr| {
        serde_json::json!({
            "target_provider_id": asr.0,
            "target_model_id": asr.1,
            "effective_provider_id": asr.2,
            "effective_model_id": asr.3,
            "last_error": asr.4,
            "reload_in_progress": asr.5,
            "has_effective_backend": asr.6,
            "remote_endpoints": asr.7,
        })
    });
    serde_json::json!({
        "ok": true,
        "dry_run": dry_run,
        "will_call_dbus": !dry_run,
        "called": !dry_run,
        "asr_backend": asr_backend,
        "dbus": {
            "service": dbus::SERVICE_BUS_NAME,
            "object_path": dbus::SERVICE_OBJECT_PATH,
            "interface": dbus::SERVICE_INTERFACE,
            "method": dbus::method::RELOAD_ASR_BACKEND,
        },
        "owner_probe": daemon_owner_probe_plan_json(),
        "next_steps": [
            "run vinpst daemon status to verify the selected ASR backend",
            "use vinpst protocol to inspect the stable method contract"
        ],
    })
}

pub(crate) fn reload_asr_backend_via_dbus() -> anyhow::Result<()> {
    request_asr_reload_via_dbus().map(|_| ())
}

fn request_asr_reload_via_dbus() -> anyhow::Result<DaemonAsrBackendStateTuple> {
    let connection = zbus::blocking::Connection::session().context("connect to session bus")?;
    request_asr_reload_on_connection(&connection)
}

pub(crate) fn reload_asr_backend_after_canonical_write(
    written_path: Option<&Path>,
    canonical_path: &Path,
) -> AsrReloadAfterWrite {
    let Some(written_path) = written_path else {
        return AsrReloadAfterWrite::NotCanonical;
    };
    if !same_path_text(written_path, canonical_path) {
        return AsrReloadAfterWrite::NotCanonical;
    }

    let connection = match zbus::blocking::Connection::session() {
        Ok(connection) => connection,
        Err(error) => {
            return AsrReloadAfterWrite::Warning(format!(
                "Config saved, but ASR backend reload was skipped: {error}"
            ));
        }
    };
    let bus_proxy = match zbus::blocking::Proxy::new(
        &connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    ) {
        Ok(proxy) => proxy,
        Err(error) => {
            return AsrReloadAfterWrite::Warning(format!(
                "Config saved, but ASR backend reload was skipped: {error}"
            ));
        }
    };
    let daemon_running =
        match bus_proxy.call::<_, _, bool>("NameHasOwner", &(dbus::SERVICE_BUS_NAME)) {
            Ok(running) => running,
            Err(error) => {
                return AsrReloadAfterWrite::Warning(format!(
                    "Config saved, but ASR backend reload was skipped: {error}"
                ));
            }
        };
    if !daemon_running {
        return AsrReloadAfterWrite::DaemonNotRunning;
    }

    match request_asr_reload_on_connection(&connection) {
        Ok(_) => AsrReloadAfterWrite::Reloaded,
        Err(error) => AsrReloadAfterWrite::Warning(format!(
            "Config saved, but failed to reload ASR backend: {error:#}"
        )),
    }
}

fn request_asr_reload_on_connection(
    connection: &zbus::blocking::Connection,
) -> anyhow::Result<DaemonAsrBackendStateTuple> {
    let proxy = daemon_service_proxy(connection)?;
    let _: () = proxy
        .call(dbus::method::RELOAD_ASR_BACKEND, &())
        .context("call ReloadAsrBackend on daemon D-Bus service")?;
    proxy
        .call(dbus::method::GET_ASR_BACKEND_STATE, &())
        .context("call GetAsrBackendState after ReloadAsrBackend")
}

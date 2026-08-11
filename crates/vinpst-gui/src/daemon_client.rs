//! Typed blocking D-Bus client helpers shared by GUI operations.

use vinpst_protocol::{AsrBackendState, TextAdapterState, dbus};

use crate::DaemonSnapshot;

/// Queries required daemon status/runtime guards plus optional text-adapter diagnostics.
pub fn query_daemon_snapshot() -> Result<DaemonSnapshot, String> {
    let connection = zbus::blocking::Connection::session().map_err(|error| error.to_string())?;
    query_daemon_snapshot_on(&connection)
}

pub(crate) fn query_daemon_snapshot_if_owned() -> Result<Option<DaemonSnapshot>, String> {
    let connection = zbus::blocking::Connection::session().map_err(|error| error.to_string())?;
    let bus_proxy =
        zbus::blocking::fdo::DBusProxy::new(&connection).map_err(|error| error.to_string())?;
    let service_name = zbus::names::BusName::try_from(dbus::SERVICE_BUS_NAME)
        .map_err(|error| error.to_string())?;
    if !bus_proxy
        .name_has_owner(service_name)
        .map_err(|error| error.to_string())?
    {
        return Ok(None);
    }
    query_daemon_snapshot_on(&connection).map(Some)
}

pub(crate) fn query_daemon_snapshot_on(
    connection: &zbus::blocking::Connection,
) -> Result<DaemonSnapshot, String> {
    let proxy = daemon_proxy(connection)?;
    let status = proxy
        .call::<_, _, String>(dbus::method::GET_STATUS, &())
        .map_err(|error| error.to_string())?;
    let runtime_json = proxy
        .call::<_, _, String>(dbus::method::GET_RUNTIME_STATUS, &())
        .map_err(|error| error.to_string())?;
    let runtime = serde_json::from_str(&runtime_json).map_err(|error| error.to_string())?;
    let text_adapters_json = proxy
        .call::<_, _, String>(dbus::method::GET_TEXT_ADAPTER_STATE, &())
        .ok();
    let text_adapters = optional_text_adapter_state(text_adapters_json.as_deref());
    let asr_backend = query_asr_backend_state(&proxy).ok().map(Box::new);
    Ok(DaemonSnapshot {
        status,
        runtime,
        text_adapters,
        asr_backend,
    })
}

pub(crate) fn query_asr_backend_state(
    proxy: &zbus::blocking::Proxy<'_>,
) -> Result<AsrBackendState, String> {
    let state: (
        String,
        String,
        String,
        String,
        String,
        bool,
        bool,
        Vec<String>,
    ) = proxy
        .call(dbus::method::GET_ASR_BACKEND_STATE, &())
        .map_err(|error| error.to_string())?;
    Ok(AsrBackendState {
        target_provider_id: state.0,
        target_model_id: state.1,
        effective_provider_id: state.2,
        effective_model_id: state.3,
        last_error: state.4,
        reload_in_progress: state.5,
        has_effective_backend: state.6,
        remote_endpoints: state.7,
    })
}

fn optional_text_adapter_state(json: Option<&str>) -> TextAdapterState {
    json.and_then(|value| serde_json::from_str(value).ok())
        .unwrap_or_default()
}

pub(crate) fn daemon_proxy(
    connection: &zbus::blocking::Connection,
) -> Result<zbus::blocking::Proxy<'_>, String> {
    zbus::blocking::Proxy::new(
        connection,
        dbus::SERVICE_BUS_NAME,
        dbus::SERVICE_OBJECT_PATH,
        dbus::SERVICE_INTERFACE,
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_invalid_adapter_diagnostics_default_without_affecting_guards() {
        assert_eq!(
            optional_text_adapter_state(None),
            TextAdapterState::default()
        );
        assert_eq!(
            optional_text_adapter_state(Some("not-json")),
            TextAdapterState::default()
        );
    }
}

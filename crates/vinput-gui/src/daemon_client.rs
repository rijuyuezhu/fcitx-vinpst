//! Typed blocking D-Bus client helpers shared by GUI operations.

use vinput_protocol::{TextAdapterState, dbus};

use crate::DaemonSnapshot;

/// Queries daemon status, runtime diagnostics, and text-adapter state.
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
    let text_adapters_json = proxy
        .call::<_, _, String>(dbus::method::GET_TEXT_ADAPTER_STATE, &())
        .map_err(|error| error.to_string())?;
    let runtime = serde_json::from_str(&runtime_json).map_err(|error| error.to_string())?;
    let text_adapters = serde_json::from_str::<TextAdapterState>(&text_adapters_json)
        .map_err(|error| error.to_string())?;
    Ok(DaemonSnapshot {
        status,
        runtime,
        text_adapters,
    })
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

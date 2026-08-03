//! Blocking D-Bus transport used by the Fcitx frontend adapter.
//!
//! This crate owns platform I/O and typed wire decoding. Pure frontend policy
//! and snapshot semantics remain in `vinput-fcitx-core`.

use thiserror::Error;
use vinput_fcitx_core::{AsrDisplaySnapshot, AsrDisplaySnapshotItem, SceneSnapshot};
use vinput_protocol::dbus;
use zbus::blocking::{Connection, Proxy};

/// One daemon operation exposed to the narrow C ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonOperation {
    /// Start normal recording.
    StartRecording,
    /// Start command recording with selected text.
    StartCommandRecording,
    /// Stop recording using a scene id and return recognition JSON.
    StopRecording,
    /// Return the current daemon status.
    GetStatus,
    /// Return the active scene and configured scene rows.
    GetSceneState,
    /// Select and optionally persist an active scene.
    SetActiveScene,
    /// Return the complete localized ASR display menu state.
    GetAsrDisplayMenuState,
    /// Select and optionally persist an ASR provider.
    SetActiveAsrProvider,
    /// Select and optionally persist an ASR provider/model target.
    SetActiveAsrTarget,
    /// Return text-adapter diagnostics JSON.
    GetTextAdapterState,
    /// Start a configured text adapter.
    StartAdapter,
    /// Stop a configured text adapter.
    StopAdapter,
    /// Return runtime diagnostics JSON.
    GetRuntimeStatus,
}

/// Typed result returned by a daemon operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonResponse {
    /// Method completed without a return value.
    None,
    /// Method returned UTF-8 text.
    Text(String),
    /// Method returned a boolean persistence result.
    Bool(bool),
    /// Method returned a Rust-owned scene snapshot.
    SceneSnapshot(SceneSnapshot),
    /// Method returned a Rust-owned ASR display snapshot.
    AsrDisplaySnapshot(AsrDisplaySnapshot),
}

/// Blocking daemon transport failure.
#[derive(Debug, Error)]
pub enum DaemonError {
    /// Failed to connect to the user session bus.
    #[error("connect to session D-Bus: {0}")]
    Connect(#[source] zbus::Error),
    /// Failed to create the daemon service proxy.
    #[error("create daemon D-Bus proxy: {0}")]
    Proxy(#[source] zbus::Error),
    /// A daemon method call failed.
    #[error("call daemon D-Bus method {method}: {source}")]
    Call {
        /// Stable D-Bus method name.
        method: &'static str,
        /// Underlying transport or daemon error.
        #[source]
        source: zbus::Error,
    },
}

/// Blocking connection to the Rust voice-input daemon.
pub struct DaemonClient {
    connection: Connection,
}

type AsrDisplayReply = (
    String,
    String,
    String,
    String,
    bool,
    String,
    Vec<(String, String, String, String, String)>,
);

impl DaemonClient {
    /// Connects to the user session bus.
    pub fn connect_session() -> Result<Self, DaemonError> {
        Connection::session()
            .map(|connection| Self { connection })
            .map_err(DaemonError::Connect)
    }

    /// Executes one typed daemon operation.
    pub fn call(
        &self,
        operation: DaemonOperation,
        first: &str,
        second: &str,
    ) -> Result<DaemonResponse, DaemonError> {
        let proxy = self.proxy()?;
        match operation {
            DaemonOperation::StartRecording => {
                Self::call_unit(&proxy, dbus::method::START_RECORDING, &())?;
                Ok(DaemonResponse::None)
            }
            DaemonOperation::StartCommandRecording => {
                Self::call_unit(&proxy, dbus::method::START_COMMAND_RECORDING, &first)?;
                Ok(DaemonResponse::None)
            }
            DaemonOperation::StopRecording => {
                Self::call_text(&proxy, dbus::method::STOP_RECORDING, &first)
                    .map(DaemonResponse::Text)
            }
            DaemonOperation::GetStatus => {
                Self::call_text(&proxy, dbus::method::GET_STATUS, &()).map(DaemonResponse::Text)
            }
            DaemonOperation::GetSceneState => {
                let reply: (String, Vec<(String, String)>) =
                    Self::call_value(&proxy, dbus::method::GET_SCENE_STATE, &())?;
                Ok(DaemonResponse::SceneSnapshot(scene_snapshot(reply)))
            }
            DaemonOperation::SetActiveScene => {
                Self::call_bool(&proxy, dbus::method::SET_ACTIVE_SCENE, &first)
                    .map(DaemonResponse::Bool)
            }
            DaemonOperation::GetAsrDisplayMenuState => {
                let reply: AsrDisplayReply =
                    Self::call_value(&proxy, dbus::method::GET_ASR_DISPLAY_MENU_STATE, &())?;
                Ok(DaemonResponse::AsrDisplaySnapshot(asr_display_snapshot(
                    reply,
                )))
            }
            DaemonOperation::SetActiveAsrProvider => {
                Self::call_bool(&proxy, dbus::method::SET_ACTIVE_ASR_PROVIDER, &first)
                    .map(DaemonResponse::Bool)
            }
            DaemonOperation::SetActiveAsrTarget => Self::call_bool(
                &proxy,
                dbus::method::SET_ACTIVE_ASR_TARGET,
                &(first, second),
            )
            .map(DaemonResponse::Bool),
            DaemonOperation::GetTextAdapterState => {
                Self::call_text(&proxy, dbus::method::GET_TEXT_ADAPTER_STATE, &())
                    .map(DaemonResponse::Text)
            }
            DaemonOperation::StartAdapter => {
                Self::call_unit(&proxy, dbus::method::START_ADAPTER, &first)?;
                Ok(DaemonResponse::None)
            }
            DaemonOperation::StopAdapter => {
                Self::call_unit(&proxy, dbus::method::STOP_ADAPTER, &first)?;
                Ok(DaemonResponse::None)
            }
            DaemonOperation::GetRuntimeStatus => {
                Self::call_text(&proxy, dbus::method::GET_RUNTIME_STATUS, &())
                    .map(DaemonResponse::Text)
            }
        }
    }

    fn proxy(&self) -> Result<Proxy<'_>, DaemonError> {
        Proxy::new(
            &self.connection,
            dbus::SERVICE_BUS_NAME,
            dbus::SERVICE_OBJECT_PATH,
            dbus::SERVICE_INTERFACE,
        )
        .map_err(DaemonError::Proxy)
    }

    fn call_unit<B>(proxy: &Proxy<'_>, method: &'static str, body: &B) -> Result<(), DaemonError>
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType,
    {
        Self::call_value(proxy, method, body)
    }

    fn call_text<B>(
        proxy: &Proxy<'_>,
        method: &'static str,
        body: &B,
    ) -> Result<String, DaemonError>
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType,
    {
        Self::call_value(proxy, method, body)
    }

    fn call_bool<B>(proxy: &Proxy<'_>, method: &'static str, body: &B) -> Result<bool, DaemonError>
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType,
    {
        Self::call_value(proxy, method, body)
    }

    fn call_value<B, R>(proxy: &Proxy<'_>, method: &'static str, body: &B) -> Result<R, DaemonError>
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType,
        R: for<'de> zbus::zvariant::DynamicDeserialize<'de>,
    {
        proxy
            .call(method, body)
            .map_err(|source| DaemonError::Call { method, source })
    }
}

fn scene_snapshot((active_scene_id, scenes): (String, Vec<(String, String)>)) -> SceneSnapshot {
    let mut snapshot = SceneSnapshot::new(active_scene_id);
    for (id, label) in scenes {
        snapshot.push(id, label);
    }
    snapshot
}

fn asr_display_snapshot(reply: AsrDisplayReply) -> AsrDisplaySnapshot {
    let (
        target_provider_id,
        target_model_id,
        effective_provider_id,
        effective_model_id,
        reload_in_progress,
        last_error,
        targets,
    ) = reply;
    let mut snapshot = AsrDisplaySnapshot::new(
        target_provider_id,
        target_model_id,
        effective_provider_id,
        effective_model_id,
        reload_in_progress,
        last_error,
    );
    for (provider_id, kind, item_id, display_title, model_value) in targets {
        snapshot.push(AsrDisplaySnapshotItem {
            provider_id,
            kind,
            item_id,
            display_title,
            model_value,
        });
    }
    snapshot
}

#[cfg(test)]
mod tests {
    use super::{asr_display_snapshot, scene_snapshot};

    #[test]
    fn assembles_scene_reply_in_wire_order() {
        let snapshot = scene_snapshot((
            "meeting".to_owned(),
            vec![
                ("raw".to_owned(), "Raw Dictation".to_owned()),
                ("meeting".to_owned(), "Meeting Notes".to_owned()),
            ],
        ));
        assert_eq!(snapshot.active_scene_id(), "meeting");
        assert_eq!(snapshot.active_label(), "Meeting Notes");
        assert_eq!(snapshot.scenes()[0].id, "raw");
    }

    #[test]
    fn assembles_asr_display_reply_in_wire_order() {
        let snapshot = asr_display_snapshot((
            "sherpa".to_owned(),
            "requested".to_owned(),
            "sherpa".to_owned(),
            "effective".to_owned(),
            true,
            String::new(),
            vec![
                (
                    "sherpa".to_owned(),
                    "local".to_owned(),
                    "requested".to_owned(),
                    "Requested Model".to_owned(),
                    "requested".to_owned(),
                ),
                (
                    "remote".to_owned(),
                    "remote".to_owned(),
                    "endpoint".to_owned(),
                    String::new(),
                    "https://example.invalid".to_owned(),
                ),
            ],
        ));
        assert_eq!(snapshot.target_base_label(), "Requested Model");
        assert_eq!(snapshot.effective_base_label(), "effective");
        assert!(snapshot.is_loading_target(&snapshot.targets()[0]));
        assert_eq!(snapshot.targets()[1].base_label(), "endpoint");
    }
}

//! `zbus` service facade for the legacy daemon D-Bus ABI.
#![allow(missing_docs)]

use std::{
    net::{IpAddr, Ipv4Addr},
    sync::Arc,
    time::Duration,
};
use tokio::{sync::Mutex, time::MissedTickBehavior};
use vinput_protocol::{AsrBackendState, ServiceStatus, dbus};
use vinput_registry::scan_installed_models;
use zbus::{Connection, DBusError, object_server::SignalEmitter};

use crate::{
    RuntimeError, RuntimeState,
    remote::{RemoteTextLifecycle, RemoteTextLifecycleError, RemoteTextLifecycleStatus},
    runtime::{
        AsrReloadWorkerStep, PendingStopRecording, locale_candidates_from_environment,
        persist_config_atomically, select_asr_provider, select_asr_target,
    },
};

/// Legacy `GetAsrBackendState` D-Bus output tuple.
type AsrBackendStateTuple = (
    String,
    String,
    String,
    String,
    String,
    bool,
    bool,
    Vec<String>,
);

fn asr_backend_state_tuple(state: AsrBackendState) -> AsrBackendStateTuple {
    (
        state.target_provider_id,
        state.target_model_id,
        state.effective_provider_id,
        state.effective_model_id,
        state.last_error,
        state.reload_in_progress,
        state.has_effective_backend,
        state.remote_endpoints,
    )
}

type DbusResult<T> = Result<T, VinputDbusError>;

const MAX_ERROR_DESCRIPTION_LEN: usize = 512;
const LIVE_PARTIAL_POLL_INTERVAL: Duration = Duration::from_millis(40);
const ASR_RELOAD_POLL_INTERVAL: Duration = Duration::from_millis(20);
const ASR_RELOAD_FAILED_CODE: &str = "asr_backend_reload_failed";

#[derive(Debug, Default)]
struct LivePartialEmissionState {
    generation: u64,
    last_emitted: Option<String>,
}

impl LivePartialEmissionState {
    fn begin(&mut self, last_emitted: Option<String>) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.last_emitted = last_emitted;
        self.generation
    }

    fn cancel(&mut self) -> Option<String> {
        self.generation = self.generation.wrapping_add(1);
        self.last_emitted.take()
    }

    fn is_current(&self, generation: u64) -> bool {
        self.generation == generation
    }
}

#[derive(Debug, DBusError)]
#[zbus(prefix = "org.fcitx.Vinput.Error")]
enum VinputDbusError {
    OperationFailed(String),
}

fn sanitize_dbus_error_message(message: &str) -> String {
    let sanitized = message.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = sanitized.to_ascii_lowercase();
    if lower.contains("authorization:") || lower.contains("bearer ") || lower.contains("api_key") {
        return "operation failed".to_owned();
    }
    if sanitized.chars().count() <= MAX_ERROR_DESCRIPTION_LEN {
        return sanitized;
    }
    let mut truncated = sanitized
        .chars()
        .take(MAX_ERROR_DESCRIPTION_LEN.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}

/// Thread-safe D-Bus facade over the daemon runtime.
#[derive(Clone)]
pub struct VinputDbusService {
    runtime: Arc<Mutex<RuntimeState>>,
    remote_text: Arc<Mutex<RemoteTextLifecycle>>,
    recording_operation: Arc<Mutex<()>>,
    live_partials: Arc<Mutex<LivePartialEmissionState>>,
    signal_emitter: Arc<Mutex<Option<SignalEmitter<'static>>>>,
}

impl VinputDbusService {
    /// Creates a service facade over an existing runtime.
    #[must_use]
    pub fn new(runtime: RuntimeState) -> Self {
        Self::new_with_remote_bind(runtime, IpAddr::V4(Ipv4Addr::UNSPECIFIED))
    }

    /// Creates a service facade with an explicit remote-text bind address.
    #[must_use]
    pub fn new_with_remote_bind(runtime: RuntimeState, bind_ip: IpAddr) -> Self {
        Self {
            runtime: Arc::new(Mutex::new(runtime)),
            remote_text: Arc::new(Mutex::new(RemoteTextLifecycle::new(bind_ip))),
            recording_operation: Arc::new(Mutex::new(())),
            live_partials: Arc::new(Mutex::new(LivePartialEmissionState::default())),
            signal_emitter: Arc::new(Mutex::new(None)),
        }
    }

    /// Registers the service object and requests the legacy bus name.
    pub async fn serve_on_session_bus(&self) -> zbus::Result<Connection> {
        let connection = Connection::session().await?;
        self.bind_signal_connection(&connection).await?;
        connection
            .object_server()
            .at(dbus::SERVICE_OBJECT_PATH, self.clone())
            .await?;
        connection.request_name(dbus::SERVICE_BUS_NAME).await?;
        Ok(connection)
    }

    /// Reconciles the daemon-owned remote service with the current runtime config.
    pub async fn start_remote_text_service(&self) -> Result<bool, RemoteTextLifecycleError> {
        let config = self.runtime.lock().await.config_snapshot();
        self.reconcile_remote_text_config(&config).await
    }

    /// Stops the daemon-owned remote service during process shutdown.
    pub async fn shutdown_remote_text_service(&self) -> Result<bool, RemoteTextLifecycleError> {
        self.remote_text.lock().await.stop().await
    }

    /// Returns redacted remote service listener state.
    pub async fn remote_text_status(&self) -> RemoteTextLifecycleStatus {
        self.remote_text.lock().await.status()
    }

    /// Binds background signal emission to the connection hosting this service.
    pub async fn bind_signal_connection(&self, connection: &Connection) -> zbus::Result<()> {
        let emitter = SignalEmitter::new(connection, dbus::SERVICE_OBJECT_PATH)?.to_owned();
        *self.signal_emitter.lock().await = Some(emitter);
        Ok(())
    }

    fn operation_failed(message: impl AsRef<str>) -> VinputDbusError {
        VinputDbusError::OperationFailed(sanitize_dbus_error_message(message.as_ref()))
    }

    fn map_runtime_error(error: &RuntimeError) -> VinputDbusError {
        Self::operation_failed(error.to_string())
    }

    fn map_json_error(error: impl std::error::Error) -> VinputDbusError {
        Self::operation_failed(format!("failed to serialize response: {error}"))
    }

    fn map_signal_error(error: &zbus::Error) -> VinputDbusError {
        Self::operation_failed(format!("failed to emit signal: {error}"))
    }

    async fn emit_asr_reload_failure(&self, message: &str) {
        let emitter = self.signal_emitter.lock().await.clone();
        let Some(emitter) = emitter else {
            return;
        };
        if let Err(error) =
            Self::daemon_notification(&emitter, ASR_RELOAD_FAILED_CODE, "", "", message).await
        {
            tracing::warn!(%error, "failed to emit ASR reload notification");
        }
    }

    async fn run_asr_reload_worker(self) {
        loop {
            let step = {
                let mut runtime = self.runtime.lock().await;
                runtime.next_asr_reload_worker_step()
            };
            match step {
                AsrReloadWorkerStep::Wait => {
                    tokio::time::sleep(ASR_RELOAD_POLL_INTERVAL).await;
                }
                AsrReloadWorkerStep::Stop => return,
                AsrReloadWorkerStep::Prepare(request) => {
                    let generation = request.generation();
                    let result = tokio::task::spawn_blocking(move || request.prepare()).await;
                    match result {
                        Ok(Ok(prepared)) => {
                            let mut prepared = Some(prepared);
                            loop {
                                let applied = {
                                    let mut runtime = self.runtime.lock().await;
                                    if runtime.can_apply_prepared_asr_reload() {
                                        let Some(prepared) = prepared.take() else {
                                            return;
                                        };
                                        runtime.complete_prepared_asr_reload(prepared);
                                        true
                                    } else {
                                        false
                                    }
                                };
                                if applied {
                                    break;
                                }
                                tokio::time::sleep(ASR_RELOAD_POLL_INTERVAL).await;
                            }
                        }
                        Ok(Err(error)) => {
                            let notification = self
                                .runtime
                                .lock()
                                .await
                                .fail_prepared_asr_reload(generation, &error);
                            if let Some(message) = notification {
                                self.emit_asr_reload_failure(&message).await;
                            }
                        }
                        Err(error) => {
                            let error = RuntimeError::BackgroundTask(error.to_string());
                            let notification = self
                                .runtime
                                .lock()
                                .await
                                .fail_prepared_asr_reload(generation, &error);
                            if let Some(message) = notification {
                                self.emit_asr_reload_failure(&message).await;
                            }
                        }
                    }
                }
            }
        }
    }

    async fn reconcile_remote_text_config(
        &self,
        config: &vinput_config::VinputConfig,
    ) -> Result<bool, RemoteTextLifecycleError> {
        self.remote_text.lock().await.reconcile_config(config).await
    }

    async fn queue_asr_reload_config(&self, config: vinput_config::VinputConfig) -> DbusResult<()> {
        let remote_config = config.clone();
        let should_spawn_worker = self
            .runtime
            .lock()
            .await
            .queue_configured_asr_reload(config);
        if should_spawn_worker {
            let service = self.clone();
            tokio::spawn(async move {
                service.run_asr_reload_worker().await;
            });
        }
        self.reconcile_remote_text_config(&remote_config)
            .await
            .map_err(|error| {
                Self::operation_failed(format!("failed to reconcile remote text service: {error}"))
            })?;
        Ok(())
    }

    async fn start_recording_state(&self) -> DbusResult<(String, Option<String>)> {
        let mut runtime = self.runtime.lock().await;
        runtime
            .start_recording()
            .map_err(|error| Self::map_runtime_error(&error))?;
        Ok((
            runtime.status().to_string(),
            runtime.partial_text().map(ToOwned::to_owned),
        ))
    }

    async fn start_command_recording_state(
        &self,
        selected_text: &str,
    ) -> DbusResult<(String, Option<String>)> {
        let mut runtime = self.runtime.lock().await;
        runtime
            .start_command_recording(selected_text)
            .map_err(|error| Self::map_runtime_error(&error))?;
        Ok((
            runtime.status().to_string(),
            runtime.partial_text().map(ToOwned::to_owned),
        ))
    }

    async fn ensure_recording_for_stop(&self) -> DbusResult<()> {
        let runtime = self.runtime.lock().await;
        if runtime.status() == ServiceStatus::Recording {
            Ok(())
        } else {
            Err(Self::map_runtime_error(&RuntimeError::NotRecording(
                runtime.status(),
            )))
        }
    }

    async fn begin_stop_recording_payload(
        &self,
        scene_id: &str,
    ) -> DbusResult<PendingStopRecording> {
        let scene = (!scene_id.is_empty()).then_some(scene_id);
        let mut runtime = self.runtime.lock().await;
        runtime
            .begin_stop_recording(scene)
            .map_err(|error| Self::map_runtime_error(&error))
    }

    async fn finish_stop_recording_payload(
        &self,
        pending: PendingStopRecording,
    ) -> DbusResult<(String, String, Option<String>)> {
        let mut runtime = self.runtime.lock().await;
        let report = runtime
            .finish_stop_recording(pending)
            .map_err(|error| Self::map_runtime_error(&error))?;
        let payload_json = report
            .payload
            .to_json_string()
            .map_err(Self::map_json_error)?;
        Ok((
            payload_json,
            runtime.status().to_string(),
            report.partial_text,
        ))
    }

    async fn abort_stop_recording_payload(&self, pending: PendingStopRecording) -> String {
        let mut runtime = self.runtime.lock().await;
        runtime.abort_stop_recording(&pending);
        runtime.status().to_string()
    }

    #[cfg(test)]
    async fn stop_recording_payload(
        &self,
        scene_id: &str,
    ) -> DbusResult<(String, String, Option<String>)> {
        let pending = self.begin_stop_recording_payload(scene_id).await?;
        self.finish_stop_recording_payload(pending).await
    }

    async fn begin_live_partial_emission(&self, last_emitted: Option<String>) -> u64 {
        self.live_partials.lock().await.begin(last_emitted)
    }

    async fn cancel_live_partial_emission(&self) -> Option<String> {
        self.live_partials.lock().await.cancel()
    }

    async fn lock_recording_operation(&self) -> tokio::sync::OwnedMutexGuard<()> {
        Arc::clone(&self.recording_operation).lock_owned().await
    }

    fn spawn_live_partial_emitter(&self, emitter: SignalEmitter<'static>, generation: u64) {
        let runtime = Arc::clone(&self.runtime);
        let live_partials = Arc::clone(&self.live_partials);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(LIVE_PARTIAL_POLL_INTERVAL);
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if !live_partials.lock().await.is_current(generation) {
                    break;
                }

                let partials = {
                    let mut runtime = runtime.lock().await;
                    if runtime.status() != ServiceStatus::Recording {
                        break;
                    }
                    match runtime.take_live_partial_texts() {
                        Ok(partials) => partials,
                        Err(_) => break,
                    }
                };

                for partial in partials {
                    let should_emit = {
                        let state = live_partials.lock().await;
                        state.is_current(generation)
                            && state.last_emitted.as_deref() != Some(partial.as_str())
                    };
                    if !should_emit {
                        continue;
                    }
                    if Self::recognition_partial(&emitter, &partial).await.is_err() {
                        return;
                    }
                    let mut state = live_partials.lock().await;
                    if state.is_current(generation) {
                        state.last_emitted = Some(partial);
                    }
                }
            }
        });
    }
}

#[allow(missing_docs)]
#[zbus::interface(name = "org.fcitx.Vinput.Service")]
impl VinputDbusService {
    /// Start normal speech recognition.
    #[zbus(name = "StartRecording")]
    async fn start_recording(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<(), VinputDbusError> {
        let _operation = self.lock_recording_operation().await;
        let (status, partial_text) = self.start_recording_state().await?;
        Self::status_changed(&emitter, &status)
            .await
            .map_err(|error| Self::map_signal_error(&error))?;
        if let Some(partial_text) = &partial_text {
            Self::recognition_partial(&emitter, partial_text)
                .await
                .map_err(|error| Self::map_signal_error(&error))?;
        }
        let generation = self.begin_live_partial_emission(partial_text).await;
        self.spawn_live_partial_emitter(emitter.to_owned(), generation);
        Ok(())
    }

    /// Start command-mode speech recognition with selected text context.
    #[zbus(name = "StartCommandRecording")]
    async fn start_command_recording(
        &self,
        selected_text: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<(), VinputDbusError> {
        let _operation = self.lock_recording_operation().await;
        let (status, partial_text) = self.start_command_recording_state(selected_text).await?;
        Self::status_changed(&emitter, &status)
            .await
            .map_err(|error| Self::map_signal_error(&error))?;
        if let Some(partial_text) = &partial_text {
            Self::recognition_partial(&emitter, partial_text)
                .await
                .map_err(|error| Self::map_signal_error(&error))?;
        }
        let generation = self.begin_live_partial_emission(partial_text).await;
        self.spawn_live_partial_emitter(emitter.to_owned(), generation);
        Ok(())
    }

    /// Stop current recording and return the legacy recognition JSON payload.
    #[zbus(name = "StopRecording")]
    async fn stop_recording(
        &self,
        scene_id: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<String, VinputDbusError> {
        let _operation = self.lock_recording_operation().await;
        self.ensure_recording_for_stop().await?;
        let last_emitted_partial = self.cancel_live_partial_emission().await;
        Self::status_changed(&emitter, "inferring")
            .await
            .map_err(|error| Self::map_signal_error(&error))?;
        let pending = match self.begin_stop_recording_payload(scene_id).await {
            Ok(pending) => pending,
            Err(error) => {
                let _ = Self::status_changed(&emitter, "idle").await;
                return Err(error);
            }
        };
        if let Err(error) = Self::status_changed(&emitter, "postprocessing").await {
            let status = self.abort_stop_recording_payload(pending).await;
            let _ = Self::status_changed(&emitter, &status).await;
            return Err(Self::map_signal_error(&error));
        }
        let (payload_json, status, partial_text) =
            match self.finish_stop_recording_payload(pending).await {
                Ok(result) => result,
                Err(error) => {
                    let _ = Self::status_changed(&emitter, "idle").await;
                    return Err(error);
                }
            };
        let result_emission = async {
            if let Some(partial_text) = partial_text
                && last_emitted_partial.as_deref() != Some(partial_text.as_str())
            {
                Self::recognition_partial(&emitter, &partial_text)
                    .await
                    .map_err(|error| Self::map_signal_error(&error))?;
            }
            Self::recognition_result(&emitter, &payload_json)
                .await
                .map_err(|error| Self::map_signal_error(&error))
        }
        .await;
        let status_emission = Self::status_changed(&emitter, &status)
            .await
            .map_err(|error| Self::map_signal_error(&error));
        result_emission?;
        status_emission?;
        Ok(payload_json)
    }

    /// Return current daemon status.
    #[zbus(name = "GetStatus")]
    async fn get_status(&self) -> String {
        let runtime = self.runtime.lock().await;
        runtime.status().to_string()
    }

    /// Return ASR backend diagnostic state using the legacy tuple signature.
    #[zbus(
        name = "GetAsrBackendState",
        out_args(
            "target_provider_id",
            "target_model_id",
            "effective_provider_id",
            "effective_model_id",
            "last_error",
            "reload_in_progress",
            "has_effective_backend",
            "remote_endpoints"
        )
    )]
    async fn get_asr_backend_state(
        &self,
    ) -> (
        String,
        String,
        String,
        String,
        String,
        bool,
        bool,
        Vec<String>,
    ) {
        let mut state = self.runtime.lock().await.asr_backend_state();
        state.remote_endpoints = self.remote_text.lock().await.endpoints();
        asr_backend_state_tuple(state)
    }

    /// Return text adapter diagnostic state JSON.
    #[zbus(name = "GetTextAdapterState")]
    async fn get_text_adapter_state(&self) -> Result<String, VinputDbusError> {
        let mut runtime = self.runtime.lock().await;
        runtime.refresh_text_adapters();
        serde_json::to_string(&runtime.configured_text_adapter_state_for_runtime())
            .map_err(Self::map_json_error)
    }

    /// Return sanitized runtime status JSON.
    #[zbus(name = "GetRuntimeStatus")]
    async fn get_runtime_status(&self) -> Result<String, VinputDbusError> {
        let mut status = {
            let mut runtime = self.runtime.lock().await;
            runtime.refresh_text_adapters();
            runtime.runtime_status_json()
        };
        let remote = self.remote_text.lock().await;
        let remote_status = remote.status();
        let endpoints = remote.endpoints();
        status["asr"]["remote_endpoints"] = serde_json::json!(endpoints);
        status["remote_text"] = serde_json::json!({
            "running": remote_status.running,
            "listen_addr": remote_status.local_addr.map(|address| address.to_string()),
            "endpoints": endpoints,
        });
        serde_json::to_string(&status).map_err(Self::map_json_error)
    }

    /// Return active scene and configured scene id/label pairs.
    #[zbus(name = "GetSceneState", out_args("active_scene", "scenes"))]
    async fn get_scene_state(&self) -> (String, Vec<(String, String)>) {
        self.runtime.lock().await.scene_state()
    }

    /// Select the active scene and persist it when an explicit config file is available.
    #[zbus(name = "SetActiveScene")]
    async fn set_active_scene(&self, scene_id: &str) -> Result<bool, VinputDbusError> {
        self.runtime
            .lock()
            .await
            .set_active_scene(scene_id)
            .map_err(|error| Self::map_runtime_error(&error))
    }

    /// Return target/effective ASR state and configured provider rows.
    #[zbus(
        name = "GetAsrMenuState",
        out_args(
            "target_provider_id",
            "effective_provider_id",
            "effective_model_id",
            "reload_in_progress",
            "last_error",
            "providers"
        )
    )]
    async fn get_asr_menu_state(
        &self,
    ) -> (
        String,
        String,
        String,
        bool,
        String,
        Vec<(String, String, String)>,
    ) {
        self.runtime.lock().await.asr_menu_state()
    }

    /// Select, persist, and queue reload for a configured ASR provider.
    #[zbus(name = "SetActiveAsrProvider")]
    async fn set_active_asr_provider(&self, provider_id: &str) -> Result<bool, VinputDbusError> {
        let (config_source, config_path) = {
            let runtime = self.runtime.lock().await;
            (
                runtime.asr_reload_config_source(),
                runtime.config_path_for_persistence(),
            )
        };
        let provider_id = provider_id.to_owned();
        let (config, persisted) = tokio::task::spawn_blocking(move || {
            let config = config_source.load()?;
            let config = select_asr_provider(config, &provider_id).map_err(RuntimeError::Asr)?;
            if let Some(path) = config_path {
                persist_config_atomically(&path, &config, "asr")?;
                Ok((config, true))
            } else {
                Ok((config, false))
            }
        })
        .await
        .map_err(|error| Self::operation_failed(format!("ASR selection task failed: {error}")))?
        .map_err(|error: RuntimeError| Self::map_runtime_error(&error))?;
        self.queue_asr_reload_config(config).await?;
        Ok(persisted)
    }

    /// Return target/effective ASR state and configured provider/model rows.
    #[zbus(
        name = "GetAsrTargetMenuState",
        out_args(
            "target_provider_id",
            "target_model_id",
            "effective_provider_id",
            "effective_model_id",
            "reload_in_progress",
            "last_error",
            "targets"
        )
    )]
    async fn get_asr_target_menu_state(
        &self,
    ) -> Result<
        (
            String,
            String,
            String,
            String,
            bool,
            String,
            Vec<(String, String, String, String)>,
        ),
        VinputDbusError,
    > {
        let model_root = self.runtime.lock().await.model_root();
        let installed_models = tokio::task::spawn_blocking(move || {
            model_root.map_or_else(
                || Ok(Vec::new()),
                |root| scan_installed_models(&root).map_err(RuntimeError::InstalledModels),
            )
        })
        .await
        .map_err(|error| {
            Self::operation_failed(format!("installed model scan task failed: {error}"))
        })?
        .map_err(|error| Self::map_runtime_error(&error))?;
        Ok(self
            .runtime
            .lock()
            .await
            .asr_target_menu_state(&installed_models))
    }

    /// Return target/effective ASR state and localized provider/model rows.
    #[zbus(
        name = "GetAsrDisplayMenuState",
        out_args(
            "target_provider_id",
            "target_model_id",
            "effective_provider_id",
            "effective_model_id",
            "reload_in_progress",
            "last_error",
            "targets"
        )
    )]
    async fn get_asr_display_menu_state(
        &self,
    ) -> Result<
        (
            String,
            String,
            String,
            String,
            bool,
            String,
            Vec<(String, String, String, String, String)>,
        ),
        VinputDbusError,
    > {
        let model_root = self.runtime.lock().await.model_root();
        let installed_models = tokio::task::spawn_blocking(move || {
            model_root.map_or_else(
                || Ok(Vec::new()),
                |root| scan_installed_models(&root).map_err(RuntimeError::InstalledModels),
            )
        })
        .await
        .map_err(|error| {
            Self::operation_failed(format!("installed model scan task failed: {error}"))
        })?
        .map_err(|error| Self::map_runtime_error(&error))?;
        let locale_candidates = locale_candidates_from_environment();
        Ok(self
            .runtime
            .lock()
            .await
            .asr_display_menu_state(&installed_models, &locale_candidates))
    }

    /// Select, persist, and queue reload for a configured ASR provider/model target.
    #[zbus(name = "SetActiveAsrTarget")]
    async fn set_active_asr_target(
        &self,
        provider_id: &str,
        model_value: &str,
    ) -> Result<bool, VinputDbusError> {
        let (config_source, config_path, model_root) = {
            let runtime = self.runtime.lock().await;
            (
                runtime.asr_reload_config_source(),
                runtime.config_path_for_persistence(),
                runtime.model_root(),
            )
        };
        let provider_id = provider_id.to_owned();
        let model_value = model_value.to_owned();
        let (config, persisted) = tokio::task::spawn_blocking(move || {
            let installed_models = model_root.map_or_else(
                || Ok(Vec::new()),
                |root| scan_installed_models(&root).map_err(RuntimeError::InstalledModels),
            )?;
            let config = config_source.load()?;
            let Some(provider) = config
                .asr
                .providers
                .iter()
                .find(|provider| provider.id == provider_id)
            else {
                return Err(RuntimeError::Asr(vinput_asr::AsrError::UnknownProvider(
                    provider_id,
                )));
            };
            let configured_model_matches = provider.model.as_deref() == Some(model_value.as_str());
            let installed_model_matches = provider.kind == vinput_config::AsrProviderKind::Local
                && installed_models
                    .iter()
                    .any(|model| model.config_model_value() == model_value);
            if !model_value.is_empty() && !configured_model_matches && !installed_model_matches {
                return Err(RuntimeError::UnknownAsrTarget {
                    provider: provider_id,
                    model: model_value,
                });
            }
            let config = select_asr_target(config, &provider_id, Some(&model_value))
                .map_err(RuntimeError::Asr)?;
            if let Some(path) = config_path {
                persist_config_atomically(&path, &config, "asr-target")?;
                Ok((config, true))
            } else {
                Ok((config, false))
            }
        })
        .await
        .map_err(|error| {
            Self::operation_failed(format!("ASR target selection task failed: {error}"))
        })?
        .map_err(|error: RuntimeError| Self::map_runtime_error(&error))?;
        self.queue_asr_reload_config(config).await?;
        Ok(persisted)
    }

    /// Reload ASR backend using the legacy void method signature.
    #[zbus(name = "ReloadAsrBackend")]
    async fn reload_asr_backend(&self) -> Result<(), VinputDbusError> {
        let config_source = self.runtime.lock().await.asr_reload_config_source();
        let config = tokio::task::spawn_blocking(move || config_source.load())
            .await
            .map_err(|error| Self::operation_failed(format!("ASR reload task failed: {error}")))?
            .map_err(|error| Self::map_runtime_error(&error))?;
        self.queue_asr_reload_config(config).await?;
        Ok(())
    }

    /// Start a configured adapter using the runtime supervisor.
    #[zbus(name = "StartAdapter")]
    async fn start_adapter(&self, adapter_id: &str) -> Result<(), VinputDbusError> {
        let mut runtime = self.runtime.lock().await;
        runtime
            .start_text_adapter(adapter_id)
            .map_err(|error| Self::map_runtime_error(&error))?;
        Ok(())
    }

    /// Stop a configured adapter using the runtime supervisor.
    #[zbus(name = "StopAdapter")]
    async fn stop_adapter(&self, adapter_id: &str) -> Result<(), VinputDbusError> {
        let mut runtime = self.runtime.lock().await;
        runtime
            .stop_text_adapter(adapter_id)
            .map_err(|error| Self::map_runtime_error(&error))?;
        Ok(())
    }

    /// Signal emitted when a final recognition result is ready.
    #[zbus(signal, name = "RecognitionResult")]
    async fn recognition_result(
        signal_emitter: &zbus::object_server::SignalEmitter<'_>,
        payload_json: &str,
    ) -> zbus::Result<()>;

    /// Signal emitted for streaming partial recognition text.
    #[zbus(signal, name = "RecognitionPartial")]
    async fn recognition_partial(
        signal_emitter: &zbus::object_server::SignalEmitter<'_>,
        text: &str,
    ) -> zbus::Result<()>;

    /// Signal emitted when daemon status changes.
    #[zbus(signal, name = "StatusChanged")]
    async fn status_changed(
        signal_emitter: &zbus::object_server::SignalEmitter<'_>,
        status: &str,
    ) -> zbus::Result<()>;

    /// Signal emitted for daemon-originated notifications.
    #[zbus(signal, name = "DaemonNotification")]
    async fn daemon_notification(
        signal_emitter: &zbus::object_server::SignalEmitter<'_>,
        code: &str,
        subject: &str,
        detail: &str,
        raw_message: &str,
    ) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::{AsrBackendStateTuple, LivePartialEmissionState, VinputDbusService};
    use crate::RuntimeState;
    use tokio::time::{Duration, sleep, timeout};
    use vinput_asr::MockAsrBackend;
    use vinput_config::{AsrProviderConfig, AsrProviderKind, LlmAdapterConfig, VinputConfig};
    use vinput_protocol::{RecognitionPayload, TextAdapterState};

    fn service() -> VinputDbusService {
        let config = VinputConfig::bundled_default().unwrap();
        VinputDbusService::new(RuntimeState::new(config).unwrap())
    }

    async fn wait_for_asr_reload(service: &VinputDbusService) -> AsrBackendStateTuple {
        timeout(Duration::from_secs(2), async {
            loop {
                let state = service.get_asr_backend_state().await;
                if !state.5 {
                    return state;
                }
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("ASR reload should finish")
    }

    fn unique_adapter_runtime_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "vinput-daemon-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ))
    }

    fn reserve_remote_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .expect("reserve remote lifecycle port")
            .local_addr()
            .expect("read reserved remote lifecycle address")
            .port()
    }

    fn remote_lifecycle_config(port: u16) -> VinputConfig {
        VinputConfig::from_json_str(
            &serde_json::to_string(&serde_json::json!({
                "version":1,
                "asr":{
                    "active_provider":"provider.vinput.remote.streaming",
                    "providers":[{
                        "id":"provider.vinput.remote.streaming",
                        "type":"command",
                        "command":"python3",
                        "args":["remote.py"],
                        "env":{
                            "VINPUT_ASR_API_KEY":"fixture-key",
                            "VINPUT_ASR_PORT":port.to_string(),
                            "VINPUT_ASR_DEBOUNCE_MS":"25"
                        }
                    }]
                },
                "scenes":{
                    "active_scene":"raw",
                    "definitions":[{"id":"raw","label":"Raw","candidate_count":0}]
                }
            }))
            .expect("serialize remote lifecycle config"),
        )
        .expect("parse remote lifecycle config")
    }

    async fn remote_health_is_ready(port: u16) -> bool {
        reqwest::Client::builder()
            .timeout(Duration::from_millis(150))
            .build()
            .expect("build remote lifecycle health client")
            .get(format!("http://127.0.0.1:{port}/health"))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
    }

    #[tokio::test]
    async fn dbus_facade_exercises_normal_mock_flow() {
        let service = service();
        assert_eq!(service.get_status().await, "idle");
        assert_eq!(
            service.start_recording_state().await.unwrap().0,
            "recording"
        );
        let payload =
            RecognitionPayload::from_json_str(&service.stop_recording_payload("").await.unwrap().0)
                .unwrap();
        assert_eq!(payload.commit_text, "mock recognition result");
        assert_eq!(service.get_status().await, "idle");
    }

    #[tokio::test]
    async fn dbus_facade_reconciles_remote_service_on_config_reload() {
        let first_port = reserve_remote_port();
        let mut second_port = reserve_remote_port();
        while second_port == first_port {
            second_port = reserve_remote_port();
        }
        let root = unique_adapter_runtime_dir("remote-reload");
        std::fs::create_dir_all(&root).expect("create remote reload test directory");
        let config_path = root.join("config.json");
        let first_config = remote_lifecycle_config(first_port);
        std::fs::write(
            &config_path,
            serde_json::to_string_pretty(&first_config).expect("serialize first remote config"),
        )
        .expect("write first remote config");
        let mut runtime = RuntimeState::new(first_config).expect("create remote runtime");
        runtime.set_config_path(Some(config_path.clone()));
        let service = VinputDbusService::new_with_remote_bind(
            runtime,
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        );

        assert!(service.start_remote_text_service().await.unwrap());
        assert!(remote_health_is_ready(first_port).await);
        assert_eq!(
            service
                .remote_text_status()
                .await
                .local_addr
                .unwrap()
                .port(),
            first_port
        );

        let second_config = remote_lifecycle_config(second_port);
        std::fs::write(
            &config_path,
            serde_json::to_string_pretty(&second_config).expect("serialize second remote config"),
        )
        .expect("write second remote config");
        service.reload_asr_backend().await.unwrap();
        assert!(!remote_health_is_ready(first_port).await);
        assert!(remote_health_is_ready(second_port).await);
        assert_eq!(
            service
                .remote_text_status()
                .await
                .local_addr
                .unwrap()
                .port(),
            second_port
        );

        let disabled = VinputConfig::bundled_default().expect("parse bundled config");
        std::fs::write(
            &config_path,
            serde_json::to_string_pretty(&disabled).expect("serialize disabled remote config"),
        )
        .expect("write disabled remote config");
        service.reload_asr_backend().await.unwrap();
        assert!(!service.remote_text_status().await.running);
        assert!(!remote_health_is_ready(second_port).await);
        assert!(!service.shutdown_remote_text_service().await.unwrap());
        std::fs::remove_dir_all(root).expect("remove remote reload test directory");
    }

    #[tokio::test]
    async fn dbus_facade_remote_bind_failure_drops_stale_listener() {
        let first_port = reserve_remote_port();
        let occupied =
            std::net::TcpListener::bind("127.0.0.1:0").expect("occupy remote reload port");
        let occupied_port = occupied.local_addr().unwrap().port();
        let root = unique_adapter_runtime_dir("remote-bind-failure");
        std::fs::create_dir_all(&root).expect("create remote bind failure directory");
        let config_path = root.join("config.json");
        let first_config = remote_lifecycle_config(first_port);
        std::fs::write(
            &config_path,
            serde_json::to_string_pretty(&first_config).expect("serialize first remote config"),
        )
        .expect("write first remote config");
        let mut runtime = RuntimeState::new(first_config).expect("create remote runtime");
        runtime.set_config_path(Some(config_path.clone()));
        let service = VinputDbusService::new_with_remote_bind(
            runtime,
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        );

        service.start_remote_text_service().await.unwrap();
        assert!(remote_health_is_ready(first_port).await);
        let blocked_config = remote_lifecycle_config(occupied_port);
        std::fs::write(
            &config_path,
            serde_json::to_string_pretty(&blocked_config).expect("serialize blocked remote config"),
        )
        .expect("write blocked remote config");
        let error = service
            .reload_asr_backend()
            .await
            .expect_err("occupied port should reject remote reload");
        assert!(error.to_string().contains("bind remote text service"));
        assert!(!service.remote_text_status().await.running);
        assert!(!remote_health_is_ready(first_port).await);

        drop(occupied);
        std::fs::remove_dir_all(root).expect("remove remote bind failure directory");
    }

    #[tokio::test]
    async fn dbus_facade_provider_selection_starts_and_stops_remote_service() {
        let port = reserve_remote_port();
        let root = unique_adapter_runtime_dir("remote-provider-selection");
        std::fs::create_dir_all(&root).expect("create remote selection directory");
        let config_path = root.join("config.json");
        let mut config = remote_lifecycle_config(port);
        config.asr.providers.push(
            serde_json::from_value(serde_json::json!({
                "id":"mock",
                "type":"local",
                "model":"fixture-model"
            }))
            .expect("parse mock provider"),
        );
        "mock".clone_into(&mut config.asr.active_provider);
        std::fs::write(
            &config_path,
            serde_json::to_string_pretty(&config).expect("serialize selection config"),
        )
        .expect("write selection config");
        let mut runtime = RuntimeState::new(config).expect("create selection runtime");
        runtime.set_config_path(Some(config_path.clone()));
        let service = VinputDbusService::new_with_remote_bind(
            runtime,
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        );

        assert!(!service.start_remote_text_service().await.unwrap());
        assert!(!service.remote_text_status().await.running);
        assert!(
            service
                .set_active_asr_provider("provider.vinput.remote.streaming")
                .await
                .unwrap()
        );
        assert!(service.remote_text_status().await.running);
        assert!(remote_health_is_ready(port).await);

        assert!(service.set_active_asr_provider("mock").await.unwrap());
        assert!(!service.remote_text_status().await.running);
        assert!(!remote_health_is_ready(port).await);
        std::fs::remove_dir_all(root).expect("remove remote selection directory");
    }

    #[tokio::test]
    async fn dbus_facade_defers_reload_while_recording() {
        let service = service();

        assert_eq!(
            service.start_recording_state().await.unwrap().0,
            "recording"
        );
        service.reload_asr_backend().await.unwrap();

        assert_eq!(service.get_status().await, "recording");
        assert!(service.get_asr_backend_state().await.5);
        let payload =
            RecognitionPayload::from_json_str(&service.stop_recording_payload("").await.unwrap().0)
                .unwrap();
        assert_eq!(payload.commit_text, "mock recognition result");
        let state = wait_for_asr_reload(&service).await;
        assert_eq!(state.2, "mock");
        assert_eq!(state.3, "mock-streaming");
        assert!(state.4.contains("Failed to reload ASR backend"));
    }

    #[tokio::test]
    async fn dbus_facade_reload_rebuilds_configured_backend() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.active_provider = "mock".to_owned();
        config.asr.providers.push(AsrProviderConfig {
            id: "mock".to_owned(),
            kind: AsrProviderKind::Local,
            timeout_ms: None,
            model: None,
            hotwords_file: None,
            command: None,
            args: Vec::new(),
            env: std::collections::HashMap::new(),
            endpoint: None,
        });
        let runtime = RuntimeState::with_asr_backend(
            config,
            Box::new(MockAsrBackend::buffered("injected final")),
        )
        .unwrap();
        let service = VinputDbusService::new(runtime);

        assert_eq!(service.get_asr_backend_state().await.3, "mock-buffered");
        service.reload_asr_backend().await.unwrap();
        let state = wait_for_asr_reload(&service).await;
        assert_eq!(state.0, "mock");
        assert_eq!(state.2, "mock");
        assert_eq!(state.3, "mock-streaming");
        assert!(!state.5);
        assert!(state.4.is_empty());
    }

    #[tokio::test]
    async fn dbus_facade_preserves_early_final_events() {
        let config = VinputConfig::bundled_default().unwrap();
        let runtime = RuntimeState::with_asr_backend(
            config,
            Box::new(MockAsrBackend::streaming_with_early_final(
                "early partial",
                "early final",
            )),
        )
        .unwrap();
        let service = VinputDbusService::new(runtime);

        assert_eq!(
            service.start_recording_state().await.unwrap().0,
            "recording"
        );
        let (payload_json, status, partial_text) =
            service.stop_recording_payload("").await.unwrap();
        let payload = RecognitionPayload::from_json_str(&payload_json).unwrap();

        assert_eq!(payload.commit_text, "early final");
        assert_eq!(partial_text.as_deref(), Some("early partial"));
        assert_eq!(status, "idle");
    }

    #[tokio::test]
    async fn dbus_facade_exercises_timeout_mock_flow() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.scenes.active_scene = "timeout-scene".to_owned();
        config
            .scenes
            .definitions
            .push(vinput_config::SceneDefinition {
                id: "timeout-scene".to_owned(),
                label: "Timeout scene".to_owned(),
                prompt: None,
                provider_id: None,
                model: None,
                candidate_count: 0,
                timeout_ms: Some(2500),
                context_lines: 0,
            });
        let service = VinputDbusService::new(RuntimeState::new(config).unwrap());

        assert_eq!(
            service.start_recording_state().await.unwrap().0,
            "recording"
        );
        let payload =
            RecognitionPayload::from_json_str(&service.stop_recording_payload("").await.unwrap().0)
                .unwrap();
        assert_eq!(
            payload.commit_text,
            "mock postprocess result: mock recognition result"
        );
    }

    #[tokio::test]
    async fn dbus_facade_exercises_command_mock_flow() {
        let service = service();
        assert_eq!(
            service
                .start_command_recording_state("selected text")
                .await
                .unwrap()
                .0,
            "recording"
        );
        let payload =
            RecognitionPayload::from_json_str(&service.stop_recording_payload("").await.unwrap().0)
                .unwrap();
        assert_eq!(
            payload.commit_text,
            "mock command result for: selected text"
        );
    }

    #[tokio::test]
    async fn dbus_facade_handles_legacy_command_asr_stdout() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.active_provider = "cmd".to_owned();
        config.asr.providers.push(AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(1_000),
            model: Some("cmd-model".to_owned()),
            hotwords_file: None,
            command: Some("sh".to_owned()),
            args: vec![
                "-c".to_owned(),
                r"cat >/dev/null; printf '%s
' 'dbus final'"
                    .to_owned(),
            ],
            env: std::collections::HashMap::new(),
            endpoint: None,
        });
        let service = VinputDbusService::new(RuntimeState::with_configured_asr(config).unwrap());

        assert_eq!(
            service.start_recording_state().await.unwrap().0,
            "recording"
        );
        let (payload_json, status, partial_text) =
            service.stop_recording_payload("").await.unwrap();
        let payload = RecognitionPayload::from_json_str(&payload_json).unwrap();

        assert_eq!(payload.commit_text, "dbus final");
        assert_eq!(status, "idle");
        assert!(partial_text.is_none());
    }

    #[tokio::test]
    async fn dbus_facade_uses_configured_text_adapter() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.active_provider = "mock".to_owned();
        config.asr.providers.push(AsrProviderConfig {
            id: "mock".to_owned(),
            kind: AsrProviderKind::Local,
            timeout_ms: None,
            model: None,
            hotwords_file: None,
            command: None,
            args: Vec::new(),
            env: std::collections::HashMap::new(),
            endpoint: None,
        });
        config.scenes.active_scene = "needs-adapter".to_owned();
        config
            .scenes
            .definitions
            .push(vinput_config::SceneDefinition {
                id: "needs-adapter".to_owned(),
                label: "Needs adapter".to_owned(),
                prompt: Some("polish".to_owned()),
                provider_id: None,
                model: None,
                candidate_count: 1,
                timeout_ms: None,
                context_lines: 0,
            });
        config.llm.adapters.push(LlmAdapterConfig {
            id: "cmd-adapter".to_owned(),
            command: "sh".to_owned(),
            args: vec![
                "-c".to_owned(),
                r#"cat >/dev/null; printf '%s
' '{"text":"dbus configured final"}'"#
                    .to_owned(),
            ],
            env: std::collections::HashMap::default(),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        });
        let service =
            VinputDbusService::new(RuntimeState::with_configured_backends(config).unwrap());

        assert_eq!(
            service.start_recording_state().await.unwrap().0,
            "recording"
        );
        let payload =
            RecognitionPayload::from_json_str(&service.stop_recording_payload("").await.unwrap().0)
                .unwrap();
        assert_eq!(payload.commit_text, "dbus configured final");
    }

    #[tokio::test]
    async fn dbus_facade_uses_running_remote_service_endpoints() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.active_provider = "remote".to_owned();
        config.asr.providers.push(AsrProviderConfig {
            id: "remote".to_owned(),
            kind: AsrProviderKind::Remote,
            timeout_ms: None,
            model: Some("cloud".to_owned()),
            hotwords_file: None,
            command: None,
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            endpoint: Some("https://asr.example.test".to_owned()),
        });
        let service = VinputDbusService::new(RuntimeState::new(config).unwrap());

        let state = service.get_asr_backend_state().await;
        assert_eq!(state.0, "remote");
        assert_eq!(state.1, "cloud");
        assert_eq!(state.2, "mock");
        assert_eq!(state.3, "mock-streaming");
        assert!(state.6);
        assert!(state.7.is_empty());
    }

    #[tokio::test]
    async fn dbus_facade_preserves_command_asr_metadata() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.active_provider = "cmd".to_owned();
        config.asr.providers.push(AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(1_500),
            model: Some("cmd-model".to_owned()),
            hotwords_file: Some("/tmp/hotwords.txt".to_owned()),
            command: Some("helper".to_owned()),
            args: vec!["--json".to_owned()],
            env: std::collections::HashMap::default(),
            endpoint: None,
        });
        let service = VinputDbusService::new(RuntimeState::new(config).unwrap());

        let state = service.get_asr_backend_state().await;
        assert!(state.6);
        assert_eq!(state.0, "cmd");
        assert_eq!(state.1, "cmd-model");
        assert_eq!(state.2, "mock");
        assert_eq!(state.3, "mock-streaming");
    }

    #[tokio::test]
    async fn dbus_facade_supervises_configured_adapter() {
        let service = service();
        let start_error = service
            .start_adapter("mock-adapter")
            .await
            .expect_err("unconfigured adapter start should fail");
        assert!(
            start_error
                .to_string()
                .contains("text adapter `mock-adapter` is not configured")
        );
        let stop_error = service
            .stop_adapter("mock-adapter")
            .await
            .expect_err("unconfigured adapter stop should fail");
        assert!(
            stop_error
                .to_string()
                .contains("text adapter `mock-adapter` is not configured")
        );

        let runtime_dir = unique_adapter_runtime_dir("dbus-supervisor");
        let pid_path = runtime_dir.join("mock-adapter.pid");
        let mut config = VinputConfig::bundled_default().unwrap();
        config.llm.adapters.push(LlmAdapterConfig {
            id: "mock-adapter".to_owned(),
            command: "sleep".to_owned(),
            args: vec!["30".to_owned()],
            env: std::collections::HashMap::default(),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        });
        let runtime = RuntimeState::new(config)
            .unwrap()
            .with_adapter_runtime_paths(vinput_text::AdapterRuntimePaths::new(runtime_dir.clone()));
        let service = VinputDbusService::new(runtime);

        service.start_adapter("mock-adapter").await.unwrap();
        assert!(pid_path.exists());
        let duplicate_error = service
            .start_adapter("mock-adapter")
            .await
            .expect_err("duplicate adapter start should fail");
        assert!(
            duplicate_error
                .to_string()
                .contains("text adapter `mock-adapter` is already running")
        );
        service.stop_adapter("mock-adapter").await.unwrap();
        assert!(!pid_path.exists());
        service.stop_adapter("mock-adapter").await.unwrap();
        let _ = std::fs::remove_dir_all(runtime_dir);
    }

    #[tokio::test]
    async fn dbus_facade_returns_text_adapter_state_json() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.llm.adapters.push(LlmAdapterConfig {
            id: "mock-adapter".to_owned(),
            command: "vinput-postprocess".to_owned(),
            args: vec!["--json".to_owned()],
            env: std::collections::HashMap::from([("TOKEN".to_owned(), "secret".to_owned())]),
            working_dir: Some("/tmp/adapter-work".to_owned()),
            extra: std::collections::HashMap::default(),
        });
        let service = VinputDbusService::new(RuntimeState::new(config).unwrap());
        let state_json = service.get_text_adapter_state().await.unwrap();
        let state: TextAdapterState = serde_json::from_str(&state_json).unwrap();
        assert!(!state_json.contains("TOKEN"));
        assert!(!state_json.contains("secret"));
        assert!(!state_json.contains("/tmp/adapter-work"));

        assert_eq!(state.adapter_count, 1);
        assert_eq!(state.adapter_ids, ["mock-adapter"]);
        assert_eq!(state.single_adapter_id.as_deref(), Some("mock-adapter"));
        assert_eq!(state.adapters[0].kind, "command");
        assert_eq!(state.adapters[0].args_count, 1);
        assert_eq!(state.adapters[0].env_count, 1);
        assert!(state.adapters[0].has_working_dir);
    }

    #[tokio::test]
    async fn dbus_facade_returns_asr_state_tuple() {
        let service = service();
        let state = service.get_asr_backend_state().await;
        assert!(state.6);
        assert_eq!(state.0, "sherpa-onnx");
        assert_eq!(state.2, "mock");
        assert_eq!(state.3, "mock-streaming");
        assert!(state.4.is_empty());
    }

    #[tokio::test]
    async fn dbus_facade_lists_and_selects_scenes() {
        let service = service();
        let state = service.get_scene_state().await;
        assert_eq!(state.0, vinput_config::RAW_SCENE_ID);
        assert_eq!(
            state.1,
            [
                (vinput_config::RAW_SCENE_ID.to_owned(), "Raw".to_owned()),
                (
                    vinput_config::COMMAND_SCENE_ID.to_owned(),
                    "Command".to_owned()
                ),
            ]
        );

        assert!(
            !service
                .set_active_scene(vinput_config::COMMAND_SCENE_ID)
                .await
                .unwrap()
        );
        assert_eq!(
            service.get_scene_state().await.0,
            vinput_config::COMMAND_SCENE_ID
        );
        assert!(service.set_active_scene("missing").await.is_err());
    }

    #[tokio::test]
    async fn dbus_facade_lists_and_selects_asr_providers() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.providers.push(AsrProviderConfig {
            id: "mock".to_owned(),
            kind: AsrProviderKind::Local,
            timeout_ms: None,
            model: Some("mock-model".to_owned()),
            hotwords_file: None,
            command: None,
            args: Vec::new(),
            env: std::collections::HashMap::new(),
            endpoint: None,
        });
        let runtime = RuntimeState::with_asr_backend(
            config,
            Box::new(MockAsrBackend::buffered("injected final")),
        )
        .unwrap();
        let service = VinputDbusService::new(runtime);

        let before = service.get_asr_menu_state().await;
        assert_eq!(before.0, "sherpa-onnx");
        assert_eq!(before.1, "mock");
        assert_eq!(before.2, "mock-buffered");
        assert_eq!(before.5[1].0, "mock");

        assert!(!service.set_active_asr_provider("mock").await.unwrap());
        let after = wait_for_asr_reload(&service).await;
        assert_eq!(after.0, "mock");
        assert_eq!(after.2, "mock");
        assert_eq!(after.3, "mock-streaming");
        assert!(after.4.is_empty());
        assert!(service.set_active_asr_provider("missing").await.is_err());
    }

    #[test]
    fn live_partial_generations_cancel_stale_pollers_and_return_last_emission() {
        let mut state = LivePartialEmissionState::default();
        let first = state.begin(Some("first".to_owned()));
        assert!(state.is_current(first));
        assert_eq!(state.last_emitted.as_deref(), Some("first"));

        assert_eq!(state.cancel().as_deref(), Some("first"));
        assert!(!state.is_current(first));
        assert!(state.last_emitted.is_none());

        let second = state.begin(None);
        assert!(state.is_current(second));
        assert_ne!(first, second);
    }

    #[tokio::test]
    async fn recording_operations_wait_for_the_prior_transaction() {
        let service = service();
        let first = service.lock_recording_operation().await;
        let waiting_service = service.clone();
        let mut waiter = tokio::spawn(async move {
            let _second = waiting_service.lock_recording_operation().await;
        });

        assert!(
            timeout(Duration::from_millis(20), &mut waiter)
                .await
                .is_err()
        );
        drop(first);
        timeout(Duration::from_secs(1), waiter)
            .await
            .expect("recording transaction should resume after the prior operation")
            .expect("recording transaction task should finish");
    }
}

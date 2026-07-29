//! ASR backend reload, deferred reload, and background preparation handling.

use std::path::PathBuf;

use vinput_asr::{AsrBackend, AsrBackendFactory};
use vinput_config::VinputConfig;
use vinput_protocol::{AsrBackendState, ServiceStatus};

use super::{RuntimeError, RuntimeState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PendingAsrReload {
    MetadataOnly,
    ConfiguredBackend,
}

/// Reload-time config source captured without holding the runtime mutex.
#[derive(Debug, Clone)]
pub(crate) enum AsrReloadConfigSource {
    /// Reload the daemon's explicit config file.
    File(PathBuf),
    /// Reuse the in-memory config when no file was supplied at startup.
    Snapshot(Box<VinputConfig>),
}

impl AsrReloadConfigSource {
    /// Loads and validates the config used by a background ASR reload.
    pub(crate) fn load(self) -> Result<VinputConfig, RuntimeError> {
        let config = match self {
            Self::File(path) => {
                VinputConfig::from_json_file(path).map_err(RuntimeError::InvalidConfig)?
            }
            Self::Snapshot(config) => *config,
        };
        config.validate().map_err(RuntimeError::InvalidConfig)?;
        Ok(config)
    }
}

/// One configured ASR reload selected for background preparation.
pub(crate) struct AsrReloadRequest {
    generation: u64,
    config: VinputConfig,
}

/// Backend and config values prepared outside the runtime mutex.
pub(crate) struct PreparedAsrReload {
    generation: u64,
    backend: Box<dyn AsrBackend>,
    config: VinputConfig,
}

/// Next action for the single D-Bus ASR reload worker.
pub(crate) enum AsrReloadWorkerStep {
    /// Keep the worker alive until the runtime becomes idle.
    Wait,
    /// Prepare the selected config outside the runtime mutex.
    Prepare(Box<AsrReloadRequest>),
    /// No queued work remains; the worker may exit.
    Stop,
}

impl AsrReloadRequest {
    /// Builds and warms the selected backend without holding runtime state.
    pub(crate) fn prepare(self) -> Result<PreparedAsrReload, RuntimeError> {
        let backend = AsrBackendFactory::build_active_prepared(
            &self.config.asr,
            Some(self.config.global.default_language.clone()),
        )
        .map_err(RuntimeError::Asr)?;
        Ok(PreparedAsrReload {
            generation: self.generation,
            backend,
            config: self.config,
        })
    }

    /// Returns this request's generation for failure reporting.
    pub(crate) const fn generation(&self) -> u64 {
        self.generation
    }
}

impl RuntimeState {
    /// Records the config file that D-Bus ASR reloads should read again.
    pub fn set_config_path(&mut self, path: Option<PathBuf>) {
        self.config_path = path;
    }

    /// Configures the installed-model root exposed to frontend menus.
    pub fn set_model_root(&mut self, model_root: Option<PathBuf>) {
        self.model_root = model_root;
    }

    /// Returns the installed-model root without performing file I/O.
    pub(crate) fn model_root(&self) -> Option<PathBuf> {
        self.model_root.clone()
    }

    /// Captures the reload config source without performing file I/O.
    /// Returns the explicit daemon config path, if any.
    pub(crate) fn config_path_for_persistence(&self) -> Option<PathBuf> {
        self.config_path.clone()
    }

    /// Captures the validated in-memory config without performing file I/O.
    pub(crate) fn config_snapshot(&self) -> VinputConfig {
        self.config.clone()
    }

    pub(crate) fn asr_reload_config_source(&self) -> AsrReloadConfigSource {
        self.config_path.as_ref().map_or_else(
            || AsrReloadConfigSource::Snapshot(Box::new(self.config.clone())),
            |path| AsrReloadConfigSource::File(path.clone()),
        )
    }

    /// Queues a validated config for the single background reload worker.
    ///
    /// Returns whether the caller must spawn the worker task.
    pub(crate) fn queue_configured_asr_reload(&mut self, config: VinputConfig) -> bool {
        self.asr_reload_generation = self.asr_reload_generation.wrapping_add(1);
        let generation = self.asr_reload_generation;

        self.config.asr.clone_from(&config.asr);
        self.config
            .global
            .default_language
            .clone_from(&config.global.default_language);
        self.pending_asr_reload = Some(PendingAsrReload::ConfiguredBackend);
        self.pending_asr_reload_config = Some((generation, config));
        self.asr_reload_last_error = None;

        if self.asr_reload_worker_running {
            false
        } else {
            self.asr_reload_worker_running = true;
            true
        }
    }

    /// Selects the next background reload action.
    pub(crate) fn next_asr_reload_worker_step(&mut self) -> AsrReloadWorkerStep {
        if !self.asr_reload_worker_running {
            return AsrReloadWorkerStep::Stop;
        }
        if self.status != ServiceStatus::Idle || self.asr_reload_preparing {
            return AsrReloadWorkerStep::Wait;
        }

        match self.pending_asr_reload {
            Some(PendingAsrReload::ConfiguredBackend) => {
                let Some((generation, config)) = self.pending_asr_reload_config.take() else {
                    self.pending_asr_reload = None;
                    self.asr_reload_worker_running = false;
                    return AsrReloadWorkerStep::Stop;
                };
                self.pending_asr_reload = None;
                self.asr_reload_preparing = true;
                AsrReloadWorkerStep::Prepare(Box::new(AsrReloadRequest { generation, config }))
            }
            Some(PendingAsrReload::MetadataOnly) => {
                self.pending_asr_reload = None;
                let _ = self.reload_asr_backend_now();
                self.asr_reload_worker_running = false;
                AsrReloadWorkerStep::Stop
            }
            None => {
                self.asr_reload_worker_running = false;
                AsrReloadWorkerStep::Stop
            }
        }
    }

    /// Returns whether a prepared candidate may be swapped now.
    pub(crate) fn can_apply_prepared_asr_reload(&self) -> bool {
        self.status == ServiceStatus::Idle
    }

    /// Applies a prepared candidate if it is still the newest request.
    pub(crate) fn complete_prepared_asr_reload(&mut self, prepared: PreparedAsrReload) {
        self.asr_reload_preparing = false;
        if prepared.generation != self.asr_reload_generation {
            return;
        }
        self.asr_backend = prepared.backend;
        self.config.asr = prepared.config.asr;
        self.config.global.default_language = prepared.config.global.default_language;
        self.asr_reload_last_error = None;
    }

    /// Records a background preparation failure if the request is still current.
    pub(crate) fn fail_prepared_asr_reload(
        &mut self,
        generation: u64,
        error: &RuntimeError,
    ) -> Option<String> {
        self.asr_reload_preparing = false;
        if generation != self.asr_reload_generation {
            return None;
        }
        let message = format!("Failed to reload ASR backend. {error}");
        self.asr_reload_last_error = Some(message.clone());
        Some(message)
    }

    /// Reloads the ASR backend state after validating config.
    ///
    /// The prototype keeps the injected runtime backend, but the returned
    /// state includes the config-selected target provider, model, and remote
    /// endpoint metadata.
    pub fn reload_asr_backend(&mut self) -> Result<AsrBackendState, RuntimeError> {
        if self.status != ServiceStatus::Idle {
            return Ok(self.defer_asr_backend_reload(PendingAsrReload::MetadataOnly));
        }
        self.reload_asr_backend_now()
    }

    /// Rebuilds the runtime ASR backend from the validated active provider.
    pub fn reload_configured_asr_backend(&mut self) -> Result<AsrBackendState, RuntimeError> {
        if self.status != ServiceStatus::Idle {
            return Ok(self.defer_asr_backend_reload(PendingAsrReload::ConfiguredBackend));
        }
        self.reload_configured_asr_backend_now()
    }

    fn defer_asr_backend_reload(&mut self, pending: PendingAsrReload) -> AsrBackendState {
        self.pending_asr_reload = Some(pending);
        self.asr_backend_state()
    }

    fn reload_asr_backend_now(&mut self) -> Result<AsrBackendState, RuntimeError> {
        self.config
            .validate()
            .map_err(RuntimeError::InvalidConfig)?;
        self.asr_reload_last_error = None;
        Ok(self.asr_backend_state())
    }

    fn reload_configured_asr_backend_now(&mut self) -> Result<AsrBackendState, RuntimeError> {
        self.config
            .validate()
            .map_err(RuntimeError::InvalidConfig)?;
        match AsrBackendFactory::build_active_prepared(
            &self.config.asr,
            Some(self.config.global.default_language.clone()),
        ) {
            Ok(backend) => {
                self.asr_backend = backend;
                self.asr_reload_last_error = None;
                Ok(self.asr_backend_state())
            }
            Err(error) => {
                let error = RuntimeError::Asr(error);
                self.asr_reload_last_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    pub(super) fn apply_pending_asr_backend_reload(&mut self) {
        if self.status != ServiceStatus::Idle || self.asr_reload_worker_running {
            return;
        }
        let Some(pending) = self.pending_asr_reload.take() else {
            return;
        };

        let result = match pending {
            PendingAsrReload::MetadataOnly => self.reload_asr_backend_now(),
            PendingAsrReload::ConfiguredBackend => self.reload_configured_asr_backend_now(),
        };
        if let Err(error) = result {
            self.asr_reload_last_error = Some(format!(
                "Failed to apply deferred ASR backend reload. {error}"
            ));
        }
    }
}

//! Supervised command text adapter process lifecycle.

use vinput_text::{
    AdapterProcessSpec, AdapterRuntimePaths, AdapterStopOutcome, start_adapter_process,
    stop_adapter_process, stop_started_adapter_process,
};

use super::{RuntimeError, RuntimeState};

impl RuntimeState {
    /// Stops supervised adapters whose definitions will disappear or change.
    ///
    /// The current config remains published when any safe stop fails, so the
    /// process stays diagnosable and can still be targeted by `StopAdapter`.
    pub(super) fn stop_reconfigured_text_adapters(
        &mut self,
        next_config: &vinput_config::VinputConfig,
    ) -> Result<(), RuntimeError> {
        let stale_adapter_ids = self
            .config
            .llm
            .adapters
            .iter()
            .filter(|current| !next_config.llm.adapters.iter().any(|next| next == *current))
            .map(|adapter| adapter.id.clone())
            .collect::<Vec<_>>();

        for adapter_id in stale_adapter_ids {
            if let Some(mut process) = self.adapter_processes.remove(&adapter_id) {
                if let Err(error) =
                    stop_started_adapter_process(&mut process, &self.adapter_runtime_paths)
                {
                    self.adapter_processes.insert(adapter_id, process);
                    return Err(RuntimeError::TextAdapterSupervisor(error));
                }
            } else {
                stop_adapter_process(&adapter_id, &self.adapter_runtime_paths)
                    .map_err(RuntimeError::TextAdapterSupervisor)?;
            }
        }
        Ok(())
    }

    /// Overrides adapter runtime paths for tests or embedded callers.
    #[must_use]
    pub fn with_adapter_runtime_paths(mut self, paths: AdapterRuntimePaths) -> Self {
        self.adapter_runtime_paths = paths;
        self
    }

    /// Reaps supervised text adapters that have already exited.
    pub fn refresh_text_adapters(&mut self) -> Vec<String> {
        let exited_adapter_ids: Vec<_> = self
            .adapter_processes
            .iter_mut()
            .filter_map(
                |(adapter_id, process)| match process.try_wait_and_cleanup() {
                    Ok(Some(_status)) => Some(adapter_id.clone()),
                    Ok(None) | Err(_) => None,
                },
            )
            .collect();
        for adapter_id in &exited_adapter_ids {
            self.adapter_processes.remove(adapter_id);
            let _ = self.adapter_runtime_paths.remove_pid(adapter_id);
        }
        exited_adapter_ids
    }

    /// Starts a configured command text adapter process.
    pub fn start_text_adapter(&mut self, adapter_id: &str) -> Result<u32, RuntimeError> {
        if self.adapter_processes.contains_key(adapter_id) {
            return Err(RuntimeError::TextAdapterAlreadyRunning(
                adapter_id.to_owned(),
            ));
        }
        let adapter = self
            .config
            .llm
            .adapters
            .iter()
            .find(|adapter| adapter.id == adapter_id)
            .ok_or_else(|| RuntimeError::TextAdapterNotConfigured(adapter_id.to_owned()))?;
        let spec = AdapterProcessSpec::from_config(adapter);
        let process = start_adapter_process(&spec, &self.adapter_runtime_paths).map_err(
            |error| match error {
                vinput_text::TextError::AdapterAlreadyRunning(adapter_id) => {
                    RuntimeError::TextAdapterAlreadyRunning(adapter_id)
                }
                error => RuntimeError::TextAdapterSupervisor(error),
            },
        )?;
        let pid = process.pid;
        self.adapter_processes
            .insert(adapter_id.to_owned(), process);
        Ok(pid)
    }

    /// Stops a configured command text adapter process.
    pub fn stop_text_adapter(
        &mut self,
        adapter_id: &str,
    ) -> Result<AdapterStopOutcome, RuntimeError> {
        if !self
            .configured_text_adapters()
            .contains_command_adapter(adapter_id)
        {
            return Err(RuntimeError::TextAdapterNotConfigured(
                adapter_id.to_owned(),
            ));
        }
        if let Some(mut process) = self.adapter_processes.remove(adapter_id) {
            return stop_started_adapter_process(&mut process, &self.adapter_runtime_paths)
                .map_err(RuntimeError::TextAdapterSupervisor);
        }
        stop_adapter_process(adapter_id, &self.adapter_runtime_paths)
            .map_err(RuntimeError::TextAdapterSupervisor)
    }
}

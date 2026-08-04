//! Supervised command text adapter process lifecycle.

use vinput_config::{
    LlmAdapterConfig, MANAGED_SCRIPT_REVISION_KEY, MANAGED_SCRIPT_ROLLBACK_REVISION_KEY,
    VinputConfig,
};
use vinput_registry::managed_script_rollback_path;
use vinput_text::{
    AdapterProcessSpec, AdapterRuntimePaths, AdapterStopOutcome, StartedAdapterProcess, TextError,
    start_adapter_process, stop_adapter_process, stop_started_adapter_process,
};

use super::{RuntimeError, RuntimeState};

#[derive(Debug)]
struct AdapterRestartPlan {
    old_spec: AdapterProcessSpec,
    new_spec: Option<AdapterProcessSpec>,
}

impl RuntimeState {
    /// Reconciles supervised adapters whose definitions will disappear or change.
    ///
    /// Running adapters that remain configured are prestarted from the new
    /// definition before the config is published. Any stop or start failure
    /// rolls back the prior running set and leaves the old config visible.
    pub(super) fn reconcile_reconfigured_text_adapters(
        &mut self,
        next_config: &VinputConfig,
    ) -> Result<(), RuntimeError> {
        let plans = adapter_restart_plans(&self.config.llm.adapters, &next_config.llm.adapters);
        let mut stopped = Vec::new();

        for plan in plans {
            match self.stop_adapter_for_reload(&plan.old_spec.id) {
                Ok(AdapterStopOutcome::Stopped { .. }) => stopped.push(plan),
                Ok(AdapterStopOutcome::NotRunning) => {}
                Err(error) => {
                    let rollback_errors = self.restart_old_adapters(&stopped);
                    return Err(reconciliation_error(
                        "stop reconfigured adapters",
                        error,
                        &rollback_errors,
                    ));
                }
            }
        }

        let mut replacements = Vec::new();
        for spec in stopped.iter().filter_map(|plan| plan.new_spec.as_ref()) {
            match start_adapter_process(spec, &self.adapter_runtime_paths) {
                Ok(process) => replacements.push(process),
                Err(error) => {
                    let mut rollback_errors = self.stop_prestarted_adapters(&mut replacements);
                    rollback_errors.extend(self.restart_old_adapters(&stopped));
                    return Err(reconciliation_error(
                        "start reconfigured adapters",
                        error,
                        &rollback_errors,
                    ));
                }
            }
        }

        for process in replacements {
            self.adapter_processes.insert(process.id.clone(), process);
        }
        Ok(())
    }

    fn stop_adapter_for_reload(
        &mut self,
        adapter_id: &str,
    ) -> Result<AdapterStopOutcome, TextError> {
        if let Some(mut process) = self.adapter_processes.remove(adapter_id) {
            return match stop_started_adapter_process(&mut process, &self.adapter_runtime_paths) {
                Ok(outcome) => Ok(outcome),
                Err(error) => {
                    self.adapter_processes
                        .insert(adapter_id.to_owned(), process);
                    Err(error)
                }
            };
        }
        stop_adapter_process(adapter_id, &self.adapter_runtime_paths)
    }

    fn restart_old_adapters(&mut self, plans: &[AdapterRestartPlan]) -> Vec<String> {
        let mut errors = Vec::new();
        for plan in plans {
            match start_adapter_process(&plan.old_spec, &self.adapter_runtime_paths) {
                Ok(process) => {
                    self.adapter_processes.insert(process.id.clone(), process);
                }
                Err(error) => errors.push(format!("{}: {error}", plan.old_spec.id)),
            }
        }
        errors
    }

    fn stop_prestarted_adapters(
        &mut self,
        processes: &mut Vec<StartedAdapterProcess>,
    ) -> Vec<String> {
        let mut errors = Vec::new();
        for mut process in processes.drain(..) {
            let id = process.id.clone();
            if let Err(error) =
                stop_started_adapter_process(&mut process, &self.adapter_runtime_paths)
            {
                self.adapter_processes.insert(id.clone(), process);
                errors.push(format!("{id}: {error}"));
            }
        }
        errors
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

fn adapter_restart_plans(
    current: &[LlmAdapterConfig],
    next: &[LlmAdapterConfig],
) -> Vec<AdapterRestartPlan> {
    current
        .iter()
        .filter_map(|current_adapter| {
            let next_adapter = next
                .iter()
                .find(|candidate| candidate.id == current_adapter.id);
            (next_adapter != Some(current_adapter)).then(|| AdapterRestartPlan {
                old_spec: adapter_rollback_spec(current_adapter, next_adapter),
                new_spec: next_adapter.map(AdapterProcessSpec::from_config),
            })
        })
        .collect()
}

fn adapter_rollback_spec(
    current: &LlmAdapterConfig,
    next: Option<&LlmAdapterConfig>,
) -> AdapterProcessSpec {
    let mut spec = AdapterProcessSpec::from_config(current);
    let Some(next) = next else {
        return spec;
    };
    let current_revision = current
        .extra
        .get(MANAGED_SCRIPT_REVISION_KEY)
        .and_then(serde_json::Value::as_str);
    let rollback_revision = next
        .extra
        .get(MANAGED_SCRIPT_ROLLBACK_REVISION_KEY)
        .and_then(serde_json::Value::as_str);
    let revision_matches = rollback_revision.is_some()
        && (current_revision.is_none() || rollback_revision == current_revision);
    if revision_matches && current.args.len() == 1 && next.args == current.args {
        spec.args[0] = managed_script_rollback_path(&current.args[0])
            .to_string_lossy()
            .into_owned();
    }
    spec
}

fn reconciliation_error(
    action: &str,
    primary: TextError,
    rollback_errors: &[String],
) -> RuntimeError {
    if rollback_errors.is_empty() {
        return RuntimeError::TextAdapterSupervisor(primary);
    }
    RuntimeError::TextAdapterSupervisor(TextError::AdapterRuntimeIo(format!(
        "{action} failed: {primary}; rollback failures: {}",
        rollback_errors.join("; ")
    )))
}

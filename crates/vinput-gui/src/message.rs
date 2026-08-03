//! Typed messages handled by the Rust management GUI.

use std::path::PathBuf;

use crate::{
    ConfigSaveOutcome, DaemonSnapshot, ModelInstallOutcome, Page, ScriptInstallOutcome,
    ScriptPreparationResult, SecretInput,
};

/// GUI messages.
#[derive(Debug, Clone)]
pub enum Message {
    /// Select a main page.
    SelectPage(Page),
    /// Update the current resource filter.
    FilterChanged(String),
    /// Refresh daemon state over D-Bus.
    RefreshDaemon,
    /// Result of an asynchronous daemon refresh.
    DaemonLoaded(Result<DaemonSnapshot, String>),
    /// Periodic non-activating daemon-owner poll.
    DaemonPollTick,
    /// Result of a periodic non-activating daemon-owner poll.
    DaemonPolled(Result<Option<DaemonSnapshot>, String>),
    /// Reload config from disk.
    ReloadConfig,
    /// Update the default recognition language draft.
    DefaultLanguageChanged(String),
    /// Update the capture target draft.
    CaptureDeviceChanged(String),
    /// Toggle output ducking in the draft.
    DuckOutputChanged(bool),
    /// Update the output ducking volume in the draft.
    DuckVolumeChanged(f32),
    /// Toggle VAD in the draft.
    VadEnabledChanged(bool),
    /// Update the VAD threshold in the draft.
    VadThresholdChanged(f32),
    /// Select the active ASR provider in the draft.
    ActiveProviderChanged(String),
    /// Select the active scene in the draft.
    ActiveSceneChanged(String),
    /// Restore editable fields from the loaded config.
    ResetConfigDraft,
    /// Validate, back up, and atomically save the config draft.
    SaveConfig,
    /// Result of an asynchronous config save.
    ConfigSaved(Result<ConfigSaveOutcome, String>),
    /// Start normal recording over D-Bus.
    StartRecording,
    /// Stop recording over D-Bus.
    StopRecording,
    /// Result of an asynchronous recording action.
    RecordingActionFinished(Result<String, String>),
    /// Update the live registry model id or short id to install.
    ModelSelectorChanged(String),
    /// Install or update the selected live registry model.
    InstallModel,
    /// Request cancellation of the active model installation.
    CancelModelInstall,
    /// Retry the last failed or cancelled model installation.
    RetryModelInstall,
    /// Refresh progress from the active model installation worker.
    ModelInstallProgressTick,
    /// Result of a live registry model installation.
    ModelInstalled {
        /// Operation generation used to reject stale completions.
        operation_id: u64,
        /// Typed worker outcome.
        outcome: ModelInstallOutcome,
    },
    /// Remove one inactive installed model directory.
    RemoveInstalledModel(PathBuf),
    /// Result of an installed model removal.
    ModelRemoved(Result<String, String>),
    /// Update the live registry ASR provider id or short id to install.
    ProviderSelectorChanged(String),
    /// Update the live registry text adapter id or short id to install.
    AdapterSelectorChanged(String),
    /// Install or update the selected command ASR provider.
    InstallProvider,
    /// Install or update the selected text adapter.
    InstallAdapter,
    /// Result of resolving one provider or adapter registry entry.
    ScriptPrepared {
        /// Operation generation used to reject stale completions.
        operation_id: u64,
        /// Typed registry-resolution outcome.
        outcome: ScriptPreparationResult,
    },
    /// Update one registry-declared environment value before installation.
    ScriptEnvironmentChanged {
        /// Stable environment variable name from the registry.
        name: String,
        /// User-entered value with redacted debug output.
        value: SecretInput,
    },
    /// Confirm the prepared provider or adapter environment and install it.
    ConfirmScriptInstall,
    /// Request cancellation of the active provider or adapter installation.
    CancelScriptInstall,
    /// Retry the last failed or cancelled provider or adapter installation.
    RetryScriptInstall,
    /// Refresh progress from the active provider or adapter worker.
    ScriptInstallProgressTick,
    /// Result of a live provider or adapter installation.
    ScriptInstalled {
        /// Operation generation used to reject stale completions.
        operation_id: u64,
        /// Typed worker outcome.
        outcome: ScriptInstallOutcome,
    },
    /// Remove one inactive managed command ASR provider.
    RemoveProvider(String),
    /// Remove one managed text adapter.
    RemoveAdapter(String),
    /// Result of a provider or adapter removal.
    ScriptRemoved(Result<String, String>),
}

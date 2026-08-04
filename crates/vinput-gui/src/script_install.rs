//! GUI state and task ownership for provider and adapter installation.

use std::{
    fmt,
    sync::{Arc, Mutex},
};

use iced::{
    Element, Length, Task,
    widget::{button, column, progress_bar, row, text, text_input},
};
use vinput_registry::{
    LiveScriptEntry, LiveScriptKind, RegistryOperationControl, RegistryOperationProgress,
};

use crate::{
    App, ConfigDocument, Message, OperationState, load_config_document,
    script_management::{
        install_registry_script_controlled, prepare_registry_script_controlled, resource_label,
    },
    script_recovery::recover_registry_script_config,
};

/// A user-entered value whose debug representation is always redacted.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretInput(String);

impl SecretInput {
    pub(crate) fn new(value: String) -> Self {
        Self(value)
    }

    pub(crate) fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Debug for SecretInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

/// One registry-declared environment value collected before installation.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ScriptEnvironmentValue {
    pub(crate) name: String,
    pub(crate) required: bool,
    pub(crate) value: String,
}

impl fmt::Debug for ScriptEnvironmentValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScriptEnvironmentValue")
            .field("name", &self.name)
            .field("required", &self.required)
            .field("value", &"<redacted>")
            .finish()
    }
}

/// A resolved provider or adapter installation request.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ScriptInstallPlan {
    pub(crate) kind: LiveScriptKind,
    pub(crate) selector: String,
    pub(crate) entry: LiveScriptEntry,
    pub(crate) script_root: std::path::PathBuf,
    pub(crate) script_path: std::path::PathBuf,
    pub(crate) environment: Vec<ScriptEnvironmentValue>,
}

impl fmt::Debug for ScriptInstallPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScriptInstallPlan")
            .field("kind", &self.kind)
            .field("selector", &self.selector)
            .field("entry_id", &self.entry.id)
            .field("script_root", &self.script_root)
            .field("script_path", &self.script_path)
            .field("environment", &self.environment)
            .finish()
    }
}

impl ScriptInstallPlan {
    pub(crate) fn missing_required_environment(&self) -> Option<&str> {
        self.environment
            .iter()
            .find(|value| value.required && value.value.trim().is_empty())
            .map(|value| value.name.as_str())
    }

    fn set_environment(&mut self, name: &str, value: String) {
        if let Some(environment) = self
            .environment
            .iter_mut()
            .find(|environment| environment.name == name)
        {
            environment.value = value;
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let mut body = column![
            text(format!(
                "Configure {} `{}` before installation",
                resource_label(self.kind),
                self.entry.id
            ))
            .size(18),
            text("Values are stored in the user configuration and hidden in diagnostics."),
        ]
        .spacing(8);

        for environment in &self.environment {
            let name = environment.name.clone();
            let requirement = if environment.required {
                "required"
            } else {
                "optional"
            };
            body = body.push(
                column![
                    text(format!("{} ({requirement})", environment.name)),
                    text_input("Enter environment value", &environment.value)
                        .secure(true)
                        .on_input(move |value| Message::ScriptEnvironmentChanged {
                            name: name.clone(),
                            value: SecretInput::new(value),
                        })
                        .width(Length::Fill),
                ]
                .spacing(4),
            );
        }

        let can_install = self.missing_required_environment().is_none();
        body.push(
            row![
                button("Install or update")
                    .on_press_maybe(can_install.then_some(Message::ConfirmScriptInstall)),
                button("Cancel").on_press(Message::CancelScriptInstall),
            ]
            .spacing(10),
        )
        .into()
    }
}

/// Result of resolving a provider or adapter registry entry before installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScriptPrepareOutcome {
    Prepared(Box<ScriptInstallPlan>),
    Cancelled,
    Failed(String),
}

/// Opaque, debug-safe result carried by the public GUI message type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptPreparationResult(ScriptPrepareOutcome);

impl ScriptPreparationResult {
    fn new(outcome: ScriptPrepareOutcome) -> Self {
        Self(outcome)
    }

    pub(crate) fn into_inner(self) -> ScriptPrepareOutcome {
        self.0
    }
}

/// Final typed outcome of a GUI provider or adapter installation worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptInstallOutcome {
    /// The script and its validated configuration entry were installed.
    Installed(String),
    /// The user or application shutdown requested cancellation.
    Cancelled,
    /// The script was published, but its configuration entry could not be committed.
    PublishedButConfigFailed {
        /// Config mutation or persistence error without environment values.
        error: String,
    },
    /// The operation failed.
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScriptRetryRequest {
    Prepare {
        kind: LiveScriptKind,
        selector: String,
    },
    Install(Box<ScriptInstallPlan>),
}

#[derive(Debug, Default)]
pub(crate) enum ScriptInstallState {
    #[default]
    Idle,
    Preparing(ActiveScriptPreparation),
    AwaitingEnvironment(Box<ScriptInstallPlan>),
    Active(Box<ActiveScriptInstall>),
    Recovering(Box<ActiveScriptRecovery>),
    RecoveryRequired {
        plan: Box<ScriptInstallPlan>,
        error: String,
    },
    Succeeded(String),
    Cancelled {
        retry: ScriptRetryRequest,
    },
    Failed {
        retry: ScriptRetryRequest,
        error: String,
    },
}

#[derive(Debug)]
pub(crate) struct ActiveScriptPreparation {
    operation_id: u64,
    kind: LiveScriptKind,
    selector: String,
    control: RegistryOperationControl,
    cancelling: bool,
}

impl Drop for ActiveScriptPreparation {
    fn drop(&mut self) {
        self.control.cancel();
    }
}

#[derive(Debug)]
pub(crate) struct ActiveScriptInstall {
    operation_id: u64,
    plan: ScriptInstallPlan,
    control: RegistryOperationControl,
    shared_progress: Arc<Mutex<RegistryOperationProgress>>,
    progress: RegistryOperationProgress,
    cancelling: bool,
}

#[derive(Debug)]
pub(crate) struct ActiveScriptRecovery {
    operation_id: u64,
    plan: ScriptInstallPlan,
}

impl Drop for ActiveScriptInstall {
    fn drop(&mut self) {
        self.control.cancel();
    }
}

impl ScriptInstallState {
    pub(crate) fn has_worker(&self) -> bool {
        matches!(
            self,
            Self::Preparing(_) | Self::Active(_) | Self::Recovering(_)
        )
    }

    pub(crate) fn blocks_operations(&self) -> bool {
        matches!(
            self,
            Self::Preparing(_)
                | Self::AwaitingEnvironment(_)
                | Self::Active(_)
                | Self::Recovering(_)
                | Self::RecoveryRequired { .. }
        )
    }

    pub(crate) fn start_preparation(
        document: ConfigDocument,
        kind: LiveScriptKind,
        selector: String,
        operation_id: u64,
    ) -> (Self, Task<Message>) {
        let control = RegistryOperationControl::default();
        let worker_control = control.clone();
        let worker_selector = selector.clone();
        let task = Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    prepare_registry_script_controlled(
                        &document,
                        kind,
                        &worker_selector,
                        &worker_control,
                    )
                })
                .await
                .unwrap_or_else(|_| {
                    ScriptPrepareOutcome::Failed(
                        "Script preparation worker stopped unexpectedly.".to_owned(),
                    )
                })
            },
            move |outcome| Message::ScriptPrepared {
                operation_id,
                outcome: ScriptPreparationResult::new(outcome),
            },
        );
        (
            Self::Preparing(ActiveScriptPreparation {
                operation_id,
                kind,
                selector,
                control,
                cancelling: false,
            }),
            task,
        )
    }

    pub(crate) fn start_install(
        document: ConfigDocument,
        plan: ScriptInstallPlan,
        operation_id: u64,
    ) -> (Self, Task<Message>) {
        let initial_progress = RegistryOperationProgress::Preparing;
        let shared_progress = Arc::new(Mutex::new(initial_progress.clone()));
        let reported = Arc::clone(&shared_progress);
        let control = RegistryOperationControl::new(move |progress| {
            *reported
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = progress;
        });
        let worker_control = control.clone();
        let worker_plan = plan.clone();
        let task = Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    install_registry_script_controlled(&document, &worker_plan, &worker_control)
                })
                .await
                .unwrap_or_else(|_| {
                    ScriptInstallOutcome::Failed(
                        "Script installation worker stopped unexpectedly.".to_owned(),
                    )
                })
            },
            move |outcome| Message::ScriptInstalled {
                operation_id,
                outcome,
            },
        );
        (
            Self::Active(Box::new(ActiveScriptInstall {
                operation_id,
                plan,
                control,
                shared_progress,
                progress: initial_progress,
                cancelling: false,
            })),
            task,
        )
    }

    pub(crate) fn start_recovery(
        document: ConfigDocument,
        plan: ScriptInstallPlan,
        operation_id: u64,
    ) -> (Self, Task<Message>) {
        let worker_plan = plan.clone();
        let task = Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    recover_registry_script_config(&document, &worker_plan)
                })
                .await
                .unwrap_or_else(|_| {
                    ScriptInstallOutcome::PublishedButConfigFailed {
                        error: "Configuration recovery worker stopped unexpectedly.".to_owned(),
                    }
                })
            },
            move |outcome| Message::ScriptInstalled {
                operation_id,
                outcome,
            },
        );
        (
            Self::Recovering(Box::new(ActiveScriptRecovery { operation_id, plan })),
            task,
        )
    }

    pub(crate) fn cancel(&mut self) {
        match self {
            Self::Preparing(active) => {
                active.control.cancel();
                active.cancelling = true;
            }
            Self::AwaitingEnvironment(plan) => {
                let retry = ScriptRetryRequest::Install(plan.clone());
                *self = Self::Cancelled { retry };
            }
            Self::Active(active) => {
                active.control.cancel();
                active.cancelling = true;
            }
            Self::Idle
            | Self::Recovering(_)
            | Self::RecoveryRequired { .. }
            | Self::Succeeded(_)
            | Self::Cancelled { .. }
            | Self::Failed { .. } => {}
        }
    }

    pub(crate) fn refresh_progress(&mut self) {
        if let Self::Active(active) = self {
            active.progress = active
                .shared_progress
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
        }
    }

    pub(crate) fn finish_preparation(
        &mut self,
        operation_id: u64,
        outcome: ScriptPrepareOutcome,
    ) -> bool {
        let (kind, selector) = match self {
            Self::Preparing(active) if active.operation_id == operation_id => {
                (active.kind, active.selector.clone())
            }
            _ => return false,
        };
        *self = match outcome {
            ScriptPrepareOutcome::Prepared(plan) => Self::AwaitingEnvironment(plan),
            ScriptPrepareOutcome::Cancelled => Self::Cancelled {
                retry: ScriptRetryRequest::Prepare { kind, selector },
            },
            ScriptPrepareOutcome::Failed(error) => Self::Failed {
                retry: ScriptRetryRequest::Prepare { kind, selector },
                error,
            },
        };
        true
    }

    pub(crate) fn take_plan_without_environment(&mut self) -> Option<ScriptInstallPlan> {
        let Self::AwaitingEnvironment(plan) = self else {
            return None;
        };
        if !plan.environment.is_empty() {
            return None;
        }
        let plan = (**plan).clone();
        *self = Self::Idle;
        Some(plan)
    }

    pub(crate) fn update_environment(&mut self, name: &str, value: String) {
        if let Self::AwaitingEnvironment(plan) = self {
            plan.set_environment(name, value);
        }
    }

    pub(crate) fn confirmed_plan(&self) -> Result<Option<ScriptInstallPlan>, String> {
        let Self::AwaitingEnvironment(plan) = self else {
            return Ok(None);
        };
        if let Some(name) = plan.missing_required_environment() {
            return Err(format!(
                "Enter a value for required environment variable `{name}` before installing."
            ));
        }
        Ok(Some((**plan).clone()))
    }

    pub(crate) fn finish_install(
        &mut self,
        operation_id: u64,
        outcome: ScriptInstallOutcome,
    ) -> bool {
        let (plan, recovering) = match self {
            Self::Active(active) if active.operation_id == operation_id => {
                (active.plan.clone(), false)
            }
            Self::Recovering(active) if active.operation_id == operation_id => {
                (active.plan.clone(), true)
            }
            _ => return false,
        };
        *self = match outcome {
            ScriptInstallOutcome::Installed(summary) => Self::Succeeded(summary),
            ScriptInstallOutcome::PublishedButConfigFailed { error } => Self::RecoveryRequired {
                plan: Box::new(plan),
                error,
            },
            ScriptInstallOutcome::Cancelled if recovering => Self::RecoveryRequired {
                plan: Box::new(plan),
                error: "Configuration recovery was cancelled before completion.".to_owned(),
            },
            ScriptInstallOutcome::Failed(error) if recovering => Self::RecoveryRequired {
                plan: Box::new(plan),
                error,
            },
            ScriptInstallOutcome::Cancelled => Self::Cancelled {
                retry: ScriptRetryRequest::Install(Box::new(plan)),
            },
            ScriptInstallOutcome::Failed(error) => Self::Failed {
                retry: ScriptRetryRequest::Install(Box::new(plan)),
                error,
            },
        };
        true
    }

    pub(crate) fn recovery_plan(&self) -> Option<ScriptInstallPlan> {
        match self {
            Self::RecoveryRequired { plan, .. } => Some((**plan).clone()),
            _ => None,
        }
    }

    pub(crate) fn dismiss_recovery(&mut self) {
        if matches!(self, Self::RecoveryRequired { .. }) {
            *self = Self::Idle;
        }
    }

    pub(crate) fn set_recovery_error(&mut self, message: String) {
        if let Self::RecoveryRequired { error, .. } = self {
            *error = message;
        }
    }

    fn retry_request(&self) -> Option<ScriptRetryRequest> {
        match self {
            Self::Cancelled { retry } | Self::Failed { retry, .. } => Some(retry.clone()),
            Self::Idle
            | Self::Preparing(_)
            | Self::AwaitingEnvironment(_)
            | Self::Active(_)
            | Self::Recovering(_)
            | Self::RecoveryRequired { .. }
            | Self::Succeeded(_) => None,
        }
    }

    pub(crate) fn view(&self) -> Option<Element<'_, Message>> {
        match self {
            Self::Idle => None,
            Self::Preparing(active) => Some(active.view()),
            Self::AwaitingEnvironment(plan) => Some(plan.view()),
            Self::Active(active) => Some(active.view()),
            Self::Recovering(active) => Some(
                column![
                    text(format!(
                        "Retrying configuration for {} `{}`…",
                        resource_label(active.plan.kind),
                        active.plan.entry.id
                    )),
                    text("The published script is being reused; no download is running."),
                ]
                .spacing(6)
                .into(),
            ),
            Self::RecoveryRequired { plan, error } => Some(
                column![
                    text("Script published; configuration incomplete").size(18),
                    text(format!(
                        "{} `{}` was published at {}.",
                        resource_label(plan.kind),
                        plan.entry.id,
                        plan.script_path.display()
                    )),
                    text(format!("Configuration error: {error}")),
                    text(
                        "Reload after resolving external changes or permissions, then retry. The script will not be downloaded again; dismissing keeps the published file."
                    ),
                    row![
                        button("Reload config").on_press(Message::ReloadConfig),
                        button("Retry configuration update")
                            .on_press(Message::RetryScriptConfigUpdate),
                        button("Dismiss (keep script)")
                            .on_press(Message::DismissScriptRecovery),
                    ]
                    .spacing(10),
                ]
                .spacing(8)
                .into(),
            ),
            Self::Succeeded(summary) => Some(text(format!("Success: {summary}")).into()),
            Self::Cancelled { .. } => Some(
                row![
                    text("Script installation cancelled."),
                    button("Retry").on_press(Message::RetryScriptInstall),
                ]
                .spacing(10)
                .into(),
            ),
            Self::Failed { error, .. } => Some(
                column![
                    text(format!("Error: {error}")),
                    button("Retry").on_press(Message::RetryScriptInstall),
                ]
                .spacing(8)
                .into(),
            ),
        }
    }
}

impl App {
    pub(crate) fn begin_script_install(&mut self, kind: LiveScriptKind) -> Task<Message> {
        let selector = match kind {
            LiveScriptKind::AsrProvider => self.provider_selector.trim(),
            LiveScriptKind::LlmAdapter => self.adapter_selector.trim(),
        }
        .to_owned();
        self.begin_script_preparation_for(kind, selector)
    }

    pub(crate) fn retry_script_install(&mut self) -> Task<Message> {
        let Some(retry) = self.script_install.retry_request() else {
            return Task::none();
        };
        match retry {
            ScriptRetryRequest::Prepare { kind, selector } => {
                match kind {
                    LiveScriptKind::AsrProvider => self.provider_selector.clone_from(&selector),
                    LiveScriptKind::LlmAdapter => self.adapter_selector.clone_from(&selector),
                }
                self.begin_script_preparation_for(kind, selector)
            }
            ScriptRetryRequest::Install(plan) => self.begin_resolved_script_install(*plan),
        }
    }

    pub(crate) fn retry_script_config_update(&mut self) -> Task<Message> {
        let Some(plan) = self.script_install.recovery_plan() else {
            return Task::none();
        };
        if let Err(error) = self.ensure_no_unsaved_config_draft() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if let Err(error) = self.ensure_no_open_scene_editor() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        let Ok(document) = &self.config else {
            let error = self
                .config
                .as_ref()
                .err()
                .cloned()
                .unwrap_or_else(|| "No valid config is loaded.".to_owned());
            self.script_install
                .set_recovery_error(format!("Reloaded config is invalid: {error}"));
            return Task::none();
        };
        let document = document.clone();
        let operation_id = self.next_script_operation_id();
        let (state, task) = ScriptInstallState::start_recovery(document, plan, operation_id);
        self.operation = OperationState::Idle;
        self.script_install = state;
        task
    }

    pub(crate) fn dismiss_script_recovery(&mut self) {
        self.script_install.dismiss_recovery();
        self.operation = OperationState::Idle;
    }

    fn begin_script_preparation_for(
        &mut self,
        kind: LiveScriptKind,
        selector: String,
    ) -> Task<Message> {
        if selector.is_empty() {
            self.operation = OperationState::Failed(format!(
                "Enter a {} registry id or short id before installing.",
                resource_label(kind)
            ));
            return Task::none();
        }
        if let Err(error) = self.ensure_no_unsaved_config_draft() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if let Err(error) = self.ensure_no_open_scene_editor() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        let Ok(document) = &self.config else {
            self.operation = OperationState::Failed("No valid config is loaded.".to_owned());
            return Task::none();
        };
        let document = document.clone();
        if self.is_busy() {
            return Task::none();
        }
        let operation_id = self.next_script_operation_id();
        let (state, task) =
            ScriptInstallState::start_preparation(document, kind, selector, operation_id);
        self.operation = OperationState::Idle;
        self.script_install = state;
        task
    }

    pub(crate) fn finish_script_preparation(
        &mut self,
        operation_id: u64,
        outcome: ScriptPrepareOutcome,
    ) -> Task<Message> {
        if !self
            .script_install
            .finish_preparation(operation_id, outcome)
        {
            return Task::none();
        }
        let Some(plan) = self.script_install.take_plan_without_environment() else {
            return Task::none();
        };
        self.begin_resolved_script_install(plan)
    }

    pub(crate) fn update_script_environment(&mut self, name: &str, value: SecretInput) {
        self.script_install
            .update_environment(name, value.into_inner());
    }

    pub(crate) fn confirm_script_install(&mut self) -> Task<Message> {
        match self.script_install.confirmed_plan() {
            Ok(Some(plan)) => self.begin_resolved_script_install(plan),
            Ok(None) => Task::none(),
            Err(error) => {
                self.operation = OperationState::Failed(error);
                Task::none()
            }
        }
    }

    fn begin_resolved_script_install(&mut self, plan: ScriptInstallPlan) -> Task<Message> {
        if let Err(error) = self.ensure_no_unsaved_config_draft() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if let Err(error) = self.ensure_no_open_scene_editor() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        let Ok(document) = &self.config else {
            self.operation = OperationState::Failed("No valid config is loaded.".to_owned());
            return Task::none();
        };
        let document = document.clone();
        if matches!(
            self.script_install,
            ScriptInstallState::Preparing(_)
                | ScriptInstallState::Active(_)
                | ScriptInstallState::Recovering(_)
        ) {
            return Task::none();
        }
        if let Some(name) = plan.missing_required_environment() {
            self.operation = OperationState::Failed(format!(
                "Enter a value for required environment variable `{name}` before installing."
            ));
            self.script_install = ScriptInstallState::AwaitingEnvironment(Box::new(plan));
            return Task::none();
        }
        let operation_id = self.next_script_operation_id();
        let (state, task) = ScriptInstallState::start_install(document, plan, operation_id);
        self.operation = OperationState::Idle;
        self.script_install = state;
        task
    }

    fn next_script_operation_id(&mut self) -> u64 {
        let operation_id = self.next_script_install_id;
        self.next_script_install_id = self.next_script_install_id.wrapping_add(1).max(1);
        operation_id
    }

    pub(crate) fn finish_script_install(
        &mut self,
        operation_id: u64,
        outcome: ScriptInstallOutcome,
    ) -> Task<Message> {
        if !self.script_install.finish_install(operation_id, outcome) {
            return Task::none();
        }
        if matches!(self.script_install, ScriptInstallState::Succeeded(_)) {
            let path = self
                .config
                .as_ref()
                .ok()
                .map(|document| document.path.clone());
            self.replace_config(load_config_document(path.as_deref()));
            return self.begin_daemon_refresh(false);
        }
        Task::none()
    }

    pub(crate) fn provider_install_controls(&self, busy: bool) -> Element<'_, Message> {
        let input = text_input("Registry provider id or short id", &self.provider_selector)
            .width(Length::Fill);
        let input = if busy {
            input
        } else {
            input.on_input(Message::ProviderSelectorChanged)
        };
        row![
            input,
            button("Install or update").on_press_maybe(
                (!busy && !self.provider_selector.trim().is_empty())
                    .then_some(Message::InstallProvider),
            ),
        ]
        .spacing(10)
        .into()
    }

    pub(crate) fn adapter_install_controls(&self, busy: bool) -> Element<'_, Message> {
        let input = text_input("Registry adapter id or short id", &self.adapter_selector)
            .width(Length::Fill);
        let input = if busy {
            input
        } else {
            input.on_input(Message::AdapterSelectorChanged)
        };
        row![
            input,
            button("Install or update").on_press_maybe(
                (!busy && !self.adapter_selector.trim().is_empty())
                    .then_some(Message::InstallAdapter),
            ),
        ]
        .spacing(10)
        .into()
    }
}

impl ActiveScriptPreparation {
    fn view(&self) -> Element<'_, Message> {
        row![
            text(format!(
                "Resolving {} catalog for `{}`…",
                resource_label(self.kind),
                self.selector
            )),
            button(if self.cancelling {
                "Cancelling…"
            } else {
                "Cancel"
            })
            .on_press_maybe((!self.cancelling).then_some(Message::CancelScriptInstall)),
        ]
        .spacing(10)
        .into()
    }
}

impl ActiveScriptInstall {
    fn view(&self) -> Element<'_, Message> {
        let mut body = column![text(progress_label(self.plan.kind, &self.progress))].spacing(8);
        if let RegistryOperationProgress::Downloading {
            downloaded_bytes,
            total_bytes: Some(total_bytes),
        } = self.progress
            && total_bytes > 0
        {
            let permille = downloaded_bytes
                .saturating_mul(1000)
                .checked_div(total_bytes)
                .unwrap_or(0)
                .min(1000);
            let permille = u16::try_from(permille).unwrap_or(1000);
            body = body.push(progress_bar(0.0..=1.0, f32::from(permille) / 1000.0));
        }
        let commit_started = matches!(
            self.progress,
            RegistryOperationProgress::UpdatingConfiguration | RegistryOperationProgress::Completed
        );
        body = body.push(
            button(if self.cancelling {
                "Cancelling…"
            } else if commit_started {
                "Finishing…"
            } else {
                "Cancel"
            })
            .on_press_maybe(
                (!self.cancelling && !commit_started).then_some(Message::CancelScriptInstall),
            ),
        );
        body.into()
    }
}

fn progress_label(kind: LiveScriptKind, progress: &RegistryOperationProgress) -> String {
    let label = resource_label(kind);
    match progress {
        RegistryOperationProgress::Preparing => format!("Preparing {label} installation…"),
        RegistryOperationProgress::ResolvingRegistry => format!("Resolving {label} catalog…"),
        RegistryOperationProgress::Downloading {
            downloaded_bytes,
            total_bytes,
        } => total_bytes.map_or_else(
            || format!("Downloading {label} script… {downloaded_bytes} bytes received"),
            |total| format!("Downloading {label} script… {downloaded_bytes} of {total} bytes"),
        ),
        RegistryOperationProgress::VerifyingChecksum => {
            format!("Verifying {label} script…")
        }
        RegistryOperationProgress::Extracting { .. } => {
            format!("Extracting {label} resources…")
        }
        RegistryOperationProgress::WritingMetadata => {
            format!("Writing {label} metadata…")
        }
        RegistryOperationProgress::Publishing => format!("Publishing {label} script…"),
        RegistryOperationProgress::UpdatingConfiguration => {
            format!("Updating configuration for {label}…")
        }
        RegistryOperationProgress::Completed => format!("{label} installation completed."),
    }
}

#[cfg(test)]
#[path = "script_install_guard_tests.rs"]
mod guard_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(environment: Vec<ScriptEnvironmentValue>) -> ScriptInstallPlan {
        ScriptInstallPlan {
            kind: LiveScriptKind::AsrProvider,
            selector: "fixture".to_owned(),
            entry: LiveScriptEntry {
                id: "provider.fixture.batch".to_owned(),
                short_id: Some("fixture".to_owned()),
                stream: false,
                command: "python3".to_owned(),
                script_urls: vec!["https://example.invalid/provider.py".to_owned()],
                readme_url: None,
                envs: Vec::new(),
            },
            script_root: "/tmp/providers".into(),
            script_path: "/tmp/providers/fixture/batch".into(),
            environment,
        }
    }

    #[test]
    fn stale_preparation_does_not_replace_active_operation() {
        let document = ConfigDocument {
            path: "/tmp/config.json".into(),
            from_disk: false,
            config: vinput_config::VinputConfig::bundled_default().expect("bundled config"),
        };
        let (mut state, _) = ScriptInstallState::start_preparation(
            document,
            LiveScriptKind::AsrProvider,
            "fixture".to_owned(),
            12,
        );

        assert!(!state.finish_preparation(11, ScriptPrepareOutcome::Cancelled));
        assert!(state.has_worker());
        assert!(state.finish_preparation(12, ScriptPrepareOutcome::Cancelled));
        assert!(matches!(
            state.retry_request(),
            Some(ScriptRetryRequest::Prepare { .. })
        ));
    }

    #[test]
    fn required_environment_blocks_confirmation_until_entered() {
        let mut state =
            ScriptInstallState::AwaitingEnvironment(Box::new(plan(vec![ScriptEnvironmentValue {
                name: "TOKEN".to_owned(),
                required: true,
                value: String::new(),
            }])));

        assert!(state.confirmed_plan().is_err());
        state.update_environment("TOKEN", "super-secret".to_owned());
        let confirmed = state
            .confirmed_plan()
            .expect("valid environment")
            .expect("pending plan");
        assert_eq!(confirmed.environment[0].value, "super-secret");
    }

    #[test]
    fn failed_install_retry_preserves_environment_without_debug_exposure() {
        let document = ConfigDocument {
            path: "/tmp/config.json".into(),
            from_disk: false,
            config: vinput_config::VinputConfig::bundled_default().expect("bundled config"),
        };
        let install_plan = plan(vec![ScriptEnvironmentValue {
            name: "TOKEN".to_owned(),
            required: true,
            value: "super-secret".to_owned(),
        }]);
        let (mut state, _) = ScriptInstallState::start_install(document, install_plan, 13);

        assert!(state.finish_install(
            13,
            ScriptInstallOutcome::Failed("fixture failure".to_owned())
        ));
        let debug = format!("{state:?}");
        assert!(!debug.contains("super-secret"));
        assert!(debug.contains("TOKEN"));
        assert!(matches!(
            state.retry_request(),
            Some(ScriptRetryRequest::Install(_))
        ));
    }

    #[test]
    fn stale_install_completion_does_not_replace_active_operation() {
        let document = ConfigDocument {
            path: "/tmp/config.json".into(),
            from_disk: false,
            config: vinput_config::VinputConfig::bundled_default().expect("bundled config"),
        };
        let (mut state, _) = ScriptInstallState::start_install(document, plan(Vec::new()), 15);

        assert!(!state.finish_install(14, ScriptInstallOutcome::Cancelled));
        assert!(state.has_worker());
        assert!(state.finish_install(15, ScriptInstallOutcome::Cancelled));
        assert!(matches!(
            state.retry_request(),
            Some(ScriptRetryRequest::Install(_))
        ));
    }

    #[test]
    fn published_script_failure_enters_redacted_recovery_state() {
        let document = ConfigDocument {
            path: "/tmp/config.json".into(),
            from_disk: false,
            config: vinput_config::VinputConfig::bundled_default().expect("bundled config"),
        };
        let install_plan = plan(vec![ScriptEnvironmentValue {
            name: "TOKEN".to_owned(),
            required: true,
            value: "super-secret".to_owned(),
        }]);
        let (mut state, _) = ScriptInstallState::start_install(document, install_plan, 16);

        assert!(state.finish_install(
            16,
            ScriptInstallOutcome::PublishedButConfigFailed {
                error: "permission denied".to_owned(),
            }
        ));

        assert!(matches!(state, ScriptInstallState::RecoveryRequired { .. }));
        assert!(state.blocks_operations());
        assert!(state.recovery_plan().is_some());
        assert!(!format!("{state:?}").contains("super-secret"));
    }

    #[test]
    fn stale_recovery_completion_is_rejected_and_dismiss_clears_state() {
        let document = ConfigDocument {
            path: "/tmp/config.json".into(),
            from_disk: false,
            config: vinput_config::VinputConfig::bundled_default().expect("bundled config"),
        };
        let (mut state, _) = ScriptInstallState::start_recovery(document, plan(Vec::new()), 17);

        assert!(!state.finish_install(
            16,
            ScriptInstallOutcome::PublishedButConfigFailed {
                error: "stale".to_owned(),
            }
        ));
        assert!(state.has_worker());
        assert!(state.finish_install(
            17,
            ScriptInstallOutcome::PublishedButConfigFailed {
                error: "still blocked".to_owned(),
            }
        ));
        assert!(matches!(state, ScriptInstallState::RecoveryRequired { .. }));

        state.dismiss_recovery();

        assert!(matches!(state, ScriptInstallState::Idle));
    }

    #[test]
    fn environment_message_debug_never_exposes_entered_value() {
        let message = Message::ScriptEnvironmentChanged {
            name: "TOKEN".to_owned(),
            value: SecretInput::new("super-secret".to_owned()),
        };

        let debug = format!("{message:?}");
        assert!(debug.contains("TOKEN"));
        assert!(!debug.contains("super-secret"));
    }

    #[test]
    fn active_script_state_cancels_when_dropped() {
        let document = ConfigDocument {
            path: "/tmp/config.json".into(),
            from_disk: false,
            config: vinput_config::VinputConfig::bundled_default().expect("bundled config"),
        };
        let (state, _) = ScriptInstallState::start_install(document, plan(Vec::new()), 14);
        let control = match &state {
            ScriptInstallState::Active(active) => active.control.clone(),
            _ => panic!("active script install state"),
        };

        drop(state);

        assert!(control.is_cancelled());
    }

    #[test]
    fn secret_input_debug_is_redacted() {
        let input = SecretInput::new("super-secret".to_owned());
        assert_eq!(format!("{input:?}"), "<redacted>");
    }
}

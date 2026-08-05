//! GUI integration for editing exact managed command-provider scripts.

use std::path::Path;

use iced::Task;
use vinput_config::AsrProviderConfig;
use vinput_registry::{
    ProviderEditorCommand, ProviderScriptEditPlan, ProviderScriptResolutionContext,
    prepare_provider_script_edit_with,
};

use crate::{
    App, GuiLocale, GuiText, Message, OperationState,
    script_management::managed_provider_script_path,
};

impl App {
    pub(crate) fn begin_provider_script_edit(&mut self, provider_id: &str) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let provider = match &self.config {
            Ok(document) => document
                .config
                .asr
                .providers
                .iter()
                .find(|provider| provider.id == provider_id)
                .cloned(),
            Err(_) => None,
        };
        let Some(provider) = provider else {
            self.operation =
                OperationState::Failed(format!("ASR provider `{provider_id}` is not configured."));
            return Task::none();
        };
        let Some(managed_path) = managed_provider_script_path(&provider) else {
            self.operation = OperationState::Failed(format!(
                "ASR provider `{provider_id}` is not an exact managed command provider."
            ));
            return Task::none();
        };
        self.operation =
            OperationState::Running(self.locale.text(GuiText::EditingManagedProviderScript));
        let self_locale = self.locale;
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    edit_managed_provider_script(&provider, &managed_path, self_locale)
                })
                .await
                .unwrap_or_else(|_| {
                    Err("Provider script editor worker stopped unexpectedly.".to_owned())
                })
            },
            Message::ProviderScriptEdited,
        )
    }

    pub(crate) fn finish_provider_script_edit(&mut self, result: Result<String, String>) {
        self.operation = match result {
            Ok(summary) => OperationState::Succeeded(summary),
            Err(error) => OperationState::Failed(error),
        };
    }
}

fn edit_managed_provider_script(
    provider: &AsrProviderConfig,
    managed_path: &Path,
    locale: GuiLocale,
) -> Result<String, String> {
    let context =
        ProviderScriptResolutionContext::from_environment().map_err(|error| error.to_string())?;
    let editor =
        ProviderEditorCommand::from_environment(None).map_err(|error| error.to_string())?;
    let plan = prepare_managed_provider_script_edit_with(provider, managed_path, &context, editor)?;
    let outcome = plan.execute().map_err(|error| error.to_string())?;
    Ok(locale.provider_script_edited(
        &outcome.provider_id,
        &outcome.script_path.display().to_string(),
        &outcome.editor_argv.join(" "),
    ))
}

fn prepare_managed_provider_script_edit_with(
    provider: &AsrProviderConfig,
    managed_path: &Path,
    context: &ProviderScriptResolutionContext,
    editor: ProviderEditorCommand,
) -> Result<ProviderScriptEditPlan, String> {
    let plan = prepare_provider_script_edit_with(provider, context, editor)
        .map_err(|error| error.to_string())?;
    if plan.script_path != managed_path {
        return Err(format!(
            "Resolved provider script `{}` did not match exact managed path `{}`.",
            plan.script_path.display(),
            managed_path.display()
        ));
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use vinput_config::AsrProviderKind;

    fn provider(script: &Path) -> AsrProviderConfig {
        AsrProviderConfig {
            id: "provider.fixture.batch".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(60_000),
            model: None,
            hotwords_file: None,
            command: Some("python3".to_owned()),
            args: vec![script.display().to_string()],
            env: std::collections::HashMap::new(),
            endpoint: None,
        }
    }

    #[test]
    fn managed_edit_plan_requires_exact_resolved_path() {
        let directory = tempfile::tempdir().expect("temp dir");
        let script = directory.path().join("provider.py");
        let other = directory.path().join("other.py");
        fs::write(&script, "provider\n").expect("write script");
        fs::write(&other, "other\n").expect("write other");
        let context = ProviderScriptResolutionContext {
            current_dir: directory.path().to_path_buf(),
            home_dir: None,
        };

        let error = prepare_managed_provider_script_edit_with(
            &provider(&script),
            &other,
            &context,
            ProviderEditorCommand::parse("true").expect("editor"),
        )
        .expect_err("path mismatch should fail");

        assert!(error.contains("did not match exact managed path"));
    }

    #[test]
    fn managed_edit_plan_executes_selected_editor() {
        let directory = tempfile::tempdir().expect("temp dir");
        let script = directory.path().join("provider.py");
        let editor = directory.path().join("editor.sh");
        fs::write(&script, "provider\n").expect("write script");
        fs::write(&editor, "#!/bin/sh\nprintf '# gui edited\\n' >> \"$1\"\n")
            .expect("write editor");
        let context = ProviderScriptResolutionContext {
            current_dir: directory.path().to_path_buf(),
            home_dir: None,
        };
        let plan = prepare_managed_provider_script_edit_with(
            &provider(&script),
            &script,
            &context,
            ProviderEditorCommand::parse(&format!("sh {}", editor.display())).expect("editor"),
        )
        .expect("prepare edit");

        plan.execute().expect("execute editor");

        assert!(
            fs::read_to_string(script)
                .expect("read script")
                .contains("# gui edited")
        );
    }
}

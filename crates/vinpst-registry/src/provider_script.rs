//! Shared provider-script resolution and editor-launch boundary.

use std::{
    env, fs,
    path::PathBuf,
    process::{Command, ExitStatus},
};

use thiserror::Error;
use vinpst_config::{AsrProviderConfig, AsrProviderKind};

/// Deterministic filesystem context used to resolve provider script candidates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderScriptResolutionContext {
    /// Directory used to resolve relative command and argument paths.
    pub current_dir: PathBuf,
    /// Home directory used to expand `~` and `~/...` candidates.
    pub home_dir: Option<PathBuf>,
}

impl ProviderScriptResolutionContext {
    /// Captures the current process directory and optional `HOME` value.
    pub fn from_environment() -> Result<Self, ProviderScriptEditError> {
        let current_dir = env::current_dir()
            .map_err(|error| ProviderScriptEditError::CurrentDirectory(error.to_string()))?;
        Ok(Self {
            current_dir,
            home_dir: env::var_os("HOME").map(PathBuf::from),
        })
    }
}

/// Parsed editor command with direct argv execution and no shell evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderEditorCommand {
    argv: Vec<String>,
}

impl ProviderEditorCommand {
    /// Parses one whitespace-separated editor command.
    pub fn parse(command: &str) -> Result<Self, ProviderScriptEditError> {
        let argv = command
            .split_whitespace()
            .filter(|part| !part.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if argv.is_empty() {
            return Err(ProviderScriptEditError::EmptyEditor);
        }
        Ok(Self { argv })
    }

    /// Resolves the legacy editor priority from an explicit value and process environment.
    pub fn from_environment(explicit: Option<&str>) -> Result<Self, ProviderScriptEditError> {
        let provider_editor = env::var("VINPST_PROVIDER_EDITOR").ok();
        let visual = env::var("VISUAL").ok();
        let editor = env::var("EDITOR").ok();
        let command = select_editor_command(
            explicit,
            provider_editor.as_deref(),
            visual.as_deref(),
            editor.as_deref(),
        );
        Self::parse(&command)
    }

    /// Returns the direct process argv.
    #[must_use]
    pub fn argv(&self) -> &[String] {
        &self.argv
    }

    /// Returns a stable user-facing representation of the editor command.
    #[must_use]
    pub fn display(&self) -> String {
        self.argv.join(" ")
    }
}

/// Prepared provider-script edit that can be inspected without launching an editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderScriptEditPlan {
    /// Stable configured provider id.
    pub provider_id: String,
    /// Existing regular script file selected from command/arguments.
    pub script_path: PathBuf,
    /// Direct editor argv.
    pub editor: ProviderEditorCommand,
}

impl ProviderScriptEditPlan {
    /// Launches the editor, waits for it to exit, and requires a successful status.
    pub fn execute(&self) -> Result<ProviderScriptEditOutcome, ProviderScriptEditError> {
        let (program, args) = self
            .editor
            .argv
            .split_first()
            .ok_or(ProviderScriptEditError::EmptyEditor)?;
        let status = Command::new(program)
            .args(args)
            .arg(&self.script_path)
            .status()
            .map_err(|error| ProviderScriptEditError::LaunchEditor {
                editor: self.editor.display(),
                message: error.to_string(),
            })?;
        if !status.success() {
            return Err(ProviderScriptEditError::EditorFailed {
                editor: self.editor.display(),
                status: exit_status_label(status),
            });
        }
        Ok(ProviderScriptEditOutcome {
            provider_id: self.provider_id.clone(),
            script_path: self.script_path.clone(),
            editor_argv: self.editor.argv.clone(),
            exit_status: status.code(),
        })
    }
}

/// Successful provider-script editor execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderScriptEditOutcome {
    /// Stable configured provider id.
    pub provider_id: String,
    /// Script file passed to the editor.
    pub script_path: PathBuf,
    /// Direct editor argv.
    pub editor_argv: Vec<String>,
    /// Process exit code when the platform reported one.
    pub exit_status: Option<i32>,
}

/// Typed provider-script resolution or editor failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProviderScriptEditError {
    /// Only command providers can reference editable helper scripts.
    #[error("ASR provider `{0}` is not a command provider and has no editable script")]
    NotCommandProvider(String),
    /// No command/argument candidate resolved to an existing regular file.
    #[error("ASR provider `{0}` does not reference an existing editable script file")]
    MissingScript(String),
    /// Current directory discovery failed.
    #[error("resolve current directory for provider script: {0}")]
    CurrentDirectory(String),
    /// A tilde path required `HOME`, but it was unavailable.
    #[error("resolve user path: HOME is unset and XDG_DATA_HOME is unset")]
    MissingHome,
    /// Filesystem metadata failed for a candidate other than not-found.
    #[error("inspect provider script candidate `{path}`: {message}")]
    InspectCandidate {
        /// Candidate path whose metadata could not be read.
        path: String,
        /// Sanitized operating-system error text.
        message: String,
    },
    /// The selected editor command had no argv.
    #[error("provider editor command is empty")]
    EmptyEditor,
    /// Starting the editor process failed.
    #[error("run provider editor `{editor}`: {message}")]
    LaunchEditor {
        /// Direct editor command selected for the operation.
        editor: String,
        /// Sanitized process-launch error text.
        message: String,
    },
    /// The editor exited unsuccessfully.
    #[error("provider editor `{editor}` exited with status {status}")]
    EditorFailed {
        /// Direct editor command selected for the operation.
        editor: String,
        /// Exit code or signal label.
        status: String,
    },
}

/// Resolves an editable provider script and editor using the current environment.
pub fn prepare_provider_script_edit(
    provider: &AsrProviderConfig,
    explicit_editor: Option<&str>,
) -> Result<ProviderScriptEditPlan, ProviderScriptEditError> {
    let context = ProviderScriptResolutionContext::from_environment()?;
    let editor = ProviderEditorCommand::from_environment(explicit_editor)?;
    prepare_provider_script_edit_with(provider, &context, editor)
}

/// Deterministic companion to [`prepare_provider_script_edit`].
pub fn prepare_provider_script_edit_with(
    provider: &AsrProviderConfig,
    context: &ProviderScriptResolutionContext,
    editor: ProviderEditorCommand,
) -> Result<ProviderScriptEditPlan, ProviderScriptEditError> {
    if provider.kind != AsrProviderKind::Command {
        return Err(ProviderScriptEditError::NotCommandProvider(
            provider.id.clone(),
        ));
    }
    let script_path = resolve_editable_provider_script_with(provider, context)?
        .ok_or_else(|| ProviderScriptEditError::MissingScript(provider.id.clone()))?;
    Ok(ProviderScriptEditPlan {
        provider_id: provider.id.clone(),
        script_path,
        editor,
    })
}

/// Resolves the first existing regular script file referenced by a command provider.
pub fn resolve_editable_provider_script(
    provider: &AsrProviderConfig,
) -> Result<Option<PathBuf>, ProviderScriptEditError> {
    let context = ProviderScriptResolutionContext::from_environment()?;
    resolve_editable_provider_script_with(provider, &context)
}

/// Deterministic companion to [`resolve_editable_provider_script`].
pub fn resolve_editable_provider_script_with(
    provider: &AsrProviderConfig,
    context: &ProviderScriptResolutionContext,
) -> Result<Option<PathBuf>, ProviderScriptEditError> {
    if provider.kind != AsrProviderKind::Command {
        return Err(ProviderScriptEditError::NotCommandProvider(
            provider.id.clone(),
        ));
    }
    if let Some(command) = provider.command.as_deref()
        && is_path_like_command(command)
        && let Some(path) = resolve_existing_regular_file(command, context)?
    {
        return Ok(Some(path));
    }
    for argument in &provider.args {
        if let Some(path) = resolve_existing_regular_file(argument, context)? {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn is_path_like_command(command: &str) -> bool {
    command.contains('/') || command.starts_with('.') || command.starts_with('~')
}

fn select_editor_command(
    explicit: Option<&str>,
    provider_editor: Option<&str>,
    visual: Option<&str>,
    editor: Option<&str>,
) -> String {
    explicit
        .or(provider_editor)
        .or(visual)
        .or(editor)
        .unwrap_or("vi")
        .to_owned()
}

fn resolve_existing_regular_file(
    candidate: &str,
    context: &ProviderScriptResolutionContext,
) -> Result<Option<PathBuf>, ProviderScriptEditError> {
    let candidate = candidate.trim();
    if candidate.is_empty() {
        return Ok(None);
    }
    let path = if candidate == "~" {
        context
            .home_dir
            .clone()
            .ok_or(ProviderScriptEditError::MissingHome)?
    } else if let Some(relative) = candidate.strip_prefix("~/") {
        context
            .home_dir
            .as_ref()
            .ok_or(ProviderScriptEditError::MissingHome)?
            .join(relative)
    } else if candidate.starts_with('~') {
        return Ok(None);
    } else {
        PathBuf::from(candidate)
    };
    let path = if path.is_absolute() {
        path
    } else {
        context.current_dir.join(path)
    };
    match fs::metadata(&path) {
        Ok(metadata) if metadata.is_file() => Ok(Some(path)),
        Ok(_) => Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ProviderScriptEditError::InspectCandidate {
            path: path.display().to_string(),
            message: error.to_string(),
        }),
    }
}

fn exit_status_label(status: ExitStatus) -> String {
    status
        .code()
        .map_or_else(|| "signal".to_owned(), |exit_code| exit_code.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(command: Option<&str>, args: &[&str]) -> AsrProviderConfig {
        AsrProviderConfig {
            id: "provider.fixture.batch".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(60_000),
            model: None,
            hotwords_file: None,
            command: command.map(str::to_owned),
            args: args.iter().map(|value| (*value).to_owned()).collect(),
            env: std::collections::HashMap::new(),
            endpoint: None,
        }
    }

    #[test]
    fn resolves_path_like_command_before_arguments() {
        let directory = tempfile::tempdir().expect("temp dir");
        let command = directory.path().join("command.py");
        let argument = directory.path().join("argument.py");
        fs::write(&command, "command").expect("write command");
        fs::write(&argument, "argument").expect("write argument");
        let provider = provider(
            command.to_str(),
            &[argument.to_str().expect("argument path")],
        );
        let context = ProviderScriptResolutionContext {
            current_dir: directory.path().to_path_buf(),
            home_dir: None,
        };

        let resolved = resolve_editable_provider_script_with(&provider, &context)
            .expect("resolve script")
            .expect("script path");

        assert_eq!(resolved, command);
    }

    #[test]
    fn resolves_relative_and_home_argument_candidates() {
        let directory = tempfile::tempdir().expect("temp dir");
        let home = directory.path().join("home");
        fs::create_dir_all(&home).expect("create home");
        let script = home.join("provider.py");
        fs::write(&script, "provider").expect("write script");
        let provider = provider(Some("python3"), &["missing.py", "~/provider.py"]);
        let context = ProviderScriptResolutionContext {
            current_dir: directory.path().to_path_buf(),
            home_dir: Some(home),
        };

        let resolved = resolve_editable_provider_script_with(&provider, &context)
            .expect("resolve script")
            .expect("script path");

        assert_eq!(resolved, script);
    }

    #[test]
    fn rejects_non_command_provider_and_missing_script() {
        let directory = tempfile::tempdir().expect("temp dir");
        let context = ProviderScriptResolutionContext {
            current_dir: directory.path().to_path_buf(),
            home_dir: None,
        };
        let mut local = provider(None, &[]);
        local.kind = AsrProviderKind::Local;

        assert!(matches!(
            prepare_provider_script_edit_with(
                &local,
                &context,
                ProviderEditorCommand::parse("true").expect("editor")
            ),
            Err(ProviderScriptEditError::NotCommandProvider(_))
        ));
        assert!(matches!(
            prepare_provider_script_edit_with(
                &provider(Some("python3"), &["missing.py"]),
                &context,
                ProviderEditorCommand::parse("true").expect("editor")
            ),
            Err(ProviderScriptEditError::MissingScript(_))
        ));
    }

    #[test]
    fn editor_plan_executes_direct_argv_and_reports_status() {
        let directory = tempfile::tempdir().expect("temp dir");
        let script = directory.path().join("provider.py");
        let editor = directory.path().join("editor.sh");
        fs::write(&script, "provider\n").expect("write script");
        fs::write(&editor, "#!/bin/sh\nprintf '# edited\\n' >> \"$1\"\n").expect("write editor");
        let context = ProviderScriptResolutionContext {
            current_dir: directory.path().to_path_buf(),
            home_dir: None,
        };
        let editor = ProviderEditorCommand::parse(&format!("sh {}", editor.display()))
            .expect("parse editor");
        let plan = prepare_provider_script_edit_with(
            &provider(Some("python3"), &[script.to_str().expect("script path")]),
            &context,
            editor,
        )
        .expect("prepare edit");

        let outcome = plan.execute().expect("run editor");

        assert_eq!(outcome.exit_status, Some(0));
        assert!(
            fs::read_to_string(script)
                .expect("read script")
                .contains("# edited")
        );
    }

    #[test]
    fn editor_failure_keeps_legacy_diagnostic_shape() {
        let directory = tempfile::tempdir().expect("temp dir");
        let script = directory.path().join("provider.py");
        fs::write(&script, "provider\n").expect("write script");
        let context = ProviderScriptResolutionContext {
            current_dir: directory.path().to_path_buf(),
            home_dir: None,
        };
        let plan = prepare_provider_script_edit_with(
            &provider(Some("python3"), &[script.to_str().expect("script path")]),
            &context,
            ProviderEditorCommand::parse("false").expect("editor"),
        )
        .expect("prepare edit");

        let error = plan.execute().expect_err("editor should fail");

        assert_eq!(
            error.to_string(),
            "provider editor `false` exited with status 1"
        );
    }

    #[test]
    fn editor_selection_preserves_legacy_priority_and_default() {
        assert_eq!(
            select_editor_command(
                Some("explicit --wait"),
                Some("provider-editor"),
                Some("visual"),
                Some("editor")
            ),
            "explicit --wait"
        );
        assert_eq!(
            select_editor_command(
                None,
                Some("provider-editor"),
                Some("visual"),
                Some("editor")
            ),
            "provider-editor"
        );
        assert_eq!(
            select_editor_command(None, None, Some("visual"), Some("editor")),
            "visual"
        );
        assert_eq!(
            select_editor_command(None, None, None, Some("editor")),
            "editor"
        );
        assert_eq!(select_editor_command(None, None, None, None), "vi");
    }
}

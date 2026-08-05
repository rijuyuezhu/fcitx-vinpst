use std::path::Path;

use anyhow::Context;

use super::service::{daemon_user_service_command, run_daemon_user_service_command};
use crate::{fs, paths::user_config_home, sandbox, write_file_atomically};

const NATIVE_SERVICE_TEMPLATE: &str = "/usr/lib/systemd/user/vinpst-daemon.service";

pub(super) fn print_daemon_install_service(
    template: Option<&Path>,
    output: Option<&Path>,
    dry_run: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    let sandboxed = sandbox::is_flatpak();
    let template_path = template.map_or_else(
        || {
            if sandboxed {
                sandbox::default_service_template_path()
            } else {
                NATIVE_SERVICE_TEMPLATE.into()
            }
        },
        Path::to_path_buf,
    );
    let output_path = match output {
        Some(path) => path.to_path_buf(),
        None => user_config_home()?
            .join("systemd")
            .join("user")
            .join("vinpst-daemon.service"),
    };
    let template_contents = fs::read_to_string(&template_path)
        .with_context(|| format!("read daemon service template {}", template_path.display()))?;
    let rendered = if sandboxed {
        sandbox::render_flatpak_service(&template_contents)?
    } else if template_contents.ends_with('\n') {
        template_contents
    } else {
        format!("{template_contents}\n")
    };

    let mut reload = serde_json::Value::Null;
    if !dry_run {
        let parent = output_path
            .parent()
            .context("daemon service output has no parent")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("create daemon service directory {}", parent.display()))?;
        write_file_atomically(&output_path, &rendered)
            .with_context(|| format!("install daemon service {}", output_path.display()))?;
        let command = daemon_user_service_command("daemon-reload", None)?;
        reload = run_daemon_user_service_command("daemon-reload", &command);
        anyhow::ensure!(
            reload["ok"].as_bool() == Some(true),
            "installed daemon service but systemd user daemon-reload failed: {}",
            reload["error"]
                .as_str()
                .or_else(|| reload["stderr"].as_str())
                .unwrap_or("unknown error")
        );
    }
    let rendered_service = dry_run.then_some(rendered.as_str());
    let value = serde_json::json!({
        "ok": true,
        "action": "install-service",
        "dry_run": dry_run,
        "sandbox": sandbox::permission_report_json(),
        "template": template_path,
        "output": output_path,
        "rewritten_for_flatpak": sandboxed,
        "rendered_service": rendered_service,
        "wrote_service": !dry_run,
        "daemon_reload": reload,
        "next_steps": [
            "run vinpst daemon restart to start the installed service",
            "run vinpst doctor to inspect Flatpak permissions and activation state"
        ],
    });
    if json_output {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("action: install-service");
        println!("dry_run: {dry_run}");
        println!("template: {}", template_path.display());
        println!("output: {}", output_path.display());
        println!("rewritten_for_flatpak: {sandboxed}");
        println!("wrote_service: {}", !dry_run);
        if dry_run {
            println!("rendered_service:\n{rendered}");
        }
    }
    Ok(())
}

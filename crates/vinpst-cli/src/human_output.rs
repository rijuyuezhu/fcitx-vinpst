use std::{ffi::OsString, path::Path};

pub(crate) fn argv_requests_json(args: impl IntoIterator<Item = OsString>) -> bool {
    for arg in args.into_iter().skip(1) {
        if arg == "--" {
            break;
        }
        if arg == "-j" || arg == "--json" {
            return true;
        }
    }
    false
}

pub(crate) fn print_runtime_error(error: &anyhow::Error, json_output: bool) {
    if json_output {
        eprintln!(
            "{}",
            serde_json::json!({
                "status": "error",
                "message": format!("{error:#}"),
            })
        );
    } else {
        eprintln!("Error: {error:?}");
    }
}

pub(crate) fn print_config_mutation(
    dry_run: bool,
    preview: &str,
    applied: &str,
    output_path: Option<&Path>,
    backup_path: Option<&Path>,
) {
    println!("{}", if dry_run { preview } else { applied });
    if dry_run {
        return;
    }
    if let Some(path) = output_path {
        println!("Updated config: {}", path.display());
    }
    if let Some(path) = backup_path {
        println!("Backup: {}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::argv_requests_json;
    use std::ffi::OsString;

    fn argv(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn json_request_detection_matches_cli_flag_positions() {
        assert!(argv_requests_json(argv(&[
            "vinpst", "--json", "provider", "list"
        ])));
        assert!(argv_requests_json(argv(&[
            "vinpst", "provider", "list", "-j"
        ])));
        assert!(!argv_requests_json(argv(&["vinpst", "provider", "list"])));
        assert!(!argv_requests_json(argv(&[
            "vinpst", "provider", "edit", "p", "--", "--json"
        ])));
    }
}

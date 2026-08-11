//! Shared validated atomic config persistence helper.

use std::path::Path;

use vinpst_config::{VinpstConfig, write_config_file};

use super::RuntimeError;

pub(crate) fn persist_config_atomically(
    path: &Path,
    config: &VinpstConfig,
    _operation: &str,
) -> Result<(), RuntimeError> {
    write_config_file(config, path, None)
        .map(|_| ())
        .map_err(RuntimeError::PersistConfig)
}

#[cfg(test)]
mod tests {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    use super::*;

    #[cfg(unix)]
    #[test]
    fn daemon_persistence_preserves_config_symlink() {
        let directory = tempfile::tempdir().expect("temp dir");
        let target_dir = directory.path().join("dotfiles");
        fs::create_dir_all(&target_dir).expect("target dir");
        let target = target_dir.join("config.json");
        let link = directory.path().join("config.json");
        let original = VinpstConfig::bundled_default().expect("bundled config");
        fs::write(
            &target,
            format!("{}\n", serde_json::to_string_pretty(&original).unwrap()),
        )
        .expect("seed target");
        symlink("dotfiles/config.json", &link).expect("symlink");
        let mut updated = original;
        updated.global.default_language = "zh-CN".to_owned();

        persist_config_atomically(&link, &updated, "test").expect("persist through symlink");

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_link(&link).expect("read link"),
            std::path::PathBuf::from("dotfiles/config.json")
        );
        let loaded = VinpstConfig::from_json_file(&target).expect("load target");
        assert_eq!(loaded.global.default_language, "zh-CN");
    }
}

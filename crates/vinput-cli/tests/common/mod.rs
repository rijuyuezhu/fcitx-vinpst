use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static TEMP_FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[allow(dead_code)]
pub fn workspace_file(path: &str) -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.push("../..");
    root.push(path);
    root
}

#[allow(dead_code)]
pub fn write_temp_json(prefix: &str, contents: &str) -> PathBuf {
    let sequence = TEMP_FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("{prefix}-{}-{sequence}.json", std::process::id()));
    fs::write(&path, contents).expect("write temporary JSON fixture");
    path
}

#[allow(dead_code)]
pub fn vinput_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_vinput"))
}

#[allow(dead_code)]
pub fn isolated_vinput_command(prefix: &str) -> (tempfile::TempDir, Command) {
    let root = tempfile::Builder::new()
        .prefix(prefix)
        .tempdir()
        .expect("create isolated CLI home");
    let mut command = vinput_command();
    command
        .env("HOME", root.path().join("home"))
        .env("XDG_CONFIG_HOME", root.path().join("config"))
        .env("XDG_DATA_HOME", root.path().join("data"))
        .env("XDG_CACHE_HOME", root.path().join("cache"));
    (root, command)
}

#[allow(dead_code)]
pub fn assert_json_success(output: Output, context: &str) -> serde_json::Value {
    let Output {
        status,
        stdout,
        stderr,
    } = output;
    assert!(
        status.success(),
        "{context}: command failed with status {:?}, stderr: {}",
        status.code(),
        String::from_utf8_lossy(&stderr)
    );
    serde_json::from_slice(&stdout).unwrap_or_else(|error| {
        panic!(
            "{context}: stdout should be JSON: {error}; stdout: {}",
            String::from_utf8_lossy(&stdout)
        )
    })
}

#[allow(dead_code)]
pub fn assert_stdout_success(output: Output, context: &str) -> String {
    let Output {
        status,
        stdout,
        stderr,
    } = output;
    assert!(
        status.success(),
        "{context}: command failed with status {:?}, stderr: {}",
        status.code(),
        String::from_utf8_lossy(&stderr)
    );
    String::from_utf8(stdout)
        .unwrap_or_else(|error| panic!("{context}: stdout should be UTF-8: {error}"))
}

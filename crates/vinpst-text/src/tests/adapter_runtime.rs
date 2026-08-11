use std::{
    os::unix::fs::PermissionsExt,
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use super::*;
use crate::{StartedAdapterProcess, stop_started_adapter_process};

fn unique_runtime_dir(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "vinpst-text-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ))
}

fn sleep_adapter_spec(args: Vec<String>) -> AdapterProcessSpec {
    AdapterProcessSpec {
        id: "cmd-adapter".to_owned(),
        command: "/bin/sh".to_owned(),
        args,
        env: std::collections::HashMap::default(),
        working_dir: None,
    }
}

fn process_is_runnable(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/status")).is_ok_and(|status| {
        status
            .lines()
            .find(|line| line.starts_with("State:"))
            .is_none_or(|line| !line.contains("Z (zombie)") && !line.contains("X (dead)"))
    })
}

fn wait_until_process_stops_running(pid: u32) {
    for _ in 0..100 {
        if !process_is_runnable(pid) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!process_is_runnable(pid), "process {pid} remained runnable");
}

fn read_proc_start_time_ticks(pid: u32) -> u64 {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let closing_paren = stat.rfind(')').unwrap();
    stat[closing_paren + 1..]
        .split_whitespace()
        .nth(19)
        .unwrap()
        .parse()
        .unwrap()
}

fn wait_for_file(
    path: &std::path::Path,
    process: &mut StartedAdapterProcess,
    paths: &AdapterRuntimePaths,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if path.exists() {
            return;
        }
        match process.try_wait_and_cleanup() {
            Ok(None) => {}
            Ok(Some(status)) => {
                let _ = paths.remove_pid(&process.id);
                panic!(
                    "adapter exited with {status} before file appeared: {}",
                    path.display()
                );
            }
            Err(error) => {
                let _ = stop_started_adapter_process(process, paths);
                panic!(
                    "failed to inspect adapter while waiting for {}: {error}",
                    path.display()
                );
            }
        }
        if Instant::now() >= deadline {
            let _ = stop_started_adapter_process(process, paths);
            panic!("file did not appear within 5 seconds: {}", path.display());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn adapter_runtime_paths_build_safe_pid_paths() {
    let paths = AdapterRuntimePaths::new("/tmp/vinpst-runtime");

    assert_eq!(
        paths.pid_path("adapter.demo").unwrap(),
        std::path::PathBuf::from("/tmp/vinpst-runtime/adapter.demo.pid")
    );
    assert_eq!(
        paths.runtime_dir(),
        std::path::Path::new("/tmp/vinpst-runtime")
    );
}

#[test]
fn adapter_runtime_paths_roundtrip_legacy_pid_files() {
    let runtime_dir = unique_runtime_dir("runtime");
    let paths = AdapterRuntimePaths::new(&runtime_dir);

    let pid_path = paths.write_pid("adapter.demo", 12345).unwrap();
    assert_eq!(pid_path, runtime_dir.join("adapter.demo.pid"));
    assert_eq!(paths.read_pid("adapter.demo").unwrap(), Some(12345));
    assert!(paths.remove_pid("adapter.demo").unwrap());
    assert_eq!(paths.read_pid("adapter.demo").unwrap(), None);
    assert!(!paths.remove_pid("adapter.demo").unwrap());
    std::fs::remove_dir_all(runtime_dir).unwrap();
}

#[test]
fn adapter_runtime_paths_reject_malformed_pid_files() {
    let runtime_dir = unique_runtime_dir("bad-pid");
    std::fs::create_dir_all(&runtime_dir).unwrap();
    std::fs::write(runtime_dir.join("adapter.demo.pid"), "not-a-pid").unwrap();
    let paths = AdapterRuntimePaths::new(&runtime_dir);

    let error = paths.read_pid("adapter.demo").unwrap_err();
    assert!(
        matches!(error, TextError::InvalidAdapterPid(message) if message.contains("not-a-pid") || message.contains("invalid digit"))
    );
    std::fs::remove_dir_all(runtime_dir).unwrap();
}

#[test]
fn adapter_runtime_paths_reject_unsafe_adapter_ids() {
    let paths = AdapterRuntimePaths::new("/tmp/vinpst-runtime");

    for adapter_id in ["", ".", "..", "../escape", "nested/id", r"nested\id"] {
        let error = paths.pid_path(adapter_id).unwrap_err();
        assert_eq!(error, TextError::InvalidAdapterId(adapter_id.to_owned()));
        assert_eq!(
            crate::validate_adapter_id(adapter_id).unwrap_err(),
            TextError::InvalidAdapterId(adapter_id.to_owned())
        );
    }
    crate::validate_adapter_id("adapter.demo").expect("safe adapter id");
}

#[test]
fn adapter_process_spec_copies_typed_config() {
    let spec = AdapterProcessSpec::from_config(&LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "helper".to_owned(),
        args: vec!["--serve".to_owned()],
        env: std::collections::HashMap::from([("MODE".to_owned(), "serve".to_owned())]),
        working_dir: Some("/tmp/vinpst-adapter".to_owned()),
        extra: std::collections::HashMap::default(),
    });

    assert_eq!(spec.id, "cmd-adapter");
    assert_eq!(spec.command, "helper");
    assert_eq!(spec.args, ["--serve"]);
    assert_eq!(spec.env.get("MODE").map(String::as_str), Some("serve"));
    assert_eq!(spec.working_dir.as_deref(), Some("/tmp/vinpst-adapter"));
}

#[test]
fn inferred_adapter_working_dir_matches_upstream_script_discovery_order() {
    let root = unique_runtime_dir("working-dir");
    let current_dir = root.join("current");
    let script_dir = current_dir.join("scripts");
    let command_dir = current_dir.join("bin");
    let home_dir = root.join("home");
    std::fs::create_dir_all(&script_dir).unwrap();
    std::fs::create_dir_all(&command_dir).unwrap();
    std::fs::create_dir_all(home_dir.join("tools")).unwrap();
    std::fs::write(script_dir.join("adapter.py"), "# fixture\n").unwrap();
    std::fs::write(command_dir.join("adapter-helper"), "# fixture\n").unwrap();
    std::fs::write(home_dir.join("tools/home.py"), "# fixture\n").unwrap();

    let mut spec = AdapterProcessSpec {
        id: "adapter.demo".to_owned(),
        command: "bin/adapter-helper".to_owned(),
        args: vec!["--serve".to_owned(), "scripts/adapter.py".to_owned()],
        env: std::collections::HashMap::new(),
        working_dir: None,
    };
    assert_eq!(
        crate::adapter_runtime::infer_adapter_working_dir(&spec, &current_dir, Some(&home_dir)),
        script_dir
    );

    spec.args = vec!["--serve".to_owned()];
    assert_eq!(
        crate::adapter_runtime::infer_adapter_working_dir(&spec, &current_dir, Some(&home_dir)),
        command_dir
    );

    spec.command = "missing-helper".to_owned();
    spec.args = vec!["~/tools/home.py".to_owned()];
    assert_eq!(
        crate::adapter_runtime::infer_adapter_working_dir(&spec, &current_dir, Some(&home_dir)),
        home_dir.join("tools")
    );

    spec.args.clear();
    assert_eq!(
        crate::adapter_runtime::infer_adapter_working_dir(&spec, &current_dir, Some(&home_dir)),
        current_dir
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn start_adapter_process_writes_atomic_fingerprinted_pid_file() {
    let runtime_dir = unique_runtime_dir("process-runtime");
    let paths = AdapterRuntimePaths::new(&runtime_dir);
    let spec = sleep_adapter_spec(vec!["-c".to_owned(), "sleep 30".to_owned()]);

    let mut started = start_adapter_process(&spec, &paths).unwrap();
    assert_eq!(started.id, "cmd-adapter");
    assert_eq!(paths.read_pid("cmd-adapter").unwrap(), Some(started.pid));
    assert_eq!(started.pid_path, runtime_dir.join("cmd-adapter.pid"));
    assert!(started.start_time_ticks > 0);
    let record: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&started.pid_path).unwrap()).unwrap();
    assert_eq!(record["version"], 1);
    assert_eq!(record["pid"], started.pid);
    assert_eq!(record["start_time_ticks"], started.start_time_ticks);
    assert_eq!(
        std::fs::metadata(&started.pid_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(std::fs::read_dir(&runtime_dir).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".tmp-")
    }));

    stop_started_adapter_process(&mut started, &paths).unwrap();
    std::fs::remove_dir_all(runtime_dir).unwrap();
}

#[test]
fn start_adapter_process_rejects_matching_live_fingerprint() {
    let runtime_dir = unique_runtime_dir("duplicate-live");
    let paths = AdapterRuntimePaths::new(&runtime_dir);
    let spec = sleep_adapter_spec(vec!["-c".to_owned(), "sleep 30".to_owned()]);
    let mut started = start_adapter_process(&spec, &paths).unwrap();

    let error = start_adapter_process(&spec, &paths).unwrap_err();
    assert_eq!(
        error,
        TextError::AdapterAlreadyRunning("cmd-adapter".to_owned())
    );
    stop_started_adapter_process(&mut started, &paths).unwrap();
    std::fs::remove_dir_all(runtime_dir).unwrap();
}

#[test]
fn start_adapter_process_rejects_legacy_pid_until_stop_clears_it() {
    let runtime_dir = unique_runtime_dir("legacy-block");
    let paths = AdapterRuntimePaths::new(&runtime_dir);
    let spec = sleep_adapter_spec(vec!["-c".to_owned(), "sleep 30".to_owned()]);
    paths.write_pid("cmd-adapter", 12345).unwrap();

    let error = start_adapter_process(&spec, &paths).unwrap_err();
    assert!(matches!(
        error,
        TextError::InvalidAdapterPid(message)
            if message.contains("legacy PID-only record") && message.contains("stop before start")
    ));
    assert_eq!(
        stop_adapter_process("cmd-adapter", &paths).unwrap(),
        AdapterStopOutcome::NotRunning
    );
    let mut started = start_adapter_process(&spec, &paths).unwrap();
    stop_started_adapter_process(&mut started, &paths).unwrap();
    std::fs::remove_dir_all(runtime_dir).unwrap();
}

#[test]
fn start_adapter_process_replaces_stale_fingerprint() {
    let runtime_dir = unique_runtime_dir("stale-replace");
    std::fs::create_dir_all(&runtime_dir).unwrap();
    let paths = AdapterRuntimePaths::new(&runtime_dir);
    let spec = sleep_adapter_spec(vec!["-c".to_owned(), "sleep 30".to_owned()]);
    let current_pid = std::process::id();
    let wrong_start_time = read_proc_start_time_ticks(current_pid) + 1;
    std::fs::write(
        paths.pid_path("cmd-adapter").unwrap(),
        format!(
            "{{\"version\":1,\"pid\":{current_pid},\"start_time_ticks\":{wrong_start_time}}}\n"
        ),
    )
    .unwrap();

    let mut started = start_adapter_process(&spec, &paths).unwrap();
    assert_ne!(started.pid, current_pid);
    assert_eq!(paths.read_pid("cmd-adapter").unwrap(), Some(started.pid));
    stop_started_adapter_process(&mut started, &paths).unwrap();
    std::fs::remove_dir_all(runtime_dir).unwrap();
}

#[test]
fn start_adapter_process_reports_spawn_failure_without_pid_file() {
    let runtime_dir = unique_runtime_dir("missing");
    let paths = AdapterRuntimePaths::new(&runtime_dir);
    let spec = AdapterProcessSpec {
        id: "cmd-adapter".to_owned(),
        command: format!("vinpst-missing-adapter-{}", std::process::id()),
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        working_dir: None,
    };

    let error = start_adapter_process(&spec, &paths).unwrap_err();
    assert!(matches!(
        error,
        TextError::AdapterFailed(message)
            if message.contains("failed to spawn text adapter `cmd-adapter`")
    ));
    assert_eq!(paths.read_pid("cmd-adapter").unwrap(), None);
}

#[test]
fn start_adapter_process_reports_immediate_exit_stderr_without_pid_file() {
    let runtime_dir = unique_runtime_dir("immediate-stderr");
    let paths = AdapterRuntimePaths::new(&runtime_dir);
    let spec = sleep_adapter_spec(vec![
        "-c".to_owned(),
        "printf 'adapter startup failed\\n' >&2; exit 7".to_owned(),
    ]);

    let error = start_adapter_process(&spec, &paths).unwrap_err();

    assert_eq!(
        error,
        TextError::AdapterFailed("adapter startup failed".to_owned())
    );
    assert_eq!(paths.read_pid("cmd-adapter").unwrap(), None);
    std::fs::remove_dir_all(runtime_dir).unwrap();
}

#[test]
fn started_adapter_process_drains_lines_and_flushes_partial_stderr_on_exit() {
    let runtime_dir = unique_runtime_dir("stderr-lines");
    let paths = AdapterRuntimePaths::new(&runtime_dir);
    let spec = sleep_adapter_spec(vec![
        "-c".to_owned(),
        "printf ' first \\nsecond\\npartial' >&2; sleep 0.5; exit 0".to_owned(),
    ]);
    let mut started = start_adapter_process(&spec, &paths).unwrap();

    assert_eq!(
        started.drain_stderr_lines(false).unwrap(),
        ["first".to_owned(), "second".to_owned()]
    );
    assert!(started.try_wait_and_cleanup().unwrap().is_none());
    std::thread::sleep(Duration::from_millis(400));
    assert!(started.try_wait_and_cleanup().unwrap().is_some());
    assert_eq!(
        started.drain_stderr_lines(true).unwrap(),
        ["partial".to_owned()]
    );
    paths.remove_pid("cmd-adapter").unwrap();
    std::fs::remove_dir_all(runtime_dir).unwrap();
}

#[test]
fn stop_started_adapter_process_terminates_group_and_descendant() {
    let runtime_dir = unique_runtime_dir("tracked-stop");
    let child_pid_path = runtime_dir.join("child.pid");
    let paths = AdapterRuntimePaths::new(&runtime_dir);
    let mut spec = sleep_adapter_spec(vec![
        "-c".to_owned(),
        "sleep 30 & echo $! > \"$CHILD_PID\"; wait".to_owned(),
    ]);
    spec.env.insert(
        "CHILD_PID".to_owned(),
        child_pid_path.to_string_lossy().into_owned(),
    );
    let mut started = start_adapter_process(&spec, &paths).unwrap();
    wait_for_file(&child_pid_path, &mut started, &paths);
    let child_pid = std::fs::read_to_string(&child_pid_path)
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();

    let outcome = stop_started_adapter_process(&mut started, &paths).unwrap();
    assert_eq!(outcome, AdapterStopOutcome::Stopped { pid: started.pid });
    wait_until_process_stops_running(child_pid);
    assert_eq!(paths.read_pid("cmd-adapter").unwrap(), None);
    std::fs::remove_dir_all(runtime_dir).unwrap();
}

#[test]
fn stop_started_adapter_process_escalates_when_term_is_ignored() {
    let runtime_dir = unique_runtime_dir("force-stop");
    let ready_path = runtime_dir.join("ready");
    let paths = AdapterRuntimePaths::new(&runtime_dir);
    let mut spec = sleep_adapter_spec(vec![
        "-c".to_owned(),
        "trap '' TERM; : > \"$READY_PATH\"; while :; do sleep 1; done".to_owned(),
    ]);
    spec.env.insert(
        "READY_PATH".to_owned(),
        ready_path.to_string_lossy().into_owned(),
    );
    let mut started = start_adapter_process(&spec, &paths).unwrap();
    wait_for_file(&ready_path, &mut started, &paths);

    let began = Instant::now();
    let outcome = stop_started_adapter_process(&mut started, &paths).unwrap();
    assert_eq!(outcome, AdapterStopOutcome::Stopped { pid: started.pid });
    assert!(began.elapsed() >= Duration::from_secs(2));
    assert!(began.elapsed() < Duration::from_secs(5));
    assert_eq!(paths.read_pid("cmd-adapter").unwrap(), None);
    std::fs::remove_dir_all(runtime_dir).unwrap();
}

#[test]
fn stop_adapter_process_terminates_untracked_fingerprinted_group() {
    let runtime_dir = unique_runtime_dir("untracked-stop");
    std::fs::create_dir_all(&runtime_dir).unwrap();
    let paths = AdapterRuntimePaths::new(&runtime_dir);
    let output = Command::new("/bin/sh")
        .args(["-c", "setsid sleep 30 </dev/null >/dev/null 2>&1 & echo $!"])
        .output()
        .unwrap();
    let pid = String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();
    let start_time_ticks = read_proc_start_time_ticks(pid);
    std::fs::write(
        paths.pid_path("cmd-adapter").unwrap(),
        format!("{{\"version\":1,\"pid\":{pid},\"start_time_ticks\":{start_time_ticks}}}\n"),
    )
    .unwrap();

    let outcome = stop_adapter_process("cmd-adapter", &paths).unwrap();
    assert_eq!(outcome, AdapterStopOutcome::Stopped { pid });
    wait_until_process_stops_running(pid);
    assert_eq!(paths.read_pid("cmd-adapter").unwrap(), None);
    std::fs::remove_dir_all(runtime_dir).unwrap();
}

#[test]
fn legacy_pid_file_is_removed_without_signaling_reused_pid() {
    let runtime_dir = unique_runtime_dir("legacy-stale");
    let paths = AdapterRuntimePaths::new(&runtime_dir);
    let mut unrelated = Command::new("sleep")
        .arg("30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    paths.write_pid("cmd-adapter", unrelated.id()).unwrap();

    assert_eq!(
        stop_adapter_process("cmd-adapter", &paths).unwrap(),
        AdapterStopOutcome::NotRunning
    );
    assert!(unrelated.try_wait().unwrap().is_none());
    assert_eq!(paths.read_pid("cmd-adapter").unwrap(), None);
    unrelated.kill().unwrap();
    unrelated.wait().unwrap();
    std::fs::remove_dir_all(runtime_dir).unwrap();
}

#[test]
fn mismatched_start_fingerprint_is_removed_without_signaling_process() {
    let runtime_dir = unique_runtime_dir("fingerprint-stale");
    std::fs::create_dir_all(&runtime_dir).unwrap();
    let paths = AdapterRuntimePaths::new(&runtime_dir);
    let mut unrelated = Command::new("sleep")
        .arg("30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pid = unrelated.id();
    let wrong_start_time = read_proc_start_time_ticks(pid) + 1;
    std::fs::write(
        paths.pid_path("cmd-adapter").unwrap(),
        format!("{{\"version\":1,\"pid\":{pid},\"start_time_ticks\":{wrong_start_time}}}\n"),
    )
    .unwrap();

    assert_eq!(
        stop_adapter_process("cmd-adapter", &paths).unwrap(),
        AdapterStopOutcome::NotRunning
    );
    assert!(unrelated.try_wait().unwrap().is_none());
    assert_eq!(paths.read_pid("cmd-adapter").unwrap(), None);
    unrelated.kill().unwrap();
    unrelated.wait().unwrap();
    std::fs::remove_dir_all(runtime_dir).unwrap();
}

#[test]
fn stop_adapter_process_reports_not_running_without_pid_file() {
    let runtime_dir = unique_runtime_dir("empty");
    let paths = AdapterRuntimePaths::new(&runtime_dir);

    assert_eq!(
        stop_adapter_process("cmd-adapter", &paths).unwrap(),
        AdapterStopOutcome::NotRunning
    );
}

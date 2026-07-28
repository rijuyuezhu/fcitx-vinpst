//! Regression tests for the public architecture documentation index.

mod common;

use std::path::{Path, PathBuf};

use common::{markdown_note_names, workspace_crate_names, workspace_file};

fn architecture_dir() -> PathBuf {
    workspace_file("docs/architecture")
}

fn has_markdown_extension(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

fn markdown_link_targets(markdown: &str) -> Vec<&str> {
    markdown
        .split(']')
        .filter_map(|suffix| suffix.strip_prefix('('))
        .filter_map(|suffix| suffix.split_once(')').map(|(target, _)| target))
        .filter(|target| has_markdown_extension(target))
        .collect()
}

#[test]
fn architecture_index_lists_all_notes() {
    let dir = architecture_dir();
    let index = std::fs::read_to_string(dir.join("README.md")).expect("read architecture index");
    for file_name in markdown_note_names(&dir) {
        assert!(
            index.contains(&file_name),
            "architecture index should link `{file_name}`"
        );
    }
}

#[test]
fn architecture_index_links_existing_notes() {
    let dir = architecture_dir();
    let index = std::fs::read_to_string(dir.join("README.md")).expect("read architecture index");
    let targets = markdown_link_targets(&index);

    assert!(!targets.is_empty(), "architecture index should link notes");
    for target in targets {
        assert!(
            dir.join(target).exists(),
            "architecture index link should exist: {target}"
        );
    }
}

#[test]
fn development_doc_lists_all_workspace_crates() {
    let development = std::fs::read_to_string(workspace_file("docs/development.md"))
        .expect("read development guide");
    for crate_name in workspace_crate_names() {
        assert!(
            development.contains(&crate_name),
            "development guide should list `{crate_name}`"
        );
    }
}

#[test]
fn target_architecture_lists_all_workspace_crates() {
    let target = std::fs::read_to_string(architecture_dir().join("target-architecture.md"))
        .expect("read target architecture doc");
    for crate_name in workspace_crate_names() {
        assert!(
            target.contains(&crate_name),
            "target architecture doc should list `{crate_name}`"
        );
    }
}

#[test]
fn dbus_architecture_labels_diagnostic_extension_and_postprocessing_gap() {
    let dbus_doc = std::fs::read_to_string(architecture_dir().join("dbus-service.md"))
        .expect("read dbus service doc");

    assert!(
        dbus_doc
            .contains("GetTextAdapterState` and `GetRuntimeStatus` are Rust diagnostic extensions"),
        "D-Bus docs must label diagnostic-only D-Bus methods as Rust extensions"
    );
    assert!(
        dbus_doc.contains("not part of the original C++ daemon vtable"),
        "D-Bus docs must keep the legacy-vs-extension boundary explicit"
    );
    assert!(
        dbus_doc.contains("A real legacy `postprocessing` runtime phase is still not wired"),
        "D-Bus docs must keep the current postprocessing runtime gap explicit"
    );
    assert!(
        dbus_doc.contains("descriptor of the backend that is actually effective in the runtime"),
        "D-Bus docs must distinguish configured ASR targets from the effective backend"
    );
    assert!(
        dbus_doc.contains("queues the configured backend through the prepare-before-swap path"),
        "D-Bus docs must pin configured backend reload behavior"
    );
    assert!(
        dbus_doc.contains("one non-blocking reload worker"),
        "D-Bus docs must pin non-blocking reload preparation"
    );
}

#[test]
fn text_architecture_pins_command_mode_payload_contract() {
    let text_doc = std::fs::read_to_string(architecture_dir().join("text-contract.md"))
        .expect("read text contract doc");

    for required in [
        "selected text as a `raw` candidate",
        "recognized command text as an `asr` candidate",
        "LLM/post-processing candidates as `llm` candidates",
        "Commit text prefers the first LLM/post-processing candidate",
        "falls back to the selected text when present",
        "retained C++ frontend owns selected-text replacement and cleanup",
        "clipboard fallback remains future frontend work",
    ] {
        assert!(
            text_doc.contains(required),
            "text contract doc should pin command-mode rule: {required}"
        );
    }
}

#[test]
fn text_architecture_pins_prompt_file_and_context_cache_rules() {
    let text_doc = std::fs::read_to_string(architecture_dir().join("text-contract.md"))
        .expect("read text contract doc");

    for required in [
        "only literal `file:///absolute/path` URIs are accepted",
        "path is loaded only when it points to a regular file",
        "reads are capped at 256 KiB",
        "unsupported variables are preserved verbatim",
        "frontend-facing code can buffer committed fragments",
        "daemon-facing request builders read raw non-empty lines",
        "XDG_CACHE_HOME/vinput/context.jsonl",
        "$HOME/.cache/vinput/context.jsonl",
        "without exposing environment keys, environment values, or working directory paths",
        "sanitized per-adapter summaries with `id`, `kind`, `args_count`, `env_count`, `has_working_dir`, `is_running`, and `pid`",
        "never include the configured command path, command arguments, environment keys, environment values, configured working directory path, or forward-compatible adapter fields",
        "Request diagnostics redact the HTTP auth header case-insensitively",
        "leaving the transport request intact",
    ] {
        assert!(
            text_doc.contains(required),
            "text contract doc should pin prompt/context rule: {required}"
        );
    }
}

#[test]
fn config_architecture_pins_summary_redaction_contract() {
    let config_doc = std::fs::read_to_string(architecture_dir().join("config-contract.md"))
        .expect("read config contract doc");

    for required in [
        "`VinputConfig::summary()` is the compact config diagnostic surface",
        "active scene/provider ids, and counts only",
        "must not serialize secret-bearing config fields",
        "LLM API keys",
        "provider or adapter environment values",
        "command arguments",
        "working directories",
        "provider base URLs",
        "forward-compatible extra bodies",
        "`asr.vad` preserves the legacy offline Silero controls",
        "threshold `0.05..=0.95`",
        "minimum speech `0.05..=2.0` seconds",
        "speech padding at most `2000` ms",
        "online/streaming recognition does not use this trimmer",
        "`vinput-daemon --config data/default-config.json print-config`",
    ] {
        assert!(
            config_doc.contains(required),
            "config contract doc should pin summary redaction rule: {required}"
        );
    }
}
#[test]
fn audio_architecture_pins_pipewire_live_test_policy() {
    let audio_doc = std::fs::read_to_string(architecture_dir().join("audio-contract.md"))
        .expect("read audio contract doc");

    for required in [
        "VINPUT_TEST_PIPEWIRE_CONTEXT",
        "VINPUT_TEST_PIPEWIRE_ENUMERATE",
        "VINPUT_TEST_PIPEWIRE_RECORD",
        "instead of running in default CI",
        "without requiring a live PipeWire daemon",
        "live probes must only run when those environment variables are set explicitly",
        "`PipeWireAudioRecorder` exists behind `pipewire-backend` as the live recorder seam",
        "creates the PipeWire stream on a worker thread",
        "captures signed 16-bit 16 kHz mono chunks through the callback seam",
        "`PipeWireStreamConfig` records the selected capture target",
        "pinned `S16LE` 16 kHz mono PCM policy",
        "deterministic chunk planning use frames rather than raw sample count",
        "chunk helpers never split a frame across chunk boundaries",
        "selects delivery from the active backend descriptor",
        "legacy-compatible 800-frame batches",
        "complete accumulated stop buffer is not replayed",
        "applies only input gain at the callback boundary",
        "1,700 mono samples become 800, 800, and 100 sample pushes",
        "`PcmBuffer::chunk_ranges_by_frames` can plan complete-frame chunk ranges without copying",
        "can use complete-frame chunk helpers for deterministic streaming callback tests",
    ] {
        assert!(
            audio_doc.contains(required),
            "audio contract doc should pin PipeWire live-test policy: {required}"
        );
    }
}

#[test]
fn asr_architecture_pins_feature_gated_sherpa_backend_scope() {
    let asr_doc = std::fs::read_to_string(architecture_dir().join("asr-contract.md"))
        .expect("read asr contract doc");

    for required in [
        "Local `sherpa-onnx` now has an explicit typed config seam",
        "optional official runtime adapter behind the `sherpa-onnx-backend` Cargo feature",
        "Default builds keep the runtime disabled",
        "accepts relative or absolute local model and hotwords paths",
        "rejects empty values and URL-like paths",
        "verifies model directories plus regular hotwords files",
        "online transducer and Zipformer2 CTC metadata/runtime layouts",
        "SenseVoice, Qwen3 ASR, Moonshine v1, and Zipformer2 CTC are proven with real registry-model WAV samples",
        "Moonshine v1",
        "just sherpa-moonshine-local-smoke",
        "just sherpa-moonshine-dbus-reload-smoke",
        "target/effective separation during background preparation",
        "After early nightfall, the yellow lamps would light up here and there the squalid quarter of the brothels.",
        "Qwen3 ASR requires typed metadata for its convolution frontend, encoder, decoder, tokenizer, and generation parameters",
        "just sherpa-offline-local-smoke",
        "just sherpa-qwen3-local-smoke",
        "Esta prenda es amplia. Recomiendo elegir una talla menor al habitual.",
        "The daemon routes recorder callbacks to chunked ASR sessions",
        "legacy-compatible 800-frame batches",
        "emits deduplicated `RecognitionPartial` D-Bus signals during recording",
        "generation-scoped 40 ms poller",
        "Stop cancels the poller and suppresses a duplicate final partial",
        "transducer construction remains metadata/feature-build tested",
        "Buffered offline recognition uses the migrated Silero VAD model",
        "500 ms cold-start guard",
        "missing or unloadable model degrades to untrimmed recognition",
        "user-install profile installs the tracked MIT-licensed model",
        "`vinput doctor` reports whether VAD is disabled, ready, or missing",
        "resolved/requested path",
        "repair hint for missing assets",
        "legacy endpoint defaults (`true`, `2.4`, `1.2`, `20.0`)",
        "legacy-compatible 200 ms silence warmup",
        "push-to-talk sessions still finalize on `StopRecording`",
        "prepare-before-swap boundary",
        "candidate backend must create and cancel a normal warmup session",
        "preparation failure leaves the previous effective backend untouched",
        "single non-blocking reload worker",
        "re-reads the daemon config file",
        "`reload_in_progress` remains true during physical preparation",
        "stale reload generations are discarded",
        "remaining model families",
        "official native API is synchronous and exposes no safe cancellation handle",
        "`not_configured`, `enforced`, or `unsupported`",
        "Command ASR providers remain genuinely cancellable",
        "runs `runtime-status` by default after install and during `VINPUT_USER_STATUS=1` checks",
        "Set `VINPUT_USER_RUNTIME_STATUS=0` to skip that validation",
        "`MockAsrBackend` can attach a shared `MockAsrAudioLog` for deterministic tests",
        "800/800/tail chunk delivery",
        "real session-bus integration test additionally proves that a partial signal arrives before `StopRecording`",
        "real desktop Fcitx/PipeWire behavior remains unproven",
        "`MockAsrAudioPush` is serde/schema-ready",
    ] {
        assert!(
            asr_doc.contains(required),
            "ASR contract doc should pin feature-gated sherpa backend scope: {required}"
        );
    }
}

#[test]
fn development_doc_pins_optional_pipewire_recipes() {
    let development = std::fs::read_to_string(workspace_file("docs/development.md"))
        .expect("read development guide");
    let justfile = std::fs::read_to_string(workspace_file("justfile")).expect("read justfile");
    let offline_smoke =
        std::fs::read_to_string(workspace_file("scripts/run-sherpa-offline-local-smoke.sh"))
            .expect("read generic sherpa offline smoke");
    let online_smoke =
        std::fs::read_to_string(workspace_file("scripts/run-sherpa-online-local-smoke.sh"))
            .expect("read generic sherpa online smoke");

    for required in [
        "just pipewire-check",
        "just pipewire-live",
        "just addon-dbus-pipewire-live",
        "just ime-pipewire-live",
        "just ime-configured-pipewire-live",
        "VINPUT_TEST_PIPEWIRE_CONTEXT=1",
        "VINPUT_TEST_PIPEWIRE_ENUMERATE=1",
        "VINPUT_TEST_PIPEWIRE_RECORD=1",
        "just sherpa-offline-local-smoke",
        "just sherpa-sense-voice-local-smoke",
        "just sherpa-qwen3-local-smoke",
        "just sherpa-online-local-smoke",
        "just sherpa-zipformer2-ctc-local-smoke",
        "just sherpa-moonshine-dbus-reload-smoke",
        "validates typed registry metadata and one WAV recognition outside Fcitx5",
        "live registry Qwen3 model has passed",
        "live registry model has passed with bundled `test_wavs/0.wav`",
        "对我做了介绍那么我想说的是呢大家如果对我的研究感兴趣呢",
        "intentionally excluded from `just ci`",
        "C++ bridge plus Rust daemon D-Bus path",
        "prints the daemon build's `audio-devices` JSON diagnostics",
        "VINPUT_DBUS_SMOKE_RECORD_MS=100",
        "without live daemon",
        "CLI/daemon audio-device diagnostics",
        "--record-ms 100",
        "start/wait/stop smoke",
        "staged D-Bus activation starts the PipeWire-enabled daemon",
        "--dbus --audio-backend pipewire",
        "Live desktop PipeWire validation",
        "target/tmp/fcitx-ime-pipewire-live-smoke",
        "prints the staged daemon's `audio-devices` JSON diagnostics",
        "Recorder setup errors are expected to include the same target/format/sample-rate/channel plan",
    ] {
        assert!(
            development.contains(required),
            "development guide should pin optional PipeWire recipe policy: {required}"
        );
    }

    for required in [
        "runtime=online",
        "backend=sherpa-streaming",
        "vinput-model.json",
        "runtime-status",
        "--once --wav",
    ] {
        assert!(
            online_smoke.contains(required),
            "generic sherpa online smoke should pin typed runtime behavior: {required}"
        );
    }

    assert!(justfile.contains("pipewire-check:"));
    assert!(justfile.contains("pipewire-live:"));
    assert!(justfile.contains("ime-pipewire-live:"));
    assert!(justfile.contains("ime-configured-pipewire-live:"));
    assert!(justfile.contains("sherpa-offline-local-smoke:"));
    assert!(justfile.contains("sherpa-sense-voice-local-smoke:"));
    assert!(justfile.contains("sherpa-qwen3-local-smoke:"));
    assert!(justfile.contains("sherpa-online-local-smoke:"));
    assert!(justfile.contains("sherpa-zipformer2-ctc-local-smoke:"));
    assert!(justfile.contains("sherpa-moonshine-dbus-reload-smoke:"));
    for required in [
        "VINPUT_SHERPA_EXPECT_FAMILY",
        "vinput-model.json",
        "metadata-free SenseVoice layout",
        "runtime-status",
        "--once --wav",
    ] {
        assert!(
            offline_smoke.contains(required),
            "generic sherpa smoke should pin native preflight/runtime behavior: {required}"
        );
    }
    let check_line = justfile
        .lines()
        .find(|line| line.starts_with("check:"))
        .expect("justfile should define check recipe");
    assert!(!check_line.contains("pipewire-live"));
    assert!(!check_line.contains("ime-pipewire-live"));
}

#[test]
fn development_doc_pins_addon_dbus_smoke_recipes() {
    let development = std::fs::read_to_string(workspace_file("docs/development.md"))
        .expect("read development guide");
    let readme = std::fs::read_to_string(workspace_file("README.md")).expect("read README");
    let justfile = std::fs::read_to_string(workspace_file("justfile")).expect("read justfile");
    let ci_workflow = std::fs::read_to_string(workspace_file(".github/workflows/ci.yml"))
        .expect("read CI workflow");
    for required in [
        "just addon-dbus-smoke",
        "just addon-dbus-activation-smoke",
        "just addon-dbus-configured-activation-smoke",
        "just addon-dbus-adapter-lifecycle-smoke",
        "just ime-e2e-smoke",
        "fake outcome sink",
        "configured text adapter start/duplicate-start/stop diagnostics over DBus",
    ] {
        assert!(
            development.contains(required),
            "development guide should pin addon DBus smoke recipe: {required}"
        );
        assert!(
            readme.contains(required),
            "README should mention addon DBus smoke recipe: {required}"
        );
    }

    for recipe in [
        "addon-dbus-smoke:",
        "addon-dbus-activation-smoke:",
        "addon-dbus-configured-activation-smoke:",
        "addon-dbus-adapter-lifecycle-smoke:",
        "ime-e2e-smoke:",
    ] {
        assert!(justfile.contains(recipe), "justfile should define {recipe}");
    }
    assert!(
        ci_workflow.contains("just addon-dbus-adapter-lifecycle-smoke"),
        "CI should run deterministic adapter lifecycle DBus smoke"
    );
    assert!(
        ci_workflow.contains("just ime-e2e-smoke"),
        "CI should run deterministic staged IME e2e smoke"
    );
}

#[test]
fn target_architecture_pins_frontend_packaging_boundary() {
    let target = std::fs::read_to_string(architecture_dir().join("target-architecture.md"))
        .expect("read target architecture doc");

    for required in [
        "retained C++ Fcitx5 frontend bridge",
        "existing `vinput-protocol` D-Bus ABI",
        "Fcitx API integration, menus, preedit/status presentation",
        "selected-text collection",
        "command-mode selected-text replacement",
        "frontend-side cleanup",
        "Backend logic, ASR/text processing, registry operations, and runtime state must stay in Rust crates",
        "Do not replace the Fcitx5 addon with a Rust addon",
        "Packaging/service install artifacts remain future work",
    ] {
        assert!(
            target.contains(required),
            "target architecture should pin T6 frontend/packaging boundary: {required}"
        );
    }
}

#[test]
fn registry_architecture_mentions_root_planning() {
    let registry_doc = std::fs::read_to_string(architecture_dir().join("registry-contract.md"))
        .expect("read registry contract doc");

    assert!(registry_doc.contains("Dry-run install plans keep install roots explicit"));
    assert!(registry_doc.contains("filesystem root stays absolute"));
    assert!(registry_doc.contains("without touching the filesystem"));
    assert!(registry_doc.contains("On `CrossesDevices`"));
    assert!(registry_doc.contains("hidden sibling on the target filesystem"));
    assert!(registry_doc.contains("resets a stale extraction directory before each retry"));
}

#[test]
fn migration_docs_pin_cli_daemon_e2e_matrix() {
    let docs_readme =
        std::fs::read_to_string(workspace_file("docs/README.md")).expect("read docs README");
    let audit = std::fs::read_to_string(workspace_file("docs/migration/function-gap-audit.md"))
        .expect("read function gap audit");
    let plan = std::fs::read_to_string(workspace_file("docs/migration/e2e-replication-plan.md"))
        .expect("read E2E replication plan");
    let matrix = std::fs::read_to_string(workspace_file("docs/migration/e2e-capability-matrix.md"))
        .expect("read E2E capability matrix");

    for required in [
        "e2e-capability-matrix.md",
        "detailed E2E capability comparison and the native runtime/frontend parity backlog",
        "what exactly is missing for real desktop and legacy parity?",
    ] {
        assert!(
            docs_readme.contains(required),
            "docs README should point at capability matrix: {required}"
        );
    }

    for required in [
        "usable CLI/daemon alpha",
        "Native SenseVoice file-input smoke",
        "Native Qwen3 ASR file-input smoke",
        "real desktop native-dictation alpha",
    ] {
        assert!(
            audit.contains(required),
            "function gap audit should pin current parity and target: {required}"
        );
    }

    for required in [
        "Completed: usable CLI/daemon alpha",
        "P0: real desktop native alpha",
        "Implemented through D-Bus",
        "deduplicated live `RecognitionPartial` signals",
        "Port Dolphin, Paraformer",
    ] {
        assert!(
            plan.contains(required),
            "E2E replication plan should pin current milestone target: {required}"
        );
    }

    for required in [
        "CLI command surface comparison",
        "Daemon capability comparison",
        "Registry/resource comparison",
        "P1.2 sherpa streaming backend",
        "Prove real desktop SenseVoice",
        "Do not claim full parity until all of these pass",
        "vinput model install <id-or-short-id>",
    ] {
        assert!(
            matrix.contains(required),
            "capability matrix should pin detailed E2E gap: {required}"
        );
    }
}

#[test]
fn user_install_smokes_isolate_stub_binaries_from_cargo_outputs() {
    let development = std::fs::read_to_string(workspace_file("docs/development.md"))
        .expect("read development guide");
    let install = std::fs::read_to_string(workspace_file("scripts/install-user-ime.sh"))
        .expect("read user install script");
    let smokes = [
        std::fs::read_to_string(workspace_file(
            "scripts/run-user-ime-real-command-asr-wav-smoke.sh",
        ))
        .expect("read real command ASR user smoke"),
        std::fs::read_to_string(workspace_file(
            "scripts/run-user-ime-sherpa-sense-voice-smoke.sh",
        ))
        .expect("read sherpa user smoke"),
    ];

    for required in [
        "VINPUT_USER_CLI_BINARY",
        "VINPUT_USER_DAEMON_BINARY",
        "target/debug/vinput",
        "target/debug/vinput-daemon",
    ] {
        assert!(
            install.contains(required),
            "user install script should expose binary source override: {required}"
        );
        assert!(
            development.contains(required),
            "development guide should document binary isolation: {required}"
        );
    }

    for smoke in smokes {
        for required in [
            "runtime_bin=",
            "VINPUT_USER_CLI_BINARY=",
            "VINPUT_USER_DAEMON_BINARY=",
        ] {
            assert!(
                smoke.contains(required),
                "user install smoke should keep stubs under its temporary tree: {required}"
            );
        }
        for forbidden in [
            "cat >target/debug/vinput",
            "rm -f target/debug/vinput",
            "backup_dir=",
        ] {
            assert!(
                !smoke.contains(forbidden),
                "user install smoke must not mutate Cargo outputs: {forbidden}"
            );
        }
    }
}

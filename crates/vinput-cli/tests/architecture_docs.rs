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
fn remote_text_architecture_pins_protocol_runtime_boundary() {
    let note = std::fs::read_to_string(architecture_dir().join("remote-text-contract.md"))
        .expect("read remote text architecture note");
    assert!(note.contains("RemoteTextProtocol"));
    assert!(note.contains("RemoteTextServer"));
    assert!(note.contains("RemoteTextLifecycle"));
    assert!(note.contains("provider.vinput.remote.streaming"));
    assert!(note.contains("VINPUT_ASR_API_KEY"));
    assert!(note.contains("OpenAI Realtime-compatible"));
    assert!(note.contains("remote-text-server"));
    assert!(note.contains("GET /health"));
    assert!(note.contains("ReloadAsrBackend"));
    assert!(note.contains("SIGTERM"));
    assert!(note.contains("run-remote-text-daemon-lifecycle-smoke.sh"));
    assert!(note.contains("GetAsrBackendState.remote_endpoints"));
    assert!(note.contains("non-loopback IPv4"));
    assert!(note.contains("GetRuntimeStatus"));
    assert!(note.contains("Remaining live proof"));
    assert!(note.contains("partial"));
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
fn dbus_architecture_labels_extensions_and_postprocessing_contract() {
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
        dbus_doc.contains("`StopRecording` has a real two-stage runtime boundary"),
        "D-Bus docs must pin the two-stage stop boundary"
    );
    assert!(
        dbus_doc.contains("`recording -> inferring -> postprocessing -> idle`"),
        "D-Bus docs must pin the legacy status order"
    );
    assert!(
        dbus_doc.contains("before calling the scene text processor"),
        "D-Bus docs must place the postprocessing signal before text finishing"
    );
    assert!(
        dbus_doc.contains("returns the runtime to `idle`"),
        "D-Bus docs must pin stop failure recovery"
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
    for required in [
        "`GetSceneState() -> sa(ss)`",
        "`SetActiveScene(s) -> b`",
        "`GetAsrMenuState() -> sssbsa(sss)`",
        "`SetActiveAsrProvider(s) -> b`",
        "`GetAsrTargetMenuState() -> ssssbsa(ssss)`",
        "`SetActiveAsrTarget(ss) -> b`",
        "`GetAsrDisplayMenuState() -> ssssbsa(sssss)`",
        "falls back to the stable registry/layout id",
        "scans the configured model root outside the runtime mutex",
        "flat Rust and legacy engine/model install layouts",
        "actual effective provider/model",
        "same non-blocking prepare-before-swap worker",
        "atomically persists the explicit or automatically discovered daemon config",
        "runtime-only selection",
    ] {
        assert!(
            dbus_doc.contains(required),
            "D-Bus docs must pin scene configuration extensions: {required}"
        );
    }
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
        "falls back to the primary-selection clipboard path",
        "multi-application live validation remains pending",
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
        "`global.duck_output_while_recording` defaults to `false`",
        "`global.duck_output_volume` defaults to `0.25`",
        "clamps finite parsed values to `0.0..=1.0` like legacy",
        "non-finite runtime values are rejected",
        "threshold `0.05..=0.95`",
        "minimum speech `0.05..=2.0` seconds",
        "speech padding at most `2000` ms",
        "online/streaming recognition does not use this trimmer",
        "`vinput-daemon --config data/default-config.json print-config`",
        "`$XDG_CONFIG_HOME/fcitx-vinput/config.json`",
        "`just daemon-default-config-smoke`",
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
        "live probes must only run when requested explicitly",
        "`PipeWireAudioRecorder` exists behind `pipewire-backend` as the live recorder seam",
        "One long-lived PipeWire worker owns the loop, context, and connected stream",
        "normal stops use `set_active(false)`",
        "uses `set_active(true)` instead of reconnecting",
        "Target changes rebuild the stream",
        "`VINPUT_CAPTURE_REUSE` defaults to enabled",
        "Cancellation and error cleanup always shut down the worker immediately",
        "attached directly to the PipeWire loop",
        "inactive streams have no sample path",
        "`VINPUT_CAPTURE_IDLE_DESTROY_MS`",
        "defaulting to 15,000 ms and capped at 600,000 ms",
        "invalid or negative values fall back to the default",
        "`0` destroys immediately",
        "a stale timeout cannot destroy a newly armed",
        "`PipeWireStartTiming` exposes `idle_gap_ms`, `create_stream_ms`, `set_active_ms`",
        "`stream_reused`, `created_new_stream`, and `start_total_ms`",
        "records `first_buffer_ms` exactly once through an atomic probe",
        "`capture_open_ms`, `session_create_ms`, and its own `start_total_ms`",
        "never include PCM samples, recognized text, provider credentials, or API keys",
        "`global.duck_output_while_recording` is enabled",
        "`wpctl get-volume @DEFAULT_AUDIO_SINK@`",
        "hard two-second timeout",
        "use direct argument passing without a shell",
        "never block recording",
        "Normal stop restores immediately after capture stops",
        "runtime drop all perform the same idempotent restore",
        "`PipeWireStreamConfig` records the selected capture target",
        "pinned `S16LE` 16 kHz mono PCM policy",
        "deterministic chunk planning use frames rather than raw sample count",
        "chunk helpers never split a frame across chunk boundaries",
        "selects delivery from the active backend descriptor",
        "legacy-compatible 800-frame batches",
        "capture begins before ASR session creation",
        "`CaptureStartGate` is installed first",
        "session-creation or gate-arming failure cancels the already-started capture",
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
        "Offline transducer, online transducer, Dolphin, SenseVoice, Paraformer, Qwen3 ASR, Moonshine v1, and Zipformer2 CTC are proven with real registry-model WAV samples",
        "Moonshine v1",
        "just sherpa-moonshine-local-smoke",
        "just sherpa-moonshine-dbus-reload-smoke",
        "target/effective separation during background preparation",
        "After early nightfall, the yellow lamps would light up here and there the squalid quarter of the brothels.",
        "Qwen3 ASR requires typed metadata for its convolution frontend, encoder, decoder, tokenizer, and generation parameters",
        "just sherpa-offline-local-smoke",
        "just sherpa-offline-transducer-local-smoke",
        "just sherpa-dolphin-local-smoke",
        "just sherpa-paraformer-local-smoke",
        "just sherpa-qwen3-local-smoke",
        "Esta prenda es amplia. Recomiendo elegir una talla menor al habitual.",
        "The daemon routes recorder callbacks to chunked ASR sessions",
        "legacy-compatible 800-frame batches",
        "emits deduplicated `RecognitionPartial` D-Bus signals during recording",
        "generation-scoped 40 ms poller",
        "Stop cancels the poller and suppresses a duplicate final partial",
        "online transducer and Zipformer2 CTC pass real WAV smokes with the 200 ms recognizer warmup",
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
        "`just daemon-unavailable-asr-smoke`",
        "unavailable backend that never fabricates text",
        "single non-blocking reload worker",
        "re-reads the daemon config file",
        "`reload_in_progress` remains true during physical preparation",
        "stale reload generations are discarded",
        "broader legacy sherpa families",
        "official native API is synchronous and exposes no safe cancellation handle",
        "`not_configured`, `enforced`, or `unsupported`",
        "Command ASR providers remain genuinely cancellable",
        "`runtime-status` runs by default after install and during `VINPUT_USER_STATUS=1` checks",
        "set `VINPUT_USER_RUNTIME_STATUS=0` only for file-placement debugging",
        "`MockAsrBackend` can attach a shared `MockAsrAudioLog` for deterministic tests",
        "800/800/tail chunk delivery",
        "real session-bus integration test proves that a partial signal arrives before `StopRecording`",
        "`ime-fcitx-native-live` proves one real acoustic PipeWire/Fcitx application path",
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

    for required in [
        "just pipewire-check",
        "just pipewire-live",
        "just addon-dbus-pipewire-live",
        "just ime-pipewire-live",
        "just ime-configured-pipewire-live",
        "just ime-fcitx-native-live",
        "just ime-gtk3-native-live normal",
        "just ime-qt6-native-live normal",
        "just ime-fcitx-focus-live",
        "just ime-fcitx-owner-loss-live",
        "VINPUT_LIVE_NATIVE_WAV=/path/to/speech.wav",
        "VINPUT_TEST_PIPEWIRE_CONTEXT=1",
        "VINPUT_TEST_PIPEWIRE_ENUMERATE=1",
        "VINPUT_TEST_PIPEWIRE_RECORD=1",
        "intentionally excluded from `just ci`",
        "without requiring a live PipeWire daemon",
        "just sherpa-offline-local-smoke",
        "just sherpa-online-local-smoke",
        "just sherpa-moonshine-dbus-reload-smoke",
    ] {
        assert!(
            development.contains(required),
            "development guide should document optional integration policy: {required}"
        );
    }

    for recipe in [
        "pipewire-check:",
        "pipewire-live:",
        "addon-dbus-pipewire-live:",
        "ime-pipewire-live:",
        "ime-configured-pipewire-live:",
        "ime-fcitx-native-live:",
        "ime-gtk3-native-live:",
        "ime-qt6-native-live:",
        "ime-fcitx-focus-live:",
        "ime-fcitx-owner-loss-live:",
        "sherpa-offline-local-smoke:",
        "sherpa-online-local-smoke:",
        "sherpa-moonshine-dbus-reload-smoke:",
    ] {
        assert!(justfile.contains(recipe), "justfile should define {recipe}");
    }

    let check_line = justfile
        .lines()
        .find(|line| line.starts_with("check:"))
        .expect("justfile should define check recipe");
    assert!(!check_line.contains("pipewire-live"));
    assert!(!check_line.contains("ime-pipewire-live"));
    assert!(!check_line.contains("ime-fcitx-native-live"));
}

#[test]
fn native_fcitx_live_gate_pins_real_client_outcomes() {
    let probe = std::fs::read_to_string(workspace_file("scripts/fcitx-live-client-probe.py"))
        .expect("read Fcitx live client probe");
    let runner = std::fs::read_to_string(workspace_file("scripts/run-ime-fcitx-native-live.sh"))
        .expect("read Fcitx native live runner");
    let justfile = std::fs::read_to_string(workspace_file("justfile")).expect("read justfile");

    for required in [
        "FcitxG",
        "update-client-side-ui",
        "delete-surrounding-text",
        "select_candidate",
        "partial_count",
        "candidate_count",
        "delete_count",
        "secondary_commit_count",
        "partial preedit leaked to the secondary context",
        "final commit leaked to the secondary context",
        "owner loss committed a partial result",
        "owner loss did not surface an unavailable preedit",
        "final commit did not match expected prefix",
        "expected_commit_prefix",
        "allow_direct_command_commit",
        "command mode did not replace selected text",
    ] {
        assert!(
            probe.contains(required),
            "live Fcitx client probe should pin outcome evidence: {required}"
        );
    }

    for required in [
        "VINPUT_LIVE_NATIVE_WAV",
        "VINPUT_LIVE_NATIVE_MODES",
        "VINPUT_LIVE_NATIVE_FOCUS_SWITCH",
        "VINPUT_LIVE_NATIVE_OWNER_LOSS",
        "VINPUT_LIVE_EXPECTED_TEXT_ADAPTER",
        "VINPUT_LIVE_EXPECTED_COMMIT_PREFIX",
        "target/tmp/ime-fcitx-native-live",
        "org.fcitx.Vinput must be idle",
        "trap restore_idle EXIT",
        "call_service StopRecording",
        "--mode \"${mode}\"",
        "--focus-switch",
        "--owner-loss",
        "--expected-commit-prefix",
        "--allow-direct-command-commit",
        "timeout 40s",
    ] {
        assert!(
            runner.contains(required),
            "live Fcitx runner should pin opt-in policy: {required}"
        );
    }
    for required in [
        "ime-fcitx-native-command-adapter-live:",
        "native-command-live-adapter",
        "adapter-backed:",
    ] {
        assert!(
            justfile.contains(required),
            "justfile should pin adapter-backed live recipe: {required}"
        );
    }
    let check_line = justfile
        .lines()
        .find(|line| line.starts_with("check:"))
        .expect("check recipe");
    assert!(!check_line.contains("ime-fcitx-native-command-adapter-live"));
}

#[test]
fn gtk3_live_probe_requires_real_toolkit_key_events() {
    let source = std::fs::read_to_string(workspace_file("scripts/gtk3-live-toolkit-probe.c"))
        .expect("read GTK3 live toolkit probe");
    let runner = std::fs::read_to_string(workspace_file("scripts/run-ime-gtk3-native-live.sh"))
        .expect("read GTK3 live toolkit runner");
    let justfile = std::fs::read_to_string(workspace_file("justfile")).expect("read justfile");

    for required in [
        "preedit-changed",
        "VINPUT_TOOLKIT_REQUIRE_PARTIAL",
        "manual_trigger\\\":true",
        "replacement_seen",
        "VINPUT_TOOLKIT_TIMEOUT_SECONDS",
    ] {
        assert!(
            source.contains(required),
            "missing GTK3 probe contract: {required}"
        );
    }
    for forbidden in [
        "gdk_event_new",
        "gtk_widget_event",
        "gtk_im_context_filter_keypress",
    ] {
        assert!(
            !source.contains(forbidden),
            "GTK3 live probe must not synthesize key events through {forbidden}"
        );
    }
    for required in [
        "GTK_IM_MODULE=fcitx",
        "VINPUT_LIVE_TOOLKIT_WAV",
        "org.fcitx.Vinput.Service.GetStatus",
        "target/tmp/ime-gtk3-native-live",
        "Use the real Fcitx shortcut",
    ] {
        assert!(
            runner.contains(required),
            "missing GTK3 runner contract: {required}"
        );
    }
    assert!(justfile.contains("toolkit-probe-check:"));
    assert!(justfile.contains("ime-gtk3-native-live:"));
    let check_line = justfile
        .lines()
        .find(|line| line.starts_with("check:"))
        .expect("check recipe");
    assert!(check_line.contains("toolkit-probe-check"));
    assert!(!check_line.contains("ime-gtk3-native-live"));
}

#[test]
fn qt6_live_probe_requires_real_toolkit_key_events() {
    let source = std::fs::read_to_string(workspace_file("scripts/qt6-live-toolkit-probe.cpp"))
        .expect("read Qt6 live toolkit probe");
    let runner = std::fs::read_to_string(workspace_file("scripts/run-ime-qt6-native-live.sh"))
        .expect("read Qt6 live toolkit runner");
    let justfile = std::fs::read_to_string(workspace_file("justfile")).expect("read justfile");

    for required in [
        "inputMethodEvent",
        "VINPUT_TOOLKIT_REQUIRE_PARTIAL",
        "manual_trigger",
        "replacement_seen",
        "VINPUT_TOOLKIT_TIMEOUT_SECONDS",
    ] {
        assert!(
            source.contains(required),
            "missing Qt6 probe contract: {required}"
        );
    }
    for forbidden in ["QKeyEvent", "sendEvent", "postEvent"] {
        assert!(
            !source.contains(forbidden),
            "Qt6 live probe must not synthesize key events through {forbidden}"
        );
    }
    for required in [
        "QT_IM_MODULE=fcitx",
        "VINPUT_LIVE_TOOLKIT_WAV",
        "org.fcitx.Vinput.Service.GetStatus",
        "target/tmp/ime-qt6-native-live",
        "Use the real Fcitx shortcut",
    ] {
        assert!(
            runner.contains(required),
            "missing Qt6 runner contract: {required}"
        );
    }
    assert!(justfile.contains("ime-qt6-native-live:"));
    let check_line = justfile
        .lines()
        .find(|line| line.starts_with("check:"))
        .expect("check recipe");
    assert!(!check_line.contains("ime-qt6-native-live"));
}

#[test]
fn fcitx_menu_live_probe_is_non_mutating() {
    let probe = std::fs::read_to_string(workspace_file("scripts/fcitx-live-menu-probe.py"))
        .expect("read Fcitx menu live probe");
    let runner = std::fs::read_to_string(workspace_file("scripts/run-ime-fcitx-menu-live.sh"))
        .expect("read Fcitx menu live runner");
    let justfile = std::fs::read_to_string(workspace_file("justfile")).expect("read justfile");

    for required in [
        "slash did not activate menu filter mode",
        "first Escape did not clear menu filter mode",
        "second Escape did not close the menu",
        "menu navigation unexpectedly committed text",
        "candidate_count",
        "commit_count",
    ] {
        assert!(
            probe.contains(required),
            "missing Fcitx menu outcome contract: {required}"
        );
    }
    for forbidden in ["select_candidate", "SetActiveScene", "SetActiveAsrTarget"] {
        assert!(
            !probe.contains(forbidden),
            "default menu probe must not mutate selection through {forbidden}"
        );
    }
    for required in [
        "VINPUT_LIVE_MENU_MODES",
        "VINPUT_LIVE_SCENE_MENU_KEY",
        "VINPUT_LIVE_ASR_MENU_KEY",
        "target/tmp/ime-fcitx-menu-live",
        "org.fcitx.Vinput must be idle",
    ] {
        assert!(
            runner.contains(required),
            "missing Fcitx menu runner contract: {required}"
        );
    }
    assert!(justfile.contains("ime-fcitx-menu-live:"));
    let check_line = justfile
        .lines()
        .find(|line| line.starts_with("check:"))
        .expect("check recipe");
    assert!(!check_line.contains("ime-fcitx-menu-live"));
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
        "just addon-dbus-asr-menu-smoke",
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
        "addon-dbus-asr-menu-smoke:",
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
        "Fcitx API integration",
        "persistent trigger and paging keys",
        "Tap/Hold/Both timing",
        "scene and installed-model-aware ASR menus",
        "selected-text collection and command-mode replacement",
        "Rust side owns runtime state, audio, ASR, text processing, registry operations",
        "`GetSceneState`/`SetActiveScene`",
        "`GetAsrDisplayMenuState`/`SetActiveAsrTarget`",
        "older target-menu methods remain",
        "full registry id",
        "fall back to stable ids without network access",
        "final commit remains driven by the synchronous stop reply",
        "Do not replace the Fcitx5 addon with a Rust addon",
        "SystemdService=vinput-daemon.service",
        "Per-user activation generated by the CLI remains direct-`Exec=`",
        "checked Arch `x86_64` recipe",
        "`docs/architecture/packaging-contract.md`",
    ] {
        assert!(
            target.contains(required),
            "target architecture should pin frontend boundary: {required}"
        );
    }
}

#[test]
fn packaging_architecture_pins_arch_release_boundary() {
    let packaging_doc = std::fs::read_to_string(architecture_dir().join("packaging-contract.md"))
        .expect("read packaging contract doc");
    let pkgbuild = std::fs::read_to_string(workspace_file("packaging/arch/PKGBUILD.in"))
        .expect("read Arch PKGBUILD template");
    let justfile = std::fs::read_to_string(workspace_file("justfile")).expect("read justfile");
    let metadata_check = std::fs::read_to_string(workspace_file("scripts/check-arch-pkgbuild.sh"))
        .expect("read Arch metadata check");
    let package_smoke =
        std::fs::read_to_string(workspace_file("scripts/run-arch-package-smoke.sh"))
            .expect("read Arch package smoke");
    let transaction_smoke = std::fs::read_to_string(workspace_file(
        "scripts/run-arch-package-transaction-smoke.sh",
    ))
    .expect("read Arch package transaction smoke");
    for required in [
        "checked Arch Linux `x86_64` template",
        "provides and conflicts with `fcitx5-vinput`",
        "official sherpa-onnx 1.13.3 Linux x64 shared-library archive",
        "`/usr/lib/fcitx-vinput`",
        "`$ORIGIN/../lib/fcitx-vinput`",
        "No ASR language model is bundled",
        "starts with an unavailable ASR backend",
        "use `DESTDIR` as the filesystem staging root",
        "must never be passed as `cmake --install --prefix`",
        "`just arch-pkgbuild-check`",
        "`just arch-package-smoke`",
        "`just arch-package-transaction-smoke`",
        "fakeroot-isolated pacman root",
        "same-version `pkgrel=2` to `pkgrel=1` rollback",
        "sentinel remains byte-identical",
        "rollback across versions with incompatible config or state",
    ] {
        assert!(
            packaging_doc.contains(required),
            "packaging contract should pin Arch release boundary: {required}"
        );
    }

    for required in [
        "pkgname=fcitx-vinput-rs",
        "arch=('x86_64')",
        "provides=(\"fcitx5-vinput=${pkgver}\")",
        "conflicts=('fcitx5-vinput')",
        "options=('!debug' '!lto')",
        "pipewire-backend,sherpa-onnx-backend",
        "libsherpa-onnx-c-api.so",
        "libonnxruntime.so",
        "patchelf --set-rpath '$ORIGIN/../lib/fcitx-vinput'",
        "--dbus --configured-backends --audio-backend pipewire",
        "data/vad/silero_vad.onnx",
    ] {
        assert!(
            pkgbuild.contains(required),
            "PKGBUILD template should pin release content: {required}"
        );
    }

    for recipe in [
        "arch-pkgbuild-check:",
        "arch-package-smoke:",
        "arch-package-transaction-smoke:",
    ] {
        assert!(justfile.contains(recipe), "justfile should define {recipe}");
    }
    assert!(metadata_check.contains("makepkg --printsrcinfo"));
    assert!(package_smoke.contains("makepkg --nodeps --noconfirm --force"));
    assert!(package_smoke.contains("makepkg --repackage --nodeps"));
    assert!(package_smoke.contains("run-arch-package-transaction-smoke.sh"));
    assert!(package_smoke.contains("patchelf --print-rpath"));
    assert!(package_smoke.contains("ldd \"${binary}\""));
    assert!(package_smoke.contains("build_root}/src"));
    assert!(transaction_smoke.contains("fakeroot pacman"));
    assert!(transaction_smoke.contains("-dd --noscriptlet -U"));
    assert!(transaction_smoke.contains("-dd --noscriptlet -R"));
    assert_eq!(
        transaction_smoke
            .matches("-U \"${initial_package}\"")
            .count(),
        2
    );
    assert!(transaction_smoke.contains("preserve-user-config"));
    assert!(transaction_smoke.contains("-Qkk"));
}

#[test]
fn packaging_architecture_pins_explicit_daemon_handoff_boundary() {
    let packaging_doc = std::fs::read_to_string(architecture_dir().join("packaging-contract.md"))
        .expect("read packaging contract doc");
    let dbus_doc = std::fs::read_to_string(architecture_dir().join("dbus-service.md"))
        .expect("read D-Bus service doc");
    let justfile = std::fs::read_to_string(workspace_file("justfile")).expect("read justfile");
    let diagnostics_smoke = std::fs::read_to_string(workspace_file(
        "scripts/run-daemon-handoff-diagnostics-smoke.sh",
    ))
    .expect("read daemon handoff diagnostics smoke");
    let handoff_smoke =
        std::fs::read_to_string(workspace_file("scripts/run-daemon-handoff-smoke.sh"))
            .expect("read daemon handoff smoke");

    for required in [
        "different executable path",
        "executable inode unlinked by package replacement",
        "explicit `vinput daemon handoff` command",
        "restarts the systemd user service only for those stale states",
        "fresh matching owner",
        "current owners are a strict no-op",
        "service-control failures leave the owner untouched",
        "Automatic package-manager invocation",
    ] {
        assert!(
            packaging_doc.contains(required),
            "packaging contract should pin daemon handoff boundary: {required}"
        );
    }
    for required in [
        "explicit mutation boundary",
        "does nothing when the owner is current",
        "systemctl --user restart vinput-daemon.service",
        "polls fresh D-Bus status",
        "never kills a PID directly",
        "contacts neither D-Bus nor systemd",
    ] {
        assert!(
            dbus_doc.contains(required),
            "D-Bus contract should pin daemon handoff behavior: {required}"
        );
    }
    for recipe in ["daemon-handoff-diagnostics-smoke:", "daemon-handoff-smoke:"] {
        assert!(justfile.contains(recipe), "justfile should define {recipe}");
    }
    assert!(diagnostics_smoke.contains("owner-executable-path-mismatch"));
    assert!(diagnostics_smoke.contains("owner-executable-deleted"));
    assert!(diagnostics_smoke.contains("automatic_restart_performed == false"));
    assert!(diagnostics_smoke.contains("run vinput daemon handoff"));
    assert!(handoff_smoke.contains("systemctl-must-not-run"));
    assert!(handoff_smoke.contains("systemctl-restart"));
    assert!(handoff_smoke.contains("verification.status == \"current-owner\""));
    assert!(handoff_smoke.contains("exit 19"));
    assert!(handoff_smoke.contains("kill -0 \"${old_pid}\""));
}

#[test]
fn packaging_architecture_pins_message_only_lifecycle_hooks() {
    let packaging_doc = std::fs::read_to_string(architecture_dir().join("packaging-contract.md"))
        .expect("read packaging contract doc");
    let pkgbuild = std::fs::read_to_string(workspace_file("packaging/arch/PKGBUILD.in"))
        .expect("read Arch PKGBUILD template");
    let install_script =
        std::fs::read_to_string(workspace_file("packaging/arch/fcitx-vinput-rs.install"))
            .expect("read Arch install script");
    let renderer = std::fs::read_to_string(workspace_file("scripts/render-arch-pkgbuild.py"))
        .expect("read Arch PKGBUILD renderer");
    let install_check =
        std::fs::read_to_string(workspace_file("scripts/check-arch-install-script.sh"))
            .expect("read Arch install script check");
    let pkgbuild_check = std::fs::read_to_string(workspace_file("scripts/check-arch-pkgbuild.sh"))
        .expect("read Arch PKGBUILD check");
    let package_smoke =
        std::fs::read_to_string(workspace_file("scripts/run-arch-package-smoke.sh"))
            .expect("read Arch package smoke");
    let justfile = std::fs::read_to_string(workspace_file("justfile")).expect("read justfile");

    for required in [
        "Package transaction messages",
        "execute no `systemctl --user`, `fcitx5`, or `vinput` command",
        "package transaction cannot restart every user session",
        "user config, models, and cache are preserved",
        "complete makepkg inputs",
        "message-only",
    ] {
        assert!(
            packaging_doc.contains(required),
            "packaging contract should pin lifecycle hook boundary: {required}"
        );
    }
    assert!(pkgbuild.contains("install=fcitx-vinput-rs.install"));
    for hook in ["post_install()", "post_upgrade()", "post_remove()"] {
        assert!(install_script.contains(hook));
    }
    assert!(install_script.contains("vinput daemon handoff"));
    assert!(install_script.contains("intentionally preserved"));
    assert!(renderer.contains("shutil.copyfile"));
    assert!(renderer.contains("args.install_script.name"));
    assert!(renderer.contains("REPOSITORY_ROOT = Path(__file__).resolve().parent.parent"));
    assert!(install_check.contains("PATH=/definitely/missing"));
    assert!(pkgbuild_check.contains("nested/PKGBUILD"));
    assert!(install_check.contains("^[[:space:]]*(systemctl|fcitx5|vinput)"));
    assert!(package_smoke.contains("bsdtar -xOf \"${package_archive}\" .INSTALL"));
    assert!(justfile.contains("arch-install-script-check:"));
}

#[test]
fn packaging_architecture_pins_local_repository_integration() {
    let packaging_doc = std::fs::read_to_string(architecture_dir().join("packaging-contract.md"))
        .expect("read packaging contract doc");
    let repository_smoke =
        std::fs::read_to_string(workspace_file("scripts/run-arch-repository-smoke.sh"))
            .expect("read Arch repository smoke");
    let package_smoke =
        std::fs::read_to_string(workspace_file("scripts/run-arch-package-smoke.sh"))
            .expect("read Arch package smoke");
    let justfile = std::fs::read_to_string(workspace_file("justfile")).expect("read justfile");

    for required in [
        "local `repo-add` database",
        "`file://` pacman repository",
        "installs the package with `pacman -S`",
        "replaces the repository entry with `pkgrel=2`",
        "pacman's cache",
        "`SigLevel = Never`",
        "signing and trust policy remain external-publication work",
        "local repository metadata/install/upgrade behavior",
        "externally hosted repository publication",
    ] {
        assert!(
            packaging_doc.contains(required),
            "packaging contract should pin local repository boundary: {required}"
        );
    }
    for required in [
        "repo-add \"${repository_database}\"",
        "Server = file://${repository_root}",
        "-Si fcitx-vinput-rs",
        "-Sdd --noscriptlet fcitx-vinput-rs",
        "cache_path}/$(basename \"${initial_package}\")",
        "cache_path}/$(basename \"${upgrade_package}\")",
        "preserve-user-config",
    ] {
        assert!(
            repository_smoke.contains(required),
            "repository smoke should prove: {required}"
        );
    }
    assert!(package_smoke.contains("run-arch-repository-smoke.sh"));
    assert!(justfile.contains("arch-repository-smoke:"));
}

#[test]
fn packaging_architecture_pins_signed_repository_trust() {
    let packaging_doc = std::fs::read_to_string(architecture_dir().join("packaging-contract.md"))
        .expect("read packaging contract doc");
    let signing_smoke =
        std::fs::read_to_string(workspace_file("scripts/run-arch-signing-smoke.sh"))
            .expect("read Arch signing smoke");
    let package_smoke =
        std::fs::read_to_string(workspace_file("scripts/run-arch-package-smoke.sh"))
            .expect("read Arch package smoke");
    let justfile = std::fs::read_to_string(workspace_file("justfile")).expect("read justfile");

    for required in [
        "ephemeral Ed25519 signing key",
        "SigLevel = Required DatabaseRequired",
        "SHA-256 Sum  Signature",
        "signer is absent from another isolated keyring",
        "same-size byte-flipped package",
        "invalid PGP signature",
        "No private key, fingerprint, or trust database is checked into the repository",
        "Production key custody",
        "ephemeral signed-repository trust/tamper enforcement",
        "production signing-key custody/rotation/revocation",
    ] {
        assert!(
            packaging_doc.contains(required),
            "packaging contract should pin signing boundary: {required}"
        );
    }
    for required in [
        "--quick-generate-key",
        "--include-sigs --sign --key",
        "--include-sigs --verify --sign --key",
        "fakeroot pacman-key --gpgdir",
        "--lsign-key",
        "SigLevel = Required DatabaseRequired",
        "Validated By    : SHA-256 Sum  Signature",
        "invalid or corrupted database (PGP signature)",
        "data[len(data) // 2] ^= 0x01",
        "invalid or corrupted package (PGP signature)",
        "target/tmp/arch-signing-smoke",
    ] {
        assert!(
            signing_smoke.contains(required),
            "signing smoke should prove: {required}"
        );
    }
    assert!(package_smoke.contains("run-arch-signing-smoke.sh"));
    assert!(justfile.contains("arch-signing-smoke:"));
}

#[test]
fn packaging_architecture_pins_release_artifact_inventory() {
    let packaging_doc = std::fs::read_to_string(architecture_dir().join("packaging-contract.md"))
        .expect("read packaging contract doc");
    let manifest_tool = std::fs::read_to_string(workspace_file("scripts/release_manifest.py"))
        .expect("read release manifest tool");
    let manifest_check =
        std::fs::read_to_string(workspace_file("scripts/check-release-manifest.sh"))
            .expect("read release manifest check");
    let bundle_smoke =
        std::fs::read_to_string(workspace_file("scripts/run-arch-release-bundle-smoke.sh"))
            .expect("read Arch release bundle smoke");
    let package_smoke =
        std::fs::read_to_string(workspace_file("scripts/run-arch-package-smoke.sh"))
            .expect("read Arch package smoke");
    let justfile = std::fs::read_to_string(workspace_file("justfile")).expect("read justfile");

    for required in [
        "Release artifact inventory",
        "flat release-artifact directory",
        "one unique role per artifact",
        "`SHA256SUMS` is sorted by artifact name",
        "rejects duplicate roles or basenames",
        "may replace only an existing bundle that already passes",
        "never publishes a partial bundle",
        "`just release-manifest-check`",
        "exactly 13 release-gate artifacts",
        "package-pkgrel2-test",
        "signing-public-key-test",
        "not a public release set",
        "external pinned key",
    ] {
        assert!(
            packaging_doc.contains(required),
            "packaging contract should pin release inventory boundary: {required}"
        );
    }

    for required in [
        "MANIFEST_NAME = \"manifest.json\"",
        "CHECKSUMS_NAME = \"SHA256SUMS\"",
        "MANIFEST_SIGNATURE_NAME = \"manifest.json.sig\"",
        "OPTIONAL_METADATA_FILES",
        "SCHEMA_VERSION = 1",
        "duplicate artifact role",
        "artifact must not be inside the output directory",
        "verify_bundle(bundle)",
        "tempfile.mkdtemp",
        "release bundle inventory mismatch",
    ] {
        assert!(
            manifest_tool.contains(required),
            "release manifest tool should pin: {required}"
        );
    }

    for required in [
        "mutated-artifact",
        "symlink-artifact",
        "nested-directory",
        "force-arbitrary",
        "inside-output",
        "duplicate-role",
    ] {
        assert!(
            manifest_check.contains(required),
            "release manifest check should prove: {required}"
        );
    }

    for required in [
        "--artifact \"source-archive=",
        "--artifact \"package-pkgrel2-test=",
        "--artifact \"signing-public-key-test=",
        "sha256sum -c SHA256SUMS",
        "sign-release-manifest.sh",
        "verify-release-bundle-signature.sh",
        "unexpected.key",
        "artifact digest mismatch",
    ] {
        assert!(
            bundle_smoke.contains(required),
            "release bundle smoke should prove: {required}"
        );
    }
    assert!(package_smoke.contains("run-arch-release-bundle-smoke.sh"));
    assert!(justfile.contains("release-manifest-check:"));
    assert!(justfile.contains("arch-release-bundle-smoke:"));
}

#[test]
fn packaging_architecture_pins_detached_manifest_trust_root() {
    let packaging_doc = std::fs::read_to_string(architecture_dir().join("packaging-contract.md"))
        .expect("read packaging contract doc");
    let sign_script = std::fs::read_to_string(workspace_file("scripts/sign-release-manifest.sh"))
        .expect("read release manifest signing script");
    let verify_script =
        std::fs::read_to_string(workspace_file("scripts/verify-release-bundle-signature.sh"))
            .expect("read release bundle signature verifier");
    let signature_check =
        std::fs::read_to_string(workspace_file("scripts/check-release-signature.sh"))
            .expect("read release signature check");
    let bundle_smoke =
        std::fs::read_to_string(workspace_file("scripts/run-arch-release-bundle-smoke.sh"))
            .expect("read Arch release bundle smoke");
    let justfile = std::fs::read_to_string(workspace_file("justfile")).expect("read justfile");

    for required in [
        "`manifest.json.sig` is optional metadata",
        "avoids a recursive digest/signature dependency",
        "caller-supplied GPG home",
        "exact primary fingerprint",
        "publishes it atomically as mode `0644`",
        "public-key file from outside the bundle",
        "independently pinned fingerprint",
        "disables automatic key retrieval",
        "matching `VALIDSIG`",
        "never trusts a key merely because the same bundle contains a copy",
        "`just release-signature-check`",
        "unsigned-after-rebuild boundary",
        "independent trusted channel",
    ] {
        assert!(
            packaging_doc.contains(required),
            "packaging contract should pin detached trust root: {required}"
        );
    }

    for required in [
        "exact secret signing key is unavailable",
        "mktemp",
        "--local-user \"${fingerprint}\"",
        "--detach-sign",
        "chmod 644",
        "mv -f",
    ] {
        assert!(
            sign_script.contains(required),
            "signing script should pin: {required}"
        );
    }
    for required in [
        "public key must come from outside the bundle",
        "--no-auto-key-retrieve",
        "--status-fd=1",
        "VALIDSIG",
        "fingerprint does not match the expected trust root",
        "release_manifest.py verify",
    ] {
        assert!(
            verify_script.contains(required),
            "verifier should pin: {required}"
        );
    }
    for required in [
        "missing-signature",
        "wrong-fingerprint",
        "wrong-key",
        "inside-key",
        "manifest-tamper",
        "signature-tamper",
        "artifact-tamper",
        "rebuilt-unsigned",
    ] {
        assert!(
            signature_check.contains(required),
            "signature check should prove: {required}"
        );
    }
    assert!(bundle_smoke.contains("sign-release-manifest.sh"));
    assert!(bundle_smoke.contains("verify-release-bundle-signature.sh"));
    assert!(bundle_smoke.contains("signature-tampered-bundle"));
    assert!(justfile.contains("release-signature-check:"));
}

#[test]
fn packaging_architecture_pins_release_candidate_promotion() {
    let packaging_doc = std::fs::read_to_string(architecture_dir().join("packaging-contract.md"))
        .expect("read packaging contract doc");
    let prepare_script =
        std::fs::read_to_string(workspace_file("scripts/prepare-arch-release-candidate.sh"))
            .expect("read release candidate preparation script");
    let verify_script =
        std::fs::read_to_string(workspace_file("scripts/verify-arch-release-candidate.sh"))
            .expect("read release candidate verifier");
    let candidate_check =
        std::fs::read_to_string(workspace_file("scripts/check-arch-release-candidate.sh"))
            .expect("read release candidate check");
    let bundle_smoke =
        std::fs::read_to_string(workspace_file("scripts/run-arch-release-bundle-smoke.sh"))
            .expect("read Arch release bundle smoke");
    let justfile = std::fs::read_to_string(workspace_file("justfile")).expect("read justfile");

    for required in [
        "Release candidate promotion",
        "explicit promotion boundary",
        "exact 13-role gate policy",
        "selects only `package-pkgrel1`",
        "rebuilds fresh `repo-add` database/files metadata",
        "exactly 11 production roles",
        "containing `test` or `synthetic` are rejected",
        "prevents the synthetic `pkgrel=2` upgrade fixture",
        "`--force` first verifies the old candidate",
        "`just release-candidate-check`",
        "not itself a public release",
        "publish only the verified candidate directory",
    ] {
        assert!(
            packaging_doc.contains(required),
            "packaging contract should pin candidate promotion: {required}"
        );
    }

    for required in [
        "expected_gate_roles",
        "package-pkgrel1",
        "package-signature-pkgrel1",
        "local args=(--sign --key",
        "repo-add --help 2>&1",
        "args=(--include-sigs",
        "signing-public-key=${candidate_public_key}",
        "verify-arch-release-candidate.sh",
        "output must not be the signed gate bundle",
        "output must not be a symlink",
    ] {
        assert!(
            prepare_script.contains(required),
            "candidate preparation should pin: {required}"
        );
    }
    for required in [
        "expected_roles",
        "production candidate policy",
        "package version",
        "repository database must contain exactly one package",
        "repository files index must contain exactly one package",
        "signing-public-key",
        "--no-auto-key-retrieve",
    ] {
        assert!(
            verify_script.contains(required),
            "candidate verifier should pin: {required}"
        );
    }
    for required in [
        "make_package 1",
        "make_package 2",
        "existing-output",
        "gate-is-not-candidate",
        "output-inside-gate",
        "invalid-force",
        "mutated-candidate",
    ] {
        assert!(
            candidate_check.contains(required),
            "candidate check should prove: {required}"
        );
    }
    assert!(bundle_smoke.contains("prepare-arch-release-candidate.sh"));
    assert!(bundle_smoke.contains("0.1.0-2|test|synthetic"));
    assert!(justfile.contains("release-candidate-check:"));
}

#[test]
fn migration_summaries_delegate_packaging_details_to_contract() {
    for relative_path in [
        "README.md",
        "docs/migration/function-gap-audit.md",
        "docs/migration/e2e-capability-matrix.md",
        "docs/migration/e2e-replication-plan.md",
    ] {
        let summary = std::fs::read_to_string(workspace_file(relative_path))
            .unwrap_or_else(|error| panic!("read {relative_path}: {error}"));
        assert!(
            summary.contains("architecture/packaging-contract.md"),
            "{relative_path} should delegate detailed packaging evidence to the contract"
        );
        for duplicated_detail in [
            "same-version-rollback",
            "test-role-free candidate promotion",
            "detached manifest signing against an external pinned key",
        ] {
            assert!(
                !summary.contains(duplicated_detail),
                "{relative_path} should not duplicate packaging detail: {duplicated_detail}"
            );
        }
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
    assert!(registry_doc.contains("`scan_installed_models`"));
    assert!(registry_doc.contains("`<root>/<managed-name>/vinput-model.json`"));
    assert!(registry_doc.contains("`<root>/<engine>/<name>/vinput-model.json`"));
    assert!(registry_doc.contains("stable `model.<engine>.<name>` id"));
    assert!(registry_doc.contains("optional installed `display` metadata"));
    assert!(registry_doc.contains("full registry id"));
    assert!(registry_doc.contains("locale-keyed titles"));
    assert!(registry_doc.contains("`InstalledModelInfo::stable_model_id`"));
    assert!(registry_doc.contains("`display_title`"));
    assert!(registry_doc.contains("registry `en_US`, the requested registry locale"));
    assert!(registry_doc.contains("`$XDG_CONFIG_HOME/vinput/i18n.local.json`"));
    assert!(registry_doc.contains("local overrides replace both"));
    assert!(
        registry_doc
            .contains("Missing preferred localization still keeps available `en_US` entries")
    );
    assert!(registry_doc.contains("malformed automatic local override is diagnostic-only"));
    assert!(registry_doc.contains("an explicitly requested `--i18n` file still fails"));
    assert!(registry_doc.contains("`local > preferred > fallback` priority"));
    assert!(registry_doc.contains("`LANGUAGE`, `LC_ALL`, `LC_MESSAGES`, then `LANG`"));
    assert!(registry_doc.contains("falling back to `en_US`"));
    assert!(registry_doc.contains("`zh` expands to `zh_CN`"));
    assert!(registry_doc.contains("`en` expands to `en_US`"));
    assert!(registry_doc.contains("`C`/`POSIX` are skipped"));
    assert!(registry_doc.contains("An explicit `--locale` remains authoritative"));
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
        "Do not maintain parity percentages in multiple files",
    ] {
        assert!(
            docs_readme.contains(required),
            "docs README should pin source-of-truth rules: {required}"
        );
    }

    for required in [
        "usable CLI/daemon alpha",
        "Current registry native ASR families",
        "Generic native user install",
        "real desktop native-dictation alpha",
        "does not assign a release percentage",
    ] {
        assert!(
            audit.contains(required),
            "function gap audit should pin current evidence: {required}"
        );
    }

    for required in [
        "Completed: usable CLI/daemon alpha",
        "P0: real desktop native alpha",
        "Implemented through D-Bus",
        "deduplicated live `RecognitionPartial` signals",
        "Execute the checked GTK3, Qt6, focus-handoff, owner-loss, menu, and native command-adapter probes",
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
        "Run and retain normal/command evidence from the GTK3 and Qt6 probes",
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

#[test]
fn native_user_install_pins_runtime_bundle_activation() {
    let readme = std::fs::read_to_string(workspace_file("README.md")).expect("read README");
    let development = std::fs::read_to_string(workspace_file("docs/development.md"))
        .expect("read development guide");
    let asr_doc = std::fs::read_to_string(architecture_dir().join("asr-contract.md"))
        .expect("read ASR architecture doc");
    let live_doc =
        std::fs::read_to_string(workspace_file("docs/migration/live-desktop-validation.md"))
            .expect("read live desktop validation doc");
    let matrix = std::fs::read_to_string(workspace_file("docs/migration/e2e-capability-matrix.md"))
        .expect("read capability matrix");
    let install = std::fs::read_to_string(workspace_file("scripts/install-user-ime.sh"))
        .expect("read user install script");
    let native_smoke = std::fs::read_to_string(workspace_file(
        "scripts/run-user-ime-sherpa-native-smoke.sh",
    ))
    .expect("read generic native user smoke");
    let sherpa_smoke = std::fs::read_to_string(workspace_file(
        "scripts/run-user-ime-sherpa-sense-voice-smoke.sh",
    ))
    .expect("read shared sherpa user smoke");

    for required in [
        "sherpa-native-live",
        "sherpa-sense-voice-live",
        "profile_cli_features",
        "profile_daemon_features",
        "VINPUT_USER_SHERPA_RUNTIME_LIB_DIR",
        "libsherpa-onnx-c-api.so",
        "libonnxruntime.so",
        "vinput-daemon-with-vinput-env.sh",
        "LD_LIBRARY_PATH",
        "installed daemon is missing or not executable",
        "with_native_runtime \"${daemon_path}\"",
        "VINPUT_USER_NATIVE_WAV",
        "runtime-status",
        "VINPUT_USER_RUNTIME_ACTIVATION",
        "runtime_activation_service_path",
        "publish_runtime_activation_service",
        "XDG_RUNTIME_DIR",
    ] {
        assert!(
            install.contains(required),
            "native installer should pin runtime activation contract: {required}"
        );
    }
    assert!(
        install
            .matches("'pipewire-backend,sherpa-onnx-backend'")
            .count()
            >= 2,
        "native CLI and daemon builds should enable the same sherpa runtime feature"
    );

    for required in [
        "VINPUT_TEST_SHERPA_PROFILE=sherpa-native-live",
        "run-user-ime-sherpa-sense-voice-smoke.sh",
    ] {
        assert!(
            native_smoke.contains(required),
            "generic native smoke wrapper should pin {required}"
        );
    }
    for required in [
        "runtime_source_dir=",
        "vinput-daemon-with-vinput-env.sh",
        "Exec=${daemon_wrapper_path} --dbus",
        "LD_LIBRARY_PATH=${runtime_lib_dir}",
        r#""family": "transducer""#,
        r#""runtime": "online""#,
        "sherpa-sense-voice-live",
        "installed native sherpa runtime library is missing",
        "VINPUT_USER_REMOVE=1",
    ] {
        assert!(
            sherpa_smoke.contains(required),
            "shared native installer smoke should cover {required}"
        );
    }

    for document in [&readme, &development, &asr_doc, &live_doc, &matrix] {
        for required in [
            "sherpa-native-live",
            "vinput-daemon-with-vinput-env.sh",
            "libsherpa-onnx",
            "libonnxruntime",
            "user-ime-sherpa-native-activation-smoke",
        ] {
            assert!(
                document.contains(required),
                "native install docs should pin runtime bundle contract: {required}"
            );
        }
    }
}

#[test]
fn native_command_profile_pins_adapter_contract() {
    let install = std::fs::read_to_string(workspace_file("scripts/install-user-ime.sh"))
        .expect("read user install script");
    let native_command_smoke = std::fs::read_to_string(workspace_file(
        "scripts/run-user-ime-sherpa-native-command-smoke.sh",
    ))
    .expect("read native command user smoke");
    let sherpa_smoke = std::fs::read_to_string(workspace_file(
        "scripts/run-user-ime-sherpa-sense-voice-smoke.sh",
    ))
    .expect("read shared sherpa user smoke");
    let readme = std::fs::read_to_string(workspace_file("README.md")).expect("read README");
    let live_doc =
        std::fs::read_to_string(workspace_file("docs/migration/live-desktop-validation.md"))
            .expect("read live desktop validation doc");

    for required in [
        "sherpa-native-command-live",
        "command_adapter",
        "native-command-live-adapter",
        "adapter-backed:",
    ] {
        assert!(
            install.contains(required),
            "native command installer should pin adapter contract: {required}"
        );
        assert!(
            sherpa_smoke.contains(required),
            "native command smoke should cover adapter contract: {required}"
        );
    }
    for required in [
        "VINPUT_TEST_SHERPA_PROFILE=sherpa-native-command-live",
        "run-user-ime-sherpa-sense-voice-smoke.sh",
    ] {
        assert!(
            native_command_smoke.contains(required),
            "native command smoke wrapper should pin {required}"
        );
    }
    for document in [&readme, &live_doc] {
        for required in [
            "sherpa-native-command-live",
            "ime-fcitx-native-command-adapter-live",
            "adapter-backed:",
        ] {
            assert!(
                document.contains(required),
                "native command docs should pin live adapter contract: {required}"
            );
        }
    }
}

#[test]
fn live_fcitx_restart_commands_daemonize_the_replacement() {
    for relative_path in [
        "scripts/install-user-ime.sh",
        "scripts/setup-live-command-demo-ime.sh",
        "scripts/run-ime-fcitx-live-probe.sh",
        "docs/migration/live-desktop-validation.md",
    ] {
        let contents = std::fs::read_to_string(workspace_file(relative_path))
            .unwrap_or_else(|error| panic!("read {relative_path}: {error}"));
        assert!(
            contents.contains("-dr"),
            "{relative_path} should restart Fcitx in daemon mode"
        );
        assert!(
            !contents.contains("fcitx5-with-vinput-env.sh\" -r")
                && !contents.contains("${fcitx_env_wrapper} -r")
                && !contents.contains("${wrapper} -r"),
            "{relative_path} should not recommend a foreground replacement"
        );
    }
}

#[test]
fn native_user_activation_pins_owner_and_recognition_roundtrip() {
    let activation_smoke = std::fs::read_to_string(workspace_file(
        "scripts/run-user-ime-sherpa-native-activation-smoke.sh",
    ))
    .expect("read native activation smoke");
    let owner_smoke = std::fs::read_to_string(workspace_file(
        "scripts/run-user-ime-activation-owner-smoke.sh",
    ))
    .expect("read activation owner smoke");
    let frontend_smoke = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/tests/native_frontend_bridge_dbus_smoke.cpp",
    ))
    .expect("read native frontend bridge smoke");
    let addon_smoke = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/tests/native_fcitx_addon_dbus_smoke.cpp",
    ))
    .expect("read native Fcitx addon smoke");
    let dbus_doc = std::fs::read_to_string(architecture_dir().join("dbus-service.md"))
        .expect("read D-Bus architecture doc");
    let justfile = std::fs::read_to_string(workspace_file("justfile")).expect("read justfile");

    for required in [
        "VINPUT_SHERPA_EXPECT_TEXT",
        "target/debug/vinput daemon status --json",
        "owner[\"process\"][\"exe\"]",
        "target/debug/vinput recording start --json",
        "target/debug/vinput recording stop --json",
        "unexpected native activation recognition",
    ] {
        assert!(
            activation_smoke.contains(required),
            "native activation smoke should pin {required}"
        );
    }

    for required in [
        "target/debug/vinput daemon status --json",
        "owner[\"ok\"] is True",
        "effective_provider_id",
    ] {
        assert!(
            owner_smoke.contains(required),
            "activation owner smoke should pin {required}"
        );
    }

    for required in [
        "after the first successful service method call",
        "owner: null",
        "user-ime-activation-owner-smoke",
    ] {
        assert!(
            dbus_doc.contains(required),
            "D-Bus architecture should pin activation owner ordering: {required}"
        );
    }

    for required in [
        "FrontendBridge bridge",
        "bridge.StartNormal(client.get(), \"raw\")",
        "BridgeOutcome::Kind::Commit",
        "CandidateSource::Raw",
        "ApplyBridgeOutcomeToSink(start, sink)",
        "ApplyBridgeOutcomeToSink(stop, sink)",
        "clear-candidates",
        "clear-preedit",
        "native frontend commit",
    ] {
        assert!(
            frontend_smoke.contains(required),
            "native frontend bridge smoke should pin {required}"
        );
    }

    for required in [
        "FcitxVinputAddon addon(nullptr, &signal_bus)",
        "FcitxTriggerAction::StartNormal",
        "FcitxTriggerAction::StopNormal",
        "TestInputContext input_context(manager)",
        "commitStringImpl",
        "native addon InputContext commit",
    ] {
        assert!(
            addon_smoke.contains(required),
            "native addon smoke should pin {required}"
        );
    }

    assert!(justfile.contains("user-ime-activation-owner-smoke:"));
    assert!(justfile.contains("user-ime-sherpa-native-smoke:"));
    assert!(justfile.contains("user-ime-sherpa-native-activation-smoke:"));
    assert!(justfile.contains("sherpa-online-transducer-user-activation-smoke:"));
    assert!(justfile.contains("sherpa-online-transducer-user-frontend-smoke:"));
    assert!(justfile.contains("sherpa-online-transducer-user-addon-smoke:"));
    assert!(justfile.contains("user-ime-sherpa-native-smoke"));
}

#[test]
fn native_command_fallback_pins_selected_text_candidates() {
    let command_source =
        std::fs::read_to_string(workspace_file("crates/vinput-text/src/command.rs"))
            .expect("read command text processor");
    let payload_source = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/src/recognition_payload.cpp",
    ))
    .expect("read recognition payload source");
    let bridge_source =
        std::fs::read_to_string(workspace_file("cpp/fcitx5-addon/src/frontend_bridge.cpp"))
            .expect("read frontend bridge source");
    let addon_smoke = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/tests/native_fcitx_addon_dbus_smoke.cpp",
    ))
    .expect("read native addon smoke");
    let justfile = std::fs::read_to_string(workspace_file("justfile")).expect("read justfile");

    for required in [
        "request.scene.id == COMMAND_SCENE_ID",
        "command_mode_payload(",
        "request.selected_text.unwrap_or_default()",
        "std::iter::empty()",
    ] {
        assert!(
            command_source.contains(required),
            "command fallback should pin {required}"
        );
    }
    for required in [
        "command_mode && payload.candidates.size() > 1",
        "ShouldShowCandidateMenu(payload, command_mode)",
    ] {
        assert!(
            payload_source.contains(required),
            "command candidate policy should pin {required}"
        );
    }
    assert!(bridge_source.contains("MakeCommitPlan(payload_json, was_command_mode)"));
    for required in [
        "VINPUT_NATIVE_ADDON_SELECTED_TEXT",
        "FcitxTriggerAction::StartCommand",
        "FcitxTriggerAction::StopCommand",
        "AppliedOutcome::CandidateMenu",
        "candidate_list->candidate(1).select(&input_context)",
        "input_context.deleted",
        "native addon command replacement",
    ] {
        assert!(
            addon_smoke.contains(required),
            "native command addon smoke should pin {required}"
        );
    }
    assert!(justfile.contains("sherpa-online-transducer-user-command-addon-smoke:"));
}

#[test]
fn native_input_context_sink_pins_real_fcitx_calls() {
    let outcome_source =
        std::fs::read_to_string(workspace_file("cpp/fcitx5-addon/src/fcitx_outcome.cpp"))
            .expect("read Fcitx outcome sink");
    let input_context_smoke = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/tests/fcitx_input_context_outcome_smoke.cpp",
    ))
    .expect("read InputContext outcome smoke");

    for required in [
        "if (!text.empty())",
        "input_context_->deleteSurroundingText",
        "input_context_->commitString",
    ] {
        assert!(
            outcome_source.contains(required),
            "production InputContext sink should pin {required}"
        );
    }
    for required in [
        "ApplyBridgeOutcomeToInputContext",
        "deleteSurroundingTextImpl",
        "commitStringImpl",
        "candidate_list->candidate(1).select(&input_context)",
        "inputPanel().preedit().empty()",
    ] {
        assert!(
            input_context_smoke.contains(required),
            "InputContext outcome smoke should pin {required}"
        );
    }
}

#[test]
fn native_partial_preedit_pins_activation_safe_streaming() {
    let audio_source = std::fs::read_to_string(workspace_file("crates/vinput-audio/src/lib.rs"))
        .expect("read audio source recorder");
    let monitor_source = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/src/fcitx_daemon_signal_monitor.cpp",
    ))
    .expect("read daemon signal monitor");
    let monitor_smoke = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/tests/fcitx_daemon_signal_monitor_smoke.cpp",
    ))
    .expect("read daemon signal monitor smoke");
    let addon_smoke = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/tests/native_fcitx_addon_dbus_smoke.cpp",
    ))
    .expect("read native addon smoke");
    let asr_doc = std::fs::read_to_string(architecture_dir().join("asr-contract.md"))
        .expect("read ASR contract doc");

    for required in [
        "pending_capture",
        "self.chunk_callback.is_some()",
        "self.deliver_capture(&captured)",
        "self.pending_capture.take()",
    ] {
        assert!(
            audio_source.contains(required),
            "source-backed streaming should pin {required}"
        );
    }
    for required in [
        "NameOwnerChanged",
        "serviceOwner",
        "message.sender() == service_owner_",
        "fcitx::dbus::MessageType::Signal",
    ] {
        assert!(
            monitor_source.contains(required),
            "activation-safe monitor should pin {required}"
        );
    }
    assert!(
        monitor_smoke.find("FcitxDaemonSignalMonitor monitor")
            < monitor_smoke.find("sender.requestName"),
        "monitor smoke should subscribe before daemon activation"
    );
    for required in [
        "FcitxVinputAddon addon(nullptr, &signal_bus)",
        "inputPanel().preedit().toString()",
        "partial_check",
        "(partial: ",
    ] {
        assert!(
            addon_smoke.contains(required),
            "native addon partial smoke should pin {required}"
        );
    }
    for required in [
        "sender-independent signal matches before daemon activation",
        "real `RecognitionPartial` value",
        "opt-in `ime-fcitx-native-live` gate crosses the real session boundary",
    ] {
        assert!(
            asr_doc.contains(required),
            "ASR docs should pin native partial evidence: {required}"
        );
    }
}

#[test]
fn dbus_architecture_pins_async_daemon_notification_forwarding() {
    let dbus_doc = std::fs::read_to_string(architecture_dir().join("dbus-service.md"))
        .expect("read dbus service doc");
    for required in [
        "owned `SignalEmitter`",
        "`asr_backend_reload_failed`",
        "only for the current reload generation",
        "matches `GetAsrBackendState.last_error`",
        "Fcitx D-Bus module",
        "real session-bus smoke",
    ] {
        assert!(
            dbus_doc.contains(required),
            "D-Bus docs should pin daemon notification rule: {required}"
        );
    }
}

#[test]
fn dbus_architecture_pins_recording_transaction_order() {
    let dbus_doc = std::fs::read_to_string(architecture_dir().join("dbus-service.md"))
        .expect("read dbus service doc");
    for required in [
        "share one asynchronous recording transaction lock",
        "held from the runtime state transition",
        "`StatusChanged`, `RecognitionPartial`, and `RecognitionResult`",
        "cannot interleave an old stop result",
        "no legacy deferred audio-stop worker",
        "upstream stop/start race hardening",
    ] {
        assert!(
            dbus_doc.contains(required),
            "D-Bus docs should pin recording transaction ordering: {required}"
        );
    }
}

#[test]
fn asr_architecture_pins_frontend_live_partial_preedit() {
    let asr_doc = std::fs::read_to_string(architecture_dir().join("asr-contract.md"))
        .expect("read asr contract doc");
    for required in [
        "Fcitx D-Bus monitor",
        "`StatusChanged(s)`",
        "`RecognitionPartial(s)`",
        "partial text takes precedence",
        "final synchronous-stop commit",
        "Representative GUI-toolkit rendering remains separate live work",
    ] {
        assert!(
            asr_doc.contains(required),
            "ASR docs should pin frontend live partial behavior: {required}"
        );
    }
}

#[test]
fn dbus_architecture_pins_frontend_cross_client_status_recovery() {
    let dbus_doc = std::fs::read_to_string(architecture_dir().join("dbus-service.md"))
        .expect("read dbus service doc");
    for required in [
        "Before issuing a local start, the addon queries `GetStatus`",
        "explicit `idle` permits the normal start path",
        "an externally started `recording` reached by the normal trigger is adopted and stopped",
        "`inferring` or `postprocessing`",
        "status-only preedit",
        "idle/error status or owner loss",
        "cross-client session-bus smoke",
    ] {
        assert!(
            dbus_doc.contains(required),
            "D-Bus docs should pin cross-client status recovery: {required}"
        );
    }
}

#[test]
fn dbus_architecture_pins_frontend_daemon_owner_loss_recovery() {
    let dbus_doc = std::fs::read_to_string(architecture_dir().join("dbus-service.md"))
        .expect("read dbus service doc");
    for required in [
        "`ServiceWatcher`",
        "initial `GetNameOwner`",
        "startup absence remains silent",
        "owner disappears during an addon-owned recording",
        "Voice input daemon is unavailable.",
        "stale synchronous client",
        "real session-bus smoke",
    ] {
        assert!(
            dbus_doc.contains(required),
            "D-Bus docs should pin frontend owner-loss recovery: {required}"
        );
    }
}

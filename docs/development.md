# Development guide

This file defines repository workflow, validation tiers, and commit style. Progress belongs in [`migration/function-gap-audit.md`](migration/function-gap-audit.md); priorities belong in [`migration/e2e-replication-plan.md`](migration/e2e-replication-plan.md).

## Project boundaries

Keep the workspace split by responsibility:

- `vinput-protocol`: public D-Bus and JSON wire contracts.
- `vinput-config`: typed config, defaults, normalization, validation, and shared diagnostic redaction.
- `vinput-http`: shared provider HTTP client construction, bounded additional-CA loading, and URL-free transport error categories.
- `vinput-audio`: PCM types, pure processing, recorder traits, and audio backends.
- `vinput-asr`: ASR traits, sessions, command backends, and native backends.
- `vinput-text`: prompts, context cache, text adapters, and provider transports.
- `vinput-registry`: registry schemas, safe downloads, extraction, and installation.
- `vinput-daemon`: runtime orchestration and D-Bus service facade.
- `vinput-cli`: user-facing commands and diagnostics over library crates.

The retained C++ frontend owns Fcitx API integration, menus, preedit/commit presentation, selected-text handling, notifications, and the bus bridge. Backend state and processing belong in Rust.

### Source organization

Keep public facades thin and place use-case logic behind domain modules:

- `vinput-cli/src/main.rs` is routing only. Clap data lives under `cli/`, command use cases under `commands/`, daemon lifecycle under `daemon_control/`, and shared path/config/registry/output services in focused support modules.
- `vinput-config/src/lib.rs` re-exports the public schema. Schema data, defaults/normalization, validation, file behavior, errors, and tests are separate modules.
- `vinput-asr/src/sherpa/` separates the public typed specification, offline layout/path inference, and the feature-gated runtime backend.
- the retained Fcitx addon separates recording/daemon integration from Scene/ASR menu implementation; do not move backend policy into C++.
- `scripts/` is grouped by deterministic tests, release operations, installation, fixtures, opt-in live evidence, and developer tools. The `justfile` is a thin facade for broad workflows; specialized gates are invoked directly from their documented script paths.

`scripts/tests/source-layout-check.sh` prevents production Rust/C++ files from growing beyond 1200 lines and gives fixture-heavy tests a 3000-line ceiling. Treat the limits as regression guards, not as targets: split earlier when data, orchestration, transport, formatting, or platform integration form distinct reasons to change.

## Coding rules

- Preserve service names, method and signal names, status strings, recognition JSON, config semantics, and frontend expectations.
- Add focused compatibility tests when a public contract changes.
- Prefer `pub(crate)` for implementation helpers and keep public APIs small.
- Workspace Rust uses edition 2024, MSRV 1.88, `unsafe_code = "forbid"`, and Clippy pedantic warnings.
- Keep code, comments, test names, documentation identifiers, and commit messages in English.
- Prefer milestone-enabling work over generic cleanup.
- Never treat deterministic seams as live desktop proof.
- Never commit files under ignored `docs/plan/`.

## Local workflow

Use the narrowest check that proves the change while iterating. Before handoff, run the complete relevant tier.

```sh
just fmt
just fmt-check
just test
just lint
just check
just ci
```

`just ci` is the deterministic project gate. It includes Rust checks, D-Bus integration, retained-addon checks, staged integration, temporary-HOME user-install smokes, and lightweight Arch package metadata validation. Live desktop, microphone, and full package builds are excluded by design.

## Validation tiers

### Documentation-only changes

```sh
git diff --check
```

Documentation is reviewed as documentation. Do not add tests that assert exact README wording, architecture prose, docstrings, source declarations, recipe names, or other implementation text. Run behavior tests only when documentation changes public commands, fixtures, generated artifacts, or executable contracts.

### Rust and core behavior

```sh
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

### D-Bus integration

```sh
just test
just lint
```

`just test` runs the isolated session-bus suite and covers legacy methods, configured backends, reload, adapters, and partial-before-stop behavior.

### Retained C++ frontend

```sh
just fmt-check
just test
```

Run `just lint` when Fcitx5 headers and `clang-tidy` are available.

### Deterministic addon and IME paths

```sh
scripts/tests/cpp/run-cpp-dbus-smoke.sh
scripts/tests/cpp/run-cpp-dbus-asr-menu-smoke.sh
scripts/tests/cpp/run-cpp-dbus-activation-smoke.sh
scripts/tests/cpp/run-cpp-dbus-configured-activation-smoke.sh
scripts/tests/cpp/run-cpp-dbus-adapter-lifecycle-smoke.sh
scripts/tests/daemon/run-remote-text-daemon-lifecycle-smoke.sh
scripts/tests/daemon/run-daemon-handoff-diagnostics-smoke.sh
scripts/tests/daemon/run-daemon-handoff-smoke.sh
scripts/tests/install/run-ime-e2e-smoke.sh
```

`scripts/tests/install/run-ime-e2e-smoke.sh` includes fake outcome sink coverage. `scripts/tests/cpp/run-cpp-dbus-adapter-lifecycle-smoke.sh` verifies configured text adapter start/duplicate-start/stop diagnostics over DBus. `scripts/tests/daemon/run-remote-text-daemon-lifecycle-smoke.sh` launches the normal daemon in a private session, proves its HTTP health endpoint, D-Bus owner, and redacted endpoint diagnostics, sends `SIGTERM`, and verifies listener release. `scripts/tests/daemon/run-daemon-handoff-diagnostics-smoke.sh` proves that `daemon status` detects both a D-Bus owner running from a different daemon path and a replaced executable whose old inode appears as ` (deleted)`, while remaining non-mutating. `scripts/tests/daemon/run-daemon-handoff-smoke.sh` proves the explicit conditional restart command: current owners never invoke systemctl, stale owners restart and pass a fresh owner-path check, and failed service control leaves the old owner alive.

### User installation

Use a temporary `HOME` unless mutation of the real profile is explicitly requested:

```sh
scripts/tests/install/run-user-ime-command-demo-smoke.sh
scripts/tests/install/run-user-ime-activation-owner-smoke.sh
scripts/tests/install/run-user-ime-real-command-asr-wav-smoke.sh
scripts/tests/install/run-user-ime-sherpa-native-smoke.sh
scripts/tests/install/run-user-ime-sherpa-native-activation-smoke.sh
scripts/tests/install/run-user-ime-sherpa-sense-voice-smoke.sh
```

`scripts/install/install-user-ime.sh` normally uses `target/debug/vinput` and `target/debug/vinput-daemon`. Tests that provide stubs must use `VINPUT_USER_CLI_BINARY` and `VINPUT_USER_DAEMON_BINARY` under their own temporary tree. Never overwrite Cargo outputs: Cargo fingerprints do not detect external binary replacement reliably.

The `sherpa-native-live` profile validates and copies `libsherpa-onnx` and `libonnxruntime`, creates `vinput-daemon-with-vinput-env.sh`, and runs `runtime-status` through the installed bundle. `sherpa-native-command-live` uses the same native runtime and adds a deterministic command adapter for real frontend validation; `sherpa-sense-voice-live` remains a compatibility alias. Set `VINPUT_USER_RUNTIME_STATUS=0` only for file-placement debugging.

### Arch packaging

The checked source of truth is `packaging/arch/PKGBUILD.in`; render release-specific source metadata with `scripts/release/render-arch-pkgbuild.py`.

```sh
scripts/release/check-arch-install-script.sh
scripts/release/check-arch-pkgbuild.sh
scripts/release/check-release-manifest.sh
scripts/release/check-release-signature.sh
scripts/release/check-arch-release-candidate.sh
just package-smoke
scripts/release/run-arch-package-transaction-smoke.sh
scripts/release/run-arch-repository-smoke.sh
scripts/release/run-arch-signing-smoke.sh
scripts/release/run-arch-release-bundle-smoke.sh
```

`scripts/release/check-release-manifest.sh` validates the strict flat-bundle schema, exact inventory, sorted checksums, atomic staging, safe `--force` replacement, and negative mutation/extra/symlink cases with tiny local fixtures. `scripts/release/check-release-signature.sh` creates ephemeral keys under `target/tmp`, proves atomic detached-manifest signing plus isolated external-key/fingerprint verification, and rejects missing/tampered signatures, manifest or artifact changes, wrong trust roots, bundled-key trust, and stale signatures after bundle rebuild. `scripts/release/check-arch-release-candidate.sh` builds minimal signed Arch packages and proves that promotion selects only the formal package, rebuilds single-version repository metadata, removes every test/synthetic role, signs the new candidate, and refuses unsafe force/output paths. `scripts/release/check-arch-install-script.sh` executes package hooks with an empty `PATH`; it proves post-install/post-remove guidance, successful and failing upgrade-helper propagation, removal-helper invocation, and the absence of unqualified user-session commands. `scripts/release/check-arch-pkgbuild.sh` is the lightweight deterministic metadata gate included in `just ci`. `just package-smoke` is the explicit release gate: it downloads checksum-pinned sherpa/ONNX Runtime assets when absent, builds a clean package through `makepkg`, verifies the embedded `.INSTALL`, extracts it without touching the host profile, validates the full file set and private rpaths, runs the packaged CLI/daemon/GUI including the display-independent GUI self-check, creates a `pkgrel=2` repackage, and proves direct pacman install/upgrade/same-version rollback/removal, local-repository install/upgrade, and signed-repository trust/tamper enforcement. It is intentionally not part of routine CI because it performs a complete release rebuild and requires network access for a cold cache. `scripts/release/run-arch-package-transaction-smoke.sh` reruns only the fast fakeroot direct-package transaction; `scripts/release/run-arch-repository-smoke.sh` reruns the unsigned `repo-add` plus `file://` path; `scripts/release/run-arch-signing-smoke.sh` creates only ephemeral keys under `target/tmp` and proves trusted signatures plus unknown-signer and tamper rejection. `scripts/release/run-arch-release-bundle-smoke.sh` assembles the source archive, rendered Arch metadata, both release-gate package revisions, package/database signatures, repository databases, and ephemeral public key into an exact `manifest.json` plus `SHA256SUMS` inventory, signs `manifest.json`, and verifies `manifest.json.sig` against the public key outside the bundle and a pinned fingerprint; the synthetic `pkgrel=2` and test key are explicitly labeled as test roles rather than public release assets. The same gate then promotes only `pkgrel=1` into an 11-role candidate with freshly signed repository metadata and verifies that no test role or `pkgrel=2` file remains.

### Native ASR evidence

Generic local recipes validate typed `vinput-model.json`, runtime construction, and one WAV recognition outside Fcitx5:

```sh
scripts/tests/asr/run-sherpa-offline-local-smoke.sh
scripts/tests/asr/run-sherpa-sense-voice-local-smoke.sh
scripts/tests/asr/run-sherpa-family-smoke.sh offline-transducer
scripts/tests/asr/run-sherpa-family-smoke.sh dolphin
scripts/tests/asr/run-sherpa-family-smoke.sh paraformer
scripts/tests/asr/run-sherpa-family-smoke.sh qwen3
scripts/tests/asr/run-sherpa-online-local-smoke.sh
scripts/tests/asr/run-sherpa-family-smoke.sh online-transducer
scripts/tests/asr/run-sherpa-family-smoke.sh zipformer2-ctc
scripts/tests/asr/run-sherpa-family-smoke.sh moonshine-reload
```

Model-dependent recipes require the documented `VINPUT_SHERPA_*` environment values. They are evidence for model/runtime support, not proof of live microphone or application behavior.

### Optional live checks

Run only when the corresponding real PipeWire, Fcitx5, browser, network, or desktop boundary is available:

```sh
scripts/tests/pipewire-check.sh
scripts/live/audio/run-pipewire-tests-live.sh
VINPUT_TEST_PIPEWIRE_RECORD=1 VINPUT_TEST_PIPEWIRE_RECORD_MS=12000 VINPUT_TEST_PIPEWIRE_MIN_PEAK=1000 cargo test -p vinput-audio --features pipewire-backend pipewire_recorder_live_capture_when_enabled -- --nocapture
scripts/live/audio/run-cpp-dbus-pipewire-live-smoke.sh
scripts/live/audio/run-ime-pipewire-live-smoke.sh
scripts/live/audio/run-ime-configured-pipewire-live-smoke.sh
scripts/live/niri/run-ime-fcitx-live-probe.sh
VINPUT_REMOTE_TEXT_BROWSER=/path/to/chromium scripts/live/network/run-remote-text-chromium-lan-live.sh
VINPUT_REMOTE_TEXT_CONFIRM_PHYSICAL_DEVICE=1 VINPUT_REMOTE_TEXT_EXTERNAL_TIMEOUT=180 scripts/live/network/run-remote-text-external-device-live.sh
scripts/live/niri/run-ime-fcitx-remote-asr-live.sh
VINPUT_LIVE_NATIVE_WAV=/path/to/speech.wav scripts/live/niri/run-ime-fcitx-virtual-source-live.sh
VINPUT_LIVE_NATIVE_WAV=/path/to/speech.wav VINPUT_LIVE_NATIVE_MODES=command VINPUT_LIVE_EXPECTED_TEXT_ADAPTER=native-command-live-adapter VINPUT_LIVE_EXPECTED_COMMIT_PREFIX='adapter-backed:' VINPUT_LIVE_VIRTUAL_OUT_DIR=target/tmp/ime-fcitx-virtual-command-live scripts/live/niri/run-ime-fcitx-virtual-source-live.sh
VINPUT_LIVE_NATIVE_WAV=/path/to/speech.wav VINPUT_LIVE_NATIVE_FOCUS_SWITCH=1 VINPUT_LIVE_VIRTUAL_OUT_DIR=target/tmp/ime-fcitx-virtual-focus-live scripts/live/niri/run-ime-fcitx-virtual-source-live.sh
VINPUT_LIVE_NATIVE_WAV=/path/to/speech.wav VINPUT_LIVE_NATIVE_OWNER_LOSS=1 VINPUT_LIVE_VIRTUAL_OUT_DIR=target/tmp/ime-fcitx-virtual-owner-loss-live scripts/live/niri/run-ime-fcitx-virtual-source-live.sh
VINPUT_LIVE_NATIVE_WAV=/path/to/speech.wav VINPUT_LIVE_RELOAD_BEFORE_PROBE=1 VINPUT_LIVE_VIRTUAL_OUT_DIR=target/tmp/ime-fcitx-virtual-reload-live scripts/live/niri/run-ime-fcitx-virtual-source-live.sh
VINPUT_LIVE_TOOLKIT_WAV=/path/to/speech.wav scripts/live/niri/run-ime-gtk3-native-live.sh normal
VINPUT_LIVE_TOOLKIT_WAV=/path/to/speech.wav scripts/live/niri/run-ime-qt6-native-live.sh normal
VINPUT_LIVE_NATIVE_WAV=/path/to/speech.wav scripts/live/niri/run-ime-vscode-virtual-live.sh normal
VINPUT_LIVE_NATIVE_WAV=/path/to/speech.wav scripts/live/niri/run-ime-vscode-virtual-live.sh command
VINPUT_LIVE_NATIVE_WAV=/path/to/speech.wav scripts/live/niri/run-ime-gtk4-soak-virtual-live.sh normal 10
VINPUT_LIVE_NATIVE_WAV=/path/to/speech.wav scripts/live/niri/run-ime-gtk4-soak-virtual-live.sh command 10
scripts/tools/bench-capture-cold-start.sh --follow
```

The live recipes are intentionally excluded from `just ci`. The same-host remote-text gate proves a real Chromium page and WebSocket path through the host's non-loopback address while explicitly retaining `cross_device_proof=false`. The separate external-device collector is also opt-in and succeeds only after a different network peer submits its random challenge and the operator explicitly confirms that peer is another physical device rather than a local VM/container; its no-device and same-host paths must fail and clean up without writing proof. `scripts/live/niri/run-ime-fcitx-virtual-source-live.sh` is the retained audio evidence gate: it creates a unique mono PipeWire sink/source pair, records a non-silent 16 kHz preflight sample, snapshots the live config and any existing backup, temporarily selects the virtual source, restarts only the verified Rust daemon, and then runs the real Fcitx client probe. Normal, local-adapter command, focus-handoff, owner-loss, and same-provider reload outcomes are selected through environment flags and each writes JSONL plus a wrapper summary. Cleanup restores the config byte-for-byte, preserves prior backup state, reactivates the original profile, and removes the virtual nodes. The successful 2026-07-30 evidence used no physical speaker or microphone; direct `scripts/live/niri/run-ime-fcitx-native-live.sh`, `ime-fcitx-focus-live`, `ime-fcitx-owner-loss-live`, `ime-fcitx-reload-live`, and `ime-fcitx-native-command-adapter-live` remain configured-source manual collectors and are not retained as proof when they depend on desktop output-to-microphone pickup. The local adapter validates configured adapter transport, not an external provider. The shared `vinput-process` unit gates prove piped roundtrips, deadline cancellation, and output overflow. Text and ASR consumer gates additionally prove deadlines across stdin write/execution/output recovery, timeout while helpers ignore large requests, whole-process-group termination including background descendants, prompt cleanup of descendants after direct-child exit without a timeout, deadlock-free collection of 256 KiB stderr, and independent 1 MiB stdout/stderr limits with fixed non-content diagnostics. `scripts/live/niri/run-ime-fcitx-remote-asr-live.sh` temporarily adds one OpenAI-compatible remote provider, verifies real F8/Enter selection plus multipart WAV/Bearer/model/language/prompt transport and a final-only commit against an independent loopback process, then restores streaming Zipformer. `scripts/tests/asr/run-openai-compatible-asr-network-smoke.sh` covers local plain-HTTP proxy routing, proxy-URL Basic authentication, `NO_PROXY`, 429/503, fail-closed 3xx handling with an untouched redirect target, distinct request and response-body timeouts, a 1 MiB cap for success and error response bodies, untrusted self-signed TLS rejection, DNS failure, connection refusal, and redaction through the production one-shot daemon. `scripts/tests/asr/run-openai-compatible-text-network-smoke.sh` applies the same cases to the production `vinput llm test` path and proves that omitting `--timeout-ms` reports and enforces the legacy 4000 ms scene deadline. Both gates also create a temporary CA, load it as an additional root through `SSL_CERT_FILE`, retain built-in `WebPKI` roots and verification, and complete Basic-authenticated CONNECT tunnels through both plain-HTTP and TLS-protected HTTPS proxy endpoints to CA-signed HTTPS origins without recording tunneled payloads or certificate private keys. A separate local interception fixture terminates and re-establishes one verified TLS exchange, relays the synthetic request in memory, and retains only TLS versions plus request/response byte counts. `scripts/tests/asr/run-provider-ca-rotation-smoke.sh` keeps one ASR daemon and one text-processing daemon alive while atomically replacing the same CA file path: CA A succeeds, a mismatched CA is rejected with the daemon returning idle, and CA B succeeds without changing owner PID or endpoint. Query-bearing failures expose only `key=REDACTED`; unit/CLI tests pin removal of URL userinfo/fragments, ASR prompt hiding, text `Debug` body hiding, preserved real request query values, known-value error-body redaction, bounded CA-bundle reads, and fixed CA-load errors that omit local paths and contents. These gates do not prove a real hosted provider, PAC, NTLM/Kerberos, enterprise TLS-interception policy and certificate deployment, or provider credential custody and production CA distribution/revocation operations. `scripts/live/niri/run-ime-fcitx-menu-live.sh` verifies non-mutating scene/ASR candidate display, slash filtering, two-stage Escape close, and zero commit. `scripts/live/niri/run-ime-fcitx-notification-localization-live.sh` restarts the installed addon under zh_CN, proves localized scene-information text and localized information/error summaries while preserving the daemon's technical error body, then restores English and the original locale environment exactly. `scripts/live/niri/run-ime-fcitx-asr-notification-localization-live.sh` adds the real F8 invalid-provider path, proves the localized ASR-switch template plus error summary, keeps the old backend effective, recognizes again after restoration, and restores configuration bytes after stopping the localized Fcitx writer. `scripts/live/niri/run-ime-gtk3-native-live.sh` and `scripts/live/niri/run-ime-qt6-native-live.sh` open real toolkit text fields and still require actual desktop F9/F10 events; they deliberately do not synthesize GDK or Qt key events under Wayland. Preserve JSONL output for every claimed result. `scripts/tests/pipewire-check.sh` exercises CLI/daemon audio-device diagnostics without requiring a live PipeWire daemon. For cold-start measurements, enable `RUST_LOG=vinput_audio=debug,vinput_daemon=debug` in the user service, use waits of at least 10 seconds for cold trials and gaps below 2 seconds for warm trials, then run `scripts/tools/bench-capture-cold-start.sh` or pass `--input` to analyze saved journal output. `scripts/tests/asr/run-capture-cold-start-smoke.sh` validates the parser with a deterministic fixture and is included in `just ci`. Live failures must record whether setup failed at the session, target, format, sample rate, channel plan, capture, ASR, frontend, or application boundary.

## Commit style

Use concise English Conventional Commits:

```text
<type>(optional-scope): <imperative summary>
```

Common types are `feat`, `fix`, `refactor`, `test`, `docs`, `ci`, `build`, and `chore`.

- Keep one reason to change per commit.
- Do not mix broad refactors with feature work.
- Do not mix implementation, tests, and documentation unless they are inseparable parts of one small change.
- Before commit, run `git diff --check`, inspect the staged diff, and run the relevant validation tier.

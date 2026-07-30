# Development guide

This file defines repository workflow, validation tiers, and commit style. Progress belongs in [`migration/function-gap-audit.md`](migration/function-gap-audit.md); priorities belong in [`migration/e2e-replication-plan.md`](migration/e2e-replication-plan.md).

## Project boundaries

Keep the workspace split by responsibility:

- `vinput-protocol`: public D-Bus and JSON wire contracts.
- `vinput-config`: typed config, defaults, normalization, and validation.
- `vinput-audio`: PCM types, pure processing, recorder traits, and audio backends.
- `vinput-asr`: ASR traits, sessions, command backends, and native backends.
- `vinput-text`: prompts, context cache, text adapters, and provider transports.
- `vinput-registry`: registry schemas, safe downloads, extraction, and installation.
- `vinput-daemon`: runtime orchestration and D-Bus service facade.
- `vinput-cli`: user-facing commands and diagnostics over library crates.

The retained C++ frontend owns Fcitx API integration, menus, preedit/commit presentation, selected-text handling, notifications, and the bus bridge. Backend state and processing belong in Rust.

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
cargo test -p vinput-cli --test architecture_docs
cargo test -p vinput-cli --test readme_layout --test readme_tooling --test readme_demo --test readme_smoke
```

Run broader checks when documentation changes public commands, fixtures, or tested contracts.

### Rust and core behavior

```sh
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

### D-Bus integration

```sh
just dbus-test
just dbus-lint
```

`just dbus-test` runs the isolated session-bus suite and covers legacy methods, configured backends, reload, adapters, and partial-before-stop behavior.

### Retained C++ frontend

```sh
just addon-format-check
just addon-test
```

Run `just addon-lint` when Fcitx5 headers and `clang-tidy` are available.

### Deterministic addon and IME paths

```sh
just addon-dbus-smoke
just addon-dbus-asr-menu-smoke
just addon-dbus-activation-smoke
just addon-dbus-configured-activation-smoke
just addon-dbus-adapter-lifecycle-smoke
just remote-text-daemon-lifecycle-smoke
just daemon-handoff-diagnostics-smoke
just daemon-handoff-smoke
just ime-e2e-smoke
```

`just ime-e2e-smoke` includes fake outcome sink coverage. `just addon-dbus-adapter-lifecycle-smoke` verifies configured text adapter start/duplicate-start/stop diagnostics over DBus. `just remote-text-daemon-lifecycle-smoke` launches the normal daemon in a private session, proves its HTTP health endpoint, D-Bus owner, and redacted endpoint diagnostics, sends `SIGTERM`, and verifies listener release. `just daemon-handoff-diagnostics-smoke` proves that `daemon status` detects both a D-Bus owner running from a different daemon path and a replaced executable whose old inode appears as ` (deleted)`, while remaining non-mutating. `just daemon-handoff-smoke` proves the explicit conditional restart command: current owners never invoke systemctl, stale owners restart and pass a fresh owner-path check, and failed service control leaves the old owner alive.

### User installation

Use a temporary `HOME` unless mutation of the real profile is explicitly requested:

```sh
just user-ime-command-demo-smoke
just user-ime-activation-owner-smoke
just user-ime-real-command-asr-wav-smoke
just user-ime-sherpa-native-smoke
just user-ime-sherpa-native-activation-smoke
just user-ime-sherpa-sense-voice-smoke
```

`scripts/install-user-ime.sh` normally uses `target/debug/vinput` and `target/debug/vinput-daemon`. Tests that provide stubs must use `VINPUT_USER_CLI_BINARY` and `VINPUT_USER_DAEMON_BINARY` under their own temporary tree. Never overwrite Cargo outputs: Cargo fingerprints do not detect external binary replacement reliably.

The `sherpa-native-live` profile validates and copies `libsherpa-onnx` and `libonnxruntime`, creates `vinput-daemon-with-vinput-env.sh`, and runs `runtime-status` through the installed bundle. `sherpa-native-command-live` uses the same native runtime and adds a deterministic command adapter for real frontend validation; `sherpa-sense-voice-live` remains a compatibility alias. Set `VINPUT_USER_RUNTIME_STATUS=0` only for file-placement debugging.

### Arch packaging

The checked source of truth is `packaging/arch/PKGBUILD.in`; render release-specific source metadata with `scripts/render-arch-pkgbuild.py`.

```sh
just arch-install-script-check
just arch-pkgbuild-check
just release-manifest-check
just release-signature-check
just release-candidate-check
just arch-package-smoke
just arch-package-transaction-smoke
just arch-repository-smoke
just arch-signing-smoke
just arch-release-bundle-smoke
```

`just release-manifest-check` validates the strict flat-bundle schema, exact inventory, sorted checksums, atomic staging, safe `--force` replacement, and negative mutation/extra/symlink cases with tiny local fixtures. `just release-signature-check` creates ephemeral keys under `target/tmp`, proves atomic detached-manifest signing plus isolated external-key/fingerprint verification, and rejects missing/tampered signatures, manifest or artifact changes, wrong trust roots, bundled-key trust, and stale signatures after bundle rebuild. `just release-candidate-check` builds minimal signed Arch packages and proves that promotion selects only the formal package, rebuilds single-version repository metadata, removes every test/synthetic role, signs the new candidate, and refuses unsafe force/output paths. `just arch-install-script-check` executes the message-only post-install, post-upgrade, and post-remove hooks with an empty `PATH`; it proves the root package script never invokes user-session commands and pins the lifecycle guidance. `just arch-pkgbuild-check` is the lightweight deterministic metadata gate included in `just ci`. `just arch-package-smoke` is the explicit release gate: it downloads checksum-pinned sherpa/ONNX Runtime assets when absent, builds a clean package through `makepkg`, verifies the embedded `.INSTALL`, extracts it without touching the host profile, validates the full file set and private rpaths, runs the packaged CLI/daemon, creates a `pkgrel=2` repackage, and proves direct pacman install/upgrade/same-version rollback/removal, local-repository install/upgrade, and signed-repository trust/tamper enforcement. It is intentionally not part of routine CI because it performs a complete release rebuild and requires network access for a cold cache. `just arch-package-transaction-smoke` reruns only the fast fakeroot direct-package transaction; `just arch-repository-smoke` reruns the unsigned `repo-add` plus `file://` path; `just arch-signing-smoke` creates only ephemeral keys under `target/tmp` and proves trusted signatures plus unknown-signer and tamper rejection. `just arch-release-bundle-smoke` assembles the source archive, rendered Arch metadata, both release-gate package revisions, package/database signatures, repository databases, and ephemeral public key into an exact `manifest.json` plus `SHA256SUMS` inventory, signs `manifest.json`, and verifies `manifest.json.sig` against the public key outside the bundle and a pinned fingerprint; the synthetic `pkgrel=2` and test key are explicitly labeled as test roles rather than public release assets. The same gate then promotes only `pkgrel=1` into an 11-role candidate with freshly signed repository metadata and verifies that no test role or `pkgrel=2` file remains.

### Native ASR evidence

Generic local recipes validate typed `vinput-model.json`, runtime construction, and one WAV recognition outside Fcitx5:

```sh
just sherpa-offline-local-smoke
just sherpa-sense-voice-local-smoke
just sherpa-offline-transducer-local-smoke
just sherpa-dolphin-local-smoke
just sherpa-paraformer-local-smoke
just sherpa-qwen3-local-smoke
just sherpa-online-local-smoke
just sherpa-online-transducer-local-smoke
just sherpa-zipformer2-ctc-local-smoke
just sherpa-moonshine-dbus-reload-smoke
```

Model-dependent recipes require the documented `VINPUT_SHERPA_*` environment values. They are evidence for model/runtime support, not proof of live microphone or application behavior.

### Optional PipeWire and desktop checks

Run only in a real user session where PipeWire and Fcitx5 are expected to work:

```sh
just pipewire-check
VINPUT_TEST_PIPEWIRE_CONTEXT=1 VINPUT_TEST_PIPEWIRE_ENUMERATE=1 VINPUT_TEST_PIPEWIRE_RECORD=1 just pipewire-live
just addon-dbus-pipewire-live
just ime-pipewire-live
just ime-configured-pipewire-live
just ime-fcitx-live-probe
VINPUT_LIVE_NATIVE_WAV=/path/to/speech.wav just ime-fcitx-virtual-source-live
VINPUT_LIVE_NATIVE_WAV=/path/to/speech.wav VINPUT_LIVE_NATIVE_MODES=command VINPUT_LIVE_EXPECTED_TEXT_ADAPTER=native-command-live-adapter VINPUT_LIVE_EXPECTED_COMMIT_PREFIX='adapter-backed:' VINPUT_LIVE_VIRTUAL_OUT_DIR=target/tmp/ime-fcitx-virtual-command-live just ime-fcitx-virtual-source-live
VINPUT_LIVE_NATIVE_WAV=/path/to/speech.wav VINPUT_LIVE_NATIVE_FOCUS_SWITCH=1 VINPUT_LIVE_VIRTUAL_OUT_DIR=target/tmp/ime-fcitx-virtual-focus-live just ime-fcitx-virtual-source-live
VINPUT_LIVE_NATIVE_WAV=/path/to/speech.wav VINPUT_LIVE_NATIVE_OWNER_LOSS=1 VINPUT_LIVE_VIRTUAL_OUT_DIR=target/tmp/ime-fcitx-virtual-owner-loss-live just ime-fcitx-virtual-source-live
VINPUT_LIVE_NATIVE_WAV=/path/to/speech.wav VINPUT_LIVE_RELOAD_BEFORE_PROBE=1 VINPUT_LIVE_VIRTUAL_OUT_DIR=target/tmp/ime-fcitx-virtual-reload-live just ime-fcitx-virtual-source-live
VINPUT_LIVE_TOOLKIT_WAV=/path/to/speech.wav just ime-gtk3-native-live normal
VINPUT_LIVE_TOOLKIT_WAV=/path/to/speech.wav just ime-qt6-native-live normal
scripts/bench-capture-cold-start.sh --follow
```

The live recipes are intentionally excluded from `just ci`. `ime-fcitx-virtual-source-live` is the retained audio evidence gate: it creates a unique mono PipeWire sink/source pair, records a non-silent 16 kHz preflight sample, snapshots the live config and any existing backup, temporarily selects the virtual source, restarts only the verified Rust daemon, and then runs the real Fcitx client probe. Normal, local-adapter command, focus-handoff, owner-loss, and same-provider reload outcomes are selected through environment flags and each writes JSONL plus a wrapper summary. Cleanup restores the config byte-for-byte, preserves prior backup state, reactivates the original profile, and removes the virtual nodes. The successful 2026-07-30 evidence used no physical speaker or microphone; direct `ime-fcitx-native-live`, `ime-fcitx-focus-live`, `ime-fcitx-owner-loss-live`, `ime-fcitx-reload-live`, and `ime-fcitx-native-command-adapter-live` remain configured-source manual collectors and are not retained as proof when they depend on desktop output-to-microphone pickup. The local adapter validates configured adapter transport, not an external provider. `ime-fcitx-menu-live` verifies non-mutating scene/ASR candidate display, slash filtering, two-stage Escape close, and zero commit. `ime-gtk3-native-live` and `ime-qt6-native-live` open real toolkit text fields and still require actual desktop F9/F10 events; they deliberately do not synthesize GDK or Qt key events under Wayland. Preserve JSONL output for every claimed result. `just pipewire-check` exercises CLI/daemon audio-device diagnostics without requiring a live PipeWire daemon. For cold-start measurements, enable `RUST_LOG=vinput_audio=debug,vinput_daemon=debug` in the user service, use waits of at least 10 seconds for cold trials and gaps below 2 seconds for warm trials, then run `scripts/bench-capture-cold-start.sh` or pass `--input` to analyze saved journal output. `just capture-cold-start-smoke` validates the parser with a deterministic fixture and is included in `just ci`. Live failures must record whether setup failed at the session, target, format, sample rate, channel plan, capture, ASR, frontend, or application boundary.

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

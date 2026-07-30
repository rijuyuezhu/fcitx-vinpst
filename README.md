# fcitx-vinput-rs

Rust-oriented rewrite of [`fcitx5-vinput`](https://github.com/xifan2333/fcitx5-vinput).

The project is a usable CLI/daemon alpha with a retained C++ Fcitx5 frontend. The native path is now live-proven in a real user session through Fcitx5, acoustic PipeWire capture, streaming ASR partials, input-panel preedit, final commit, and selected-text command replacement in a real Fcitx client application. The active milestone remains **real desktop native alpha** while menu behavior, failure recovery, and command replacement are broadened across GUI toolkits.

## Architecture

The Rust workspace is split by responsibility:

- `crates/vinput-protocol`: stable D-Bus names, status strings, and recognition payloads.
- `crates/vinput-config`: typed configuration, defaults, normalization, and validation.
- `crates/vinput-audio`: PCM types, audio transforms, recorder traits, and optional PipeWire capture.
- `crates/vinput-asr`: ASR traits plus mock, command, and optional native `sherpa-onnx` backends.
- `crates/vinput-text`: scene prompts, command adapters, context cache, and OpenAI-compatible transport.
- `crates/vinput-registry`: live model/script registry metadata, checksums, safe extraction, and managed installation.
- `crates/vinput-daemon`: runtime orchestration and the legacy-compatible D-Bus service.
- `crates/vinput-cli`: the `vinput` management and diagnostics CLI.

`cpp/fcitx5-addon` remains C++ deliberately. It owns only Fcitx API integration, key handling, menus, preedit/commit presentation, selected-text handling, notifications, and the D-Bus bridge. Backend behavior belongs in Rust.

## Current capability

Implemented and deterministically validated:

- legacy-compatible D-Bus methods, signals, status strings, and recognition JSON;
- `vinput init`, config mutation, model/provider/hotword/device/scene/LLM/adapter management, daemon control, recording control, and `vinput doctor`;
- live model registry fetch, SHA-256 verification, safe archive extraction, install/use/remove, and installed-model discovery;
- live ASR provider registry listing/install/update-by-reinstall, guarded removal, and legacy-compatible command-provider script editing, preserving batch/streaming protocol selection, managed paths, timeout/env values, config backup, and overwrite protection;
- live adapter registry listing and `vinput adapter install`, including short ids, mirror fallback, executable script publication, config backup, environment placeholders, and guarded managed updates;
- native offline and online registry-model ASR families currently used by the project;
- `sherpa-native-live` user installation with a copied `libsherpa-onnx` and `libonnxruntime` bundle;
- a checked Arch Linux `x86_64` package and signed release-candidate pipeline; the exact artifact, trust, transaction, and handoff contracts live in [`docs/architecture/packaging-contract.md`](docs/architecture/packaging-contract.md);
- wrapper-based activation through `vinput-daemon-with-vinput-env.sh`;
- activation-safe `RecognitionPartial` delivery, concrete Fcitx preedit, final commit, and command candidate replacement in temporary-HOME smokes;
- persistent frontend keys, Tap/Hold/Both trigger behavior, searchable scene/ASR menus, localization, notifications, and daemon-owner recovery.

Live-proven in a real user session:

- installed native runtime activation through the current session bus;
- F9 -> live acoustic PipeWire capture -> streaming native ASR -> partial input-panel updates -> one application commit;
- F10 -> selected surrounding text -> live partials -> candidate selection -> deletion -> replacement commit;
- repeatable opt-in evidence through `VINPUT_LIVE_NATIVE_WAV=/path/to/speech.wav just ime-fcitx-native-live`.

Still requiring live proof or implementation:

- command replacement and clipboard fallback across multiple GUI applications/toolkits;
- real menu, focus-transition, notification, owner-loss, and reload behavior;
- remote text live cross-device browser proof;
- production publication and lifecycle policy beyond the checked Arch candidate, including automatic package-manager handoff, incompatible-state rollback, production key operations, external repository hosting, and live installed-desktop proof;
- the legacy Qt GUI, which is intentionally deferred.

See [`docs/migration/function-gap-audit.md`](docs/migration/function-gap-audit.md) for status and [`docs/migration/e2e-replication-plan.md`](docs/migration/e2e-replication-plan.md) for priorities.

Install a registry adapter without editing JSON manually:

```sh
vinput adapter list --available
vinput adapter install <id-or-short-id> --in-place
```

Use `--registry /path/to/registry/adapters.json`, `--adapter-root`, and `--dry-run --json` for deterministic local validation.

Install an external ASR provider from the current script registry:

```sh
vinput provider list --available
vinput provider install <id-or-short-id> --in-place
vinput provider edit-script <id-or-short-id> --registry registry/providers.json
vinput provider remove <machine-id> --in-place
```

Run the standalone remote browser-input service when the active provider is
`provider.vinput.remote.streaming` and its environment contains
`VINPUT_ASR_API_KEY`:

```sh
vinput-daemon --config ~/.config/fcitx-vinput/config.json remote-text-server
```

This command exposes `/`, `/health`, `/ws`, and the loopback-only
`/v1/realtime` compatibility endpoint for isolated diagnostics. The normal
`vinput-daemon --dbus` process also owns the service automatically when
`provider.vinput.remote.streaming` is active: provider selection and
`ReloadAsrBackend` reconcile the listener, while `SIGINT`/`SIGTERM` shut it
down gracefully.

Use `--registry /path/to/registry/providers.json`, `--provider-root`, and `--dry-run --json` for deterministic local validation. Select the installed machine id with `vinput provider use <machine-id>`. Removal keeps local providers, allows an active command/remote provider to be removed, and clears the active selection instead of choosing a fallback; pass `--registry` to resolve a registry short id during removal.

Provider and adapter available lists load localized titles and descriptions from `i18n/<locale>.json` while preserving stable machine ids. Use `--locale zh_CN` for mirrors or `--i18n /path/to/i18n.json` with a local registry fixture.

## Build and check

Use `just` as the project interface:

```sh
just fmt-check
just test
just dbus-test
just addon-test
just ci
just smoke
```

`just ci` is the full deterministic project gate. Optional live PipeWire and real-desktop checks are intentionally excluded.

Useful focused integration recipes:

```sh
just addon-dbus-smoke
just capture-cold-start-smoke
just addon-dbus-asr-menu-smoke
just addon-dbus-activation-smoke
just addon-dbus-configured-activation-smoke
just addon-dbus-adapter-lifecycle-smoke
just ime-e2e-smoke
```

`just ime-e2e-smoke` includes fake outcome sink coverage. `just addon-dbus-adapter-lifecycle-smoke` covers configured text adapter start/duplicate-start/stop diagnostics over DBus.

Run the committed deterministic demo with:

```sh
just e2e-demo
```

It uses `data/e2e-command-demo-config.json` and a generated WAV to exercise audio input, command ASR, command text processing, and recognition JSON without requiring a desktop session.

After installing a native live profile in a real desktop session, run the acoustic Fcitx client gate with a validated speech WAV:

```sh
VINPUT_LIVE_NATIVE_WAV=/path/to/speech.wav just ime-fcitx-native-live
```

The gate plays the WAV through the current output device, captures it from the configured PipeWire source, and verifies normal partial/commit behavior plus command candidate deletion/replacement. It is intentionally excluded from `just ci`.

## Native user installation

Before changing the real user profile, use the temporary-HOME checks:

```sh
just user-ime-sherpa-native-smoke
just user-ime-sherpa-native-activation-smoke
```

For an explicitly approved real profile:

```sh
VINPUT_USER_PROFILE=sherpa-native-live \
  VINPUT_USER_SHERPA_MODEL=/path/to/installed-model \
  VINPUT_USER_SHERPA_RUNTIME_LIB_DIR=/path/to/runtime/lib \
  scripts/install-user-ime.sh
```

The legacy `sherpa-sense-voice-live` profile remains as a compatibility alias. The installer validates and copies `libsherpa-onnx` and `libonnxruntime`, then activates the installed daemon through `vinput-daemon-with-vinput-env.sh` so readiness checks and D-Bus activation use the same native runtime.

Live desktop validation is documented in [`docs/migration/live-desktop-validation.md`](docs/migration/live-desktop-validation.md).

## Repository tooling

- `rust-toolchain.toml`: stable Rust toolchain with `rustfmt` and Clippy.
- `rustfmt.toml`: Rust formatting policy.
- `clippy.toml` and workspace lints in `Cargo.toml`: lint policy.
- `.clang-format` and `.clang-tidy`: retained addon policy.
- `.pre-commit-config.yaml`: optional local hooks.
- `justfile`: local and CI command interface.
- `AGENTS.md`: required repository instructions.
- `docs/README.md`: documentation map and source-of-truth rules.

Local scratch belongs under ignored `docs/plan/`; it must not be treated as tracked project truth.

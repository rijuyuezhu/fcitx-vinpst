# fcitx-vinput-rs

Rust-oriented rewrite of [`fcitx5-vinput`](https://github.com/xifan2333/fcitx5-vinput).

The project is a usable CLI/daemon alpha with a retained C++ Fcitx5 frontend. The deterministic native path now covers user installation, D-Bus activation, streaming partial preedit, final commit, and command-mode replacement through a concrete test `fcitx::InputContext`. The active milestone is **real desktop native alpha**: prove the same path with a running Fcitx5 session, live PipeWire capture, and a real application.

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
- live ASR provider registry listing and `vinput provider install`, preserving batch/streaming protocol selection, managed paths, timeout/env values, config backup, and overwrite protection;
- live adapter registry listing and `vinput adapter install`, including short ids, mirror fallback, executable script publication, config backup, environment placeholders, and guarded managed updates;
- native offline and online registry-model ASR families currently used by the project;
- `sherpa-native-live` user installation with a copied `libsherpa-onnx` and `libonnxruntime` bundle;
- wrapper-based activation through `vinput-daemon-with-vinput-env.sh`;
- activation-safe `RecognitionPartial` delivery, concrete Fcitx preedit, final commit, and command candidate replacement in temporary-HOME smokes;
- persistent frontend keys, Tap/Hold/Both trigger behavior, searchable scene/ASR menus, localization, notifications, and daemon-owner recovery.

Still requiring live proof or implementation:

- real Fcitx5 -> PipeWire -> native ASR -> partial/preedit -> application commit;
- command replacement and clipboard fallback across real applications;
- provider/adapter update polish;
- remote text service parity, distro packaging, upgrades, and release hardening;
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
vinput provider remove <machine-id> --in-place
```

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

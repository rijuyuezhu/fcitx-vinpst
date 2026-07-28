# Development guide

This guide defines project workflow, commit style, and validation tiers. Migration direction lives in [`migration/e2e-replication-plan.md`](migration/e2e-replication-plan.md); parity baseline lives in [`migration/function-gap-audit.md`](migration/function-gap-audit.md); the detailed native runtime/frontend E2E gap matrix lives in [`migration/e2e-capability-matrix.md`](migration/e2e-capability-matrix.md).

## Project style

- Keep the Rust workspace split by responsibility:
  - `vinput-protocol`: wire names, status strings, and payload contracts.
  - `vinput-config`: typed config, defaults, validation, and legacy normalization decisions.
  - `vinput-audio`: PCM data, audio processing, capture traits, and audio backends.
  - `vinput-asr`: ASR traits, sessions, mock/command backends, and local ASR backends.
  - `vinput-text`: prompt rendering, context cache, text adapters, and provider transports.
  - `vinput-registry`: registry schema, validation, planning, staging, and install mechanics.
  - `vinput-daemon`: runtime orchestration and service facade.
  - `vinput-cli`: diagnostics and user-facing command entry points over library crates.
- Preserve user-visible legacy behavior before improving internals: service names, method names, status strings, recognition JSON, config semantics, command-mode behavior, and frontend expectations must stay explicit and tested.
- Prefer E2E-enabling implementation over generic cleanup. The active goal is real desktop alpha, then legacy feature parity.
- Treat mock/seam coverage as contract coverage, not feature parity.
- Keep public APIs small. Prefer `pub(crate)` for helpers after module splits.
- Keep assistant/user communication in Chinese. Keep code, comments, test names, docs identifiers, and commit messages in English unless surrounding text requires otherwise.

## Commit message style

Use concise Conventional Commit style:

```text
<type>(optional-scope): <imperative summary>
```

Common types: `feat`, `fix`, `refactor`, `test`, `docs`, `ci`, `build`, `chore`.

Examples:

```text
docs(migration): track feature parity audit
fix(ime): improve live probe diagnostics
test(addon): cover selected text fallback
feat(asr): add initial sherpa runtime
```

Rules:

- Use English commit messages.
- Keep the summary short and imperative.
- Prefer small commits with one reason to change.
- Do not mix pure docs, tests, and feature implementation in one commit unless the change is intentionally tiny and inseparable.
- Do not mix broad refactors with feature implementation unless explicitly approved.

## Validation tiers

Use the narrowest tier that proves the change, then add integration checks for touched boundaries.

### Docs-only changes

```sh
git status --porcelain=v1 -b
git diff --check
```

Run more checks if docs alter public command examples, contracts, or test instructions.

### Rust/core changes

```sh
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

### Service integration changes

```sh
just dbus-test
just dbus-lint
```

### C++ addon/frontend changes

```sh
just addon-format-check
just addon-test
```

Run `just addon-lint` when Fcitx5 headers and clang-tidy are available.

### Deterministic IME path changes

```sh
just ime-e2e-smoke
just user-ime-command-demo-smoke
just user-ime-real-command-asr-wav-smoke
just user-ime-sherpa-sense-voice-smoke
```

These checks prove the deterministic product spine. They do not prove live desktop behavior.

### User install changes

Prefer a temporary `HOME` unless the user explicitly wants to mutate the real profile:

```sh
tmp_home="$(mktemp -d)"
HOME="$tmp_home" VINPUT_USER_PROFILE=command-demo scripts/install-user-ime.sh
HOME="$tmp_home" VINPUT_USER_PROFILE=real-command-asr-wav \
  VINPUT_USER_AUDIO_BACKEND=mock \
  VINPUT_USER_COMMAND_ASR_WAV_COMMAND='cat "$VINPUT_ASR_WAV" >/dev/null; printf ready' \
  scripts/install-user-ime.sh
mkdir -p "$tmp_home/model"
printf 'onnx\n' >"$tmp_home/model/model.int8.onnx"
printf '<blank> 0\n' >"$tmp_home/model/tokens.txt"
HOME="$tmp_home" VINPUT_USER_PROFILE=sherpa-sense-voice-live \
  VINPUT_USER_AUDIO_BACKEND=mock \
  VINPUT_USER_SHERPA_MODEL="$tmp_home/model" \
  scripts/install-user-ime.sh
HOME="$tmp_home" VINPUT_USER_STATUS=1 scripts/install-user-ime.sh
rm -rf "$tmp_home"
```

### Optional live desktop checks

Run only inside a real desktop session where Fcitx5 and PipeWire are expected to work:

```sh
just ime-fcitx-live-probe
VINPUT_LIVE_INSTALL_COMMAND_DEMO=1 just ime-fcitx-live-probe
just pipewire-check
just pipewire-live
just addon-dbus-pipewire-live
just ime-pipewire-live
just ime-configured-pipewire-live
```

Optional PipeWire recipes are intentionally excluded from `just ci`. `just pipewire-check` runs without live daemon access and covers CLI/daemon audio-device diagnostics.
`just pipewire-live` uses `VINPUT_TEST_PIPEWIRE_CONTEXT=1`, `VINPUT_TEST_PIPEWIRE_ENUMERATE=1`, and `VINPUT_TEST_PIPEWIRE_RECORD=1`.
`just sherpa-onnx-check` compiles the official feature-gated sherpa backend without running model inference.
`just sherpa-sense-voice-local-smoke` requires `VINPUT_SHERPA_MODEL` and `VINPUT_SHERPA_WAV`; it validates model loading and one WAV recognition outside Fcitx5 before live desktop debugging.
The local sherpa smoke defaults `VINPUT_SHERPA_RUNTIME_LIB_DIR` to `target/debug` to prefer the shared libraries provided by the cargo build over system-wide sherpa/ONNX Runtime libraries.
`VINPUT_USER_PROFILE=sherpa-sense-voice-live scripts/install-user-ime.sh` runs `runtime-status` by default after install and during status checks; set `VINPUT_USER_RUNTIME_STATUS=0` to skip native model construction when debugging only file placement.
`just addon-dbus-pipewire-live` covers the C++ bridge plus Rust daemon D-Bus path, prints the daemon build's `audio-devices` JSON diagnostics, uses `VINPUT_DBUS_SMOKE_RECORD_MS=100`, and passes `--record-ms 100` through the start/wait/stop smoke.
`just ime-pipewire-live` staged D-Bus activation starts the PipeWire-enabled daemon with `--dbus --audio-backend pipewire`, writes under `target/tmp/fcitx-ime-pipewire-live-smoke`, and prints the staged daemon's `audio-devices` JSON diagnostics.
Live desktop PipeWire validation still needs manual confirmation. Recorder setup errors are expected to include the same target/format/sample-rate/channel plan.

If a live check fails, record the exact failure and do not mark the feature as done.

`just addon-dbus-adapter-lifecycle-smoke` verifies configured text adapter start/duplicate-start/stop diagnostics over DBus.
`just ime-e2e-smoke` includes fake outcome sink coverage for preedit, commit, command-mode selected-text deletion, candidate menus, and fallback commit behavior.

## Common commands

Use `just` as the primary local interface. The recipes mirror CI and make command intent explicit.

```sh
just fmt
just fmt-check
just lint
just test
just dbus-test
just dbus-lint
just addon-format-check
just addon-test
just addon-smoke
just addon-dbus-smoke
just addon-dbus-activation-smoke
just addon-dbus-configured-activation-smoke
just addon-dbus-adapter-lifecycle-smoke
just ime-configured-activation-smoke
just ime-e2e-smoke
just user-ime-command-demo-smoke
just user-ime-real-command-asr-wav-smoke
just user-ime-sherpa-sense-voice-smoke
just user-ime-command-demo
just user-ime-pipewire-live
just user-ime-status
just user-ime-clear
just check
just ci
just smoke
just e2e-demo
just pipewire-check
just sherpa-onnx-check
just sherpa-sense-voice-local-smoke
just ime-fcitx-live-probe
```

Current CI covers `just ci`, deterministic configured activation, deterministic IME E2E, adapter lifecycle, and PipeWire feature compile/test coverage. Live desktop recipes stay outside CI by design.

## Dependency notes

Arch Linux local native dependencies for the current C++ addon slice:

```sh
sudo pacman -S --needed base-devel cmake clang just pkgconf fcitx5
```

`fcitx5` provides the Fcitx5 Core/Utils headers and CMake/pkg-config metadata used by addon build and lint paths.

## Active work direction

The active migration target is **real desktop alpha**:

1. user install succeeds;
2. Fcitx5 is restarted with generated environment;
3. `fcitx5-vinput.so` loads;
4. normal trigger starts/stops recording;
5. live PipeWire capture or deterministic command input feeds a real recognition path;
6. result commits into a real application;
7. command mode can replace selected text;
8. `vinput doctor` and live probe clearly diagnose failures.

For priority details, use [`migration/e2e-replication-plan.md`](migration/e2e-replication-plan.md). For current gap status, use [`migration/function-gap-audit.md`](migration/function-gap-audit.md). For the native runtime/frontend parity backlog, use [`migration/e2e-capability-matrix.md`](migration/e2e-capability-matrix.md).

## Work selection rules

- Prefer work that moves real desktop alpha or real ASR alpha forward.
- Do not count deterministic smoke tests as live desktop proof.
- Keep user-profile mutations explicit and opt-in.
- Preserve deterministic tests for every new live-facing path.
- Defer distro packaging and broad GUI polish until real desktop alpha and real ASR alpha are proven.

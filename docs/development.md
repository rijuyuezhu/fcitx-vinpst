# Development guide

This guide defines project workflow, commit style, and validation tiers. Migration direction lives in [`migration/e2e-replication-plan.md`](migration/e2e-replication-plan.md); parity baseline lives in [`migration/function-gap-audit.md`](migration/function-gap-audit.md).

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
```

These checks prove the deterministic product spine. They do not prove live desktop behavior.

### User install changes

Prefer a temporary `HOME` unless the user explicitly wants to mutate the real profile:

```sh
tmp_home="$(mktemp -d)"
HOME="$tmp_home" VINPUT_USER_PROFILE=command-demo scripts/install-user-ime.sh
HOME="$tmp_home" VINPUT_USER_STATUS=1 scripts/install-user-ime.sh
rm -rf "$tmp_home"
```

### Optional live desktop checks

Run only inside a real desktop session where Fcitx5 and PipeWire are expected to work:

```sh
just ime-fcitx-live-probe
VINPUT_LIVE_INSTALL_COMMAND_DEMO=1 just ime-fcitx-live-probe
just pipewire-check
just ime-configured-pipewire-live
```

If a live check fails, record the exact failure and do not mark the feature as done.

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
just user-ime-command-demo
just user-ime-pipewire-live
just user-ime-status
just user-ime-clear
just check
just ci
just smoke
just e2e-demo
just pipewire-check
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

For priority details, use [`migration/e2e-replication-plan.md`](migration/e2e-replication-plan.md). For current gap status, use [`migration/function-gap-audit.md`](migration/function-gap-audit.md).

## Work selection rules

- Prefer work that moves real desktop alpha or real ASR alpha forward.
- Do not count deterministic smoke tests as live desktop proof.
- Keep user-profile mutations explicit and opt-in.
- Preserve deterministic tests for every new live-facing path.
- Defer distro packaging and broad GUI polish until real desktop alpha and real ASR alpha are proven.

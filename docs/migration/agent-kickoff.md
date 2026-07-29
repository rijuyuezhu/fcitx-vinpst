# Agent kickoff

Use this as the compact handoff for implementation work. It is a pointer to current sources, not another progress log.

## Mission

Continue `fcitx-vinput-rs` from the usable CLI/daemon alpha toward **real desktop native alpha**. Do not rebuild completed management, registry, native ASR, activation, or retained-frontend surfaces.

The next proof is:

```text
Fcitx trigger -> live PipeWire capture -> native ASR -> partial/preedit -> application commit
```

Then prove command-mode selected-text replacement across real applications.

## Repositories

- Rust rewrite: `/workspace/fcitx-vinput-rs`
- Legacy reference: `/workspace/fcitx5-vinput`

## Required reading

1. `AGENTS.md`
2. `docs/README.md`
3. `docs/development.md`
4. `docs/migration/function-gap-audit.md`
5. `docs/migration/e2e-capability-matrix.md`
6. `docs/migration/e2e-replication-plan.md`
7. `docs/migration/live-desktop-validation.md`
8. the relevant `docs/architecture/*` contract
9. `docs/legacy/source-annotations.md` only when comparing legacy behavior

## Current baseline

- CLI and daemon management are broadly implemented and deterministically tested.
- Current registry native ASR families have typed runtime mappings and real-WAV evidence.
- Current `registry/adapters.json` listing and adapter install are implemented with short ids, mirror fallback, executable scripts, environment placeholders, config backups, and guarded managed updates.
- Current `registry/providers.json` listing/install/update-by-reinstall are implemented with short ids, batch/streaming validation, mirror fallback, executable scripts, legacy timeout/env preservation, config backups, guarded managed updates, removal parity, and command-provider script editing.
- Provider and adapter available lists load localized title/description keys from root-level registry i18n files while preserving stable machine ids and short selectors.
- Provider removal protects local entries, permits active non-local removal, clears the active selection, and reports an explicit no-provider ASR diagnostic without choosing a fallback.
- `sherpa-native-live` validates and copies `libsherpa-onnx` and `libonnxruntime`, then activates through `vinput-daemon-with-vinput-env.sh`.
- `user-ime-sherpa-native-activation-smoke` proves temporary-HOME activation and exact recognition.
- Native online partials reach concrete Fcitx preedit before stop.
- Final normal commit and command candidate replacement reach concrete test `InputContext` implementations.
- Persistent keys, Tap/Hold/Both, scene/ASR menus, filtering, i18n, notifications, and daemon recovery are implemented.
- Remote text settings, protocol semantics, browser assets, and a standalone Axum HTTP/WebSocket runtime are deterministic; automatic D-Bus daemon reload/shutdown integration and live cross-device proof remain.
- Real Fcitx, live PipeWire, and real application behavior remain unproven.
- Remote services, packaging, upgrades, and optional GUI work remain later milestones.

Do not maintain or quote a parity percentage. Use the evidence stages in the audit and matrix.

## Start-of-session checks

```sh
cd /workspace/fcitx-vinput-rs
git status --porcelain=v1 -b
git log -8 --oneline --decorate
```

Run the narrowest relevant check before editing and the full relevant tier before handoff.

## Implementation rules

- Communicate with the user in Chinese.
- Keep code, comments, test names, docs identifiers, and commit messages in English.
- Preserve legacy service names, methods, signals, status strings, config semantics, and recognition JSON.
- Keep the C++ frontend thin; backend logic belongs in Rust.
- Treat mock/file/session-bus/temporary-HOME evidence as deterministic, not live.
- Keep real-profile changes explicit and opt-in.
- Prefer one milestone-enabling change over broad cleanup.
- Add focused regression coverage for every live-facing fix.
- Keep commits small and Conventional Commit formatted.

## Validation

Documentation-only:

```sh
git diff --check
cargo test -p vinput-cli --test architecture_docs
```

Rust/core:

```sh
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

D-Bus/frontend:

```sh
just dbus-test
just dbus-lint
just addon-format-check
just addon-test
```

Full deterministic handoff:

```sh
just ci
```

Live checks require a real desktop and follow `live-desktop-validation.md`. Record exact failures; never mark a path complete because a deterministic smoke passed.

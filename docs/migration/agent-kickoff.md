# Agent kickoff

Use this as the compact handoff for implementation work. It is a pointer to current sources, not another progress log.

## Mission

Continue `fcitx-vinput-rs` from the usable CLI/daemon alpha and the proven core **real desktop native alpha** path. Do not rebuild completed management, registry, native ASR, activation, or retained-frontend surfaces.

The core proof is complete:

```text
Fcitx trigger -> isolated PipeWire virtual source -> native ASR -> partial/input-panel updates -> application commit
```

Normal/command application paths, default physical-microphone dictation, local-adapter and loopback OpenAI-compatible HTTP-provider command replacement plus HTTP-404 selected-text preservation/recovery, clipboard fallback, scene/ASR selection and paging, installed-catalog zh_CN menu, official English/zh_CN configuration-form labels and trigger-mode choices, scene-info/ASR-switch/error-summary notification localization, persisted Tap/Hold/Both modes, notifications, focus/owner recovery, reload, one real model-switch roundtrip, internal/command and independent Whisper provider roundtrips, invalid-remote prepare failure/recovery, and a successful loopback remote HTTP ASR roundtrip are live-proven. The next proof is additional device/application breadth, real cloud text-provider behavior, real hosted-ASR network/credential behavior, and additional locales.

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
- `ime-fcitx-virtual-source-live` proves F9 normal dictation and F10 command transformation through the installed addon, current session-bus activation, a preflight-verified isolated PipeWire sink/source, streaming partial input-panel updates, surrounding-text deletion/replacement, and zero-delete Wayland primary-selection fallback without physical audio devices.
- GTK3, Qt6, and Chromium/Ozone normal and command paths, scene/ASR display/filter, scene selection, configured-key scene paging, focus handoff, verified owner loss, and same-provider reload are live-proven.
- F8/Enter model selection switches streaming Zipformer to offline Paraformer, proves a final commit, restores the original profile, reloads Zipformer, and proves streaming partials plus another commit; service/profile/Fcitx/backend restoration is retained evidence.
- Information notifications and daemon-originated error notifications are observed from the current Fcitx/daemon PIDs with exact payload and timeout checks.
- Scene paging and a 14-target ASR paging profile both prove `1/2 -> 2/2 -> 1/2`, zero commits, configured-key handling, and exact restoration.
- Persistent keys and Tap/Hold/Both are implemented and live-proven; installed user-catalog zh_CN Scene/ASR titles/status, localized scene-information text and information/error summaries, verbatim technical error bodies, and English/original-locale restoration are live-proven; filtering, broader i18n, notifications, and daemon recovery are implemented, with notification/recovery paths also live-proven.
- Remote text settings, protocol semantics, browser assets, Axum runtime, normal D-Bus daemon startup/provider-selection/reload ownership, `SIGTERM` shutdown, and redacted endpoint diagnostics are deterministic; live cross-device browser proof remains.
- ASR provider switching is live-proven with a compatibility child that reuses Sherpa, an independent whisper.cpp recognizer/model, an invalid remote endpoint that proves prepare-failure preservation, and a successful OpenAI-compatible remote HTTP path. The remote success gate verifies real F8/Enter selection, multipart WAV/Bearer/model/language/prompt transport, a final-only application commit, redacted traces, and Zipformer restoration against a loopback process. The external OpenAI-compatible text-provider process is likewise live-proven against loopback for both exact successful replacement and HTTP-404 preservation with no delete/commit followed by recovery. Real hosted-service DNS/TLS/proxy/rate-limit/outage behavior, credential rotation/custody, additional locales/devices, and broader cross-application behavior remain unproven.
- Packaging now covers automatic executable-replacement handoff for current systemd/direct metadata and guarded manual handoff for older owners. `run-daemon-handoff-smoke.sh` proves current-owner no-op, old-systemd reload/restart, old-direct same-user idle termination/reactivation, and reload-failure preservation; `just systemd-upgrade-live` proves replacement-triggered PID restart under the real user systemd manager and exact restoration of the original direct profile. A package-installed upgrade, automatic cross-user invocation, removal handling, production publication, incompatible-state rollback, and the deferred Rust GUI remain; see [`../architecture/packaging-contract.md`](../architecture/packaging-contract.md) and [`../architecture/gui-contract.md`](../architecture/gui-contract.md).

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
```

Review documentation directly; do not add source-text or prose-presence tests.

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

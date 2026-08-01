# Agent kickoff

Use this as the compact handoff for implementation work. It is a pointer to current sources, not another progress log.

## Mission

Continue `fcitx-vinput-rs` from the usable CLI/daemon alpha and the proven core **real desktop native alpha** path. Do not rebuild completed management, registry, native ASR, activation, or retained-frontend surfaces.

The core proof is complete:

```text
Fcitx trigger -> isolated PipeWire virtual source -> native ASR -> partial/input-panel updates -> application commit
```

Normal/command GTK3, GTK4, Qt6, Chromium, GNOME Text Editor, kitty, and VS Code/Electron application paths plus three-cycle repetition and ten-cycle bounded GTK4 normal/command soak in one window and one daemon ownership generation, default physical-microphone dictation, local-adapter and loopback OpenAI-compatible HTTP-provider command replacement plus HTTP-404 selected-text preservation/recovery, primary-selection fallback, double-empty no-selection rejection with exact primary-selection restoration, scene/ASR selection and paging, installed-catalog zh_CN menu, official English/zh_CN configuration-form labels and trigger-mode choices, scene-info/ASR-switch/error-summary notification localization, persisted Tap/Hold/Both modes, notifications, focus/owner recovery, reload, one real model-switch roundtrip, internal/command and independent Whisper provider roundtrips, invalid-remote prepare failure/recovery, and a successful loopback remote HTTP ASR roundtrip are live-proven. The next proof is additional device/application breadth, hour-scale soak, and real hosted-provider credential lifecycle and production CA distribution/revocation operations; local ASR and text-provider plain-HTTP proxy routing, proxy-URL Basic authentication for direct HTTP over plain-HTTP proxies and CONNECT through both plain-HTTP and TLS-protected HTTPS proxy endpoints, `NO_PROXY`, additional PEM roots through `SSL_CERT_FILE`, retained built-in `WebPKI` verification, one local CA-signed TLS interception relay with no retained plaintext, same-daemon atomic replacement of one CA-file path with mismatch rejection and idle recovery, 429/503, fail-closed 3xx handling with an untouched redirect target, request and response-body timeouts, a 1 MiB cap for success and error response bodies, untrusted self-signed TLS rejection, DNS failure, connection refusal, and redaction semantics are deterministic; the text path also preserves the legacy 4000 ms default when a scene omits `timeout_ms`, and local command helpers are parent-enforced with whole-process-group termination at that deadline and independent 1 MiB stdout/stderr limits. English fallback plus zh_CN matches the legacy product locale set; any further UI locale is optional expansion, not a migration blocker.

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
- `scripts/tests/install/run-user-ime-sherpa-native-activation-smoke.sh` proves temporary-HOME activation and exact recognition.
- Native online partials reach concrete Fcitx preedit before stop.
- Final normal commit and command candidate replacement reach concrete test `InputContext` implementations.
- `scripts/live/niri/run-ime-fcitx-virtual-source-live.sh` proves F9 normal dictation and F10 command transformation through the installed addon, current session-bus activation, a preflight-verified isolated PipeWire sink/source, streaming partial input-panel updates, surrounding-text deletion/replacement, and zero-delete Wayland primary-selection fallback without physical audio devices.
- GTK3, GTK4, Qt6, Chromium/Ozone, GNOME Text Editor, kitty, and VS Code/Electron normal and command paths, scene/ASR display/filter, scene selection, configured-key scene paging, focus handoff, verified owner loss, and same-provider reload are live-proven. The GTK4 gate verifies the exact niri-focused window, sends real `/dev/uinput` F9/F10 events, and uses isolated PipeWire audio rather than synthetic GDK events. Its repeat gate completes three normal cycles and three command cycles in one window and one daemon ownership generation. The bounded-soak gate extends that to ten normal plus ten command cycles, with 20 matching keys per mode, ten completion events, nine ready transitions, at least seven D-Bus partials per cycle, increasing results, and idle same-owner proof before exact profile restoration. The first ten-cycle attempt exposed the old fixed 60-second timeout after eight cycles; the runner now budgets 15 seconds per expected cycle with a 60-second floor unless explicitly overridden. The standalone GNOME Text Editor gate additionally records daemon partials and verifies the saved temporary-file bytes after real F9 insertion or Ctrl+A/F10 replacement followed by Ctrl+S. The kitty gate verifies exact niri PID/window focus, normal terminal insertion, command-mode PRIMARY-selection fallback, foreground terminal output bytes, and byte-for-byte PRIMARY-selection restoration. The automatic Chromium/Ozone gate reruns normal and command paths with isolated PipeWire audio and real uinput keys while proving that the browser has no sandbox-disable flag and a real renderer has `NoNewPrivs=1`, seccomp filter mode, zero effective capabilities, and a nested PID namespace. Chromium and VS Code command modes deliberately use distinct application-selection and PRIMARY-selection sentinels: both results prove PRIMARY fallback rather than surrounding-text transport, and the current-run PRIMARY bytes are restored. VS Code additionally verifies saved file bytes, an isolated profile, zero process/window residue, and an Electron renderer with `NoNewPrivs=1`, seccomp filter mode, zero effective capabilities, and a nested PID namespace.
- F8/Enter model selection switches streaming Zipformer to offline Paraformer, proves a final commit, restores the original profile, reloads Zipformer, and proves streaming partials plus another commit; service/profile/Fcitx/backend restoration is retained evidence.
- Information notifications and daemon-originated error notifications are observed from the current Fcitx/daemon PIDs with exact payload and timeout checks.
- Scene paging and a 14-target ASR paging profile both prove `1/2 -> 2/2 -> 1/2`, zero commits, configured-key handling, and exact restoration.
- Persistent keys and Tap/Hold/Both are implemented and live-proven; installed user-catalog zh_CN Scene/ASR titles/status, localized scene-information text and information/error summaries, verbatim technical error bodies, and English/original-locale restoration are live-proven; this English-plus-zh_CN surface matches the legacy locale set, while notification/recovery paths are also live-proven.
- Remote text settings, protocol semantics, browser assets, Axum runtime, normal D-Bus daemon startup/provider-selection/reload ownership, `SIGTERM` shutdown, and redacted endpoint diagnostics are deterministic. A real sandboxed Chromium page passes through the host's non-loopback LAN address with exact Realtime output and cleanup evidence. `scripts/live/network/run-remote-text-external-device-live.sh` now provides the fail-closed random-challenge collector; another physical device still needs to complete it.
- ASR provider switching is live-proven with a compatibility child that reuses Sherpa, an independent whisper.cpp recognizer/model, an invalid remote endpoint that proves prepare-failure preservation, and a successful OpenAI-compatible remote HTTP path. The remote success gate verifies real F8/Enter selection, multipart WAV/Bearer/model/language/prompt transport, a final-only application commit, redacted traces, and Zipformer restoration against a loopback process. The external OpenAI-compatible text-provider process is likewise live-proven against loopback for both exact successful replacement and HTTP-404 preservation with no delete/commit followed by recovery. The production remote-ASR daemon and `vinput llm test` text-provider path now have deterministic plain-HTTP proxy routing, proxy-URL Basic authentication including CONNECT through TLS-protected HTTPS proxy endpoints, one local CA-signed TLS interception relay with no retained plaintext, same-daemon atomic replacement of one CA-file path with mismatch rejection and idle recovery, `NO_PROXY`, 429/503, fail-closed 3xx handling with an untouched redirect target, separately classified request and response-body timeouts, the legacy 4000 ms default for text scenes without `timeout_ms`, a 1 MiB cap for success and error response bodies, self-signed TLS rejection, DNS failure, connection-refusal, and secret-redaction process coverage. Provider diagnostics share a fail-closed URL representation that removes userinfo/fragments and hides query values without changing request-time query parameters; ASR `Debug` hides prompts, text `Debug` hides body contents, and known credentials echoed in HTTP error bodies are replaced. Real third-party DNS/TLS, PAC, NTLM/Kerberos, enterprise TLS-interception policy and certificate deployment, provider-specific outage/rate-limit behavior, provider credential rotation/custody and production CA distribution/revocation, additional devices, and broader cross-application behavior remain unproven. Extra UI locales beyond the legacy English/zh_CN set are optional future expansion.
- Packaging now covers checked release-time native-runtime bundle selection, automatic executable-replacement handoff for current systemd/direct metadata, automatic ownership-verified cross-user dispatch into the guarded handoff for existing older owners, and guarded package-removal preparation. `run-daemon-handoff-smoke.sh` proves current-owner no-op, old-systemd reload/restart, old-direct same-user idle termination/reactivation, and reload-failure preservation; `scripts/tests/daemon/run-package-upgrade-handoff-smoke.sh` proves no-owner skip, trusted user environment construction, exact guarded command dispatch, and failure propagation; `scripts/live/system/run-systemd-upgrade-live.sh` proves replacement-triggered PID restart under the real user systemd manager and exact restoration of the original direct profile. The removal gates prove two-phase all-session preflight before mutation, activation-cache invalidation before process/service changes, no-owner/systemd/direct handling, refusal to interrupt an active recording for either owner type, trusted runtime-bus cross-user dispatch, and activation-file/cache rollback when any session rejects removal. Future-schema refusal and byte-preserving package install/upgrade/pkgrel-rollback/removal are proven. The external-user Arch lifecycle guide and command smoke are complete. An actual host package-installed upgrade, live production multi-user upgrade/removal, production publication/key operations, regression on an unrelated external machine, and the deferred Rust GUI remain; rollback of a real schema migration becomes relevant only when schema 2 exists; see [`../architecture/packaging-contract.md`](../architecture/packaging-contract.md) and [`../architecture/gui-contract.md`](../architecture/gui-contract.md).

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
just test
just lint
just fmt-check
just test
```

Full deterministic handoff:

```sh
just ci
```

Live checks require a real desktop and follow `live-desktop-validation.md`. Record exact failures; never mark a path complete because a deterministic smoke passed.

# E2E replication plan

This is the active execution plan. Status belongs in [`function-gap-audit.md`](function-gap-audit.md); subsystem detail belongs in [`e2e-capability-matrix.md`](e2e-capability-matrix.md).

## Product target

A compatible replacement must let a user:

- initialize configuration and managed directories from CLI;
- discover, install, select, and diagnose a registry model;
- load the retained addon in Fcitx5;
- dictate normally with visible partial preedit and final application commit;
- dictate commands over selected text and replace it safely;
- configure keys, scenes, models, providers, adapters, and devices without manual JSON edits;
- diagnose daemon, activation, native runtime, audio, and frontend failures;
- install, upgrade, and remove the product predictably.

Compatibility means preserving user-visible contracts, not mechanically translating C++ source.

## Milestones

| Milestone | State | Exit criteria |
| --- | --- | --- |
| M0 Repository health | complete | clean deterministic checks and current docs |
| M1 Deterministic product spine | complete | staged addon/daemon and outcome smokes |
| M2 Native ASR proof | complete for current families | registry model construction and real WAV recognition |
| M3 Usable CLI/daemon alpha | complete | management flow without manual JSON edits |
| M4 Real desktop native alpha | active; core, toolkit, fallback, menu, localization, notification, model-switch, command/Whisper/remote provider success/failure, external-text-provider, trigger-mode, physical-microphone, and recovery paths live-proven | real Fcitx client, isolated PipeWire injection, default physical ALSA Digital Microphone dictation, GTK3/Qt6/Chromium normal and command paths, local and loopback OpenAI-compatible text replacement, primary-selection fallback, scene/ASR selection and paging, installed-catalog zh_CN menu, official English/zh_CN configuration-form labels and trigger-mode choices, scene-info/ASR-switch/error-summary notification localization, same-provider/command/independent-Whisper switching, invalid-remote prepare preservation, and successful remote HTTP multipart recognition, persisted Tap/Hold/Both timing, notifications, focus handoff, owner loss, and reload are proven; additional physical-device switching, real hosted-ASR and cloud-text operations, and additional locales remain |
| M5 Resource parity | complete | provider/adapter install and update-by-reinstall, localized discovery, provider script editing/removal, adapter removal, and adapter runtime selectors |
| M6 Release readiness | partial | The checked Arch package, signed candidate pipeline, explicit handoff, and automatic executable-replacement restart for daemons started by the new systemd unit are deterministic; production publication, first-upgrade/direct-activation/removal handoff, incompatible-state rollback, production key operations, live installed proof, and external-user regression remain. Detailed evidence belongs in [`../architecture/packaging-contract.md`](../architecture/packaging-contract.md). |

## Completed: usable CLI/daemon alpha

The following are implemented and covered by deterministic tests:

- model registry list/info/install/use/remove;
- current ASR provider registry list/install with batch/streaming validation, short ids, mirror fallback, managed overwrite protection, executable scripts, timeout/env preservation, and config backups;
- legacy-compatible provider removal with local-provider protection, active-selection clearing, and explicit short-id resolution;
- legacy-compatible command-provider script editing with installed-selector validation, command/argument file resolution, editor fallback, and dry-run diagnostics;
- current adapter registry list/install with short ids, mirror fallback, managed overwrite protection, executable scripts, and config backups;
- adapter short-id removal with config backup, `--output` preservation, and guarded in-place cleanup limited to the expected managed script path;
- adapter start/stop/status selectors validate installed config entries and resolve explicit registry short ids before D-Bus calls;
- localized provider/adapter title and description loading from root-level registry i18n files with stable machine-id fallback;
- config initialization and mutation;
- provider, hotword, device, scene, LLM, and adapter management;
- daemon and recording control;
- doctor/runtime/audio/owner diagnostics;
- native offline/online ASR for current registry families;
- generic native runtime bundle installation and D-Bus activation;
- retained-addon menus, keys, filtering, i18n, notifications, owner recovery, partial preedit, commit, and command replacement.

Implemented through D-Bus, the streaming path delivers recorder chunks, emits deduplicated live `RecognitionPartial` signals, renders partial-first preedit, and preserves final results for synchronous stop.

Live in a real user session, `ime-fcitx-virtual-source-live` now proves F9 normal dictation and F10 selected-text command replacement through the installed addon, current session-bus activation, a non-silent preflight-verified PipeWire virtual source, a streaming native model, input-panel partials, deletion, and final commit in real Fcitx clients. The gate does not use or claim physical speaker/microphone behavior.

## P0: real desktop native alpha

1. Run the deterministic gate before live work.
2. Install `sherpa-native-live` with a registry-installed supported model.
3. Restart Fcitx5 through the generated environment wrapper.
4. Prove addon discovery and D-Bus activation in the real session.
5. Keep the live-proven GTK3, Qt6, Chromium/Ozone, surrounding-text, and primary-selection-fallback paths green.
6. Keep the live-proven focus-handoff, owner-loss, same-provider reload, default physical-microphone dictation, scene/ASR selection and paging, installed-catalog zh_CN menu, official English/zh_CN configuration-form labels and trigger-mode choices, plus scene-info/ASR-switch/error-summary notification localization, F8 model/command/Whisper/remote success plus remote prepare-failure preservation, Tap/Hold/Both timing, and notification paths green; next exercise additional device switching, real hosted-ASR operational behavior, and additional locales.
7. Keep both live-proven command paths green: the local adapter and the loopback OpenAI-compatible HTTP provider; next prove real cloud-provider credentials/network failure behavior.
8. Record exact failures and add deterministic regressions before fixing them.

The validation procedure is [`live-desktop-validation.md`](live-desktop-validation.md).

## P1: parity after live alpha

- Port other remaining native model layouts only when registry or user demand is concrete.
- Validate one real third-party OpenAI-compatible text service, including credential handling and network failure behavior.
- Broaden daemon-originated notification categories from observed needs.
- Reduce oversized modules only along feature boundaries.
- Prove a real browser/device flow using the redacted endpoints reported by `vinput daemon status`.

## P2: release readiness

- keep the checked Arch package for the CLI, daemon, addon, metadata, translations, VAD asset, activation service, and private native runtime green through `just arch-pkgbuild-check` and `just arch-package-smoke`;
- validate the implemented automatic executable-replacement restart in a live installed session, then define first-upgrade/direct-activation/removal handoff, incompatible-state rollback, and destructive direct-PID stale-owner behavior; keep the explicit systemd-user handoff conditional and post-verified;
- publish the selected production package, signatures, repository metadata, detached manifest signature and independently distributed pinned fingerprint, and a short supported installation path; do not publish synthetic `pkgrel=2` or ephemeral-key test artifacts;
- run live validation on supported desktop/application combinations;
- add external-user regression coverage.

## Work selection rules

- Prefer work that directly moves M4.
- Keep mock, file-input, session-bus, and temporary-HOME checks green.
- Do not call deterministic evidence live proof.
- Keep real-profile mutation explicit and opt-in.
- Preserve public wire and frontend contracts.
- Keep commits focused and avoid broad cleanup.

## Next recommended slice

Validate the implemented remote ASR client against a real hosted service, cover real cloud text-provider operational behavior, additional physical-device switching, and additional locales while keeping the retained remote success/failure, independent Whisper, external HTTP text-provider, ASR cross-provider, physical-microphone, toolkit, installed-catalog localization, trigger-mode, ASR paging, model-selection, notification, fallback, scene-menu, and recovery evidence green. Port other model families, package formats, or remote services only when they unblock or follow from that evidence. Keep the standalone Rust GUI deferred until the spike criteria in [`../architecture/gui-contract.md`](../architecture/gui-contract.md) are justified.

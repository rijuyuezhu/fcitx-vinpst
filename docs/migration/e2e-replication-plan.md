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
| M4 Real desktop native alpha | active; core, toolkit, fallback, menu, localization, notification, model-switch, command/Whisper/remote provider success/failure, external-text-provider, trigger-mode, physical-microphone, and recovery paths live-proven | real Fcitx client, isolated PipeWire injection, typed same-daemon source A -> source B switching, default physical ALSA Digital Microphone dictation, GTK3/GTK4/Qt6/Chromium, GNOME Text Editor, kitty, and VS Code/Electron normal and command paths including explicit Chromium and VS Code renderer-sandbox proof, three-cycle GTK4 repetition, and ten-cycle normal/command bounded GTK4 soak in one window and one daemon owner, local and loopback OpenAI-compatible text replacement, primary-selection fallback, scene/ASR selection and paging, installed-catalog zh_CN menu, official English/zh_CN configuration-form labels and trigger-mode choices, scene-info/ASR-switch/error-summary notification localization, same-provider/command/independent-Whisper switching, invalid-remote prepare preservation, and successful remote HTTP multipart recognition, persisted Tap/Hold/Both timing, notifications, focus handoff, owner loss, and reload are proven; additional physical-device switching breadth, hour-scale soak, and real hosted-ASR/cloud-text plus credential operations remain; deterministic ASR and text-provider plain-HTTP proxy routing, Basic authentication for direct HTTP over plain-HTTP proxies and CONNECT through both plain-HTTP and TLS-protected HTTPS proxy endpoints, `NO_PROXY`, additional PEM roots through `SSL_CERT_FILE`, retained built-in `WebPKI` verification, one local CA-signed TLS interception relay with no retained plaintext, same-daemon atomic replacement of one CA-file path with mismatch rejection and idle recovery, 429/503, fail-closed 3xx handling with an untouched redirect target, request and response-body timeouts, a 1 MiB cap for success and error response bodies, untrusted self-signed TLS rejection, DNS failure, connection-refusal and redaction semantics are complete, the text path preserves the legacy 4000 ms default for omitted scene timeouts, and local text and command-ASR helpers share whole-process-group cleanup, direct-child descendant cleanup, and independent 1 MiB stdout/stderr limits; text has an effective scene deadline while command ASR keeps omitted timeout explicitly unconfigured; extra UI locales are optional beyond the legacy English/zh_CN set |
| M5 Resource parity | complete | provider/adapter install and update-by-reinstall, localized discovery, provider script editing/removal, adapter removal, and adapter runtime selectors |
| M6 Release readiness | partial | Checked release-time native-runtime bundle selection, the Arch package and signed candidate pipeline now including the Rust GUI, current-metadata automatic handoff, guarded old-systemd/direct handoff, private-session direct replacement proof, real user-systemd restart/restore proof, automatic ownership-verified cross-user upgrade/removal dispatch, and unsupported-future-schema refusal with byte-preserving transactions are complete. Flatpak host routing, permission diagnostics, and service generation are deterministic. A checked Flatpak bundle/install gate, Debian/Fedora/openSUSE recipes, an actual host package-installed upgrade, production multi-user lifecycle, publication/key operations, and unrelated-machine regression remain. Detailed evidence belongs in [`../architecture/packaging-contract.md`](../architecture/packaging-contract.md). |
| M7 Rust management GUI | active baseline | Iced Control/Resources/LLM/Hotwords pages, typed config validation, redacted diagnostics, direct D-Bus status queries, headless package self-check, desktop entry/icons, and Arch integration are complete. Typed mutations, resource lifecycle/progress/cancellation, owner subscriptions/reconnect, localization/accessibility, and real Wayland/X11 interaction proof remain. |

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

Live in a real user session, `scripts/live/niri/run-ime-fcitx-virtual-source-live.sh` now proves F9 normal dictation and F10 selected-text command replacement through the installed addon, current session-bus activation, a non-silent preflight-verified PipeWire virtual source, a streaming native model, input-panel partials, deletion, and final commit in real Fcitx clients. The gate does not use or claim physical speaker/microphone behavior.

## P0: real desktop native alpha

1. Run the deterministic gate before live work.
2. Install `sherpa-native-live` with a registry-installed supported model.
3. Restart Fcitx5 through the generated environment wrapper.
4. Prove addon discovery and D-Bus activation in the real session.
5. Keep the live-proven GTK3, Qt6, Chromium/Ozone and VS Code/Electron renderer-sandbox, surrounding-text, and primary-selection-fallback paths green.
6. Keep the live-proven focus-handoff, owner-loss, same-provider reload, default physical-microphone dictation, same-recorder two-source PipeWire target switching, isolated real-`wpctl` output duck/restore, scene/ASR selection and paging, installed-catalog zh_CN menu, official English/zh_CN configuration-form labels and trigger-mode choices, plus scene-info/ASR-switch/error-summary notification localization, F8 model/command/Whisper/remote success plus remote prepare-failure preservation, Tap/Hold/Both timing, and notification paths green; next exercise additional physical-device breadth, audible hardware-output ducking, and real hosted-ASR/provider credential behavior beyond the deterministic local network-semantics gate. English fallback plus zh_CN already matches the legacy locale set.
7. Keep both live-proven command paths and the deterministic `vinput llm test` network-semantics gate green; next prove real hosted-provider credential lifecycle and production CA distribution/revocation operations, PAC, NTLM/Kerberos, enterprise TLS-interception policy and certificate deployment, provider-specific outage behavior, and cross-application recovery.
8. Record exact failures and add deterministic regressions before fixing them.

The validation procedure is [`live-desktop-validation.md`](live-desktop-validation.md).

## P1: parity after live alpha

- Port other remaining native model layouts only when registry or user demand is concrete.
- Validate one real third-party OpenAI-compatible text service, including credential handling and network failure behavior.
- Broaden daemon-originated notification categories from observed needs.
- Keep the source-layout regression guard green and continue splitting only when data, orchestration, transport, formatting, or platform integration form distinct feature boundaries.
- Keep the real Chromium same-host LAN gate green, then complete `scripts/live/network/run-remote-text-external-device-live.sh` from another physical device and repeat that flow using a redacted endpoint reported by `vinput daemon status`.

## P2: release readiness

- keep checked release-time native-runtime bundle selection and the Arch package for the CLI, daemon, addon, metadata, translations, VAD asset, activation service, and private runtime green through `scripts/release/check-arch-pkgbuild.sh` and `just package-smoke`;
- validate current-metadata restart and the implemented ownership-verified cross-user guarded handoff through an actual host package-installed upgrade and live multi-user environment; keep every handoff conditional, identity-guarded, and post-verified. Keep unknown future schemas refusal-only and byte-preserving; add migration rollback only when a second production schema exists;
- publish the selected production package, signatures, repository metadata, detached manifest signature and independently distributed pinned fingerprint, following the tracked external-user path in [`../user/installation.md`](../user/installation.md); do not publish synthetic `pkgrel=2` or ephemeral-key test artifacts;
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

Advance three release-critical tracks in parallel: add typed GUI mutations and daemon/resource actions on the implemented Rust/Iced baseline; turn the deterministic Flatpak runtime contract into a checked bundle/build/install transaction; and add the next non-Arch package recipe. Keep the existing desktop, provider, network, lifecycle, and Arch evidence green, then continue hosted-provider and additional physical-device proof where credentials/hardware are available. Do not regress into a second C++ GUI or duplicate business logic in presentation code.

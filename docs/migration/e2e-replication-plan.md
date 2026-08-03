# E2E replication plan

Reviewed: 2026-08-03

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
| M5 Resource parity | complete | provider/adapter install and update-by-reinstall, localized discovery, provider script editing/removal, adapter removal/runtime selectors, and GUI model install/update/discovery/inactive removal |
| M6 Release readiness | partial | Arch package/repository/signature/candidate, Debian 12/Ubuntu 24.04 Docker transactions, locked Nix build, RPM-family transaction baseline, and the checked x86_64 Flatpak extension build/install/update/remove/bundle transaction are complete. Live Flatpak desktop/Fcitx/PipeWire/host-systemd proof, Flatpak publication/signing policy, Fedora/openSUSE repository/signing/SELinux/live-scriptlet proof, Nix binary-cache publication, actual host upgrade, production multi-user lifecycle, publication keys, and unrelated-machine regression remain. |
| M7 Rust management GUI | active interactive baseline | Control editing/recording and Resources scene/model lifecycle use shared typed persistence and safety contracts. Scene forms add and edit fully typed definitions, keep ids immutable while editing, validate optional and numeric fields through the complete config contract, select the active scene, refuse active-scene removal, and remove inactive scenes with conflict-aware atomic persistence, backups, and daemon reload reporting. Model install/update/discovery/inactive removal uses shared registry safety, reports typed phase/byte progress, supports cleanup-safe cancellation and exact-selector retry, rejects stale completions, and requests cancellation on GUI shutdown. Resources and LLM install/update registry command providers and text adapters with pre-download required/optional environment entry, secure inputs and redacted diagnostics, managed-update value prefill/replacement, unrelated-environment preservation, managed script publication, user-defined-entry refusal, atomic validated config persistence, daemon reload, cooperative cancellation, retry, and stale preparation/install/recovery rejection. A published-script/config-save split failure enters an explicit recovery state that reloads and commits current config without re-downloading, revalidates the regular script path, and supports deliberate dismissal while keeping the script. Selectable model/provider/LLM/adapter detail panels expose typed metadata, redacted endpoints, and configuration counts without raw credentials or process contents. Exact managed command providers can be opened through a shared typed editor plan that preserves CLI behavior, launches direct argv, and refuses path mismatches. They also remove only exact managed-root entries, reject active providers and user-defined resources, commit validated config removal first, and clean up the unreferenced script. A service-name-filtered `NameOwnerChanged` subscription is installed before a non-activating owner sample, immediately reconciles owner loss/return, invalidates stale snapshot generations, reconnects after stream failure, and enables a serialized 30-second non-activating fallback only while degraded. The LLM page adds and edits typed providers with secure API-key entry, immutable edit ids, redacted diagnostics, unknown-field preservation, dirty/reset state, shared guarded persistence, reload reporting, and reference-safe removal. It tests configured providers through the shared production text adapter on a blocking worker, keeps test input/response content out of generic state diagnostics, applies the legacy 4000 ms timeout, and reports only candidate counts. LLM provider selection, hotword lifecycle and selection flows, broader resource-specific error taxonomy, command-mode selection, localization/accessibility, and live GUI proof remain. |

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

- keep shared release-time native-runtime validation, the Arch package/repository/signature/candidate pipeline, and the RPM release-1/release-2 build plus user-namespace transaction green through `just package-check`, `just package-smoke`, and `just rpm-package-smoke`;
- validate current-metadata restart and the implemented ownership-verified cross-user guarded handoff through an actual host package-installed upgrade and live multi-user environment; keep every handoff conditional, identity-guarded, and post-verified. Keep unknown future schemas refusal-only and byte-preserving; add migration rollback only when a second production schema exists;
- publish the selected production package, signatures, repository metadata, detached manifest signature and independently distributed pinned fingerprint, following the tracked external-user path in [`../user/installation.md`](../user/installation.md); do not publish synthetic `pkgrel=2` or ephemeral-key test artifacts;
- run live validation on supported desktop/application combinations;
- add external-user regression coverage.

## Work selection rules

- Prefer work that directly advances M7, the Rust management GUI baseline.
- Treat M4 desktop and M6 release work as regression maintenance unless a blocking issue or explicit user request requires new expansion.
- Keep mock, file-input, session-bus, temporary-HOME, desktop, and package evidence green.
- Do not call deterministic evidence live proof.
- Keep real-profile mutation explicit and opt-in.
- Preserve public wire and frontend contracts.
- Keep commits focused and avoid broad cleanup.

## Next recommended slice

Continue the GUI track with LLM provider selection, then hotword lifecycle and selection flows; after that add broader error taxonomy, command-mode, localization/accessibility, and live Wayland/X11 proof. Keep the recorded release and platform gaps intact, but defer new packaging work until the GUI management baseline advances; retained package and desktop evidence must remain green.

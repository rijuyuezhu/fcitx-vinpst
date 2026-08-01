# Function gap audit

Reviewed: 2026-07-31

This document is the current Rust-versus-legacy status baseline. It records implementation stage and evidence; it does not assign a release percentage.

## Review baseline

- Rust branch: `feat/m4-toolkit-parity`
- Implementation baseline: current branch, including provider/adapter registry installation and the checked Arch package recipe.
- Legacy reference: `/workspace/fcitx5-vinput` at `6cdcac8`.

## Executive conclusion

The Rust rewrite is a **usable CLI/daemon alpha** with a strong deterministic product spine. It is not a full desktop replacement yet.

The project has crossed the main implementation threshold for local native ASR and retained-frontend integration:

- Current registry native ASR families are mapped and real-WAV tested, including offline and online paths.
- The current upstream provider and adapter registries are parsed directly; short ids, batch/streaming protocol validation, mirror-backed script install, executable publication, environment placeholders, config backups, timeout/env preservation, and guarded managed updates are deterministic-test proven.
- Provider removal now matches the legacy safety contract: local providers cannot be removed, active non-local removal clears the active selection, exact empty selection is valid, and ASR diagnostics report an explicit unselected state.
- Generic native user install copies the validated runtime bundle, supports D-Bus activation, and completes exact native recognition round trips in a temporary HOME.
- The retained addon applies partial preedit, final commit, command candidate selection, selected-text deletion, and replacement through deterministic frontends and a live Fcitx client application.
- CLI, config, registry, daemon, recording, diagnostics, and frontend configuration surfaces are broadly implemented.

The core **real desktop native-dictation alpha** path is live-proven through Fcitx trigger -> a preflight-verified isolated PipeWire virtual source -> native ASR -> partial/input-panel updates -> commit. The same installed profile also proves F9 dictation through the default physical ALSA Digital Microphone with streaming partials and a final commit, GTK3/GTK4/Qt6/Chromium application commits plus explicit Chromium and VS Code/Electron renderer-sandbox evidence, three-cycle GTK4 repetition plus ten-cycle normal/command bounded soak in one window and one daemon owner, and GNOME Text Editor/VS Code saved-file plus kitty terminal-output proof, local adapter-backed surrounding-text replacement, and F10 replacement through an independent loopback OpenAI-compatible HTTP process. The HTTP gate first proves failure preservation against a real loopback HTTP 404: the operation error is surfaced, no candidate is committed, no surrounding text is deleted, the selected buffer is unchanged, and the daemon returns idle. It then verifies recovery with Bearer authentication without recording the token, one valid `/v1/chat/completions` request containing real selected text and raw ASR text, three Fcitx candidates, selected-text deletion, exact HTTP-candidate commit, distinct daemon ownership generations, and restoration of the local adapter/profile/backup/service/backend. It is external-process HTTP application-error/recovery proof, not third-party cloud-service proof. Zero-delete Wayland primary-selection fallback, scene/ASR menu display/filtering, scene selection, configured-key scene and ASR paging with exact restoration, installed user-catalog zh_CN Scene/ASR titles and status text with English restoration, F8/Enter model selection from streaming Zipformer to offline Paraformer, and F8 switching from the internal sherpa runtime to an external legacy-command process followed by final-only recognition and restoration to streaming recognition are also live-proven. The external ASR process bridge converts legacy raw PCM to a temporary WAV, removes the WAV, and records the child boundary. Its compatibility gate launches a separate one-shot daemon that reuses the original sherpa/Zipformer model, while `scripts/live/niri/run-ime-fcitx-whisper-provider-live.sh` selects an independent whisper.cpp v1.9.1 process and multilingual `ggml-base.bin`, commits its distinct final text, records fixed source/binary/model hashes, then restores Zipformer streaming partials. Persisted Tap/Hold/Both timing, the official Fcitx configuration form in English and zh_CN including localized trigger-mode choices, information and daemon-originated error notifications, zh_CN scene-information and ASR-switch text plus information/error summaries with verbatim technical error bodies, old-backend recovery, and English/original-locale restoration, two-context focus handoff, verified daemon-owner loss, and same-provider reload are also live-proven. The remote-provider failure gate selects an unsupported endpoint scheme through F8/Enter, proves that the original Zipformer remains effective, verifies exact daemon/Fcitx error notification senders and payloads, restores profile/backup state, and produces streaming partials plus a final commit afterward. The companion success gate selects a real `type=remote` backend, sends multipart WAV/Bearer/model/language/prompt to an independent loopback OpenAI-compatible process, commits its final text, retains redacted evidence, and restores Zipformer streaming. The physical, text-provider, and ASR cross-provider gates preserve user state. The active target is now additional physical-device switching breadth, hour-scale soak, real hosted-ASR DNS/TLS/proxy/rate-limit and credential behavior, real cloud text-provider behavior, and broader cross-application behavior. English fallback plus zh_CN matches the legacy product locale set; further UI locales are optional expansion.

## Readiness summary

| Target | Current state |
| --- | --- |
| Deterministic command-demo product spine | Complete and CI-usable |
| CLI and daemon management | Usable alpha; broad command coverage |
| Current registry native ASR families | Real-WAV proven for supported offline/online layouts |
| Generic native user install | Activation, runtime bundle, partial preedit, final commit, and command replacement deterministically proven |
| Real desktop normal dictation | Live-proven through a real Fcitx client with both an isolated PipeWire virtual source and the default physical ALSA Digital Microphone; both paths require streaming partials and a non-empty final commit, and GTK4 additionally passes a ten-cycle same-window/same-daemon bounded soak |
| Real desktop command dictation | Local adapter-backed surrounding-text deletion/replacement, loopback OpenAI-compatible HTTP-provider replacement, and zero-delete Wayland primary-selection fallback are live-proven; GTK3, GTK4, Qt6, Chromium, GNOME Text Editor, kitty, and VS Code/Electron command paths also pass; Chromium and VS Code command modes are PRIMARY-fallback proof because their application selection is not exposed as Fcitx surrounding text in these gates; GTK4 additionally passes a ten-cycle same-window/same-daemon replacement soak | Broader cross-application and real cloud-provider proof remain |
| Frontend menus/configuration | Scene and ASR candidate display/filtering, F7 scene selection, F8 ASR model selection/reload, configured-key scene/ASR paging, persisted Tap/Hold/Both timing, installed-catalog zh_CN Scene/ASR titles/status, localized scene-information and ASR-switch text plus information/error summaries, and the official Fcitx configuration form with English/zh_CN labels and localized trigger-mode choices pass with zero unintended commits, old-backend recovery, and exact English/original-locale restoration | Legacy locale parity complete; extra UI locales are optional expansion |
| Adapter resource lifecycle | Implemented for current script registry with localized title/description display, update-by-reinstall, installed short-id start/stop/status resolution, short-id removal, guarded managed-script cleanup, and config backup |
| ASR provider resource lifecycle | Implemented for current script registry with localized title/description display, update-by-reinstall, guarded removal, and command-provider script editing |
| Remote text service | Protocol/config core, browser assets, standalone and normal-daemon HTTP/WebSocket ownership, provider-selection/config-reload reconciliation, bind-failure cleanup, `SIGTERM` shutdown, redacted LAN endpoint diagnostics, real local-socket tests, private-session process smoke, a real sandboxed Chromium same-host LAN path, and a fail-closed external-device challenge collector with explicit physical-device confirmation implemented; a successful run from another physical device remains |
| Distro packaging and upgrades | Partial: checked release-time native-runtime bundle selection, the Arch `x86_64` package, isolated install/upgrade/pkgrel rollback/removal transactions, signed release-gate inventory, candidate promotion, current/old-metadata handoff, private-session direct replacement, real user-systemd restart/restore, automatic ownership-verified cross-user upgrade dispatch, guarded removal preparation/busy rollback, and unsupported-future-schema refusal with byte-identical user config are complete. The external-user Arch lifecycle guide and isolated command smoke are complete. Production publication, an actual host package-installed upgrade, live production multi-user upgrade/removal, production key operations and regression on an unrelated external machine remain. Real schema-migration rollback becomes applicable when schema 2 exists. See [`../architecture/packaging-contract.md`](../architecture/packaging-contract.md). |
| Standalone management GUI | Deferred; any future GUI must be implemented in Rust, with iced 0.14 as the provisional first spike. See [`../architecture/gui-contract.md`](../architecture/gui-contract.md). |

## Capability inventory

| Area | Rust state | Remaining gap |
| --- | --- | --- |
| D-Bus compatibility | Legacy names, methods, signals, status strings, and payloads preserved; diagnostic extensions added | Real-session compatibility hardening |
| Runtime lifecycle | Normal/command flow, capture-first startup, partial polling, inferring/postprocessing stop, reload deferral, adapter supervision, plus live focus handoff, owner loss, same-provider reload/model switching, internal-to-command-provider roundtrips, independent Whisper selection/restoration, remote prepare-failure preservation, and successful remote HTTP selection/restoration | Broad application and hosted-service operational behavior |
| Native ASR | Offline transducer, Dolphin, SenseVoice, Paraformer, Qwen3 ASR, Moonshine v1, online transducer, and Zipformer2 CTC mapped and WAV-proven | Other legacy families only when concrete demand exists |
| Command and remote ASR | Batch/streaming command protocols, raw-PCM-to-WAV bridging, independent Whisper, OpenAI-compatible multipart WAV/Bearer/model/language/prompt transport, enforced HTTP deadlines, final-only remote recognition, prepare-failure preservation, and real F8 success/failure roundtrips | Real hosted-ASR DNS/TLS/proxy/rate-limit/outage behavior and credential rotation/custody |
| Audio | Typed PCM, processing, mock/file sources, reusable PipeWire streams, serialized recording transactions, diagnostics, live typed same-daemon and same-recorder target switching across two isolated PipeWire sources, live real-`wpctl` duck/restore against an isolated virtual sink, live non-silent capture through an isolated virtual source, and default physical ALSA microphone recognition | Additional physical-device switching breadth, audible hardware-output ducking, and broader device proof |
| Offline VAD | Silero model, legacy-compatible settings, fallback, cold-start guard, install, and diagnostics | Real microphone validation |
| Text processing | Command adapters, OpenAI-compatible transport, prompts, context cache, scenes, candidates, and live loopback HTTP-provider selected-text replacement | Real third-party cloud credentials, rate limits, timeouts, and disconnect recovery |
| Registry | Live model lifecycle plus current provider/adapter registry list/install/update-by-reinstall, legacy locale detection/normalization, `en_US`/requested/local-override display metadata layering, guarded config materialization, executable script publication, and managed adapter removal | No current script-registry lifecycle gap |
| CLI | Init, config, model, provider, hotword, device, scene, LLM, adapter, daemon, recording, and doctor; provider removal preserves local entries and supports active-clear semantics; command-provider scripts can be opened through resolved installed selectors | UX polish and continued feature-driven module extraction |
| Fcitx frontend | Persistent keys, live Tap/Hold/Both timing, menus, filtering, installed-catalog zh_CN Scene/ASR and scene-info/ASR-switch/error-summary notification localization, official English/zh_CN configuration-form labels and trigger-mode choices, notifications, owner recovery, plus live normal, surrounding-command, primary-fallback, toolkit, scene/ASR display/filter, scene/ASR selection/paging, F8 same/cross-provider selection/reload and failure preservation, information/error notifications, focus, and owner-loss outcomes | Legacy English/zh_CN locale parity complete; broader multi-application proof remains |
| User install | Temporary-HOME profiles, the tracked external-user Arch lifecycle guide plus isolated command smoke, direct/systemd activation, checked release-time native-runtime bundle selection, checked package construction, current/old-metadata handoff, private-session direct replacement, real user-systemd restart/restore, automatic ownership-verified cross-user upgrade dispatch, guarded removal handoff/rollback, and future-schema refusal/preservation through package install/upgrade/pkgrel rollback/removal | Actual host package-installed upgrade, live production multi-user upgrade/removal, production repository/key policy, and regression on an unrelated external machine; migration rollback waits for schema 2 |
| Diagnostics | Doctor, runtime status, ASR state, audio devices, owner/PID/procfs, live probe | Live error-message refinement |
| Tests | Workspace, session-bus, C++ addon, staged activation, temporary-HOME/native model smokes, and opt-in real Fcitx/toolkit/default-physical-microphone gates with exact restoration | Additional device/application breadth and external-user matrices remain manual/opt-in |

## Highest-risk gaps

1. **Application breadth:** GTK3, GTK4, Qt6, Chromium, GNOME Text Editor, kitty, and VS Code/Electron normal/command paths are live-proven, and Chromium plus VS Code renderers are sandbox-attested; additional terminals, sandbox packaging formats/applications, and hour-scale or longer soak behavior remain; the ten-cycle (~91-92-second per mode) bounded GTK4 soak is complete.
2. **Command-mode provider behavior:** surrounding-text replacement is live-proven with both a local adapter and a loopback OpenAI-compatible HTTP provider, primary-selection fallback is live-proven locally, a loopback HTTP 404 preserves selected text with no delete/commit before successful recovery, and the double-empty boundary (no surrounding selection and no primary selection) is live-proven to reject before recording or provider access. Real hosted DNS/TLS/proxy/rate-limit/outage and credential behavior plus cross-application cloud-provider behavior still need live proof.
3. **Release boundary:** checked release-time native-runtime bundle selection, the Arch candidate, current/old-metadata handoff, private-session direct replacement, real user-systemd restart/restore, automatic ownership-verified cross-user upgrade dispatch, guarded package-removal preparation/rollback, and unsupported-future-schema refusal with byte-preserving package transactions are complete. The external-user lifecycle guide and isolated command smoke are complete. An actual host package-installed upgrade, live production multi-user upgrade/removal, production publication/key operations, and installation/regression on an unrelated external machine remain unproven; migration rollback becomes a concrete requirement only when schema 2 exists. Detailed evidence belongs in [`../architecture/packaging-contract.md`](../architecture/packaging-contract.md).
4. **Remote parity:** settings, authentication, ownership, debounce, Realtime-compatible event semantics, HTTP/WebSocket serving, D-Bus daemon reconciliation, shutdown, redacted endpoint diagnostics, and a real sandboxed Chromium same-host LAN path are proven. The external-device collector requires explicit physical-device confirmation, rejects local peers, and cleans up on timeout; a successful challenge from another physical device remains.
5. **Maintainability:** the CLI router, command domains, daemon-control domains, config schema/validation, Sherpa layout/backend, and retained Fcitx menu/core boundaries are split. `scripts/tests/source-layout-check.sh` prevents production Rust/C++ files from regrowing beyond 1200 lines; future extraction should remain feature-driven rather than mechanical.

## Rust improvements beyond legacy

- Deterministic file-input and temporary-HOME product paths.
- Explicit crate boundaries and typed compatibility contracts.
- Safer registry download, checksum, extraction, and atomic publication.
- Better redacted diagnostics and owner/runtime visibility.
- Generation-scoped partial delivery and prepare-before-swap reload behavior.
- Native runtime bundle validation before desktop restart.

## Completion gate

Do not claim full parity until the documented user path works without manual JSON editing:

```sh
vinput init
vinput model list
vinput model install <id-or-short-id>
vinput model use <id-or-short-id>
vinput doctor
vinput daemon status
vinput recording start
vinput recording stop
```

The same installation must then pass live normal dictation, live command replacement, restart/reload behavior, and uninstall/upgrade checks in a real desktop session.

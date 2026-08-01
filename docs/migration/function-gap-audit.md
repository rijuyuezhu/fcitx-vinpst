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

The core **real desktop native-dictation alpha** path is live-proven through Fcitx trigger -> a preflight-verified isolated PipeWire virtual source -> native ASR -> partial/input-panel updates -> commit. The same installed profile also proves F9 dictation through the default physical ALSA Digital Microphone with streaming partials and a final commit, GTK3/GTK4/Qt6/Chromium application commits plus GNOME Text Editor saved-file normal/command proof, local adapter-backed surrounding-text replacement, and F10 replacement through an independent loopback OpenAI-compatible HTTP process. The HTTP gate first proves failure preservation against a real loopback HTTP 404: the operation error is surfaced, no candidate is committed, no surrounding text is deleted, the selected buffer is unchanged, and the daemon returns idle. It then verifies recovery with Bearer authentication without recording the token, one valid `/v1/chat/completions` request containing real selected text and raw ASR text, three Fcitx candidates, selected-text deletion, exact HTTP-candidate commit, distinct daemon ownership generations, and restoration of the local adapter/profile/backup/service/backend. It is external-process HTTP application-error/recovery proof, not third-party cloud-service proof. Zero-delete Wayland primary-selection fallback, scene/ASR menu display/filtering, scene selection, configured-key scene and ASR paging with exact restoration, installed user-catalog zh_CN Scene/ASR titles and status text with English restoration, F8/Enter model selection from streaming Zipformer to offline Paraformer, and F8 switching from the internal sherpa runtime to an external legacy-command process followed by final-only recognition and restoration to streaming recognition are also live-proven. The external ASR process bridge converts legacy raw PCM to a temporary WAV, removes the WAV, and records the child boundary. Its compatibility gate launches a separate one-shot daemon that reuses the original sherpa/Zipformer model, while `ime-fcitx-whisper-provider-live` selects an independent whisper.cpp v1.9.1 process and multilingual `ggml-base.bin`, commits its distinct final text, records fixed source/binary/model hashes, then restores Zipformer streaming partials. Persisted Tap/Hold/Both timing, the official Fcitx configuration form in English and zh_CN including localized trigger-mode choices, information and daemon-originated error notifications, zh_CN scene-information and ASR-switch text plus information/error summaries with verbatim technical error bodies, old-backend recovery, and English/original-locale restoration, two-context focus handoff, verified daemon-owner loss, and same-provider reload are also live-proven. The remote-provider failure gate selects an unsupported endpoint scheme through F8/Enter, proves that the original Zipformer remains effective, verifies exact daemon/Fcitx error notification senders and payloads, restores profile/backup state, and produces streaming partials plus a final commit afterward. The companion success gate selects a real `type=remote` backend, sends multipart WAV/Bearer/model/language/prompt to an independent loopback OpenAI-compatible process, commits its final text, retains redacted evidence, and restores Zipformer streaming. The physical, text-provider, and ASR cross-provider gates preserve user state. The active target is now additional physical-device switching breadth, real hosted-ASR DNS/TLS/proxy/rate-limit and credential behavior, real cloud text-provider behavior, additional locales, and broader cross-application behavior.

## Readiness summary

| Target | Current state |
| --- | --- |
| Deterministic command-demo product spine | Complete and CI-usable |
| CLI and daemon management | Usable alpha; broad command coverage |
| Current registry native ASR families | Real-WAV proven for supported offline/online layouts |
| Generic native user install | Activation, runtime bundle, partial preedit, final commit, and command replacement deterministically proven |
| Real desktop normal dictation | Live-proven through a real Fcitx client with both an isolated PipeWire virtual source and the default physical ALSA Digital Microphone; both paths require streaming partials and a non-empty final commit |
| Real desktop command dictation | Local adapter-backed surrounding-text deletion/replacement, loopback OpenAI-compatible HTTP-provider replacement, and zero-delete Wayland primary-selection fallback are live-proven; GTK3, GTK4, Qt6, Chromium, and GNOME Text Editor command paths also pass | Broader cross-application and real cloud-provider proof remain |
| Frontend menus/configuration | Scene and ASR candidate display/filtering, F7 scene selection, F8 ASR model selection/reload, configured-key scene/ASR paging, persisted Tap/Hold/Both timing, installed-catalog zh_CN Scene/ASR titles/status, localized scene-information and ASR-switch text plus information/error summaries, and the official Fcitx configuration form with English/zh_CN labels and localized trigger-mode choices pass with zero unintended commits, old-backend recovery, and exact English/original-locale restoration | Additional locales remain |
| Adapter resource lifecycle | Implemented for current script registry with localized title/description display, update-by-reinstall, installed short-id start/stop/status resolution, short-id removal, guarded managed-script cleanup, and config backup |
| ASR provider resource lifecycle | Implemented for current script registry with localized title/description display, update-by-reinstall, guarded removal, and command-provider script editing |
| Remote text service | Protocol/config core, browser assets, standalone and normal-daemon HTTP/WebSocket ownership, provider-selection/config-reload reconciliation, bind-failure cleanup, `SIGTERM` shutdown, redacted LAN endpoint diagnostics, real local-socket tests, and private-session process smoke implemented; live cross-device browser proof missing |
| Distro packaging and upgrades | Partial: the checked Arch `x86_64` package, isolated transaction/signature tests, signed release-gate inventory, candidate promotion, current-metadata automatic handoff, guarded old-systemd reload/restart, guarded old-direct termination/reactivation, private-session direct replacement proof, real user-systemd restart/restore proof, and guarded removal preparation with busy-session rollback are complete. Production publication, an actual package-installed upgrade, live production multi-user removal, incompatible-state rollback, production key operations, and external-user regression remain. See [`../architecture/packaging-contract.md`](../architecture/packaging-contract.md). |
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
| Fcitx frontend | Persistent keys, live Tap/Hold/Both timing, menus, filtering, installed-catalog zh_CN Scene/ASR and scene-info/ASR-switch/error-summary notification localization, official English/zh_CN configuration-form labels and trigger-mode choices, notifications, owner recovery, plus live normal, surrounding-command, primary-fallback, toolkit, scene/ASR display/filter, scene/ASR selection/paging, F8 same/cross-provider selection/reload and failure preservation, information/error notifications, focus, and owner-loss outcomes | Additional locales and broader multi-application proof |
| User install | Temporary-HOME profiles, direct per-user activation, staged systemd-backed activation, environment wrapper, native runtime bundle, checked Arch package construction, current-metadata automatic handoff, guarded old-systemd/direct handoff, private-session direct replacement proof, real user-systemd restart/restore proof, and private-session package-removal handoff/rollback proof | Actual package-installed upgrade, live production multi-user removal, incompatible-state rollback, version-selection, production repository, and live external-user policy |
| Diagnostics | Doctor, runtime status, ASR state, audio devices, owner/PID/procfs, live probe | Live error-message refinement |
| Tests | Workspace, session-bus, C++ addon, staged activation, temporary-HOME/native model smokes, and opt-in real Fcitx/toolkit/default-physical-microphone gates with exact restoration | Additional device/application breadth and external-user matrices remain manual/opt-in |

## Highest-risk gaps

1. **Application breadth:** GTK3, GTK4, Qt6, Chromium, and GNOME Text Editor normal/command paths are live-proven, but terminals, sandboxed applications, and repeated long-session behavior remain.
2. **Command-mode provider behavior:** surrounding-text replacement is live-proven with both a local adapter and a loopback OpenAI-compatible HTTP provider, primary-selection fallback is live-proven locally, a loopback HTTP 404 preserves selected text with no delete/commit before successful recovery, and the double-empty boundary (no surrounding selection and no primary selection) is live-proven to reject before recording or provider access. Real hosted DNS/TLS/proxy/rate-limit/outage and credential behavior plus cross-application cloud-provider behavior still need live proof.
3. **Release boundary:** the checked Arch candidate, current-metadata automatic handoff, guarded old-systemd/direct owner handoff, private-session direct replacement proof, real user-systemd restart/restore proof, and guarded package-removal preparation/rollback proof are complete, but an actual package-installed upgrade, live production multi-user removal, production publication, incompatible-state rollback, production key operations, and live external-user installation remain unproven. Detailed evidence belongs in [`../architecture/packaging-contract.md`](../architecture/packaging-contract.md).
4. **Remote parity:** settings, authentication, ownership, debounce, Realtime-compatible event semantics, HTTP/WebSocket serving, D-Bus daemon reconciliation, shutdown, and redacted endpoint diagnostics are deterministic; live cross-device browser proof remains.
5. **Maintainability:** `vinput-cli/src/main.rs` remains oversized and should be split only along future feature work.

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

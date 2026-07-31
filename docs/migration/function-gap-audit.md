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

The core **real desktop native-dictation alpha** path is live-proven through Fcitx trigger -> a preflight-verified isolated PipeWire virtual source -> native ASR -> partial/input-panel updates -> commit. The same installed profile also proves F9 dictation through the default physical ALSA Digital Microphone with streaming partials and a final commit, GTK3/Qt6/Chromium application commits, local adapter-backed surrounding-text replacement, zero-delete Wayland primary-selection fallback with exact selection restoration, scene/ASR menu display/filtering, scene selection, configured-key scene and ASR paging with exact restoration, installed user-catalog zh_CN Scene/ASR titles and status text with English restoration, F8/Enter model selection from streaming Zipformer to offline Paraformer, and F8 switching from the internal sherpa runtime to an external legacy-command process followed by final-only recognition and restoration to streaming recognition. The external process bridge converts legacy raw PCM to a temporary WAV, launches a separate one-shot daemon, removes the WAV, and records the child boundary; its underlying recognizer still reuses the original sherpa/Zipformer model, so it is not third-party model proof. Persisted Tap/Hold/Both timing, information and daemon-originated error notifications, two-context focus handoff, verified daemon-owner loss, and same-provider reload are also live-proven. The physical and cross-provider gates preserve profile/service/addon/backend state. The active target is now additional physical-device switching, a genuinely independent third-party ASR recognizer/model, remaining localization surfaces/locales, broader cross-application behavior, and one external text-processing provider.

## Readiness summary

| Target | Current state |
| --- | --- |
| Deterministic command-demo product spine | Complete and CI-usable |
| CLI and daemon management | Usable alpha; broad command coverage |
| Current registry native ASR families | Real-WAV proven for supported offline/online layouts |
| Generic native user install | Activation, runtime bundle, partial preedit, final commit, and command replacement deterministically proven |
| Real desktop normal dictation | Live-proven through a real Fcitx client with both an isolated PipeWire virtual source and the default physical ALSA Digital Microphone; both paths require streaming partials and a non-empty final commit |
| Real desktop command dictation | Local adapter-backed surrounding-text deletion/replacement and zero-delete Wayland primary-selection fallback are live-proven; GTK3, Qt6, and Chromium command paths also pass | Broader cross-application and external-provider proof remain |
| Frontend menus/configuration | Scene and ASR candidate display/filtering, F7 scene selection, F8 ASR model selection/reload, configured-key scene/ASR paging, persisted Tap/Hold/Both timing, and installed-catalog zh_CN Scene/ASR titles/status pass with zero unintended commits and exact restoration | Localized notification/configuration surfaces and additional locales remain |
| Adapter resource lifecycle | Implemented for current script registry with localized title/description display, update-by-reinstall, installed short-id start/stop/status resolution, short-id removal, guarded managed-script cleanup, and config backup |
| ASR provider resource lifecycle | Implemented for current script registry with localized title/description display, update-by-reinstall, guarded removal, and command-provider script editing |
| Remote text service | Protocol/config core, browser assets, standalone and normal-daemon HTTP/WebSocket ownership, provider-selection/config-reload reconciliation, bind-failure cleanup, `SIGTERM` shutdown, redacted LAN endpoint diagnostics, real local-socket tests, and private-session process smoke implemented; live cross-device browser proof missing |
| Distro packaging and upgrades | Partial: the checked Arch `x86_64` package, isolated transaction/signature tests, signed release-gate inventory, candidate promotion, owner diagnostics, and explicit systemd-user handoff are deterministic. Production publication, automatic package-manager handoff, incompatible-state rollback, production key operations, external-user regression, and live installed-desktop proof remain. See [`../architecture/packaging-contract.md`](../architecture/packaging-contract.md). |
| Legacy Qt GUI | Deferred |

## Capability inventory

| Area | Rust state | Remaining gap |
| --- | --- | --- |
| D-Bus compatibility | Legacy names, methods, signals, status strings, and payloads preserved; diagnostic extensions added | Real-session compatibility hardening |
| Runtime lifecycle | Normal/command flow, capture-first startup, partial polling, inferring/postprocessing stop, reload deferral, adapter supervision, plus live focus handoff, owner loss, same-provider reload/model switching, and internal-to-command-provider roundtrip behavior | Broad application and cross-provider failure/recovery behavior |
| Native ASR | Offline transducer, Dolphin, SenseVoice, Paraformer, Qwen3 ASR, Moonshine v1, online transducer, and Zipformer2 CTC mapped and WAV-proven | Other legacy families only when concrete demand exists |
| Command ASR | Batch and streaming command protocols, partials, cancellation, timeout enforcement, raw-PCM-to-WAV bridge, and live external-process final-only recognition | Independent third-party recognizer/model and live external-provider failure/recovery testing |
| Audio | Typed PCM, processing, mock/file sources, reusable PipeWire streams, target-change rebuild, serialized recording transactions, diagnostics, output ducking lifecycle, live non-silent capture through an isolated virtual source, and default physical ALSA microphone recognition | Additional device switching, audible output-ducking, and broader device proof |
| Offline VAD | Silero model, legacy-compatible settings, fallback, cold-start guard, install, and diagnostics | Real microphone validation |
| Text processing | Command adapters, OpenAI-compatible transport, prompts, context cache, scenes, candidates | One real desktop provider flow |
| Registry | Live model lifecycle plus current provider/adapter registry list/install/update-by-reinstall, legacy locale detection/normalization, `en_US`/requested/local-override display metadata layering, guarded config materialization, executable script publication, and managed adapter removal | No current script-registry lifecycle gap |
| CLI | Init, config, model, provider, hotword, device, scene, LLM, adapter, daemon, recording, and doctor; provider removal preserves local entries and supports active-clear semantics; command-provider scripts can be opened through resolved installed selectors | UX polish and continued feature-driven module extraction |
| Fcitx frontend | Persistent keys, live Tap/Hold/Both timing, menus, filtering, installed-catalog zh_CN Scene/ASR localization, notifications, owner recovery, plus live normal, surrounding-command, primary-fallback, toolkit, scene/ASR display/filter, scene/ASR selection/paging, F8 same/cross-provider selection/reload, information/error notifications, focus, and owner-loss outcomes | Localized notification/configuration surfaces, additional locales, cross-provider failure recovery, and broader multi-application proof |
| User install | Temporary-HOME profiles, direct per-user activation, staged systemd-backed system activation, environment wrapper, native runtime bundle, and checked Arch package construction | Upgrade, rollback/uninstall, version-selection, repository, and live external-user policy |
| Diagnostics | Doctor, runtime status, ASR state, audio devices, owner/PID/procfs, live probe | Live error-message refinement |
| Tests | Workspace, session-bus, C++ addon, staged activation, temporary-HOME/native model smokes, and opt-in real Fcitx/toolkit/default-physical-microphone gates with exact restoration | Additional device/application breadth and external-user matrices remain manual/opt-in |

## Highest-risk gaps

1. **Application breadth:** GTK3, Qt6, and Chromium normal/command paths are live-proven, but additional editors, terminals, sandboxed applications, and repeated long-session behavior remain.
2. **Command-mode provider behavior:** surrounding-text and primary-selection replacement are live-proven with a local adapter; one external provider and safe no-selection/error behavior still need live proof.
3. **Release boundary:** the checked Arch candidate is deterministic, but production publication, automatic package-manager handoff, incompatible-state rollback, production key operations, destructive stale-owner policy, and live external-user installation remain unproven. Detailed evidence belongs in [`../architecture/packaging-contract.md`](../architecture/packaging-contract.md).
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

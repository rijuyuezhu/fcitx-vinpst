# Function gap audit

Reviewed: 2026-07-29

This document is the current Rust-versus-legacy status baseline. It records implementation stage and evidence; it does not assign a release percentage.

## Review baseline

- Rust branch: `feat/accelerate-port-refactor`
- Implementation baseline: current branch, including provider/adapter registry installation.
- Legacy reference: `/workspace/fcitx5-vinput` at `6cdcac8`.

## Executive conclusion

The Rust rewrite is a **usable CLI/daemon alpha** with a strong deterministic product spine. It is not a full desktop replacement yet.

The project has crossed the main implementation threshold for local native ASR and retained-frontend integration:

- Current registry native ASR families are mapped and real-WAV tested, including offline and online paths.
- The current upstream provider and adapter registries are parsed directly; short ids, batch/streaming protocol validation, mirror-backed script install, executable publication, environment placeholders, config backups, timeout/env preservation, and guarded managed updates are deterministic-test proven.
- Provider removal now matches the legacy safety contract: local providers cannot be removed, active non-local removal clears the active selection, exact empty selection is valid, and ASR diagnostics report an explicit unselected state.
- Generic native user install copies the validated runtime bundle, supports D-Bus activation, and completes exact native recognition round trips in a temporary HOME.
- The retained addon deterministically applies partial preedit, final commit, command candidate selection, selected-text deletion, and replacement through concrete Fcitx test frontends.
- CLI, config, registry, daemon, recording, diagnostics, and frontend configuration surfaces are broadly implemented.

The active target is **real desktop native-dictation alpha**: prove Fcitx trigger -> live PipeWire capture -> native ASR -> partial/preedit -> commit in a real application, then prove command replacement across applications.

## Readiness summary

| Target | Current state |
| --- | --- |
| Deterministic command-demo product spine | Complete and CI-usable |
| CLI and daemon management | Usable alpha; broad command coverage |
| Current registry native ASR families | Real-WAV proven for supported offline/online layouts |
| Generic native user install | Activation, runtime bundle, partial preedit, final commit, and command replacement deterministically proven |
| Real desktop normal dictation | Not live-proven |
| Real desktop command dictation | Not live-proven across applications |
| Frontend menus/configuration | Implemented and deterministically tested; live UI proof missing |
| Adapter resource lifecycle | Implemented for current script registry with localized title/description display, installed short-id start/stop/status resolution, short-id removal, guarded managed-script cleanup, and config backup; update polish remains |
| ASR provider resource installation | Implemented for current script registry with localized title/description display; update polish remains |
| Remote text service | Missing |
| Distro packaging and upgrades | Missing |
| Legacy Qt GUI | Deferred |

## Capability inventory

| Area | Rust state | Remaining gap |
| --- | --- | --- |
| D-Bus compatibility | Legacy names, methods, signals, status strings, and payloads preserved; diagnostic extensions added | Real-session compatibility hardening |
| Runtime lifecycle | Normal/command flow, chunk callbacks, partial polling, reload deferral, non-blocking prepare-before-swap, adapter supervision | Live microphone and application behavior |
| Native ASR | Offline transducer, Dolphin, SenseVoice, Paraformer, Qwen3 ASR, Moonshine v1, online transducer, and Zipformer2 CTC mapped and WAV-proven | Other legacy families only when concrete demand exists |
| Command ASR | Batch and streaming command protocols, partials, cancellation, and timeout enforcement | Live external-provider recovery testing |
| Audio | Typed PCM, processing, mock/file sources, optional PipeWire recorder and diagnostics | Real PipeWire capture proof |
| Offline VAD | Silero model, legacy-compatible settings, fallback, cold-start guard, install, and diagnostics | Real microphone validation |
| Text processing | Command adapters, OpenAI-compatible transport, prompts, context cache, scenes, candidates | One real desktop provider flow |
| Registry | Live model lifecycle plus current provider/adapter registry list/install, localized display metadata, guarded config materialization, executable script publication, and managed adapter removal | Resource update polish |
| CLI | Init, config, model, provider, hotword, device, scene, LLM, adapter, daemon, recording, and doctor; provider removal preserves local entries and supports active-clear semantics | UX polish and continued feature-driven module extraction |
| Fcitx frontend | Persistent keys, Tap/Hold/Both, menus, filtering, i18n, notifications, owner recovery, partial preedit, outcome application | Real desktop rendering and multi-application proof |
| User install | Temporary-HOME profiles, activation services, environment wrapper, native runtime bundle | Packaging, upgrade, and version-selection policy |
| Diagnostics | Doctor, runtime status, ASR state, audio devices, owner/PID/procfs, live probe | Live error-message refinement |
| Tests | Workspace, session-bus, C++ addon, staged activation, temporary-HOME and native model smokes | Real desktop checks remain manual/opt-in |

## Highest-risk gaps

1. **Live desktop chain:** deterministic evidence stops before a real Fcitx process, live microphone, and real application rendering.
2. **Command-mode application behavior:** surrounding-text and primary-selection fallback need proof across applications and toolkits.
3. **Release boundary:** there is no distro packaging, upgrade policy, or external-user installation path yet.
4. **Remote parity:** legacy remote ASR/text services are not implemented.
5. **Resource lifecycle:** model, provider, and adapter installation plus script-registry i18n are available; provider removal matches legacy, adapter removal resolves short ids while deleting only verified in-place managed scripts, and adapter runtime commands validate installed full/short selectors before D-Bus. Provider/adapter update polish remains.
6. **Maintainability:** `vinput-cli/src/main.rs` remains oversized and should be split only along future feature work.

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

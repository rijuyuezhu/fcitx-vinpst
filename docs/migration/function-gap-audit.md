# Function gap audit

Reviewed: 2026-07-30

This document is the current Rust-versus-legacy status baseline. It records implementation stage and evidence; it does not assign a release percentage.

## Review baseline

- Rust branch: `feat/accelerate-port-refactor`
- Implementation baseline: current branch, including provider/adapter registry installation and the checked Arch package recipe.
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
| Adapter resource lifecycle | Implemented for current script registry with localized title/description display, update-by-reinstall, installed short-id start/stop/status resolution, short-id removal, guarded managed-script cleanup, and config backup |
| ASR provider resource lifecycle | Implemented for current script registry with localized title/description display, update-by-reinstall, guarded removal, and command-provider script editing |
| Remote text service | Protocol/config core, browser assets, standalone and normal-daemon HTTP/WebSocket ownership, provider-selection/config-reload reconciliation, bind-failure cleanup, `SIGTERM` shutdown, redacted LAN endpoint diagnostics, real local-socket tests, and private-session process smoke implemented; live cross-device browser proof missing |
| Distro packaging and upgrades | Partial: checked Arch Linux `x86_64` recipe, private sherpa/ONNX Runtime bundle, systemd/D-Bus activation, VAD/licenses, message-only lifecycle hooks, local install/upgrade/remove runbook, metadata gate, clean `makepkg`/extracted-runtime smoke, fakeroot pacman install/upgrade/same-version-rollback/uninstall transactions, local `repo-add` database and `file://` pacman install/upgrade, running-owner path/deleted-inode diagnostics, and explicit conditional systemd-user handoff with post-restart verification are implemented; incompatible-state rollback, automatic package-manager-triggered handoff inside user sessions, destructive direct-PID stale-owner cleanup, signing/trust, externally hosted repository publication, and live installed-desktop proof remain |
| Legacy Qt GUI | Deferred |

## Capability inventory

| Area | Rust state | Remaining gap |
| --- | --- | --- |
| D-Bus compatibility | Legacy names, methods, signals, status strings, and payloads preserved; diagnostic extensions added | Real-session compatibility hardening |
| Runtime lifecycle | Normal/command flow, unavailable-but-running configured ASR startup, capture-first startup with early-chunk gating, chunk callbacks, partial polling, two-stage inferring/postprocessing stop, reload deferral, non-blocking prepare-before-swap, adapter supervision | Live microphone and application behavior |
| Native ASR | Offline transducer, Dolphin, SenseVoice, Paraformer, Qwen3 ASR, Moonshine v1, online transducer, and Zipformer2 CTC mapped and WAV-proven | Other legacy families only when concrete demand exists |
| Command ASR | Batch and streaming command protocols, partials, cancellation, and timeout enforcement | Live external-provider recovery testing |
| Audio | Typed PCM, processing, mock/file sources, capture-before-session ordering, reusable inactive PipeWire streams, bounded generation-safe idle teardown, target-change rebuild and opt-out, serialized D-Bus recording transactions, structured cold-start/first-buffer diagnostics, and best-effort `wpctl` output ducking with restore-on-all-paths | Real capture and output-ducking proof |
| Offline VAD | Silero model, legacy-compatible settings, fallback, cold-start guard, install, and diagnostics | Real microphone validation |
| Text processing | Command adapters, OpenAI-compatible transport, prompts, context cache, scenes, candidates | One real desktop provider flow |
| Registry | Live model lifecycle plus current provider/adapter registry list/install/update-by-reinstall, legacy locale detection/normalization, `en_US`/requested/local-override display metadata layering, guarded config materialization, executable script publication, and managed adapter removal | No current script-registry lifecycle gap |
| CLI | Init, config, model, provider, hotword, device, scene, LLM, adapter, daemon, recording, and doctor; provider removal preserves local entries and supports active-clear semantics; command-provider scripts can be opened through resolved installed selectors | UX polish and continued feature-driven module extraction |
| Fcitx frontend | Persistent keys, Tap/Hold/Both, menus, filtering, i18n, notifications, owner recovery, partial preedit, outcome application | Real desktop rendering and multi-application proof |
| User install | Temporary-HOME profiles, direct per-user activation, staged systemd-backed system activation, environment wrapper, native runtime bundle, and checked Arch package construction | Upgrade, rollback/uninstall, version-selection, repository, and live external-user policy |
| Diagnostics | Doctor, runtime status, ASR state, audio devices, owner/PID/procfs, live probe | Live error-message refinement |
| Tests | Workspace, session-bus, C++ addon, staged activation, temporary-HOME and native model smokes | Real desktop checks remain manual/opt-in |

## Highest-risk gaps

1. **Live desktop chain:** deterministic evidence stops before a real Fcitx process, live microphone, and real application rendering.
2. **Command-mode application behavior:** surrounding-text and primary-selection fallback need proof across applications and toolkits.
3. **Release boundary:** the Arch `x86_64` package recipe, extracted runtime, isolated package transactions including same-version rollback, local repository install/upgrade, owner diagnostics, and explicit conditional systemd-user handoff are deterministic, but incompatible-state rollback, automatic package-manager-triggered handoff, destructive direct-PID stale-owner cleanup, signing/trust, externally hosted repository publication, and live external-user installation remain unproven.
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

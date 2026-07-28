# Function gap audit

Date: 2026-07-28

Tracked audit for Rust versus legacy feature parity. This file answers "where are we?". The detailed user-journey and native runtime/frontend backlog lives in [`e2e-capability-matrix.md`](e2e-capability-matrix.md).

## State snapshot

- Audited Rust HEAD: `fdd4a46 feat(asr): support qwen3 registry models`
- Audit branch: `feat/accelerate-port-refactor`, based on local `main` at `cab5e0d`.
- Worktree at audit time: clean before this documentation refresh.
- Remote `origin/main` at audit time: `73e1418 docs(migration): update cli gap wording`.

## Executive conclusion

The Rust rewrite is approximately **70-75%** of legacy user-visible feature parity. This is a planning estimate, not a release metric.

The Rust version now has a real product spine: retained C++ Fcitx5 addon, Rust daemon compatibility layer, deterministic command-demo path, command ASR and text adapter process runners, optional PipeWire recorder, user-level installation, broad model/provider/config/hotword/device/scene/LLM/adapter/daemon/recording CLI management, live model registry installation, `vinput doctor`, staged IME smokes, bus activation smokes, adapter lifecycle checks, and feature-gated native sherpa SenseVoice and Qwen3 ASR inference paths.

A real local ASR file-input path has been proven: live registry model `model.sherpa-onnx.sense-voice-zh-en-ja-ko-yue-int8` was downloaded, sha256-verified, extracted, and used with bundled `test_wavs/zh.wav`; `just sherpa-sense-voice-local-smoke` produced `开放时间早上九点至下午五点`.

It is now a **usable CLI/daemon alpha**, not beta and not a complete legacy replacement. The main blocker is no longer CLI coverage: it is proving and hardening the real desktop chain from Fcitx trigger through PipeWire capture and native ASR to application commit. Native SenseVoice, Qwen3 ASR, Moonshine v1, and online Zipformer2 CTC have proven local WAV smokes; online transducer metadata/runtime construction is implemented and contract-tested. Live D-Bus partial emission is implemented and session-bus tested. Offline Silero VAD is migrated, installed by the native user profile, and real-WAV tested without recognition regression. Online endpoint rule defaults/overrides and the legacy 200 ms warmup are implemented. Command timeouts are enforced; native synchronous decode explicitly reports configured timeouts as diagnostic-only instead of pretending cancellation. Configured startup and reload now prepare a warmup session before swap, preserve the old effective backend on failure, and retain busy-time deferral. The user-facing D-Bus reload now rebuilds the configured backend rather than refreshing metadata only, while backend-state diagnostics distinguish configured target from the actual effective backend. A single non-blocking reload worker now re-reads explicit daemon config files, prepares outside the runtime mutex, exposes physical progress, discards stale generations, and preserves the old backend on failure. The mock-to-Moonshine transition is proven with a real native model and D-Bus WAV recognition through `just sherpa-moonshine-dbus-reload-smoke`. A minimal Right-Shift scene menu and an F8 provider-level ASR menu are implemented: the C++ addon reads typed state, supports keyboard/paging/digit/mouse candidate selection, persists selections through Rust, and the ASR path queues the existing background prepare-before-swap reload. The ASR provider switch is proven through C++ -> sd-bus -> Rust with explicit-config persistence and subsequent recognition. Remaining sherpa families, installed-model-level ASR selection, persistent trigger-mode configuration, packaging, and remote services remain incomplete.

| Target | Readiness |
| --- | --- |
| Deterministic command-demo product spine | Usable for development and CI |
| Native SenseVoice file-input smoke | Proven with one registry model |
| Native Qwen3 ASR file-input smoke | Proven with the live registry model and bundled Spanish WAV |
| Real desktop trial | Prototype usable / early alpha |
| Legacy CLI experience | Mostly implemented; `init`, UX polish, and some resource install paths remain |
| Legacy daemon runtime coverage | Partial |
| Legacy feature parity | Usable alpha, incomplete parity |
| Distribution/release readiness | Not ready |

## Capability inventory

| Area | Legacy C++ capability | Rust current capability | Status |
| --- | --- | --- | --- |
| Fcitx addon | Full addon with metadata, event watchers, menus, config UI, context cache, and notifications. | Retained addon handles key events, surrounding text, daemon calls, preedit/commit, selected-text deletion, result candidates, and a typed scene-selection menu. | Mostly done, needs live verification |
| Hotkeys | Fcitx config for normal, command, scene menu, ASR menu, paging, tap/hold/both, debounce. | Normal, command, scene-menu, and ASR-menu triggers have legacy-compatible defaults plus environment overrides; persistent Fcitx config and tap/hold modes remain. | Partial |
| Normal dictation | Trigger, PipeWire capture, ASR, scene postprocess, commit. | Deterministic mock/command path works; native SenseVoice works for WAV file input; live capture/commit is not proven. | Partial |
| Command dictation | Selected text plus clipboard fallback, command scene postprocess, replace selection. | Surrounding text, primary-selection clipboard fallback, command path, and replacement logic exist. | Mostly done, needs live proof across applications |
| Selected text | Surrounding text and clipboard fallback. | Surrounding text and primary-selection clipboard fallback are implemented in the retained addon. | Mostly done, needs live proof |
| Preedit/commit | Recording preedit, clear, commit, partial/final result handling. | Retained addon and fake sink cover preedit, clear, commit, candidate fallback. | Mostly done, needs live proof |
| Candidate UI | Result, scene, ASR, and paging menus. | Result candidates, a minimal scene menu, and a provider-level ASR menu support cursor, paging, digit, enter, escape, and mouse selection. Installed-model rows and legacy search remain. | Partial |
| Bus service contract | Legacy service names, object path, interface, methods, signals, status strings. | Legacy contract is preserved and diagnostic extensions exist. | Mostly done |
| Runtime lifecycle | Async workers, capture callbacks, status transitions, reload, adapter lifecycle, remote service. | Normal/command lifecycle, reload deferral, adapter supervisor, file input, activation. | Partial |
| ASR mock/demo | Command backends, but no first-class deterministic product spine. | Strong mock and deterministic command-demo path. | Rust improved |
| Real ASR | Real `sherpa-onnx` offline/streaming, command batch, command streaming, remote providers. | Command batch/streaming implemented; native offline SenseVoice and Qwen3 ASR are feature-gated and WAV-verified. | Partial |
| File audio | Not a first-class daemon path. | `--wav` and `--pcm16le` deterministic inputs. | Rust improved |
| Live PipeWire | PipeWire capture with fixed 16 kHz mono format and target object support. | Feature-gated PipeWire recorder and diagnostics exist. | Partial, needs live verification |
| Text postprocess | OpenAI-compatible HTTP, prompt files/placeholders/context, candidates, command fallback. | Command adapter and OpenAI-compatible paths plus LLM/adapter/scene CLI management exist; real-provider desktop validation remains limited. | Mostly done with risks |
| Config | JSON core config plus Fcitx addon config and CLI mutation. | Typed JSON validation, pointer get/set/edit, and resource-specific model/provider/device/scene/LLM/adapter mutations exist; frontend config parity remains incomplete. | Mostly done |
| CLI | Full init/config/model/provider/hotword/device/scene/LLM/adapter/daemon/recording management. | All major management groups except a complete `init` workflow are implemented; daemon/recording control has begun moving out of the monolithic `main.rs`. | Mostly done |
| Registry/resources | GUI/CLI model/provider/adapter install and cache/fetch flows. | Live model fetch/cache/checksum/extract/install/use/remove is implemented; provider/adapter live installation and GUI resource flows remain incomplete. | Mostly done for models, partial overall |
| User install | user service, activation, addon metadata, distro packaging. | User install script installs daemon/addon/metadata/activation/env and has command-demo/PipeWire/native sherpa profiles. | Mostly done locally |
| Diagnostics | CLI, GUI, notifications, logs. | `vinput doctor`, runtime/audio/asr/text diagnostics, user addon/activation status. | Rust improved |
| CI/tests | Thin relative to feature surface. | Strong deterministic workspace/addon/staged/user install coverage. | Rust improved |
| Distribution | Multiple packaging paths. | Local/staged install spine only. | Missing/partial |

## Highest-risk gaps

1. Live desktop validation has not proven the complete native path: Fcitx trigger, PipeWire capture, native inference, postprocess, and commit in a real application.
2. Native online decoding and live D-Bus partial emission are implemented and session-bus tested, but the complete Fcitx/PipeWire/partial-preedit path remains unproven on a real desktop.
3. Prepare-before-swap warm reload, config-file re-read, busy-time deferral, physical `reload_in_progress`, generation coalescing, and non-blocking backend preparation are implemented and session-bus tested.
4. SenseVoice, Qwen3 ASR, Moonshine v1, transducer, and Zipformer2 CTC mappings are implemented; Dolphin, Paraformer, and other registry-compatible families still need runtime support. Moonshine Tiny int8 is live-registry installed and real-WAV proven.
5. Native shared-library resolution is improved for local smoke but still needs a robust activation, desktop-install, and distribution story.
6. Fcitx UX parity is incomplete: the minimal scene menu and provider-level ASR menu are C++/D-Bus tested but not live-desktop proven; installed-model ASR rows, frontend config UI, tap/hold/both behavior, legacy search, and rich notifications remain missing or partial.
7. Remote ASR/text services, full provider/adapter registry installation, distro packaging, and the legacy GUI remain incomplete.
8. `vinput-cli/src/main.rs` remains large even after daemon and recording control were extracted; further feature-driven module splits are needed.

## Rust improvements beyond legacy

1. Deterministic product spine for CI and local development.
2. Clear crate boundaries and smaller testable seams.
3. Better diagnostics and safer error reporting.
4. User-level install profiles for command-demo, command WAV ASR, configured PipeWire experiments, and native sherpa SenseVoice experiments.
5. File input for deterministic ASR/text pipeline tests.
6. Safer registry primitives for checksum, staging, archive extraction, and atomic materialization.
7. Real native SenseVoice smoke that can validate model loading and one WAV recognition before desktop debugging.
8. Typed live-registry family classification plus a real native Qwen3 ASR registry install and WAV smoke.
9. Primary-selection clipboard fallback for command mode in applications without usable surrounding text.
10. Delivery-mode-aware recorder callbacks that stream 800-frame PCM batches, preserve metadata, propagate callback errors, and avoid stop-time replay.
11. Generation-scoped live `RecognitionPartial` emission with deduplication, stop-time cancellation, and a real session-bus partial-before-stop regression test.
12. VAD-aware `vinput doctor` output that reports model source/readiness and actionable repair guidance without making optional trimming fatal.

## Current priority

The next target is **real desktop native-dictation alpha**. First prove the complete native path, including streaming partial preedit, in a real Fcitx session. Then port the remaining live-registry model families and complete the frontend configuration surfaces needed for live use. Frontend menus/configuration and packaging should advance in parallel where they directly support that path.

Do not claim full parity until the documented happy path works through a real desktop session and no longer depends on implementation-only profiles:

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

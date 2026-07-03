# Function gap audit

Date: 2026-07-03

Tracked audit for Rust versus legacy feature parity. This file answers "where are we?". The detailed user-journey and CLI/daemon backlog lives in [`e2e-capability-matrix.md`](e2e-capability-matrix.md).

## State snapshot

- Audited Rust HEAD: `ced48b6 fix(asr): prefer local sherpa runtime libs`
- Branch state at audit time: `main...origin/main [ahead 9, behind 1]`
- Worktree at audit time: clean after the last commit.
- Latest remote CI at audit time: previous remote `docs-helper` success; local commits were not pushed.

## Executive conclusion

The Rust rewrite is approximately **55-65%** of legacy user-visible feature parity.

The Rust version now has a real product spine: retained C++ Fcitx5 addon, Rust daemon compatibility layer, deterministic command-demo path, command ASR and text adapter process runners, optional PipeWire recorder, user-level installation, `vinput doctor`, staged IME smokes, bus activation smokes, adapter lifecycle checks, and a feature-gated native sherpa SenseVoice path.

A real local ASR file-input path has been proven: live registry model `model.sherpa-onnx.sense-voice-zh-en-ja-ko-yue-int8` was downloaded, sha256-verified, extracted, and used with bundled `test_wavs/zh.wav`; `just sherpa-sense-voice-local-smoke` produced `开放时间早上九点至下午五点`.

It is still **early alpha / prototype usable**, not beta and not near full legacy parity. The blocker is no longer "no real ASR at all"; the blocker is that normal users still cannot complete the legacy workflow through CLI/daemon commands. The Rust CLI lacks model/provider/config/scene/daemon/recording management, the registry CLI does not yet install from live `models.json/providers.json/adapters.json`, native sherpa is only proven for a SenseVoice offline file-input path, and real desktop Fcitx capture/commit with native ASR remains unproven.

| Target | Readiness |
| --- | --- |
| Deterministic command-demo product spine | Usable for development and CI |
| Native SenseVoice file-input smoke | Proven with one registry model |
| Real desktop trial | Prototype usable / early alpha |
| Legacy CLI experience | Not ready |
| Legacy daemon runtime coverage | Partial |
| Legacy feature parity | Alpha, not near parity |
| Distribution/release readiness | Not ready |

## Capability inventory

| Area | Legacy C++ capability | Rust current capability | Status |
| --- | --- | --- | --- |
| Fcitx addon | Full addon with metadata, event watchers, menus, config UI, context cache, and notifications. | Retained addon can handle key events, request surrounding text, call the daemon, set preedit, commit text, delete selected text, and show result candidates. | Mostly done, needs live verification |
| Hotkeys | Fcitx config for normal, command, scene menu, ASR menu, paging, tap/hold/both, debounce. | Environment-configured normal/command triggers only. | Partial |
| Normal dictation | Trigger, PipeWire capture, ASR, scene postprocess, commit. | Deterministic mock/command path works; native SenseVoice works for WAV file input; live capture/commit is not proven. | Partial |
| Command dictation | Selected text plus clipboard fallback, command scene postprocess, replace selection. | Selected text via surrounding text, command path, and replacement logic exist. | Mostly done, missing fallback/live proof |
| Selected text | Surrounding text and clipboard fallback. | Surrounding text only. | Partial |
| Preedit/commit | Recording preedit, clear, commit, partial/final result handling. | Retained addon and fake sink cover preedit, clear, commit, candidate fallback. | Mostly done, needs live proof |
| Candidate UI | Result, scene, ASR, and paging menus. | Result candidates only. | Partial |
| Bus service contract | Legacy service names, object path, interface, methods, signals, status strings. | Legacy contract is preserved and diagnostic extensions exist. | Mostly done |
| Runtime lifecycle | Async workers, capture callbacks, status transitions, reload, adapter lifecycle, remote service. | Normal/command lifecycle, reload deferral, adapter supervisor, file input, activation. | Partial |
| ASR mock/demo | Command backends, but no first-class deterministic product spine. | Strong mock and deterministic command-demo path. | Rust improved |
| Real ASR | Real `sherpa-onnx` offline/streaming, command batch, command streaming, remote providers. | Command batch/streaming implemented; native `sherpa-onnx` offline SenseVoice is feature-gated and verified with a WAV. | Partial |
| File audio | Not a first-class daemon path. | `--wav` and `--pcm16le` deterministic inputs. | Rust improved |
| Live PipeWire | PipeWire capture with fixed 16 kHz mono format and target object support. | Feature-gated PipeWire recorder and diagnostics exist. | Partial, needs live verification |
| Text postprocess | OpenAI-compatible HTTP, prompt files/placeholders/context, candidates, command fallback. | Command adapter and OpenAI-compatible paths exist; CLI/config UX still thin. | Mostly done with risks |
| Config | JSON core config plus Fcitx addon config and CLI mutation. | Typed JSON config and validation exist; general CLI mutation and frontend config parity are missing. | Partial |
| CLI | Full init/config/model/provider/hotword/device/scene/LLM/adapter/daemon/recording management. | Diagnostic/config-validation/registry-plan/activation/test helpers. | Major gap |
| Registry/resources | GUI/CLI model/provider/adapter install and cache/fetch flows. | Safe primitives and sample dry-run planner; no live registry install UX. | Partial |
| User install | user service, activation, addon metadata, distro packaging. | User install script installs daemon/addon/metadata/activation/env and has command-demo/PipeWire/native sherpa profiles. | Mostly done locally |
| Diagnostics | CLI, GUI, notifications, logs. | `vinput doctor`, runtime/audio/asr/text diagnostics, user addon/activation status. | Rust improved |
| CI/tests | Thin relative to feature surface. | Strong deterministic workspace/addon/staged/user install coverage. | Rust improved |
| Distribution | Multiple packaging paths. | Local/staged install spine only. | Missing/partial |

## Highest-risk gaps

1. CLI parity is the largest user-facing gap: no `init`, config mutation, model install/use/info/remove, provider/hotword/device/scene/LLM commands, daemon lifecycle, or recording control commands.
2. Live registry install is missing: Rust still has an `index.json` dry-run planner, while the real registry uses `registry/models.json`, `registry/providers.json`, `registry/adapters.json`, i18n files, and script resources.
3. Native sherpa runtime is partial: SenseVoice offline directory inference works, but registry `vinput_model` metadata mapping, Dolphin/Qwen/Qwen3 families, streaming, VAD, timeout enforcement, and warm reload parity are incomplete.
4. Live desktop validation has not proven native dictation commit in a real Fcitx session.
5. Native shared-library resolution is fragile: local smoke now prefers `target/debug`, but D-Bus activation/Desktop install still needs a robust runtime-library environment story.
6. Fcitx UX parity is incomplete: scene menu, ASR menu, frontend config UI, tap/hold/both behavior, paging/search menus, and rich notifications are missing or partial.
7. Command mode selected-text fallback is weaker than legacy: surrounding text is implemented, clipboard fallback is not.

## Rust improvements beyond legacy

1. Deterministic product spine for CI and local development.
2. Clear crate boundaries and smaller testable seams.
3. Better diagnostics and safer error reporting.
4. User-level install profiles for command-demo, command WAV ASR, configured PipeWire experiments, and native sherpa SenseVoice experiments.
5. File input for deterministic ASR/text pipeline tests.
6. Safer registry primitives for checksum, staging, archive extraction, and atomic materialization.
7. Real native SenseVoice smoke that can validate model loading and one WAV recognition before desktop debugging.

## Current priority

The next target is **usable CLI/daemon alpha**. Implement the P0 slices in [`e2e-capability-matrix.md`](e2e-capability-matrix.md): live registry parsing, model list/install/use/info/remove, config mutation, daemon/recording D-Bus CLI commands, and native sherpa activation runtime-library hardening.

Do not claim full parity until the documented happy path no longer requires manual JSON edits:

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

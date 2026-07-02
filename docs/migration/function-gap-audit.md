# Function gap audit

Date: 2026-07-03

Tracked audit for Rust versus legacy feature parity. This file is now the tracked source of truth; `docs/plan/` remains local scratch only.

## State snapshot

- Audited Rust HEAD: `49b6fa8 feat(ime): write fcitx user env file`
- Branch: `main`
- Worktree at audit time: clean, aligned with `origin/main`
- Latest CI at audit time: success for the audited HEAD.

## Executive conclusion

The Rust rewrite is approximately **60-65%** of legacy user-visible feature parity.

The Rust version has a real product spine: retained C++ Fcitx5 addon, Rust daemon compatibility layer, deterministic command-demo path, command ASR and text adapter process runners, optional PipeWire recorder, user-level installation, `vinput doctor`, staged IME smokes, bus activation smokes, and adapter lifecycle checks.

It is still **early alpha / prototype usable**, not beta and not near full legacy parity. The default real local ASR backend is not implemented, legacy Fcitx menu/config UX is missing, resource/model installation is not a complete user flow, and live desktop Fcitx validation has not passed in the audited environment.

| Target | Readiness |
| --- | --- |
| Deterministic command-demo product spine | Usable for development and CI |
| Real desktop trial | Prototype usable / early alpha |
| Legacy feature parity | Alpha, not near parity |
| Distribution/release readiness | Not ready |

## Capability inventory

| Area | Legacy C++ capability | Rust current capability | Status |
| --- | --- | --- | --- |
| Fcitx addon | Full addon with metadata, event watchers, menus, config UI, context cache, and notifications. | Retained addon can handle key events, request surrounding text, call the daemon, set preedit, commit text, delete selected text, and show result candidates. | Mostly done, needs live verification |
| Hotkeys | Fcitx config for normal, command, scene menu, ASR menu, paging, tap/hold/both, debounce. | Environment-configured normal/command triggers only. | Partial |
| Normal dictation | Trigger, PipeWire capture, ASR, scene postprocess, commit. | Deterministic mock/command path works; real default ASR is not implemented. | Partial |
| Command dictation | Selected text plus clipboard fallback, command scene postprocess, replace selection. | Selected text via surrounding text, command path, and replacement logic exist. | Mostly done, missing fallback/live proof |
| Selected text | Surrounding text and clipboard fallback. | Surrounding text only. | Partial |
| Preedit/commit | Recording preedit, clear, commit, partial/final result handling. | Retained addon and fake sink cover preedit, clear, commit, candidate fallback. | Mostly done, needs live proof |
| Candidate UI | Result, scene, ASR, and paging menus. | Result candidates only. | Partial |
| Bus service contract | Legacy service names, object path, interface, methods, signals, status strings. | Legacy contract is preserved and diagnostic extensions exist. | Mostly done |
| Runtime lifecycle | Async workers, capture callbacks, status transitions, reload, adapter lifecycle, remote service. | Normal/command lifecycle, reload deferral, adapter supervisor, file input, activation. | Partial |
| ASR mock/demo | Command backends, but no first-class deterministic product spine. | Strong mock and deterministic command-demo path. | Rust improved |
| Real ASR | Real `sherpa-onnx`, command batch, command streaming, remote text provider. | Command batch/streaming implemented; `sherpa-onnx` seam exists but runtime is unavailable. | Partial / stub |
| File audio | Not a first-class daemon path. | `--wav` and `--pcm16le` deterministic inputs. | Rust improved |
| Live PipeWire | PipeWire capture with fixed 16 kHz mono format and target object support. | Feature-gated PipeWire recorder and diagnostics exist. | Partial, needs live verification |
| Text postprocess | OpenAI-compatible HTTP, prompt files/placeholders/context, candidates, command fallback. | Command adapter and OpenAI-compatible paths exist; real provider validation still needed. | Mostly done with risks |
| Config | JSON core config plus Fcitx addon config. | Typed JSON config and validation exist; frontend config parity is missing. | Partial |
| Registry/resources | GUI/CLI model/provider/adapter install and cache/fetch flows. | Schema, planning, mirror fetch/cache, checksum, staging, archive extraction, materialization primitives. | Partial |
| User install | user service, activation, addon metadata, distro packaging. | User install script installs daemon/addon/metadata/activation/env and has command-demo/PipeWire profiles. | Mostly done locally |
| Diagnostics | CLI, GUI, notifications, logs. | `vinput doctor`, runtime/audio/asr/text diagnostics, user addon/activation status. | Rust improved |
| CI/tests | Thin relative to feature surface. | Strong deterministic workspace/addon/staged/user install coverage. | Rust improved |
| Distribution | Multiple packaging paths. | Local/staged install spine only. | Missing/partial |

## Highest-risk gaps

1. Real local ASR is not implemented. The default `sherpa-onnx` provider reports runtime unavailable.
2. Fcitx UX parity is incomplete: scene menu, ASR menu, frontend config UI, tap/hold/both behavior, paging/search menus, and rich notifications are missing or partial.
3. Live desktop validation failed in the audited environment: Fcitx5 was running, but the user-installed Rust addon/activation files were missing and the diagnostic runtime method was unavailable on the current service.
4. Registry/resource flow is not user-facing yet: planning and staging exist, but model install and config mutation are future work.
5. Text provider integration needs real validation: request building exists, but URL semantics, timeout/cancel, and multi-provider behavior need tests.
6. Command mode selected-text fallback is weaker than legacy: surrounding text is implemented, clipboard fallback is not.

## Rust improvements beyond legacy

1. Deterministic product spine for CI and local development.
2. Clear crate boundaries and smaller testable seams.
3. Better diagnostics and safer error reporting.
4. User-level install profiles for command-demo and configured PipeWire experiments.
5. File input for deterministic ASR/text pipeline tests.
6. Safer registry primitives for checksum, staging, archive extraction, and atomic materialization.

## Validation evidence from audit

| Command | Result | Notes |
| --- | --- | --- |
| `gh run list --repo rijuyuezhu/fcitx-vinput-rs --limit 12` | PASS | Latest CI success for audited HEAD. |
| `cargo test --workspace --all-targets` | PASS | Full workspace tests. |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS | Full workspace lint. |
| `just addon-format-check` | PASS | C++ retained addon format check. |
| `just addon-test` | PASS | C++ retained bridge/addon test suite. |
| `just ime-e2e-smoke` | PASS | Deterministic staged daemon/addon/config/WAV/activation smoke. |
| `just user-ime-command-demo-smoke` | PASS | User-profile install plus activation smoke with command-demo backends. |
| `VINPUT_USER_PROFILE=command-demo scripts/install-user-ime.sh` | PASS | Run under temporary `HOME` during audit. |
| `VINPUT_USER_STATUS=1 scripts/install-user-ime.sh` | PASS | Run under the same temporary `HOME`. |
| `just ime-fcitx-live-probe` | FAIL | Current Fcitx5 session existed, but Rust user addon/activation files were missing and live runtime diagnostics were unavailable. Do not count deterministic smokes as live desktop proof. |

## Not verified yet

- Real local model inference in Rust.
- Live PipeWire capture feeding a real ASR backend.
- Fcitx restart followed by loading the user-installed `fcitx5-vinput.so`.
- Real application selected-text deletion and candidate selection.
- Real text provider request/response against an external or local HTTP mock server.
- Registry asset download/install that updates config for a real model.

## Final judgement

The next milestone should be **real desktop alpha**, not full legacy parity. The acceptance bar is:

1. user-level install succeeds;
2. Fcitx5 is restarted with the generated environment;
3. `fcitx5-vinput.so` loads;
4. normal trigger starts/stops recording;
5. live PipeWire capture or deterministic command input feeds a real recognition path;
6. result commits into a real application;
7. command mode can replace selected text;
8. `vinput doctor` and live probe clearly diagnose failures.

Full legacy parity comes later, after real ASR, live desktop commit, frontend menus/config, and resource installation are usable.

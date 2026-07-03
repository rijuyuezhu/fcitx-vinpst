# Complete E2E replication plan

This is the active milestone plan for moving from the current deterministic product spine and one proven native SenseVoice file-input path to a user-usable replacement. Read this after [`function-gap-audit.md`](function-gap-audit.md) and use [`e2e-capability-matrix.md`](e2e-capability-matrix.md) for the detailed CLI/daemon backlog.

## Product target

Functional replication means user-visible compatibility, not line-by-line C++ replication:

- a user can initialize config and managed directories without knowing the JSON schema;
- a user can discover, install, select, and diagnose a registry model from CLI;
- Fcitx5 loads the addon and routes trigger keys to the Rust daemon;
- normal dictation captures audio, recognizes text, optionally postprocesses it, and commits into a real application;
- command dictation uses selected text, transforms it, replaces the selected text, and fails safely when selection is unavailable;
- daemon status, recording control, adapter control, and logs are available from user-facing CLI commands;
- service contracts, config semantics, install paths, diagnostics, and logs remain understandable and compatible.

## Milestones

| Milestone | Name | Exit criteria |
| --- | --- | --- |
| M0 | Repository health | CI green, clean worktree, audit and plan docs current. |
| M1 | Deterministic product spine | `just ime-e2e-smoke` and user-profile smokes pass. |
| M2 | Native file-input ASR proof | Native SenseVoice file-input smoke: `just sherpa-sense-voice-local-smoke` passes with a registry-downloaded model and real WAV. |
| M3 | Usable CLI/daemon alpha | `vinput init`, model list/install/use/info, config mutation, `doctor`, daemon status, and recording start/stop work without manual JSON edits. |
| M4 | Real desktop native alpha | User install, Fcitx restart, addon load, trigger, preedit, PipeWire capture, native ASR, commit, command replacement, and live diagnostics work in one real desktop session. |
| M5 | Legacy UX parity slice | Minimal scene menu, ASR menu, frontend trigger config, selected-text fallback, and text-provider validation are available. |
| M6 | Resource/install parity slice | Model/provider/adapter install can download, verify, materialize, update config, and run runtime validation. |
| M7 | Release candidate | Packaging, install docs, live validation checklist, and regression tests are ready for external users. |

## P0: usable CLI/daemon alpha

This is the next target. Details and acceptance criteria live in [`e2e-capability-matrix.md`](e2e-capability-matrix.md).

1. Add live registry parsing for `models.json/providers.json/adapters.json` with i18n and `short_id` support.
2. Add `vinput model list/install/use/info/remove` around the existing safe fetch/checksum/archive/staging/materialization primitives.
3. Add `vinput init` and `vinput config get/set/edit` so users do not hand-edit JSON.
4. Add `vinput provider`, `vinput hotword`, and `vinput device` commands for the most common ASR setup tasks.
5. Add daemon/recording D-Bus CLI commands for `vinput daemon status/start/stop/restart/log` and `vinput recording start/stop/toggle`.
6. Harden native sherpa runtime library handling for D-Bus activation, not just local smoke.

## P0: real desktop native alpha

1. Keep deterministic smokes green before each live change.
2. Install a native sherpa profile using a registry-installed model, not a hand-written config.
3. Prove Fcitx discovers `fcitx5-vinput.so` from `FCITX_ADDON_DIRS` and metadata from `XDG_DATA_HOME`.
4. Prove normal trigger, command trigger, preedit, commit, command replacement, candidate fallback, and error preedit in a real application.
5. Stabilize PipeWire diagnostics and keep deterministic smokes separate from live checks.

## P1: daemon runtime parity

1. Map registry/local `vinput_model` metadata into Rust native sherpa config instead of relying only on directory inference.
2. Add broader offline families such as Dolphin and Qwen/Qwen3 as supported or explicitly diagnosed unsupported paths.
3. Add sherpa streaming backend support and map partial events to `RecognitionPartial`.
4. Add VAD trimming and timeout semantics where legacy exposes them.
5. Validate OpenAI-compatible text provider behavior with a local mock server.
6. Preserve legacy status strings, method names, signal names, and recognition payload shape.

## P1: legacy UX parity

1. Add persistent frontend config for normal trigger, command trigger, and trigger mode; keep environment overrides as a development escape hatch.
2. Add minimal scene and ASR menus before recreating every legacy menu detail.
3. Verify command-mode candidate selection replaces selected text.
4. Add selected-text fallback beyond surrounding text so command mode works in more applications.
5. Add LLM/provider/adapter/scene CLI parity after the model/provider/daemon core is in place.

## P2: release readiness

1. Add distro packaging only after usable CLI/daemon alpha and real desktop native alpha pass.
2. Write a short install guide around user-level install first.
3. Keep release packaging separate from migration correctness commits.
4. Add upgrade/removal notes for daemon, addon, metadata, activation service, env file, and native runtime libraries.

## Work selection rules

- Prefer tasks that move M3 or M4 forward.
- Do not count deterministic smoke tests as full parity unless they prove the relevant real behavior.
- Avoid broad refactors unless they unblock a milestone.
- Preserve legacy service names, method names, status strings, config semantics, and recognition payload shape.
- Keep deterministic tests for every new live-facing path.
- Keep commits small and scoped.

## Suggested next slices

1. Add live `registry/models.json` parser and fixture tests.
2. Add `vinput model list --json` and text output.
3. Add model install dry-run and real install/materialize.
4. Add `vinput model use` and minimal config mutation primitives.
5. Add `vinput daemon status` and `vinput recording start/stop` D-Bus client commands.
6. Harden D-Bus activation library-path handling for native sherpa.

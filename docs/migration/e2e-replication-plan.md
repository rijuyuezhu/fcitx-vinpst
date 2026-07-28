# Complete E2E replication plan

This is the active milestone plan for moving from the current usable CLI/daemon alpha and proven native SenseVoice file-input path to a real desktop replacement. Read this after [`function-gap-audit.md`](function-gap-audit.md) and use [`e2e-capability-matrix.md`](e2e-capability-matrix.md) for the detailed runtime/frontend backlog.

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

## Completed: usable CLI/daemon alpha

The M3 management surface is implemented and covered by deterministic tests. Remaining live-registry work is provider/adapter installation breadth rather than the core model workflow.

1. Implemented: live model registry parsing with i18n and `short_id` support.
2. Implemented: `vinput model list/install/use/info/remove` around safe fetch/checksum/archive/staging/materialization primitives.
3. Implemented: `vinput init` and `vinput config get/set/edit` for normal configuration workflows.
4. Implemented: provider, hotword, device, scene, LLM, and adapter management commands.
5. Implemented: daemon and recording D-Bus/lifecycle CLI commands.
6. Implemented locally: native sherpa activation profiles and runtime-library diagnostics; distribution hardening remains under M4/M7.

## P0: real desktop native alpha

1. Keep deterministic smokes green before each live change.
2. Install a native sherpa profile using a registry-installed model, not a hand-written config.
3. Prove Fcitx discovers `fcitx5-vinput.so` from `FCITX_ADDON_DIRS` and metadata from `XDG_DATA_HOME`.
4. Prove normal trigger, command trigger, preedit, commit, command replacement, candidate fallback, and error preedit in a real application.
5. Stabilize PipeWire diagnostics and keep deterministic smokes separate from live checks.

## P1: daemon runtime parity

1. Implemented and WAV-proven for SenseVoice and Qwen3 ASR: map registry/local `vinput_model` metadata into native sherpa config; unknown and unsupported family names remain explicit.
2. Add transducer, Zipformer2 CTC, Moonshine, Dolphin, and Paraformer layouts in registry-priority order.
3. Add sherpa streaming backend support, deliver live PipeWire chunks, and map partial events to `RecognitionPartial`.
4. Add VAD trimming, warmup, decode timeout, and warm reload semantics where legacy exposes them.
5. Implemented deterministically: OpenAI-compatible text provider behavior uses local mock-server tests; add one real desktop provider validation.
6. Continue preserving legacy status strings, method names, signal names, and recognition payload shape.

## P1: legacy UX parity

1. Add persistent frontend config for normal trigger, command trigger, and trigger mode; keep environment overrides as a development escape hatch.
2. Add minimal scene and ASR menus before recreating every legacy menu detail.
3. Verify command-mode candidate selection replaces selected text in multiple real applications.
4. Validate the implemented primary-selection clipboard fallback where surrounding text is unavailable.
5. LLM/provider/adapter/scene CLI parity is implemented; validate live providers and improve user-facing errors.

## P2: release readiness

1. Add distro packaging only after usable CLI/daemon alpha and real desktop native alpha pass.
2. Write a short install guide around user-level install first.
3. Keep release packaging separate from migration correctness commits.
4. Add upgrade/removal notes for daemon, addon, metadata, activation service, env file, and native runtime libraries.

## Work selection rules

- Prefer tasks that move M4 or native runtime parity forward.
- Do not count deterministic smoke tests as full parity unless they prove the relevant real behavior.
- Avoid broad refactors unless they unblock a milestone.
- Preserve legacy service names, method names, status strings, config semantics, and recognition payload shape.
- Keep deterministic tests for every new live-facing path.
- Keep commits small and scoped.

## Suggested next slices

1. Prove real desktop SenseVoice dictation from Fcitx trigger through PipeWire capture to application commit.
2. Wire live PipeWire chunks into native sherpa streaming recognition.
3. Add native VAD, timeout, warmup, and warm reload behavior.
4. Port the remaining live-registry model families.
5. Add scene/ASR menus, persistent frontend config, packaging, and further feature-driven CLI module extraction.

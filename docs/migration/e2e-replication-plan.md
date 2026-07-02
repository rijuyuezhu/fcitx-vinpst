# Complete E2E replication plan

This is the active plan for moving from the current deterministic product spine to real desktop alpha and then legacy feature parity. Read this after [`function-gap-audit.md`](function-gap-audit.md).

## Product target

Functional replication means user-visible compatibility, not line-by-line C++ replication:

- Fcitx5 loads the addon and routes trigger keys to the Rust daemon.
- Normal dictation captures audio, recognizes text, optionally postprocesses it, and commits into a real application.
- Command dictation uses selected text, transforms it, replaces the selected text, and fails safely when selection is unavailable.
- The service contract, config semantics, install paths, diagnostics, and logs remain understandable and compatible.
- Deterministic tests keep the spine stable, while live checks prove real desktop behavior.

## Milestones

| Milestone | Name | Exit criteria |
| --- | --- | --- |
| M0 | Repository health | CI green, clean worktree, audit and plan docs current. |
| M1 | Deterministic product spine | `just ime-e2e-smoke` and `just user-ime-command-demo-smoke` pass. |
| M2 | Real desktop alpha | User install, Fcitx restart, addon load, trigger, preedit, commit, command replacement, and live diagnostics work in one real desktop session. |
| M3 | Real ASR alpha | At least one real local ASR backend or robust command backend works for normal dictation without demo WAV input. |
| M4 | Legacy UX parity slice | Minimal scene menu, ASR menu, frontend trigger config, and selected-text fallback are available. |
| M5 | Resource/install parity slice | Model/resource install can download, verify, materialize, and update config without manual file editing. |
| M6 | Release candidate | Packaging, install docs, live validation checklist, and regression tests are ready for external users. |

## P0: real desktop alpha

1. Make `just ime-fcitx-live-probe` actionable and non-mutating by default.
2. Document the explicit opt-in install/probe path for a real user session.
3. Prove Fcitx discovers `fcitx5-vinput.so` from `FCITX_ADDON_DIRS` and metadata from `XDG_DATA_HOME`.
4. Prove normal trigger, command trigger, preedit, commit, command replacement, candidate fallback, and error preedit in a real application.
5. Add selected-text fallback beyond surrounding text so command mode works in more applications.
6. Stabilize live PipeWire capture diagnostics and keep deterministic smokes separate from live checks.

## P0: real ASR alpha

1. Prefer the smallest working `sherpa-onnx` path compatible with existing config.
2. Accept an interim real command ASR helper only if it is documented and tested.
3. Keep `vinput-asr` responsible for backend/session traits, `vinput-audio` for PCM/capture, `vinput-daemon` for lifecycle, and `vinput-registry` for model assets.
4. Require `vinput doctor` to report a ready effective backend before calling the path usable.

## P1: legacy UX parity

1. Add persistent frontend config for normal trigger, command trigger, and trigger mode; keep environment overrides as a development escape hatch.
2. Add minimal scene and ASR menus before recreating every legacy menu detail.
3. Verify command-mode candidate selection replaces selected text.
4. Validate text provider request/response behavior with a local mock server.
5. Add user-facing registry install flow using existing fetch, checksum, staging, archive, and materialization primitives.

## P2: release readiness

1. Add distro packaging only after real desktop alpha and real ASR alpha pass.
2. Write a short install guide around user-level install first.
3. Keep release packaging separate from migration correctness commits.
4. Add upgrade/removal notes for daemon, addon, metadata, activation service, and env file.

## Work selection rules

- Prefer tasks that move M2 or M3 forward.
- Do not count deterministic smoke tests as full parity unless they prove the relevant real behavior.
- Avoid broad refactors unless they unblock a milestone.
- Preserve legacy service names, method names, status strings, config semantics, and recognition payload shape.
- Keep deterministic tests for every new live-facing path.
- Keep commits small and scoped.

## Suggested next slices

1. Improve live probe diagnostics and document the opt-in install path.
2. Add `docs/migration/live-desktop-validation.md` with a real desktop checklist.
3. Implement selected-text fallback for command mode.
4. Add the first real ASR path.
5. Add a local mock-server test for text postprocess behavior.
6. Add the first user-facing registry install command.

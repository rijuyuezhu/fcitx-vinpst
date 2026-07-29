# E2E replication plan

This is the active execution plan. Status belongs in [`function-gap-audit.md`](function-gap-audit.md); subsystem detail belongs in [`e2e-capability-matrix.md`](e2e-capability-matrix.md).

## Product target

A compatible replacement must let a user:

- initialize configuration and managed directories from CLI;
- discover, install, select, and diagnose a registry model;
- load the retained addon in Fcitx5;
- dictate normally with visible partial preedit and final application commit;
- dictate commands over selected text and replace it safely;
- configure keys, scenes, models, providers, adapters, and devices without manual JSON edits;
- diagnose daemon, activation, native runtime, audio, and frontend failures;
- install, upgrade, and remove the product predictably.

Compatibility means preserving user-visible contracts, not mechanically translating C++ source.

## Milestones

| Milestone | State | Exit criteria |
| --- | --- | --- |
| M0 Repository health | complete | clean deterministic checks and current docs |
| M1 Deterministic product spine | complete | staged addon/daemon and outcome smokes |
| M2 Native ASR proof | complete for current families | registry model construction and real WAV recognition |
| M3 Usable CLI/daemon alpha | complete | management flow without manual JSON edits |
| M4 Real desktop native alpha | active | live Fcitx, PipeWire, partial/preedit, commit, command replacement |
| M5 Resource parity | active | provider/adapter install, remove, localized discovery, and adapter runtime selectors complete; update polish pending |
| M6 Release readiness | pending | packaging, upgrades, install docs, external-user regression |

## Completed: usable CLI/daemon alpha

The following are implemented and covered by deterministic tests:

- model registry list/info/install/use/remove;
- current ASR provider registry list/install with batch/streaming validation, short ids, mirror fallback, managed overwrite protection, executable scripts, timeout/env preservation, and config backups;
- legacy-compatible provider removal with local-provider protection, active-selection clearing, and explicit short-id resolution;
- current adapter registry list/install with short ids, mirror fallback, managed overwrite protection, executable scripts, and config backups;
- adapter short-id removal with config backup, `--output` preservation, and guarded in-place cleanup limited to the expected managed script path;
- adapter start/stop/status selectors validate installed config entries and resolve explicit registry short ids before D-Bus calls;
- localized provider/adapter title and description loading from root-level registry i18n files with stable machine-id fallback;
- config initialization and mutation;
- provider, hotword, device, scene, LLM, and adapter management;
- daemon and recording control;
- doctor/runtime/audio/owner diagnostics;
- native offline/online ASR for current registry families;
- generic native runtime bundle installation and D-Bus activation;
- retained-addon menus, keys, filtering, i18n, notifications, owner recovery, partial preedit, commit, and command replacement.

Implemented through D-Bus, the streaming path delivers recorder chunks, emits deduplicated live `RecognitionPartial` signals, renders partial-first preedit, and preserves final results for synchronous stop.

## P0: real desktop native alpha

1. Run the deterministic gate before live work.
2. Install `sherpa-native-live` with a registry-installed supported model.
3. Restart Fcitx5 through the generated environment wrapper.
4. Prove addon discovery and D-Bus activation in the real session.
5. Prove normal trigger -> PipeWire -> native ASR -> partial/preedit -> application commit.
6. Prove command trigger -> selected text -> candidate/postprocess -> replacement.
7. Exercise scene/ASR menus, persistent keys, Tap/Hold/Both, localization, notifications, owner loss, and reload.
8. Record exact failures and add deterministic regressions before fixing them.

The validation procedure is [`live-desktop-validation.md`](live-desktop-validation.md).

## P1: parity after live alpha

- Port other remaining native model layouts only when registry or user demand is concrete.
- Complete provider/adapter update flows only when upstream registry semantics require behavior beyond reinstall.
- Validate one real OpenAI-compatible or command text provider in desktop command mode.
- Broaden daemon-originated notification categories from observed needs.
- Reduce oversized modules only along feature boundaries.
- Implement remote services if they remain part of the replacement target.

## P2: release readiness

- package the CLI, daemon, addon, metadata, translations, VAD asset, activation service, and native runtime policy;
- define upgrade, rollback, uninstall, and stale-owner migration behavior;
- publish a short supported installation path;
- run live validation on supported desktop/application combinations;
- add external-user regression coverage.

## Work selection rules

- Prefer work that directly moves M4.
- Keep mock, file-input, session-bus, and temporary-HOME checks green.
- Do not call deterministic evidence live proof.
- Keep real-profile mutation explicit and opt-in.
- Preserve public wire and frontend contracts.
- Keep commits focused and avoid broad cleanup.

## Next recommended slice

Prove real desktop native dictation first. Port other remaining families, package formats, remote services, or GUI surfaces only after they unblock or follow from that evidence.

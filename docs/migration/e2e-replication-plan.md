# 0.1.0 functional-parity plan

Reviewed: 2026-08-06

This is the active execution plan. Current implementation status belongs in [`function-gap-audit.md`](function-gap-audit.md), the user-task mapping belongs in [`user-capability-audit.md`](user-capability-audit.md), detailed evidence belongs in [`e2e-capability-matrix.md`](e2e-capability-matrix.md), and real-session procedures belong in [`live-desktop-validation.md`](live-desktop-validation.md).

## Product target

Vinpst 0.1.0 should let users complete substantially the same useful tasks as the upstream C++ project:

- install and initialize the product under Vinpst names and paths;
- discover, install, select, and diagnose ASR models/providers;
- dictate normally with visible partial preedit and final application commits;
- speak commands over selected text and replace it safely;
- configure keys, scenes, LLM providers, adapters, devices, VAD, hotwords, and output ducking without requiring manual JSON editing;
- manage resources through the Rust GUI or focused CLI commands;
- diagnose daemon, activation, native runtime, audio, provider, and frontend failures;
- install, update, and remove Vinpst predictably;
- use clear user-facing installation, usage, configuration, troubleshooting, and limitation documentation.

This is practical functional parity, not identity or implementation compatibility. Vinpst keeps its own package, executable, addon, D-Bus, systemd, environment-variable, and XDG identities. It does not replace or migrate another package, and pre-0.1.0 Vinpst interfaces may change when needed.

## Milestones

| Milestone | State | Exit criteria |
| --- | --- | --- |
| M0 Repository health | complete | Clean deterministic checks, bounded source layout, and current developer contracts. |
| M1 Product spine | complete | CLI, daemon, typed config, D-Bus service, retained Fcitx addon, and deterministic command-demo paths work together. |
| M2 Native and provider ASR | complete for the current registry families | Local offline/online models, command providers, remote providers, failure preservation, and representative real-WAV/live paths pass. |
| M3 Real desktop input | complete for the core 0.1.0 path | Normal dictation, command replacement, menus, localization, notifications, focus/owner recovery, model/provider switching, physical microphone, and representative applications are live-proven. |
| M4 Resource management | complete for ordinary workflows | CLI and GUI manage models, providers, adapters, scenes, LLM providers, devices, and hotwords without manual JSON editing. |
| M5 Rust management GUI | interactive baseline complete; accessibility/result proof active | Control, Resources, LLM, and Hotwords workflows are typed, conflict-aware, redacted, keyboard-operable, and packaged; remaining release work is assistive-technology policy, broader error taxonomy, and representative install/recovery result proof. |
| M6 Exhaustive user-capability audit | active | The generated 164-file/1,559-callable baseline is current, every delta is reviewed, and every meaningful upstream user task is mapped to Vinpst implementation/evidence or an explicit non-applicable rationale. |
| M7 User documentation | active | Strict MkDocs site covers installation, quick start, usage, ASR, scenes, settings, CLI, troubleshooting, and limitations using only Vinpst identities and verified commands. |
| M8 Release readiness | active | Selected artifacts build from one checked source archive, install/runtime smokes run on produced artifacts, manifest/checksum/signing policy is wired, required CI is enforced, and an unrelated environment passes the release candidate. |
| M9 0.1.0 publication | pending | Version/tag consistency, release notes, publication, post-publication install, normal dictation, command replacement, diagnostics, and removal all pass. |

## Current priority order

### P0: exhaustive capability review

1. Keep [`../legacy/upstream-source-inventory.json`](../legacy/upstream-source-inventory.json) synchronized with a clean upstream checkout.
2. Review every source/callable delta through [`user-capability-audit.md`](user-capability-audit.md).
3. Treat source mechanics as `not applicable` when the user task is already provided through a different Vinpst design.
4. Mark a real `missing` item only when a user cannot complete a meaningful upstream task through any normal Vinpst path.
5. Add deterministic tests or live evidence for newly discovered behavior gaps.

### P1: close release-relevant functional gaps

- Resolve the GUI assistive-technology accessibility decision for 0.1.0.
- Add representative live result proof for remaining GUI install/recovery and resource mutations.
- Continue broadening application/device behavior only where the audit finds a practical user gap or a release blocker.
- Keep normal dictation, command replacement, provider failure preservation, owner recovery, menus, localization, and current package transactions green.

### P2: user documentation

- Keep the root README concise and task-oriented.
- Build the Markdown tree with MkDocs Material in strict mode.
- Write commands from actual `vinpst --help` surfaces and exercise representative flows in isolated tests.
- Keep migration/evidence detail out of ordinary user procedures; link to limitations rather than embedding test transcripts.
- Borrow useful topic structure from upstream documentation only after rewriting it for Vinpst behavior, names, and paths.
- Continue to use rustdoc for Rust API documentation rather than mixing crate API reference into the user guide.

### P3: release pipeline

- Select the public 0.1.0 artifacts and architectures.
- Keep the completed one-source boundary green: current Debian and Flatpak tag jobs consume the exact archive generated by the source job, and Flatpak rechecks the copied archive digest before publication selection.
- Add the already-implemented Arch/RPM/release-manifest/signature boundaries to the tag workflow where selected for publication.
- Verify installation and a basic runtime path from each produced artifact.
- Require source, Rust, Nix, package, and docs checks before merge/release rather than relying on optional green jobs.
- Exercise release assembly without publishing before creating the tag.

## Completion gate

Do not claim 0.1.0 functional parity until a clean installation can complete this user path without manual JSON editing:

```sh
vinpst init
vinpst model list --available
vinpst model install <id-or-short-id>
vinpst model use <id-or-short-id> --in-place --reload-daemon
vinpst doctor
vinpst daemon status
```

The same installation must then pass:

- live normal dictation with partials and a final commit;
- live command replacement with failure preservation;
- scene and ASR selection;
- restart, reload, owner-loss/recovery, and diagnostics;
- GUI or CLI resource management for the selected release workflows;
- package removal while preserving Vinpst user state;
- strict documentation build and verified release artifacts.

The final review must freeze the current upstream commit and confirm that no known user-facing capability was silently omitted. It must not add upstream package/path identities merely to make names match.

## Work rules

- Prefer user journeys and release blockers over generic cleanup.
- Keep the retained Fcitx C++ layer thin and the standalone GUI in Rust.
- Distinguish `implemented`, `deterministic`, and `live-proven`.
- Keep real-profile mutation explicit and opt-in.
- Preserve stable Vinpst contracts where intentional; do not create compatibility debt for unreleased internal interfaces.
- Keep commits reviewable and update the user-capability audit when status changes.

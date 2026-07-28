# Documentation map

This directory is task-oriented. New agents should follow the reading order instead of scanning every file blindly.

## Required reading order

1. [`../AGENTS.md`](../AGENTS.md): short root instruction file for agents.
2. [`development.md`](development.md): project style, commit message style, validation tiers, and test commands.
3. [`migration/function-gap-audit.md`](migration/function-gap-audit.md): current tracked legacy-vs-Rust parity baseline.
4. [`migration/e2e-capability-matrix.md`](migration/e2e-capability-matrix.md): detailed E2E capability comparison and the native runtime/frontend parity backlog.
5. [`migration/e2e-replication-plan.md`](migration/e2e-replication-plan.md): active milestone plan for real user workflows.
6. [`migration/live-desktop-validation.md`](migration/live-desktop-validation.md): manual/live checklist for proving real Fcitx desktop behavior.
7. [`migration/e2e-port-plan.md`](migration/e2e-port-plan.md): historical E2E entry point that now links to the active plan.
8. [`migration/agent-kickoff.md`](migration/agent-kickoff.md): copyable context for a fresh implementation agent.
9. [`architecture/README.md`](architecture/README.md): architecture contract index. Read the contract document for the crate or subsystem you will touch.
10. [`legacy/README.md`](legacy/README.md) and [`legacy/source-annotations.md`](legacy/source-annotations.md): legacy C++ source analysis when behavior comparison is needed.

## Directory roles

- [`architecture/`](architecture/): tracked stable contracts for crate boundaries, bus service behavior, config, registry, ASR, audio, and text behavior.
- [`legacy/`](legacy/): tracked migration record for the original C++ source tree.
- [`migration/`](migration/): tracked audits, active execution plans, validation checklists, and agent handoff prompts for the Rust port.
- `plan/`: ignored local scratch. Do not manually track files under this directory, and do not treat it as source of truth.

## How to update docs

- Update `migration/function-gap-audit.md` when the parity baseline changes.
- Update `migration/e2e-capability-matrix.md` when detailed CLI/daemon/user-journey parity changes.
- Update `migration/e2e-replication-plan.md` when priorities, milestones, or acceptance criteria change.
- Update `migration/live-desktop-validation.md` when live desktop validation steps change.
- Update `migration/agent-kickoff.md` when a new agent needs different startup context, checks, or first-task guidance.
- Update `development.md` when workflow, validation tiers, test commands, or commit conventions change.
- Update `architecture/*` when a public contract or compatibility rule changes.
- Update `legacy/*` only when source analysis of the original project changes.

## Consistency rules

- `function-gap-audit.md` answers "where are we?".
- `e2e-capability-matrix.md` answers "what exactly is missing for real desktop and legacy parity?".
- `e2e-replication-plan.md` answers "what should we do next?".
- `live-desktop-validation.md` answers "how do we prove real desktop behavior?".
- `agent-kickoff.md` answers "how should a fresh agent start?".
- `development.md` answers "how should changes be made and validated?".
- Avoid duplicating long plans across files; link to the active source instead.

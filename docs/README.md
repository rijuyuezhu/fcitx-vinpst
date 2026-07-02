# Documentation map

This directory is task-oriented. New agents should follow the reading order instead of scanning every file blindly.

## Required reading order

1. [`../AGENTS.md`](../AGENTS.md): short root instruction file for agents.
2. [`development.md`](development.md): project style, commit message style, validation tiers, and test commands.
3. [`migration/function-gap-audit.md`](migration/function-gap-audit.md): current tracked legacy-vs-Rust parity baseline.
4. [`migration/e2e-replication-plan.md`](migration/e2e-replication-plan.md): active plan for real desktop alpha and full E2E replication.
5. [`migration/live-desktop-validation.md`](migration/live-desktop-validation.md): manual/live checklist for proving real Fcitx desktop behavior.
6. [`migration/e2e-port-plan.md`](migration/e2e-port-plan.md): historical E2E entry point that now links to the active plan.
7. [`migration/agent-kickoff.md`](migration/agent-kickoff.md): copyable context for a fresh implementation agent.
8. [`architecture/README.md`](architecture/README.md): architecture contract index. Read the contract document for the crate or subsystem you will touch.
9. [`legacy/README.md`](legacy/README.md) and [`legacy/source-annotations.md`](legacy/source-annotations.md): legacy C++ source analysis when behavior comparison is needed.

## Directory roles

- [`architecture/`](architecture/): tracked stable contracts for crate boundaries, bus service behavior, config, registry, ASR, audio, and text behavior.
- [`legacy/`](legacy/): tracked migration record for the original C++ source tree.
- [`migration/`](migration/): tracked audits, active execution plans, validation checklists, and agent handoff prompts for the Rust port.
- `plan/`: ignored local scratch. Do not manually track files under this directory, and do not treat it as source of truth.

## How to update docs

- Update `migration/function-gap-audit.md` when the parity baseline changes.
- Update `migration/e2e-replication-plan.md` when priorities, milestones, or acceptance criteria change.
- Update `migration/live-desktop-validation.md` when live desktop validation steps change.
- Update `migration/agent-kickoff.md` when a new agent needs different startup context, checks, or first-task guidance.
- Update `development.md` when workflow, validation tiers, test commands, or commit conventions change.
- Update `architecture/*` when a public contract or compatibility rule changes.
- Update `legacy/*` only when source analysis of the original project changes.

## Consistency rules

- `function-gap-audit.md` answers "where are we?".
- `e2e-replication-plan.md` answers "what should we do next?".
- `live-desktop-validation.md` answers "how do we prove real desktop behavior?".
- `agent-kickoff.md` answers "how should a fresh agent start?".
- `development.md` answers "how should changes be made and validated?".
- Avoid duplicating long plans across files; link to the active source instead.

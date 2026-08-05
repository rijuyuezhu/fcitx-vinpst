# Documentation map

The tracked documentation is split by purpose. Read only the source needed for the task; do not copy status text between files. The root [`README.md`](../README.md) is a concise project landing page, not a second migration report.

## User documentation

External users following the currently supported Arch package path should start with [`user/installation.md`](user/installation.md).

## Required developer reading order

1. [`../AGENTS.md`](../AGENTS.md): repository rules and current product priority.
2. [`development.md`](development.md): workflow, validation tiers, and commit style.
3. [`migration/function-gap-audit.md`](migration/function-gap-audit.md): current implementation status.
4. [`migration/e2e-capability-matrix.md`](migration/e2e-capability-matrix.md): detailed E2E capability comparison and the native runtime/frontend parity backlog.
5. [`migration/e2e-replication-plan.md`](migration/e2e-replication-plan.md): active milestones and next work.
6. [`migration/live-desktop-validation.md`](migration/live-desktop-validation.md): real-session validation procedure.
7. [`architecture/README.md`](architecture/README.md): stable architecture contracts, including the canonical identity policy.
8. [`legacy/README.md`](legacy/README.md): source map for the original C++ project.

[`live-toolkit-debugging.md`](live-toolkit-debugging.md) is the reusable real-application probe troubleshooting runbook. [`migration/agent-kickoff.md`](migration/agent-kickoff.md) is a short handoff summary. [`migration/e2e-port-plan.md`](migration/e2e-port-plan.md) is a compatibility redirect for older references.

## Sources of truth

- `function-gap-audit.md` answers **where are we?**
- `e2e-capability-matrix.md` answers **what exactly is missing for real desktop and legacy parity?**
- `e2e-replication-plan.md` answers **what should be done next?**
- `live-desktop-validation.md` answers **how is live behavior proven?**
- `live-toolkit-debugging.md` answers **how are real application probe failures diagnosed?**
- `architecture/*` defines stable contracts, not progress reports.
- `development.md` defines how changes are made and checked.

Do not maintain parity percentages in multiple files. Prefer evidence-based stage labels such as `implemented`, `deterministic`, `live-proven`, `partial`, and `missing`.

## Directory roles

- [`user/installation.md`](user/installation.md): supported external-user Arch lifecycle and its evidence boundary.
- [`architecture/`](architecture/): crate boundaries and compatibility contracts.
- [`migration/`](migration/): current status, capability gaps, execution plan, and live validation.
- [`legacy/`](legacy/): tracked analysis of the original C++ source.
- `plan/`: ignored local scratch; never commit or cite it as project truth.

## Maintenance rules

- Update the audit when implementation status changes.
- Update the matrix when a user journey or subsystem capability changes.
- Update the plan only when priorities or milestone acceptance changes.
- Update live validation only when the real-session procedure changes.
- Update architecture documents only when a stable boundary or compatibility rule changes.
- Link subsystem evidence from migration summaries instead of copying detailed package, protocol, or runtime checklists into multiple status files.
- Keep historical transcripts, one-off review notes, and long command output out of tracked documentation.

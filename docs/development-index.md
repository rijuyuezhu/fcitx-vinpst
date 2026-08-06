# Documentation map

The tracked documentation is split by purpose. Read only the source needed for the task; do not copy status text between files. The repository root README is a concise project landing page, not a second migration report.

## User documentation

Users should start with [`index.md`](index.md), then follow:

- [`user/installation.md`](user/installation.md): release status, supported identities, installation, and removal;
- [`user/quick-start.md`](user/quick-start.md): first configuration, model selection, daemon startup, and first dictation;
- [`user/usage.md`](user/usage.md): normal dictation, command editing, trigger modes, menus, and application behavior;
- [`user/asr.md`](user/asr.md): local, command, and remote ASR providers, models, and hotwords;
- [`user/scenes.md`](user/scenes.md): scenes, LLM providers, and text adapters;
- [`user/settings.md`](user/settings.md): daemon and Fcitx settings;
- [`user/cli.md`](user/cli.md): CLI command groups and safe mutation patterns;
- [`user/troubleshooting.md`](user/troubleshooting.md): diagnosis and recovery;
- [`user/limitations.md`](user/limitations.md): current release and evidence boundaries.

The same Markdown is built as a MkDocs Material site. Run `just docs` for a strict build or `just docs-serve` for a local preview. Rust crate APIs remain documented through `cargo doc` rather than being duplicated into the user site.

## Required developer reading order

1. repository `AGENTS.md`: repository rules and current product priority.
2. [`development.md`](development.md): workflow, validation tiers, and commit style.
3. [`migration/function-gap-audit.md`](migration/function-gap-audit.md): current implementation status.
4. [`migration/user-capability-audit.md`](migration/user-capability-audit.md): upstream user journeys mapped to Vinpst implementations and evidence.
5. [`migration/e2e-capability-matrix.md`](migration/e2e-capability-matrix.md): detailed E2E capability comparison and the native runtime/frontend parity backlog.
6. [`migration/e2e-replication-plan.md`](migration/e2e-replication-plan.md): active milestones and next work.
7. [`migration/live-desktop-validation.md`](migration/live-desktop-validation.md): real-session validation procedure.
8. [`architecture/README.md`](architecture/README.md): stable architecture contracts, including the canonical identity policy.
9. [`legacy/README.md`](legacy/README.md): source and callable inventory for the upstream C++ project.

[`live-toolkit-debugging.md`](live-toolkit-debugging.md) is the reusable real-application probe troubleshooting runbook. [`migration/agent-kickoff.md`](migration/agent-kickoff.md) is a short handoff summary. [`migration/e2e-port-plan.md`](migration/e2e-port-plan.md) is a compatibility redirect for older references.

## Sources of truth

- `function-gap-audit.md` answers **where are we?**
- `user-capability-audit.md` answers **which user tasks from upstream exist in Vinpst, and what evidence supports them?**
- `e2e-capability-matrix.md` answers **what exactly is missing for real desktop and practical feature parity?**
- `e2e-replication-plan.md` answers **what should be done next?**
- `live-desktop-validation.md` answers **how is live behavior proven?**
- `live-toolkit-debugging.md` answers **how are real application probe failures diagnosed?**
- `architecture/*` defines stable contracts, not progress reports.
- `development.md` defines how changes are made and checked.

Do not maintain parity percentages in multiple files. Prefer evidence-based stage labels such as `implemented`, `deterministic`, `live-proven`, `partial`, and `missing`.

## Directory roles

- [`index.md`](index.md) and [`user/installation.md`](user/installation.md): entry points for user-facing product, installation, usage, configuration, and troubleshooting documentation.
- [`architecture/README.md`](architecture/README.md): crate boundaries and stable Vinpst contracts.
- [`migration/function-gap-audit.md`](migration/function-gap-audit.md): current status, capability gaps, execution plan, and live validation links.
- [`legacy/README.md`](legacy/README.md): tracked analysis of the upstream C++ source.
- `plan/`: ignored local scratch; never commit or cite it as project truth.

## Maintenance rules

- Update the audit when implementation status changes.
- Update the matrix when a user journey or subsystem capability changes.
- Update the plan only when priorities or milestone acceptance changes.
- Update live validation only when the real-session procedure changes.
- Update architecture documents only when a stable boundary or compatibility rule changes.
- Link subsystem evidence from migration summaries instead of copying detailed package, protocol, or runtime checklists into multiple status files.
- Keep historical transcripts, one-off review notes, and long command output out of tracked documentation.

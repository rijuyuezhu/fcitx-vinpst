# Architecture contracts

This directory contains tracked architecture and compatibility contracts for the Rust rewrite. Read [`../development.md`](../development.md), [`../migration/function-gap-audit.md`](../migration/function-gap-audit.md), [`../migration/e2e-capability-matrix.md`](../migration/e2e-capability-matrix.md), and [`../migration/e2e-replication-plan.md`](../migration/e2e-replication-plan.md) first, then use this index to choose the subsystem document relevant to the task.

## Reading order

1. [`target-architecture.md`](target-architecture.md): crate boundaries, runtime actors, state machine target, and migration principles.
2. Subsystem contract for the area being changed:
   - [`dbus-service.md`](dbus-service.md): legacy D-Bus service facade, diagnostic extension, and compatibility rules.
   - [`config-contract.md`](config-contract.md): default config fixture, parsing, validation, and diagnostics behavior.
   - [`registry-contract.md`](registry-contract.md): registry metadata, dry-run planning, and sample fixture contracts.
   - [`asr-contract.md`](asr-contract.md): ASR backend/session seams, command ASR behavior, and diagnostics.
   - [`audio-contract.md`](audio-contract.md): PCM layout, WAV/raw byte policy, recorder lifecycle, and PipeWire scaffold.
   - [`text-contract.md`](text-contract.md): text post-processing, prompt/context cache, command adapters, and OpenAI-compatible seams.
   - [`remote-text-contract.md`](remote-text-contract.md): remote input settings, authentication, protocol state, the standalone HTTP/WebSocket runtime, and the pending daemon-lifecycle boundary.
3. [`../migration/function-gap-audit.md`](../migration/function-gap-audit.md), for current parity baseline.
4. [`../migration/e2e-capability-matrix.md`](../migration/e2e-capability-matrix.md), for detailed native runtime/frontend/user-flow parity gaps.
5. [`../migration/e2e-replication-plan.md`](../migration/e2e-replication-plan.md), for active migration direction.
6. `../plan/`, when present locally, for ignored scratch notes only.

## Maintenance rules

- These files are tracked and should describe stable contracts or explicit compatibility targets.
- Do not use these files as scratch planning space; use ignored `docs/plan/` for that.
- Delete or ignore stale review snapshots after consolidating their conclusions into tracked `docs/migration/` or these contract docs.
- Keep statements precise: distinguish `implemented`, `mock/seam only`, `configured behind an explicit flag`, and `future work`.

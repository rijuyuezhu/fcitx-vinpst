# Architecture contracts

This directory contains tracked architecture and compatibility contracts for Vinpst. Read [`../development.md`](../development.md), then use this index to choose the subsystem document relevant to the task. Historical migration evidence remains under [`../migration/`](../migration/function-gap-audit.md).

## Reading order

1. [`target-architecture.md`](target-architecture.md): crate boundaries, runtime actors, state machine target, and migration principles.
2. [`identity-contract.md`](identity-contract.md): canonical Vinpst names, independent product identity, and the explicit absence of old-name migration or aliases.
3. Subsystem contract for the area being changed:
   - [`dbus-service.md`](dbus-service.md): current Vinpst D-Bus service contract, diagnostic extensions, and atomic change rules.
   - [`config-contract.md`](config-contract.md): default config fixture, parsing, validation, and diagnostics behavior.
   - [`process-contract.md`](process-contract.md): shared command-helper process groups, deadlines, descendant cleanup, zombie-aware liveness, and bounded output capture.
   - [`registry-contract.md`](registry-contract.md): registry metadata, dry-run planning, and sample fixture contracts.
   - [`asr-contract.md`](asr-contract.md): ASR backend/session seams, command ASR behavior, and diagnostics.
   - [`audio-contract.md`](audio-contract.md): PCM layout, WAV/raw byte policy, recorder lifecycle, and PipeWire scaffold.
   - [`text-contract.md`](text-contract.md): text post-processing, prompt/context cache, command adapters, and OpenAI-compatible seams.
   - [`remote-text-contract.md`](remote-text-contract.md): remote input settings, authentication, protocol state, HTTP/WebSocket runtime, daemon-owned lifecycle, and remaining endpoint/live-validation boundary.
   - [`packaging-contract.md`](packaging-contract.md): package identity, runtime contents, private native-library policy, lifecycle handoff, transaction evidence, and release validation.
   - [`gui-contract.md`](gui-contract.md): implemented Rust/Iced management baseline, package integration, and remaining parity criteria.
4. [`../migration/function-gap-audit.md`](../migration/function-gap-audit.md), for current parity baseline.
5. [`../migration/e2e-capability-matrix.md`](../migration/e2e-capability-matrix.md), for detailed native runtime/frontend/user-flow parity gaps.

## Maintenance rules

- These files are tracked and should describe stable contracts or explicit compatibility targets.
- Delete stale review snapshots after consolidating conclusions into tracked migration evidence or these contract docs.
- Keep statements precise: distinguish `implemented`, `mock/seam only`, `configured behind an explicit flag`, and `future work`.

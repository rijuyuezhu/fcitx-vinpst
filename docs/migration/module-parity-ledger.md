# Module parity ledger

The canonical ownership ledger is [`module-parity-ledger.json`](module-parity-ledger.json). It exists to make the upstream-to-Rust audit exhaustive and non-overlapping rather than relying on conversational progress notes.

## Invariants

`scripts/tests/check-module-parity-ledger.py` enforces two exact partitions:

- every source file in the frozen upstream inventory is owned by exactly one completed/active audit unit or appears exactly once in `upstream_pending`;
- every current production Rust source file and retained Fcitx C++ frontend source file is owned by exactly one completed/active audit unit or appears exactly once in `current_pending`.

A duplicate claim, an omitted source file, a deleted/renamed stale path, or a newly added current production source file fails the repository check. Test-only Rust modules are deliberately excluded from current-source ownership; they may appear in `supporting_current_files` as evidence without being treated as audited production ownership.

## Audit states

- `active`: the file set has been removed from pending and is being compared now.
- `audited-aligned`: behavior matches upstream for the owned responsibilities.
- `audited-fixed`: comparison found migration drift and the current implementation was corrected.
- `audited-intentional-divergence`: differences were reviewed and deliberately retained.
- `audited-with-gap`: reviewed behavior is mostly accounted for, but an explicit remaining gap is recorded in the unit notes and migration gap audit.

`supporting_current_files` is informational only. A file listed there remains pending until its **whole production responsibility** is reviewed in its own owning audit unit. This prevents a small glue change from falsely marking a large daemon or GUI file complete.

## Workflow

When starting a comparison, create one uniquely named audit unit and move the exact upstream/current production files from the pending lists into that unit with state `active`. Compare the whole owned file/module responsibility, including defaults, state transitions, errors, fallback behavior, user-visible effects, and relevant tests. When complete, change the state to the appropriate audited state and record concise findings. Do not put the same production file in a later unit; if another audit merely touches it, record it under `supporting_current_files` instead.

The frozen upstream side currently comes from commit recorded in `docs/legacy/upstream-source-inventory.json`. The checker deliberately reuses that inventory, so there is one upstream file truth rather than a second manually maintained source list.

## Completed units

| Unit | State | Scope |
| --- | --- | --- |
| `CFG-CORE-001` | `audited-fixed` | Core config schema/defaults/normalization/persistence/validation and scene config contract. |
| `TXT-PROMPT-001` | `audited-fixed` | Prompt file URI loading and interpolation. |
| `TXT-POST-001` | `audited-with-gap` | LLM post-processing, fallback payloads, context, request/response contract; active shutdown cancellation remains open. |
| `TXT-ADAPTER-001` | `audited-intentional-divergence` | Command adapter process metadata/cwd behavior with stronger Rust process supervision retained. |

The JSON ledger contains the exact file lists and is the source of truth for coverage counts.

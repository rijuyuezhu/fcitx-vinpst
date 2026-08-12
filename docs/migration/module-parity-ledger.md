# Module parity ledger

The historical ownership ledger is [`module-parity-ledger.json`](module-parity-ledger.json). It records how the upstream-to-Rust audit was partitioned while the exhaustive comparison was active.

## Status

The ledger is audit evidence, not a CI contract. Current source files may be renamed, split, or removed without updating an exact ownership partition just to satisfy a structural check. Repository validation instead relies on behavior, public ABI/wire contracts, builds, package artifacts, and the frozen upstream inventory artifact.

The file lists remain useful for reconstructing the completed comparison, but they are not required to mirror the current repository layout after later refactors.

## Audit states

- `active`: the file set has been removed from pending and is being compared now.
- `audited-aligned`: behavior matches upstream for the owned responsibilities.
- `audited-fixed`: comparison found migration drift and the current implementation was corrected.
- `audited-intentional-divergence`: differences were reviewed and deliberately retained.
- `audited-with-gap`: reviewed behavior is mostly accounted for, but an explicit remaining gap is recorded in the unit notes and migration gap audit.

`supporting_current_files` is historical evidence only; it does not require the referenced file to keep the same name or location after the audit.

## Workflow

For future upstream refreshes, regenerate the upstream inventory and review user-visible capability deltas. Do not reintroduce an exact current-source ownership gate. Record new audit notes only where they help explain behavior or a deliberate divergence.

The frozen upstream side comes from the commit recorded in `docs/legacy/upstream-source-inventory.json`, which remains the source artifact for the completed comparison.

## Completed units

| Unit | State | Scope |
| --- | --- | --- |
| `CFG-CORE-001` | `audited-fixed` | Core config schema/defaults/normalization/persistence/validation and scene config contract. |
| `TXT-PROMPT-001` | `audited-fixed` | Prompt file URI loading and interpolation. |
| `TXT-POST-001` | `audited-with-gap` | LLM post-processing, fallback payloads, context, request/response contract; active shutdown cancellation remains open. |
| `TXT-ADAPTER-001` | `audited-intentional-divergence` | Command adapter process metadata/cwd behavior with stronger Rust process supervision retained. |

The JSON ledger contains the file lists captured during the audit; current readiness is tracked in the migration status and capability documents rather than inferred from those paths.

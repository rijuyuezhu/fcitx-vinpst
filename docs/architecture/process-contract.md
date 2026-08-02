# Command process supervision contract

`crates/vinput-process` owns the Unix process boundary shared by command ASR providers and command-backed text adapters. Protocol encoding and response parsing remain in `vinput-asr` and `vinput-text`; process creation, cancellation, descendant cleanup, and bounded output capture do not.

## Runtime boundary

`run_piped_command` starts each helper in a new Unix process group with piped stdin, stdout, and stderr. Callers provide an optional deadline and one stdin writer closure. When a deadline is present, the watchdog starts immediately after spawn, so it covers request writing, helper execution, and output recovery rather than only the final wait.

Stdout and stderr are drained concurrently and capped independently at 1 MiB. Reading byte 1,048,577 records a stream-specific fixed error, kills the process group and direct child, and never exposes captured output in the overflow diagnostic. Read failures, reader-thread failures, watchdog failures, and child-wait failures have typed categories so each consumer can preserve its public error vocabulary.

The direct child defines the helper lifecycle. On Linux, the supervisor observes its exit with `waitid(WNOWAIT)`, terminates the remaining process group while the unreaped child still reserves the PID/PGID, and only then calls `Child::wait` and joins output readers. This avoids PID-reuse races and prevents a background descendant from keeping inherited pipes open indefinitely even when no deadline was configured. Timeout and output-failure paths use the same whole-group cleanup.

The supervisor returns a stdin-write error alongside a completed process result. Consumers therefore preserve the existing priority rule: a non-zero helper exit and bounded stderr diagnostic take precedence over a broken stdin pipe; otherwise the original stdin-write failure is surfaced.

## Long-lived process groups

The crate also exposes the process-group primitives used by long-running command text adapters. `configure_process_group` creates the same isolated group boundary as `run_piped_command`. Signaling targets the group first and falls back to the direct child only when the group operation fails, avoiding a redundant PID signal after successful whole-group delivery. Tracked children use `try_wait_child_and_cleanup` and `terminate_child_process_group`; on Linux the same `waitid(WNOWAIT)` reservation keeps the direct child PID/PGID unavailable for reuse until remaining descendants are terminated and the child is reaped.

These primitives do not define adapter PID-file ownership or restart policy. `vinput-text` owns those compatibility decisions and supplies the legacy TERM/KILL timing.

## Consumer contracts

Command text adapters always pass the effective scene deadline, including the legacy 4000 ms default. Command ASR providers pass their configured optional `timeout_ms`: a configured value is enforced across the whole helper lifecycle, while an omitted value remains explicitly `not_configured`. Consequently, a command ASR helper can still run indefinitely when `timeout_ms` is omitted if it never exits, including while ignoring a blocked stdin writer; the runtime does not invent an undocumented ASR default.

Legacy batch ASR, legacy streaming ASR, JSON command ASR, and JSON command text all use the same supervisor. Their protocol and public error messages remain consumer-owned.

## Deterministic evidence

Shared tests prove stdin/stdout/stderr roundtrips, deadline cancellation, and prompt rejection of oversized output. Consumer tests additionally prove:

- a helper that ignores a large stdin request is cancelled at its configured deadline;
- timeout kills a background descendant;
- a direct child that exits while leaving a background descendant is collected promptly even without a deadline;
- 256 KiB stderr does not deadlock the parent;
- stdout and stderr above the independent 1 MiB limits are rejected with fixed, content-free diagnostics.

These are local Unix process-semantics guarantees. They do not claim sandboxing, seccomp confinement, resource limits beyond captured output, or policy for untrusted third-party helper binaries.

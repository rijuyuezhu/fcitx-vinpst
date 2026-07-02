# E2E Rust port plan

This file is kept as the stable historical entry point for the E2E port. The active plan has been consolidated into [`e2e-replication-plan.md`](e2e-replication-plan.md), and the current parity baseline is tracked in [`function-gap-audit.md`](function-gap-audit.md).

## Current objective

Move from deterministic product-spine coverage to **real desktop alpha**, then to legacy feature parity.

The goal is functional replication of legacy user-visible behavior, not a line-by-line C++ port. Rust internals should stay simpler and more testable while preserving service contracts, config semantics, Fcitx integration expectations, ASR/text flow behavior, install paths, and diagnostics.

## Current state

The Rust rewrite already has:

- clear workspace crates for protocol, config, audio, ASR, text, registry, daemon, and CLI;
- retained C++ Fcitx5 addon under `cpp/fcitx5-addon`;
- deterministic command-demo E2E path;
- user install script for daemon, addon module, addon metadata, activation service, config, WAV fixture, and env file;
- optional PipeWire recorder path;
- `vinput doctor` diagnostics;
- strong deterministic CI/smoke coverage.

The main gaps are:

- real local ASR backend;
- live desktop addon/load/trigger/commit verification;
- frontend config and legacy menus;
- selected-text fallback;
- user-facing model/resource install;
- release packaging.

## Required reading order for implementation agents

1. [`function-gap-audit.md`](function-gap-audit.md)
2. [`e2e-replication-plan.md`](e2e-replication-plan.md)
3. [`agent-kickoff.md`](agent-kickoff.md)
4. [`../development.md`](../development.md)
5. The relevant file under [`../architecture/`](../architecture/)

## Next recommended slice

Start with a P0 slice from [`e2e-replication-plan.md`](e2e-replication-plan.md): improve live desktop probe diagnostics, add a real desktop validation checklist, or implement selected-text fallback. Do not start broad refactors or release packaging before real desktop alpha is proven.

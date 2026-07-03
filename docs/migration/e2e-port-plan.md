# E2E Rust port plan

This file is kept as the stable historical entry point for the E2E port. The active plan has been consolidated into [`e2e-replication-plan.md`](e2e-replication-plan.md), the current parity baseline is tracked in [`function-gap-audit.md`](function-gap-audit.md), and the detailed CLI/daemon E2E gap matrix lives in [`e2e-capability-matrix.md`](e2e-capability-matrix.md).

## Current objective

Move from deterministic product-spine coverage and one proven native SenseVoice file-input path to **usable CLI/daemon alpha**, then real desktop native dictation, then legacy feature parity.

The goal is functional replication of legacy user-visible behavior, not a line-by-line C++ port. Rust internals should stay simpler and more testable while preserving service contracts, config semantics, Fcitx integration expectations, ASR/text flow behavior, install paths, and diagnostics.

## Current state

The Rust rewrite already has:

- clear workspace crates for protocol, config, audio, ASR, text, registry, daemon, and CLI;
- retained C++ Fcitx5 addon under `cpp/fcitx5-addon`;
- deterministic command-demo E2E path;
- user install script for daemon, addon module, addon metadata, activation service, config, WAV fixture, and env file;
- optional PipeWire recorder path;
- feature-gated native sherpa SenseVoice backend with a verified registry model/WAV smoke;
- `vinput doctor` diagnostics;
- strong deterministic CI/smoke coverage.

The main gaps are now:

- legacy-style CLI management for init/config/model/provider/hotword/device/scene/LLM/adapter/daemon/recording;
- live registry `models.json/providers.json/adapters.json` resource install;
- live desktop addon/load/trigger/commit verification with native ASR;
- frontend config and legacy menus;
- selected-text fallback;
- broader sherpa model families, streaming, VAD, and runtime metadata mapping;
- release packaging.

## Required reading order for implementation agents

1. [`function-gap-audit.md`](function-gap-audit.md)
2. [`e2e-replication-plan.md`](e2e-replication-plan.md)
3. [`e2e-capability-matrix.md`](e2e-capability-matrix.md)
4. [`agent-kickoff.md`](agent-kickoff.md)
5. [`../development.md`](../development.md)
6. The relevant file under [`../architecture/`](../architecture/)

## Next recommended slice

Start with a P0 slice from [`e2e-capability-matrix.md`](e2e-capability-matrix.md): live registry `models.json` parsing, `vinput model list/install/use`, config mutation, daemon/recording D-Bus CLI commands, or native sherpa activation library-path hardening. Do not start GUI or distro packaging before the terminal-first CLI/daemon happy path works without manual JSON edits.

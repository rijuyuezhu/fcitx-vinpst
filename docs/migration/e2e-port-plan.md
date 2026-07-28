# E2E Rust port plan

This file is kept as the stable historical entry point for the E2E port. The active plan has been consolidated into [`e2e-replication-plan.md`](e2e-replication-plan.md), the current parity baseline is tracked in [`function-gap-audit.md`](function-gap-audit.md), and the detailed native-runtime/frontend gap matrix lives in [`e2e-capability-matrix.md`](e2e-capability-matrix.md).

## Current objective

Move from the completed **usable CLI/daemon alpha** and proven native SenseVoice file-input path to real desktop native dictation, broader native ASR parity, then legacy feature parity.

The goal is functional replication of legacy user-visible behavior, not a line-by-line C++ port. Rust internals should stay simpler and more testable while preserving service contracts, config semantics, Fcitx integration expectations, ASR/text flow behavior, install paths, and diagnostics.

## Current state

The Rust rewrite already has:

- clear workspace crates for protocol, config, audio, ASR, text, registry, daemon, and CLI;
- retained C++ Fcitx5 addon under `cpp/fcitx5-addon`;
- deterministic command-demo E2E path and user install/activation profiles;
- broad CLI management for config, models, providers, hotwords, devices, scenes, LLMs, adapters, daemon, and recording;
- safe live model registry fetch, install, use, info, and remove flows;
- optional PipeWire recorder path;
- feature-gated native SenseVoice and Qwen3 ASR with verified registry-model WAV smokes;
- surrounding-text plus primary-selection clipboard fallback in command mode;
- `vinput doctor` diagnostics and strong deterministic CI/smoke coverage.

The main gaps are now:

- live desktop addon/load/trigger/PipeWire/native-ASR/commit proof;
- real desktop streaming-partial proof and a non-blocking reload worker;
- broader sherpa model families;
- frontend config, scene/ASR menus, and richer notifications;
- provider/adapter registry installation breadth and remote services;
- release packaging and legacy GUI parity.

## Required reading order for implementation agents

1. [`function-gap-audit.md`](function-gap-audit.md)
2. [`e2e-replication-plan.md`](e2e-replication-plan.md)
3. [`e2e-capability-matrix.md`](e2e-capability-matrix.md)
4. [`agent-kickoff.md`](agent-kickoff.md)
5. [`../development.md`](../development.md)
6. The relevant file under [`../architecture/`](../architecture/)

## Next recommended slice

Start with an M4 or native-runtime slice from [`e2e-capability-matrix.md`](e2e-capability-matrix.md): prove the real desktop native chain including partial preedit, move prepared reload to a non-blocking worker, or port the next registry model family. Offline Silero VAD, endpoint rules, recognizer warmup, timeout diagnostics, and prepare-before-swap reload are already implemented. Do not start broad GUI polish or distro packaging before real desktop native alpha is proven.

# fcitx-vinput-rs

Rust-oriented rewrite of [`fcitx5-vinput`](https://github.com/xifan2333/fcitx5-vinput).

The project currently provides a usable Rust CLI/daemon alpha, a retained thin C++ Fcitx5 addon, and a packaged Rust/Iced management GUI baseline. Native dictation, command replacement, menus, localization, provider switching, recovery paths, and several real desktop applications have live evidence; package and release behavior has broad deterministic coverage. It is not yet a full legacy replacement.

## Start here

- Checked Arch package lifecycle: [`docs/user/installation.md`](docs/user/installation.md)
- Documentation map and sources of truth: [`docs/README.md`](docs/README.md)
- Current implementation status: [`docs/migration/function-gap-audit.md`](docs/migration/function-gap-audit.md)
- Detailed capability matrix: [`docs/migration/e2e-capability-matrix.md`](docs/migration/e2e-capability-matrix.md)
- Active work and priorities: [`docs/migration/e2e-replication-plan.md`](docs/migration/e2e-replication-plan.md)
- Architecture contracts: [`docs/architecture/README.md`](docs/architecture/README.md)
- Development workflow: [`docs/development.md`](docs/development.md)

The current development priority is the Rust management GUI: complete richer resource details and error recovery, signal-driven daemon reconciliation, command-mode integration, localization/accessibility, and real Wayland/X11 interaction proof. Existing desktop and packaging evidence must remain green, but new packaging expansion is deferred while this baseline advances.

## Workspace

- `vinput-protocol`: public D-Bus and recognition payload contracts.
- `vinput-config`: typed configuration, validation, normalization, persistence, and redaction.
- `vinput-http`: shared bounded and credential-safe provider HTTP construction.
- `vinput-process`: Unix helper supervision, deadlines, process-group cleanup, and bounded output.
- `vinput-audio`: PCM processing, recorder traits, and optional PipeWire capture.
- `vinput-asr`: mock, command, remote, and optional native `sherpa-onnx` backends.
- `vinput-text`: scenes, prompts, command adapters, context cache, and OpenAI-compatible text transport.
- `vinput-registry`: registry metadata, safe download/extraction, and managed publication.
- `vinput-daemon`: runtime orchestration and the legacy-compatible D-Bus service.
- `vinput-cli`: the `vinput` management and diagnostics CLI.
- `vinput-gui`: the standalone Rust/Iced management application.

`cpp/fcitx5-addon` remains C++ deliberately. It owns only the Fcitx API boundary: key handling, menus, preedit/commit presentation, selected-text handling, notifications, and D-Bus integration. Backend policy belongs in Rust.

## Build and check

Use `just` as the project interface:

```sh
just fmt-check
just lint
just test
just check
```

`just ci` is the complete deterministic project gate. Optional real-desktop, microphone, network-device, and full release builds are documented separately and are not implied by a passing deterministic smoke.

Run the committed file-input demo with:

```sh
just demo
```

For a display-independent GUI/configuration check:

```sh
cargo run -p vinput-gui -- --check --offline
```

## License

GPL-3.0-only. See [`LICENSE`](LICENSE).

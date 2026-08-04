# Agent kickoff

Reviewed: 2026-08-03

Use this as the compact handoff for implementation work. It points to current sources of truth; it is not another progress log.

## Mission

Continue `fcitx-vinput-rs` from the usable CLI/daemon alpha, live-proven retained Fcitx frontend, broad deterministic release baseline, and packaged Rust management GUI baseline. Do not rebuild completed management, registry, native ASR, activation, frontend, or package transaction surfaces.

The active priority is **M7: Rust management GUI**. Broaden resource-specific error taxonomy, add command-mode integration, complete localization/accessibility, and obtain real Wayland/X11 interaction proof. Treat new packaging and desktop breadth as deferred expansion unless a regression, blocker, or explicit user request requires it.

## Repositories

- Rust rewrite: `/workspace/fcitx-vinput-rs`
- Legacy reference: `/workspace/fcitx5-vinput`

## Required reading

1. [`../../AGENTS.md`](../../AGENTS.md)
2. [`../README.md`](../README.md)
3. [`../development.md`](../development.md)
4. [`function-gap-audit.md`](function-gap-audit.md)
5. [`e2e-capability-matrix.md`](e2e-capability-matrix.md)
6. [`e2e-replication-plan.md`](e2e-replication-plan.md)
7. [`live-desktop-validation.md`](live-desktop-validation.md) when touching live behavior
8. the relevant [`../architecture/`](../architecture/) contract
9. [`../legacy/source-annotations.md`](../legacy/source-annotations.md) only when comparing legacy behavior

## Current baseline

- CLI, daemon, typed configuration, registry lifecycle, native/command/remote ASR, text processing, diagnostics, and retained-addon integration are broadly implemented and tested.
- The real Fcitx path is live-proven for normal and command dictation, multiple GTK/Qt/Chromium/Electron/application surfaces, menus, localization, provider/model switching, notifications, focus handoff, owner loss, reload, a physical microphone, and bounded GTK4 soak. Exact scope and remaining live gaps belong in the audit and matrix.
- The Rust/Iced GUI provides Control, Resources, LLM, and Hotwords pages. It uses shared typed config/protocol/registry APIs; supports scene and LLM-provider lifecycle, configured-only scene provider selection, model and managed provider/adapter install/update, progress, cancellation, retry, guarded removal, production-adapter connectivity testing, hotword provider/path/content lifecycle with conflict-aware atomic writes and reload rollback, and non-activating owner recovery; and is installed by the checked package paths.
- Arch, Debian 12, Ubuntu 24.04, Nix, RPM-family, and Flatpak baselines are deterministic. Production publication, host-installed and multi-user lifecycle proof, broader supported-distro operations, and unrelated-machine regression remain release work.
- Command helpers use shared process-group supervision with lifecycle-wide deadlines when configured, concurrent bounded stdout/stderr, and descendant cleanup. Long-lived adapters create their runtime directory before spawn, fingerprint PID records, preserve legacy TERM/KILL timing, and treat Linux zombie-only groups as cleaned without ignoring live descendants.
- The implementation through `c507807` with synchronized GUI status documentation passes the complete deterministic `just ci` gate. The last pushed baseline with the complete remote Rust, Nix, Debian 12, and Ubuntu 24.04 matrix is `9d31f70`.

Do not maintain or quote a parity percentage. Use the evidence stages in the audit and matrix.

## Start-of-session checks

```sh
cd /workspace/fcitx-vinput-rs
git status --porcelain=v1 -b
git log -8 --oneline --decorate
```

Run the narrowest relevant check while iterating and the complete relevant tier before handoff.

## Implementation rules

- Communicate with the user in Chinese.
- Keep code, comments, test names, documentation identifiers, and commit messages in English.
- Preserve legacy service names, methods, signals, status strings, config semantics, and recognition JSON.
- Keep the retained C++ frontend thin; backend logic and the standalone GUI belong in Rust.
- Treat mock, file, session-bus, package, and temporary-HOME evidence as deterministic, not live.
- Keep real-profile changes explicit and opt-in.
- Prefer one milestone-enabling change over broad cleanup.
- Add focused regression coverage for every live-facing fix.
- Keep commits small and Conventional Commit formatted.

## Validation

Documentation-only:

```sh
git diff --check
```

Rust/core:

```sh
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

D-Bus/frontend:

```sh
just fmt-check
just test
just lint
```

Full deterministic handoff:

```sh
just ci
```

Live checks require a real desktop and follow [`live-desktop-validation.md`](live-desktop-validation.md). Record exact failures; never mark a path complete because a deterministic smoke passed.

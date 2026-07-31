# Rust GUI contract

This document defines the boundary for a future standalone management GUI. The GUI is deferred; this contract prevents a second C++ application from becoming part of the rewrite.

## Direction

- Implement the GUI in Rust as a future `vinput-gui` crate.
- Do not port or retain the legacy Qt/C++ GUI as a product component.
- Keep the existing C++ code limited to the thin Fcitx addon boundary.
- Reuse typed config, registry, protocol, and diagnostic APIs. Runtime operations must use D-Bus or shared Rust libraries, not parsed CLI text.
- Keep GUI introduction behind a separate milestone after the CLI/daemon, desktop, and release paths are stable.

## Toolkit choice

Use `iced` for the first implementation spike and as the default toolkit unless the spike finds a blocking Linux desktop issue. Use the current stable iced release when GUI work begins instead of pinning the design to an older release solely to preserve the current toolchain.

Reasons:

- it is a Rust GUI framework with a typed state/message/update/view model that fits daemon and D-Bus events;
- it supports Linux, Wayland/X11, asynchronous tasks, native windows, and software or GPU renderers;
- it avoids adding a second C++ application boundary;
- the project may raise `rust-version` when required by iced or its ecosystem, provided the new version is stable, documented, and used by CI and packaging.

The choice remains provisional because iced describes itself as experimental. Before adding `vinput-gui` to the workspace, build a small spike that proves:

1. D-Bus status subscriptions and reconnect after daemon owner changes;
2. editable model/provider/scene/device forms backed by typed values;
3. long lists, filtering, progress, cancellation, and error presentation;
4. zh_CN input, rendering, accessibility, clipboard, and desktop notifications under Wayland;
5. package size, startup time, software-renderer fallback, and Arch packaging.

## Alternatives

- `gtk4-rs` is the fallback when native Linux integration, accessibility, or input-method behavior is materially better. GUI code would still be Rust, but the application would depend on the GTK C runtime and platform development packages.
- Slint is not the initial choice because it introduces a separate UI language and licensing decision.
- egui/eframe is not the initial choice because its immediate-mode model is a weaker fit for a conventional settings and resource-management application.

## Testing rules

- Test state transitions, typed request construction, D-Bus behavior, and rendered user outcomes.
- Do not add tests that assert source declarations, widget implementation names, exact documentation wording, docstrings, or toolkit-generated source text.
- Keep screenshot tests limited to stable, user-visible layout regressions; do not use them as substitutes for interaction tests.

## Research baseline

Reviewed 2026-07-31 against the official project documentation. The current stable iced release is 0.14.0; re-check the stable release and its Rust requirement when the GUI spike begins.

- iced book and release metadata: <https://book.iced.rs/> and <https://docs.rs/iced/latest/iced/>
- gtk4-rs documentation: <https://gtk-rs.org/gtk4-rs/stable/latest/docs/gtk4/>
- Slint documentation and release metadata: <https://docs.slint.dev/latest/docs/slint/> and <https://docs.rs/slint/1.17.1/slint/>
- egui/eframe documentation: <https://docs.rs/egui/latest/egui/> and <https://docs.rs/eframe/latest/eframe/>

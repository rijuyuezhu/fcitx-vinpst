# Rust GUI contract

`vinput-gui` is the standalone Rust management application. It replaces the legacy Qt/C++ management application without expanding the retained C++ boundary beyond the thin Fcitx addon.

## Implemented baseline

The first Iced 0.14 implementation is part of the workspace and provides four legacy-aligned top-level pages:

- **Control** shows daemon status plus the validated active scene/provider, capture target, language, VAD, and output-ducking settings.
- **Resources** lists and filters typed ASR providers and scenes.
- **LLM** lists providers with redacted endpoints and lists command adapters without exposing API keys.
- **Hotwords** lists provider hotword files.

The application reads `VinputConfig` directly, validates an explicit or discovered user config, and falls back to the bundled default only when the user file is absent. Daemon state is queried through the shared `vinput-protocol` D-Bus constants and typed zbus calls to `GetStatus` and `GetRuntimeStatus`; the GUI does not parse CLI output.

`vinput-gui --check --offline` produces a redacted machine-readable snapshot without opening a window or requiring a session bus. Packaging uses this mode to verify the installed binary, typed config loading, page inventory, and secret-safe diagnostics. `--check` without `--offline` additionally probes the live daemon.

The Arch package installs the GUI binary, desktop entry, and hicolor icons. This is a runnable product baseline, but it is not yet full legacy GUI parity.

## Boundary rules

- Keep GUI code in Rust. Do not port or retain the legacy Qt/C++ GUI as a product component.
- Keep the existing C++ code limited to the thin Fcitx addon boundary.
- Reuse typed config, registry, protocol, and diagnostic APIs. Runtime operations must use D-Bus or shared Rust libraries, not parsed CLI text.
- Never display API keys, authorization headers, raw prompts, or unredacted provider credentials.
- Config mutations must use the same validation, atomic-write, backup, and active-session guards as the CLI/daemon paths.
- Long-running resource operations must expose progress, cancellation, and a final typed result; closing the window must not orphan helper processes.

## Toolkit choice

Iced 0.14 is the current implementation toolkit. The application uses its typed state/message/update/view model, asynchronous tasks, native Wayland/X11 backends, and software renderer. `gtk4-rs` remains the fallback only if live desktop validation finds a blocking accessibility, input-method, or platform-integration defect that cannot be solved in Iced.

The initial spike has proven:

1. typed D-Bus status queries and explicit refresh;
2. typed validated config loading and redacted display;
3. long-list filtering for providers and scenes;
4. Wayland/X11-capable compilation with software rendering;
5. headless CI/package checking and Arch package integration.

## Remaining parity

The next GUI slices are:

1. typed editable forms for global/audio/VAD settings and atomic save/reload;
2. provider, model, scene, LLM, adapter, and hotword lifecycle actions;
3. recording controls plus daemon owner-change subscriptions and reconnect;
4. download/install progress, cancellation, retry, and error presentation;
5. zh_CN UI localization, accessibility review, keyboard navigation, clipboard, and desktop notification validation;
6. real Wayland/X11 launch and interaction gates, startup/package-size measurements, and visual regression coverage for stable layouts.

Until those slices are complete, documentation must call the GUI a management baseline rather than a full replacement.

## Testing rules

- Test state transitions, typed request construction, D-Bus behavior, redaction, config validation, and rendered user outcomes.
- Keep a display-independent check that packaging can run without D-Bus.
- Do not add tests that assert source declarations, widget implementation names, exact documentation wording, docstrings, or toolkit-generated source text.
- Keep screenshot tests limited to stable user-visible layout regressions; do not use them as substitutes for interaction tests.

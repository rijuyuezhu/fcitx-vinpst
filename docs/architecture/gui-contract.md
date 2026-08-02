# Rust GUI contract

`vinput-gui` is the standalone Rust management application. It replaces the legacy Qt/C++ management application without expanding the retained C++ boundary beyond the thin Fcitx addon.

## Implemented baseline

The first Iced 0.14 implementation is part of the workspace and provides four legacy-aligned top-level pages:

- **Control** shows daemon status, starts/stops normal recording, and edits the active scene/provider, capture target, language, VAD, and output-ducking settings.
- **Resources** lists and filters typed ASR providers and scenes, installs or updates live-registry ASR models by ID/short ID, scans managed installed models, and removes inactive managed model directories.
- **LLM** lists providers with redacted endpoints and lists command adapters without exposing API keys.
- **Hotwords** lists provider hotword files.

The application reads `VinputConfig` directly, validates an explicit or discovered user config, and falls back to the bundled default only when the user file is absent. GUI and CLI config mutations share the `vinput-config` persistence API: validation precedes any filesystem mutation, an existing file receives an adjacent `.bak`, the replacement is written and synchronized beside the destination, and rename publishes it atomically. The GUI refuses to overwrite a file changed after loading and refuses a save while a reachable daemon is non-idle or reports an active session. A missing daemon does not block an otherwise valid offline config save; the result explicitly reports that reload was skipped.

Daemon state and actions use the shared `vinput-protocol` D-Bus constants and typed zbus calls. Status uses `GetStatus` and `GetRuntimeStatus`, save requests `ReloadAsrBackend`, and recording controls use `StartRecording`/`StopRecording`; the GUI does not parse CLI output or launch the CLI as a helper. Model installation calls the shared registry fetch/checksum/safe-extraction/atomic-materialization APIs. GUI and CLI deletion share one typed managed-root boundary that rejects the root itself, paths outside it, non-directories, and active configured local-model paths.

`vinput-gui --check --offline` produces a redacted machine-readable snapshot without opening a window or requiring a session bus. Packaging uses this mode to verify the installed binary, typed config loading, page inventory, and secret-safe diagnostics. `--check` without `--offline` additionally probes the live daemon.

The checked Arch, Debian, RPM, and Nix package paths install the GUI binary, desktop entry, and hicolor icons. This is a runnable product baseline, but it is not yet full legacy GUI parity.

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

The first mutation slice additionally proves typed form state, dirty/reset handling, first-file creation, backup-preserving replacement, external-change conflict rejection, active-session save guards, daemon reload reporting, and direct recording actions. The resource slice proves mirror fallback without credential/URL leakage, checksum-aware install/update, installed-model discovery, shared managed-root deletion, active-model refusal, and post-operation reconciliation.

## Remaining parity

The next GUI slices are:

1. provider, scene, LLM, adapter, and hotword lifecycle actions beyond active provider/scene selection, plus richer model details and selection;
2. daemon owner-change subscriptions, reconnect, and operation-state reconciliation;
3. download/install progress, cancellation, retry, and richer error presentation;
4. command-mode recording and selected-text integration;
5. zh_CN UI localization, accessibility review, keyboard navigation, clipboard, and desktop notification validation;
6. real Wayland/X11 launch and interaction gates, startup/package-size measurements, and visual regression coverage for stable layouts.

Until those slices are complete, documentation must call the GUI a management baseline rather than a full replacement.

## Testing rules

- Test state transitions, typed request construction, D-Bus behavior, redaction, config validation, and rendered user outcomes.
- Keep a display-independent check that packaging can run without D-Bus.
- Do not add tests that assert source declarations, widget implementation names, exact documentation wording, docstrings, or toolkit-generated source text.
- Keep screenshot tests limited to stable user-visible layout regressions; do not use them as substitutes for interaction tests.

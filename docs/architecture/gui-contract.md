# Rust GUI contract

`vinput-gui` is the standalone Rust management application. It replaces the legacy Qt/C++ management application without expanding the retained C++ boundary beyond the thin Fcitx addon.

## Implemented baseline

The first Iced 0.14 implementation is part of the workspace and provides four legacy-aligned top-level pages:

- **Control** shows daemon status, starts/stops normal recording, and edits the active scene/provider, capture target, language, VAD, and output-ducking settings.
- **Resources** lists and filters typed ASR providers and scenes, installs or updates live-registry ASR models by ID/short ID, scans managed installed models, and removes inactive managed model directories.
- **LLM** lists providers with redacted endpoints and lists command adapters without exposing API keys.
- **Hotwords** lists provider hotword files.

The application reads `VinputConfig` directly, validates an explicit or discovered user config, and falls back to the bundled default only when the user file is absent. GUI and CLI config mutations share the `vinput-config` persistence API: validation precedes any filesystem mutation, an existing file receives an adjacent `.bak`, the replacement is written and synchronized beside the destination, and rename publishes it atomically. The GUI refuses to overwrite a file changed after loading and refuses a save while a reachable daemon is non-idle or reports an active session. A missing daemon does not block an otherwise valid offline config save; the result explicitly reports that reload was skipped.

Daemon state and actions use the shared `vinput-protocol` D-Bus constants and typed zbus calls. Status uses `GetStatus` and `GetRuntimeStatus`, save requests `ReloadAsrBackend`, and recording controls use `StartRecording`/`StopRecording`; the GUI does not parse CLI output or launch the CLI as a helper. A serialized two-second `NameHasOwner` poll detects owner loss without activating a missing service and automatically refreshes status when the owner returns. Model installation runs in a dedicated blocking worker and uses the shared registry fetch/checksum/safe-extraction/atomic-materialization APIs. The Resources page reports catalog, byte download, checksum, extraction, metadata, and publication phases; a typed cancel control is observed during archive download, extraction, and cross-filesystem publication. Cancelling removes temporary download/extraction/publication state, preserves an existing installed model, rejects stale worker completions by operation generation, and retains the exact selector for retry. Command ASR provider and text-adapter installs use the same owned worker state: the GUI resolves a full or short registry id, refuses to replace user-defined entries, publishes the executable script under the managed data root, preserves existing environment values during update-by-reinstall, validates and atomically saves the resulting typed config, and requests daemon reload. Cancellation is cooperative through script download; once configuration commit begins the UI enters a non-cancellable finishing state so a published script is not intentionally left without its config reference. Dropping any active GUI install operation, including application shutdown, requests cooperative cancellation rather than orphaning the worker. GUI and CLI model deletion share one typed managed-root boundary that rejects the root itself, paths outside it, non-directories, and active configured local-model paths.

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

The first mutation slice additionally proves typed form state, dirty/reset handling, first-file creation, backup-preserving replacement, external-change conflict rejection, active-session save guards, daemon reload reporting, and direct recording actions. The model resource slice proves mirror fallback without credential/URL leakage, checksum-aware install/update, installed-model discovery, shared managed-root deletion, active-model refusal, and post-operation reconciliation. The operation slice proves typed phase and byte progress, cooperative cancellation against a slow HTTP body and a large archive, cleanup of partial state, preservation of an existing target before publication, exact-selector retry, stale-completion rejection, and shutdown-owned cancellation. The script resource slice proves provider/adapter registry resolution, managed executable publication, user-defined-entry refusal, preserved environment values, validated atomic config persistence, daemon reload reporting, exact-selector retry, stale-completion rejection, and shutdown-owned cancellation. The recovery slice proves non-activating owner detection, serialized refreshes, explicit unavailable state, and automatic status recovery after daemon restart.

## Remaining parity

The next GUI slices are:

1. provider/adapter removal, provider script editing, required environment-value entry, and scene/LLM/hotword lifecycle actions, plus richer model details and selection;
2. signal-driven daemon owner-change subscriptions and richer operation-state reconciliation beyond the current safe polling recovery;
3. richer model/provider/adapter metadata and error details, including explicit recovery guidance when script publication succeeds but config persistence fails;
4. command-mode recording and selected-text integration;
5. zh_CN UI localization, accessibility review, keyboard navigation, clipboard, and desktop notification validation;
6. real Wayland/X11 launch and interaction gates, startup/package-size measurements, and visual regression coverage for stable layouts.

Until those slices are complete, documentation must call the GUI a management baseline rather than a full replacement.

## Testing rules

- Test state transitions, typed request construction, D-Bus behavior, redaction, config validation, and rendered user outcomes.
- Keep a display-independent check that packaging can run without D-Bus.
- Do not add tests that assert source declarations, widget implementation names, exact documentation wording, docstrings, or toolkit-generated source text.
- Keep screenshot tests limited to stable user-visible layout regressions; do not use them as substitutes for interaction tests.

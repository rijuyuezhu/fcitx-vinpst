# Rust GUI contract

`vinput-gui` is the standalone Rust management application. It replaces the legacy Qt/C++ management application without expanding the retained C++ boundary beyond the thin Fcitx addon.

## Implemented baseline

The first Iced 0.14 implementation is part of the workspace and provides four legacy-aligned top-level pages:

- **Control** shows daemon status, starts/stops normal recording, and edits the active scene/provider, capture target, language, VAD, and output-ducking settings.
- **Resources** lists and filters typed ASR providers and scenes, creates and edits validated scene definitions, selects the active scene, removes only inactive scenes, installs or updates live-registry ASR models by ID/short ID, scans managed installed models, removes inactive managed model directories, and exposes selectable secret-safe model/provider detail panels.
- **LLM** lists providers with redacted endpoints, adds and edits typed OpenAI-compatible provider definitions through secure API-key inputs, removes only providers whose deletion leaves the full config valid, lists command adapters without exposing API keys, and exposes selectable provider/adapter details built only from redacted typed summaries and configuration counts.
- **Hotwords** lists provider hotword files.

The application reads `VinputConfig` directly, validates an explicit or discovered user config, and falls back to the bundled default only when the user file is absent. GUI and CLI config mutations share the `vinput-config` persistence API: validation precedes any filesystem mutation, an existing file receives an adjacent `.bak`, the replacement is written and synchronized beside the destination, and rename publishes it atomically. The GUI refuses to overwrite a file changed after loading and refuses a save while a reachable daemon is non-idle or reports an active session. A missing daemon does not block an otherwise valid offline config save; the result explicitly reports that reload was skipped.

Daemon state and actions use the shared `vinput-protocol` D-Bus constants and typed zbus calls. Status uses `GetStatus` and `GetRuntimeStatus`, save requests `ReloadAsrBackend`, and recording controls use `StartRecording`/`StopRecording`; the GUI does not parse CLI output or launch the CLI as a helper. A long-lived async zbus subscription installs a service-name-filtered `NameOwnerChanged` match before sampling `NameHasOwner`, so owner loss and recovery reconcile immediately without activating a missing service. Every daemon snapshot and fallback query carries an operation generation; owner transitions invalidate older generations so stale snapshots cannot restore a departed owner. If the signal stream is unavailable, the GUI reports degraded monitoring, reconnects the stream, performs one immediate non-activating query, and enables a serialized 30-second non-activating fallback only while degraded. Model installation runs in a dedicated blocking worker and uses the shared registry fetch/checksum/safe-extraction/atomic-materialization APIs. The Resources page reports catalog, byte download, checksum, extraction, metadata, and publication phases; a typed cancel control is observed during archive download, extraction, and cross-filesystem publication. Cancelling removes temporary download/extraction/publication state, preserves an existing installed model, rejects stale worker completions by operation generation, and retains the exact selector for retry. Command ASR provider and text-adapter installs use the same owned worker state: the GUI first resolves a full or short registry id into a typed installation plan, refuses to replace user-defined entries, and collects every registry-declared environment value in secure inputs before any script download. Required values block confirmation while optional values may remain empty. Managed updates prefill existing declared values, allow explicit replacement, and preserve unrelated environment entries. The confirmed transaction publishes the executable script under the managed data root, validates and atomically saves the resulting typed config, and requests daemon reload. If publication succeeds but config persistence fails, the GUI keeps a typed recovery state with the published path and error, blocks conflicting resource operations, and offers config reload, config-only retry, or explicit dismissal that keeps the script. Recovery revalidates the current config, registry environment contract, and existing regular-file path and never downloads the script again. Exact managed command-provider rows also expose an Edit script action. The GUI launches the same shared typed provider-script plan used by the CLI, preserves the legacy explicit/`VINPUT_PROVIDER_EDITOR`/`VISUAL`/`EDITOR`/`vi` priority, executes direct argv without a shell, and verifies that the resolved existing regular file is exactly the managed path before opening it. Cancellation is cooperative through script download; once configuration commit begins the UI enters a non-cancellable finishing state so a published script is not intentionally left without its config reference. Dropping any active GUI install operation, including application shutdown, requests cooperative cancellation rather than orphaning the worker. GUI and CLI model deletion share one typed managed-root boundary that rejects the root itself, paths outside it, non-directories, and active configured local-model paths.

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

The first mutation slice additionally proves typed form state, dirty/reset handling, first-file creation, backup-preserving replacement, external-change conflict rejection, active-session save guards, daemon reload reporting, and direct recording actions. The scene lifecycle slice proves typed add/edit forms, immutable ids during edit, trimmed optional values, numeric parsing, duplicate and unknown-provider rejection, full-config validation before mutation, active-scene selection, active-scene removal refusal, inactive removal, shared atomic persistence, daemon reload reporting, and form invalidation after config replacement or page changes. The model resource slice proves mirror fallback without credential/URL leakage, checksum-aware install/update, installed-model discovery, shared managed-root deletion, active-model refusal, and post-operation reconciliation. The operation slice proves typed phase and byte progress, cooperative cancellation against a slow HTTP body and a large archive, cleanup of partial state, preservation of an existing target before publication, exact-selector retry, stale-completion rejection, and shutdown-owned cancellation. The script resource slice proves provider/adapter registry resolution, managed executable publication, user-defined-entry refusal, secure required/optional environment entry before download, managed-update prefill and replacement, preservation of unrelated environment values, redacted message/state diagnostics, validated atomic config persistence, daemon reload reporting, exact-selector retry, stale preparation/install/recovery rejection, shutdown-owned cancellation, exact managed provider-script editing through the shared typed editor boundary, and config-only recovery after successful publication without re-download. The resource-detail slice proves selectable model/ASR/LLM/adapter summaries from typed metadata, redacted URLs, configured-state/count fields, stale-selection handling, and structural exclusion of credentials, command arguments, environment values, working directories, and raw backend JSON. Its removal path recognizes only exact managed-root script arguments, refuses active providers and user-defined entries, validates and atomically commits config removal before deleting the now-unreferenced script, and reports cleanup failures without misrepresenting the committed config state. The daemon reconciliation slice proves a filtered real private-bus owner acquire/loss stream, registration-before-sampling race avoidance, generation-based stale snapshot rejection, immediate explicit unavailable state, automatic status recovery after owner return, serialized degraded fallback queries, and process-clean private-bus test ownership.

## Remaining parity

The next GUI slices are:

1. LLM provider connectivity testing and selection flows, hotword lifecycle actions, and remaining model/provider/adapter selection flows;
2. additional resource-specific error taxonomy beyond the completed published-script recovery path;
3. command-mode recording and selected-text integration;
4. zh_CN UI localization, accessibility review, keyboard navigation, clipboard, and desktop notification validation;
5. real Wayland/X11 launch and interaction gates, startup/package-size measurements, and visual regression coverage for stable layouts.

Until those slices are complete, documentation must call the GUI a management baseline rather than a full replacement.

## Testing rules

- Test state transitions, typed request construction, D-Bus behavior, redaction, config validation, and rendered user outcomes.
- Keep a display-independent check that packaging can run without D-Bus.
- Do not add tests that assert source declarations, widget implementation names, exact documentation wording, docstrings, or toolkit-generated source text.
- Keep screenshot tests limited to stable user-visible layout regressions; do not use them as substitutes for interaction tests.

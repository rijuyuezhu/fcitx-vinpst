# D-Bus service contract

`vinput-daemon` exposes the legacy daemon D-Bus ABI while the backend implementation is rewritten in Rust. The service must remain compatible with the existing C++ Fcitx5 frontend.

## What exists now

- `crates/vinput-protocol/src/dbus.rs` owns shared wire constants, method names, and signal names.
- `crates/vinput-daemon/src/dbus_service.rs` wraps `RuntimeState` in a `zbus` interface named `org.fcitx.Vinput.Service`.
- `vinput-daemon --dbus` registers the legacy bus/object/interface on the session bus.
- `crates/vinput-daemon/tests/dbus_integration.rs` exercises real bus calls under `dbus-run-session`.
- The default runtime still uses deterministic mock ASR/text/audio seams, while explicit configured paths can exercise configured command ASR/text seams. This is not full backend parity.

## Wire names to preserve

- Bus name: `org.fcitx.Vinput`
- Object path: `/org/fcitx/Vinput`
- Service interface: `org.fcitx.Vinput.Service`
- Fcitx bus: `org.fcitx.Fcitx5`
- Frontend notifier object: `/org/fcitx/Fcitx5/Vinput`
- Frontend notifier interface: `org.fcitx.Fcitx5.Vinput1`

## Service methods

Preserve these legacy method names and payload shapes:

- `StartRecording`
- `StartCommandRecording`
- `StopRecording`
- `GetStatus`
- `GetAsrBackendState`
- `ReloadAsrBackend`
- `StartAdapter`
- `StopAdapter`

`GetTextAdapterState` and `GetRuntimeStatus` are Rust diagnostic extensions. `GetSceneState() -> sa(ss)`, `SetActiveScene(s) -> b`, `GetCaptureDevice() -> s`, `SetCaptureDevice(s) -> b`, `GetAsrMenuState() -> sssbsa(sss)`, `SetActiveAsrProvider(s) -> b`, `GetAsrTargetMenuState() -> ssssbsa(ssss)`, `SetActiveAsrTarget(ss) -> b`, and `GetAsrDisplayMenuState() -> ssssbsa(sssss)` are Rust configuration extensions used by retained frontends and future Rust management surfaces. They can remain available, but they are not part of the original C++ daemon vtable and should be documented as extensions whenever listed.

## Status strings

Preserve these legacy status strings and their lowercase wire format:

- `idle`
- `recording`
- `inferring`
- `postprocessing`
- `error`

## Signals

Preserve these signal names and payload shapes:

- `RecognitionResult(s)`
- `RecognitionPartial(s)`
- `StatusChanged(s)`
- `DaemonNotification(ssss)`, carrying code, subject, detail, and raw message.

The service keeps an owned `SignalEmitter` bound to the connection hosting the object so background workers can emit without borrowing a method call. A failed ASR backend preparation emits code `asr_backend_reload_failed` only for the current reload generation; stale generations are discarded without notification. Its raw message exactly matches `GetAsrBackendState.last_error`, while the previously effective backend remains active.

`StartRecording`, `StartCommandRecording`, and `StopRecording` share one asynchronous recording transaction lock. The lock is held from the runtime state transition through every `StatusChanged`, `RecognitionPartial`, and `RecognitionResult` emission and the synchronous method reply. Concurrent push-to-talk operations therefore cannot interleave an old stop result or final `idle` status after a newer start has begun. The Rust runtime has no legacy deferred audio-stop worker, and its audio start/stop calls already run under one runtime mutex; transaction serialization is the Rust equivalent of the upstream stop/start race hardening rather than a copy of the legacy cleanup flags.

`StopRecording` has a real two-stage runtime boundary. Capture shutdown, audio delivery, ASR finalization, and raw payload extraction run under `inferring`. The runtime then owns a pending stop object and enters `postprocessing`; the service emits `StatusChanged("postprocessing")` before calling the scene text processor. Only after text finishing succeeds does it emit the final partial/result and `idle`. The session-bus contract is therefore `recording -> inferring -> postprocessing -> idle`. ASR failure, text-processing failure, or failure to emit the intermediate status cancels the retained session and returns the runtime to `idle`; final `idle` emission is still attempted when partial/result signal emission fails.

The retained C++ addon listens through the Fcitx D-Bus module rather than adding another thread or event loop. It also uses Fcitx `ServiceWatcher`, whose initial `GetNameOwner` query closes the registration race before subsequent owner changes are delivered. Daemon startup absence remains silent. Every ownership transition invalidates the stale synchronous client; when the owner disappears during an addon-owned recording or status-only recovery view, the frontend ends or clears that state and presents `Voice input daemon is unavailable.` through the normal localized error path. Before issuing a local start, the addon queries `GetStatus`; an explicit `idle` permits the normal start path, an externally started `recording` reached by the normal trigger is adopted and stopped through the existing synchronous result path, and `recording` from the command trigger plus `inferring` or `postprocessing` are rendered as tracked status-only preedit. That preedit is cleared by idle/error status or owner loss. The cross-client session-bus smoke starts recording through a separate client and verifies the addon normal trigger returns the daemon to idle without another Start. The same monitor consumes `StatusChanged(s)` and `RecognitionPartial(s)`: partial text takes precedence over localized status fallback for addon-owned sessions, idle/error status clears frontend state, and final commit remains on the synchronous stop reply. Error-like notification payloads reset frontend recording/timer state before presentation; raw informational payloads are presented without interrupting recording. A real session-bus smoke sends and decodes all three signal shapes through the Fcitx bus implementation. Current daemon notification emission is intentionally limited to background ASR reload failures; other asynchronous notification categories remain future work.

CLI owner diagnostics deliberately run after the first successful service method call. Proxy construction alone does not activate a D-Bus service, so collecting `GetNameOwner` earlier made the initial `vinput daemon status` response report `owner: null` even though that same query started the daemon. `scripts/tests/install/run-user-ime-activation-owner-smoke.sh` covers the corrected first-query behavior against a generated per-user activation service.

`vinput daemon status` also derives a non-mutating package handoff diagnostic from the D-Bus owner PID and `/proc/<pid>/exe`. It compares the owner executable with the `vinput-daemon` sibling of the running CLI, strips Linux's ` (deleted)` suffix for path comparison, reports whether the old inode has been unlinked, and recommends `vinput daemon handoff` for a deleted owner inode or a concrete executable-path mismatch. `scripts/tests/daemon/run-daemon-handoff-diagnostics-smoke.sh` proves both mismatch and replaced-inode cases on private session buses.

`vinput daemon handoff` is the explicit mutation boundary. It reads the same status snapshot and does nothing when the owner is current. A verified systemd-owned stale daemon is handled through user-service `MainPID` inspection, `daemon-reload`, restart, and fresh-owner verification. A non-systemd owner may be terminated only after the CLI proves that it is idle, has no active recording session, belongs to the current user, still has the expected daemon identity, and was not adopted by systemd; the CLI then reloads D-Bus activation and verifies the replacement owner. A successful control command alone is never sufficient: verification requires the new executable to match the current installation and not carry Linux's ` (deleted)` suffix. `--dry-run` contacts neither D-Bus nor external control tools. `scripts/tests/daemon/run-daemon-handoff-smoke.sh` proves current-owner no-op, stale systemd restart, guarded direct-owner replacement, post-checks, and failure preservation on private session buses.

When `/.flatpak-info` exists, CLI daemon-management commands treat service control and direct-owner signaling as host operations. The shared command builder prefixes `systemctl`, `journalctl`, and guarded `kill` invocations with `flatpak-spawn --host`; daemon logs use the legacy Flatpak journal filter `journalctl --user -t flatpak --grep vinput` rather than a host unit selector. JSON and text diagnostics preserve the logical tool separately from the host wrapper and expose the final argv. `vinput doctor` parses the Flatpak `[Context]` section and reports missing `pipewire`, `xdg-config/systemd`, and `xdg-cache` grants. `vinput daemon install-service` reads the packaged user-service template, rewrites `ExecStart` to `flatpak run --command=/app/addons/Vinput/bin/vinput-daemon org.fcitx.Fcitx5` while preserving daemon arguments, adds the legacy-compatible `ExecStop`, atomically installs the user unit, and reloads the host user manager; dry-run exposes the exact rendered unit without writing. `VINPUT_FLATPAK_INFO_PATH`, `VINPUT_FLATPAK_SPAWN`, `VINPUT_FLATPAK_APP_ID`, and `VINPUT_FLATPAK_ADDON_ROOT` are deterministic test/tool overrides. A real Flatpak bundle/build/install gate remains packaging work; the runtime control, permission-diagnostic, and unit-rewrite contracts are deterministic.

## Test coverage

Unit tests call the service facade directly and assert runtime transitions and JSON payloads. The optional integration test runs through a real session bus:

```sh
dbus-run-session -- cargo test -p vinput-daemon --features dbus-integration --test dbus_integration
```

That test starts the Rust service, builds a `zbus::Proxy`, calls legacy methods by their exact wire names, and parses returned recognition payload JSON.

`vinput-cli protocol` serializes method and signal names from `vinput-protocol`, so smoke commands and service tests read the same member list.

## Compatibility status

The Rust service pins these legacy-visible behaviors with unit and D-Bus integration tests:

- operation failures use the legacy error name `org.fcitx.Vinput.Error.OperationFailed`;
- `GetAsrBackendState` combines the configured target provider/model with the descriptor of the backend that is actually effective in the runtime; it must not report a merely constructible configured backend as already active;
- `ReloadAsrBackend` re-reads the daemon config file when an explicit startup path exists, updates the ASR/default-language target, and queues the configured backend through the prepare-before-swap path rather than refreshing metadata only;
- one non-blocking reload worker performs backend construction and warmup outside the runtime mutex, while `reload_in_progress` covers both queued and physical preparation;
- `ReloadAsrBackend` returns success while recording/inferring, keeps the request pending until idle, coalesces repeated requests by generation, and discards stale prepared generations;
- failed background or deferred reloads keep the previously working backend and surface the error in diagnostics; current-generation background preparation failures also emit `DaemonNotification` with the same message;
- configured daemon startup failures leave the service idle with no effective ASR backend, preserve the target/error in `GetAsrBackendState`, reject recording without mock output, and remain recoverable through `ReloadAsrBackend`;
- `GetSceneState` returns the active scene plus typed id/label pairs without making the C++ frontend parse daemon config JSON;
- `SetActiveScene` is idle-only, rejects unknown scenes with the legacy operation error, updates runtime state, and atomically persists the explicit or automatically discovered daemon config when one exists; its boolean reply distinguishes persistent and runtime-only selection;
- `GetCaptureDevice` returns the normalized config value used by the next recording; `SetCaptureDevice` is idle-only, validates through the typed `CaptureTarget` parser, atomically persists the explicit or discovered config when available, and changes the target used by the same daemon/recorder on the next start without restarting the owner;
- `GetAsrMenuState` exposes configured target, actual effective provider/model, reload progress, the last reload error, and typed provider id/kind/model rows without making C++ parse config JSON;
- `SetActiveAsrProvider` rejects unknown providers, atomically persists the explicit or automatically discovered daemon config when one exists, and queues the selected provider through the same non-blocking prepare-before-swap worker; its boolean reply distinguishes persistent and runtime-only selection;
- `GetAsrTargetMenuState` scans the configured model root outside the runtime mutex, combines flat Rust and legacy engine/model install layouts with configured provider rows, and preserves its original stable item-id/concrete-value ABI;
- `GetAsrDisplayMenuState` adds a display-title field without changing that older method, uses the daemon locale preference order, reads only installed metadata, and falls back to the stable registry/layout id for old or unmanaged models;
- `SetActiveAsrTarget` accepts only a configured model or a path returned by installed-model discovery, atomically persists provider/model selection, and queues the same background prepare-before-swap reload;
- `StopRecording` exposes the legacy `postprocessing` phase before scene text processing, preserves the synchronous final payload reply, and returns to idle on every begin/finish/abort path;
- status strings and core legacy method/signal names remain centralized in `vinput-protocol`.

## Compatibility rule

The frontend should not need to know whether the daemon is C++ or Rust. Any service method rename, object path change, status string change, signal shape change, recognition payload shape change, or D-Bus error behavior change must be pinned by compatibility tests before it reaches runtime code.

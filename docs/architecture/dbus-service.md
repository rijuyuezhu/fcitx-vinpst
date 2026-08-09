# D-Bus service contract

`vinpst-daemon` exposes the Vinpst D-Bus ABI consumed by the retained C++ Fcitx5 frontend, the CLI, and the Rust GUI. All names use the canonical Vinpst identity; the service does not export upstream or old-name aliases.

## What exists now

- `crates/vinpst-protocol/src/dbus.rs` owns shared wire constants, method names, and signal names.
- `crates/vinpst-daemon/src/dbus_service.rs` wraps `RuntimeState` in a `zbus` interface named `org.fcitx.Vinpst.Service`.
- Direct `vinpst-daemon` startup registers the canonical Vinpst bus/object/interface, loads configured backends, and uses PipeWire when the packaged `pipewire-backend` feature is available. `--no-asr` preserves that service mode while disabling ASR for the lifetime of the process, including later reload requests.
- Explicit `--dbus`, `--configured-backends`, audio/file-input switches, executable-replacement watching, and diagnostic subcommands remain internal/package test seams and are hidden from normal daemon help.
- The default CMake-generated activation service invokes the daemon without `--dbus`, so source installs use the same direct-start service defaults as upstream. Distribution packages may still pass explicit configured-backend/PipeWire arguments to pin their build/runtime contract.
- `crates/vinpst-daemon/tests/dbus_integration.rs` exercises real bus calls under `dbus-run-session`.
- Explicit test modes keep deterministic mock defaults unless they request configured backends. This is separate from normal direct service startup.

## Current wire names

- Bus name: `org.fcitx.Vinpst`
- Object path: `/org/fcitx/Vinpst`
- Service interface: `org.fcitx.Vinpst.Service`
- Fcitx bus: `org.fcitx.Fcitx5`
- Frontend notifier object: `/org/fcitx/Fcitx5/Vinpst`
- Frontend notifier interface: `org.fcitx.Fcitx5.Vinpst1`

The frozen names are the same shapes under the independent `Vinput` identity (`org.fcitx.Vinput`, `/org/fcitx/Vinput`, `org.fcitx.Vinput.Service`, and `org.fcitx.Vinput.Error.OperationFailed`). Vinpst intentionally substitutes only its product namespace and does not export aliases. The frozen shared header also declares `Notify`, but that member belongs to the separate Fcitx frontend-notifier interface rather than the daemon service vtable; current protocol metadata makes the same distinction by excluding `Notify` from `LEGACY_SERVICE_METHODS`.

The upstream daemon requests its well-known name with replacement allowed. Vinpst deliberately does not preserve that ownership policy: the service requests the name with `DoNotQueue` only, so an accidental second daemon cannot replace the current owner or wait to take ownership later. Upgrade/removal replacement must go through the guarded handoff path, which verifies owner identity, UID, process state, active-session state, and the replacement executable before and after termination/restart. A real session-bus integration test pins `NameTaken` for a second service while the first owner remains unchanged.

## Service methods

Keep these Vinpst method names and payload shapes synchronized across the protocol crate and all current clients:

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

Keep these Vinpst status strings and their lowercase wire format synchronized:

- `idle`
- `recording`
- `inferring`
- `postprocessing`
- `error`

`ServiceStatus::parse_wire` is intentionally strict for Rust-side consumers: an unknown daemon status is treated as protocol drift rather than silently interpreted as idle. `ServiceStatus::parse_legacy_wire` exposes the frozen C++ `StringToStatus` behavior separately and maps every unknown value to `idle`. This preserves the frozen compatibility helper without weakening current internal validation.

## Signals

Keep these signal names and payload shapes synchronized:

- `RecognitionResult(s)`
- `RecognitionPartial(s)`
- `StatusChanged(s)`
- `DaemonNotification(ssss)`, carrying code, subject, detail, and raw message.

The service keeps an owned `SignalEmitter` bound to the connection hosting the object so background workers can emit without borrowing a method call. A failed ASR backend preparation emits code `asr_backend_reload_failed` only for the current reload generation; stale generations are discarded without notification. Its raw message exactly matches `GetAsrBackendState.last_error`, while the previously effective backend remains active.

`StartRecording`, `StartCommandRecording`, and `StopRecording` share one asynchronous recording transaction lock. The lock is held from the runtime state transition through every `StatusChanged`, `RecognitionPartial`, and `RecognitionResult` emission and the synchronous method reply. Concurrent push-to-talk operations therefore cannot interleave an old stop result or final `idle` status after a newer start has begun. The Rust runtime has no legacy deferred audio-stop worker, and its audio start/stop calls already run under one runtime mutex; transaction serialization is the Rust equivalent of the upstream stop/start race hardening rather than a copy of the legacy cleanup flags.

`StopRecording` has a real capture gate plus two-stage inference/postprocessing boundary. Capture shutdown happens first while the runtime is still in `recording`. A capture shorter than 8000 raw signed-16 samples cancels the recognition session, transitions directly `recording -> idle`, returns the legacy empty-string method payload, and emits neither `inferring`/`postprocessing` nor `RecognitionResult`. At 8000 samples or more, the service enters `inferring`, completes audio delivery, ASR finalization, and raw payload extraction. If ASR completes without FinalText or Error, the empty recognition skips scene text processing and transitions directly `inferring -> idle` while still emitting the empty `RecognitionResult` payload. Otherwise it enters `postprocessing` before calling the scene text processor. Only after text finishing succeeds does it emit the final result and `idle`, so the normal recognized path remains `recording -> inferring -> postprocessing -> idle`. While recording, non-empty streaming `PartialText` and early `FinalText` events are both projected as `RecognitionPartial`, matching upstream; an early final is retained internally for the eventual result but is not emitted again at stop. ASR failure, text-processing failure, or failure to emit the intermediate status cancels the retained session and returns the runtime to `idle`; final `idle` emission is still attempted when partial/result signal emission fails.

A validated config with an empty active ASR provider is a normal disabled state. `ReloadAsrBackend` clears the effective backend without reporting a reload failure, `GetAsrBackendState` reports empty target/effective ids and empty `last_error`, and new recording requests fail because no ASR backend is ready. This differs from process-level `--no-asr`: Vinpst intentionally keeps that command-line disable reason visible in `last_error` across reloads, while frozen upstream prints the reason to stderr and leaves the legacy state tuple error field empty.

Live ASR `Error` events use the frozen `common/dbus/error_info.*` taxonomy. A streaming error observed while recording is drained in event order and emitted immediately as `DaemonNotification(code, subject, detail, raw_message)`; the session still remembers its fatal state, so stop produces no fabricated text and does not re-emit the already-drained error. An error that first appears during `StopRecording` emits the same structured notification before the method returns its existing operation failure. Duplicate-start busy errors are likewise classified through the frozen nested start-recording rule and emit `daemon_busy`. Private-session D-Bus tests prove all three cases and a bounded no-duplicate interval.

The retained C++ addon listens through the Fcitx D-Bus module rather than adding another thread or event loop. It also uses Fcitx `ServiceWatcher`, whose initial `GetNameOwner` query closes the registration race before subsequent owner changes are delivered. Daemon startup absence remains silent. Every ownership transition invalidates the stale blocking status/menu client; when the owner disappears during an addon-owned recording or status-only recovery view, the frontend ends or clears that state and presents `Voice input daemon is unavailable.` through the normal localized error path. Like frozen upstream, every key event remembers the most recent live InputContext. An unsolicited external `recording`, `inferring`, or `postprocessing` status therefore adopts a normal frontend session onto that remembered context even when the user never pressed a Vinpst trigger; subsequent partials and `RecognitionResult` are rendered/committed there. Before issuing a local start, the addon still queries `GetStatus`; an explicit `idle` permits the normal start path, an externally started `recording` reached by the normal trigger is adopted and stopped through the existing asynchronous Stop path, and `recording` from the command trigger plus `inferring` or `postprocessing` are rendered as tracked status-only preedit. Latency-sensitive Start/Stop methods are dispatched with the Fcitx event-loop `callAsync` boundary and the frozen five-second deadline; successful Start dispatch publishes its preedit immediately, while its later reply only confirms transport health. The same monitor consumes `StatusChanged(s)`, `RecognitionPartial(s)`, and `RecognitionResult(s)`: partial text takes precedence over localized status fallback, and the final result signal uses a dedicated Rust completion path independent of any pending Stop call. That distinction is required for externally started sessions, which have no local Stop operation. Pending Start/Stop call slots outlive frontend session reset and are reclaimed by their own callbacks, so result/idle signals may safely precede the Stop reply. Connect, method, or async transport failure activates the frozen 1.5-second daemon-sync cooldown; successful calls and owner recovery clear it. Error-like notification payloads reset frontend recording/timer state before presentation; raw informational payloads are presented without interrupting recording. Private-session and native InputContext smokes cover cross-client takeover, command start/stop, unsolicited external status/partial/result following, final result delivery, and an event-loop timer firing after Stop dispatch but before the final result. Daemon emission now covers current-generation reload failures, start-time classified recording failures, live streaming ASR errors, stop-time ASR errors, and text postprocessing failures through one structured classifier. Frozen `daemon_start_failed` / `daemon_restart_failed` codes are generated by the separate legacy CLI systemd client rather than this daemon classifier and remain assigned to the pending CLI-service audit.

CLI owner diagnostics deliberately run after the first successful service method call. Proxy construction alone does not activate a D-Bus service, so collecting `GetNameOwner` earlier made the initial `vinpst daemon status` response report `owner: null` even though that same query started the daemon. `scripts/tests/install/run-user-ime-activation-owner-smoke.sh` covers the corrected first-query behavior against a generated per-user activation service.

`vinpst daemon status` also derives a non-mutating package handoff diagnostic from the D-Bus owner PID and `/proc/<pid>/exe`. It compares the owner executable with the `vinpst-daemon` sibling of the running CLI, strips Linux's ` (deleted)` suffix for path comparison, reports whether the old inode has been unlinked, and recommends `vinpst daemon handoff` for a deleted owner inode or a concrete executable-path mismatch. `scripts/tests/daemon/run-daemon-handoff-diagnostics-smoke.sh` proves both mismatch and replaced-inode cases on private session buses.

`vinpst daemon handoff` is the explicit mutation boundary. It reads the same status snapshot and does nothing when the owner is current. A verified systemd-owned stale daemon is handled through user-service `MainPID` inspection, `daemon-reload`, restart, and fresh-owner verification. A non-systemd owner may be terminated only after the CLI proves that it is idle, has no active recording session, belongs to the current user, still has the expected daemon identity, and was not adopted by systemd; the CLI then reloads D-Bus activation and verifies the replacement owner. A successful control command alone is never sufficient: verification requires the new executable to match the current installation and not carry Linux's ` (deleted)` suffix. `--dry-run` contacts neither D-Bus nor external control tools. `scripts/tests/daemon/run-daemon-handoff-smoke.sh` proves current-owner no-op, stale systemd restart, guarded direct-owner replacement, post-checks, and failure preservation on private session buses.

When `/.flatpak-info` exists, CLI daemon-management commands treat service control and direct-owner signaling as host operations. The shared command builder prefixes `systemctl`, `journalctl`, and guarded `kill` invocations with `flatpak-spawn --host`; daemon logs use the legacy Flatpak journal filter `journalctl --user -t flatpak --grep vinpst` rather than a host unit selector. JSON and text diagnostics preserve the logical tool separately from the host wrapper and expose the final argv. `vinpst doctor` parses the Flatpak `[Context]` section and reports missing `pipewire`, `xdg-config/systemd`, and `xdg-cache` grants. `vinpst daemon install-service` reads the packaged user-service template, rewrites `ExecStart` to `flatpak run --command=/app/addons/Vinpst/bin/vinpst-daemon org.fcitx.Fcitx5` while preserving daemon arguments, adds the checked `ExecStop`, atomically installs the user unit, and reloads the host user manager; dry-run exposes the exact rendered unit without writing. `VINPST_FLATPAK_INFO_PATH`, `VINPST_FLATPAK_SPAWN`, `VINPST_FLATPAK_APP_ID`, and `VINPST_FLATPAK_ADDON_ROOT` are deterministic test/tool overrides. The checked Flatpak extension now has a real build/install/update/bundle/remove transaction gate; live desktop-session proof of host-systemd control, PipeWire recording, and Fcitx addon loading remains.

## Test coverage

Unit tests call the service facade directly and assert runtime transitions and JSON payloads. The optional integration test runs through a real session bus:

```sh
dbus-run-session -- cargo test -p vinpst-daemon --features dbus-integration --test dbus_integration
```

That test starts the Rust service, builds a `zbus::Proxy`, calls the current methods by their exact wire names, and parses returned recognition payload JSON.

`vinpst-cli protocol` serializes method and signal names from `vinpst-protocol`, so smoke commands and service tests read the same member list.

## Contract status

The Rust service pins these current Vinpst behaviors with unit and D-Bus integration tests:

- operation failures use the Vinpst error name `org.fcitx.Vinpst.Error.OperationFailed`;
- the eight legacy method signatures and four legacy signal signatures are kept byte-for-byte compatible after applying the independent Vinpst bus/interface identity; live introspection pins `GetAsrBackendState` as the legacy `sssssbbas` tuple rather than a JSON transport;
- direct `vinpst-daemon` startup uses service defaults, while `--no-asr` keeps D-Bus/config/text behavior active and permanently blocks ASR construction/reload with the legacy-compatible reason `ASR disabled by command line.`;
- a second daemon cannot replace or queue behind the current owner; guarded handoff is the only supported replacement boundary;
- `GetAsrBackendState` keeps the frozen eight-field `sssssbbas` shape and the same requested-backend classification order. Vinpst reports the model configured on the actual target/effective provider. Frozen upstream instead derives both model-id fields through `ResolvePreferredLocalModel`, so an active command provider can expose an unrelated local model id. The Vinpst projection is intentionally more direct for command/remote providers while remaining self-consistent for `Applied`, `ConfigSaved`, and reload-failure classification;
- `ReloadAsrBackend` re-reads the daemon config file when an explicit startup path exists, updates the ASR/default-language target, and queues the configured backend through the prepare-before-swap path rather than refreshing metadata only;
- one non-blocking reload worker performs backend construction and warmup outside the runtime mutex, while `reload_in_progress` covers both queued and physical preparation;
- `ReloadAsrBackend` returns success while recording/inferring, keeps the request pending until idle, coalesces repeated requests by generation, and discards stale prepared generations;
- failed background or deferred reloads keep the previously working backend and surface the error in diagnostics; current-generation background preparation failures also emit `DaemonNotification` with the same message;
- configured daemon startup failures leave the service idle with no effective ASR backend, preserve the target/error in `GetAsrBackendState`, reject recording without mock output, and remain recoverable through `ReloadAsrBackend`;
- `GetSceneState` returns the active scene plus typed id/label pairs without making the C++ frontend parse daemon config JSON;
- `SetActiveScene` is idle-only, rejects unknown scenes with the Vinpst operation error, updates runtime state, and atomically persists the explicit or automatically discovered daemon config when one exists; its boolean reply distinguishes persistent and runtime-only selection;
- `GetCaptureDevice` returns the normalized config value used by the next recording; `SetCaptureDevice` is idle-only, validates through the typed `CaptureTarget` parser, atomically persists the explicit or discovered config when available, and changes the target used by the same daemon/recorder on the next start without restarting the owner;
- `GetAsrMenuState` exposes configured target, actual effective provider/model, reload progress, the last reload error, and typed provider id/kind/model rows without making C++ parse config JSON;
- `SetActiveAsrProvider` rejects unknown providers, atomically persists the explicit or automatically discovered daemon config when one exists, and queues the selected provider through the same non-blocking prepare-before-swap worker; its boolean reply distinguishes persistent and runtime-only selection;
- `GetAsrTargetMenuState` scans the configured model root outside the runtime mutex, combines supported installed-model layouts with configured provider rows, and preserves its current item-id/concrete-value ABI within the Vinpst client set;
- `GetAsrDisplayMenuState` adds a display-title field without changing that older method, uses the daemon locale preference order, reads only installed metadata, and falls back to the stable registry/layout id for old or unmanaged models;
- `SetActiveAsrTarget` accepts only a configured model or a path returned by installed-model discovery, atomically persists provider/model selection, and queues the same background prepare-before-swap reload;
- `StopRecording` exposes the `postprocessing` phase before scene text processing, preserves the synchronous final payload reply, and returns to idle on every begin/finish/abort path;
- status strings and core method/signal names remain centralized in `vinpst-protocol`.

## Change rule

Every current Vinpst client must agree on the same service contract. Before 0.1.0, a method, path, status, signal, payload, or error may change when it improves the product, but the protocol crate, daemon, retained frontend, CLI/GUI clients, activation metadata, tests, and documentation must change atomically. Do not retain aliases solely for unreleased internal compatibility.

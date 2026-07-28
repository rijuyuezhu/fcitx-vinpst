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

`GetTextAdapterState` and `GetRuntimeStatus` are Rust diagnostic extensions. They can remain available, but they are not part of the original C++ daemon vtable and should be documented as extensions whenever listed.

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
- failed background or deferred reloads keep the previously working backend and surface the error in diagnostics;
- status strings and core legacy method/signal names remain centralized in `vinput-protocol`.

A real legacy `postprocessing` runtime phase is still not wired; current text finishing runs synchronously inside stop handling. Status ordering must stay covered by tests when that phase becomes real.

## Compatibility rule

The frontend should not need to know whether the daemon is C++ or Rust. Any service method rename, object path change, status string change, signal shape change, recognition payload shape change, or D-Bus error behavior change must be pinned by compatibility tests before it reaches runtime code.

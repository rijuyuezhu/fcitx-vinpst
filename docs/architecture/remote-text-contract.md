# Remote text service contract

The legacy remote text service combines two clients around one shared text buffer:

- a browser/input client on `/ws` that authenticates, reports `text_update`, and requests `finalize`;
- a loopback-only OpenAI Realtime-compatible output client on `/v1/realtime` that receives committed transcription events.

The Rust rewrite keeps protocol behavior separate from the network runtime. `vinput-daemon::remote` implements the deterministic settings/protocol core, a structured Axum-based `RemoteTextServer`, and a `RemoteTextLifecycle` manager owned by the normal D-Bus daemon. The standalone `vinput-daemon remote-text-server` command remains available for isolated diagnostics.

## Activation and settings

The service is enabled only when the active ASR provider is the command provider `provider.vinput.remote.streaming`. Settings are derived with the legacy policy:

- `VINPUT_ASR_PORT`: explicit listen port in `1..=65535`;
- otherwise the explicit port in `VINPUT_ASR_URL`, when present;
- otherwise port `8080`;
- `VINPUT_ASR_DEBOUNCE_MS`: positive integer, default `1500`;
- `VINPUT_ASR_API_KEY`: required and never exposed by diagnostics or `Debug` output.

Other active providers disable this service without error. Invalid explicit settings fail rather than silently changing values.

## Authentication and connection policy

The browser/input protocol begins with `{ "type": "auth", "api_key": "..." }`. Successful authentication emits `auth_ok` and then `init` with the current output connection state. Only one input client and one output client may own the protocol at a time.

The Realtime-compatible endpoint is restricted to loopback peers and requires the configured API key as a bearer token. API-key comparison does not exit early on equal-length byte mismatches. The future HTTP layer must apply this authorization before upgrading `/v1/realtime`.

## Protocol transitions

The deterministic `RemoteTextProtocol` consumes parsed JSON and returns typed effects for the future runtime:

- input `text_update` replaces the current text and requests debounce scheduling;
- input `finalize` cancels debounce and emits a final result when text and an output client exist;
- output `session.update` emits `session.updated` with the supplied session object;
- output `input_audio_buffer.append` is ignored because the service receives text from the browser;
- output `input_audio_buffer.commit` emits an empty committed event or finalizes pending text;
- debounce expiry emits `input_audio_buffer.committed`, transcription `delta`, and transcription `completed`, then clears the text;
- output connect/disconnect notifies the input client; output disconnect also cancels debounce and clears text.

Generated event and item ids are opaque. Tests use deterministic ids; the network runtime may replace them with process-unique ids without changing message shape.

## HTTP/WebSocket runtime

`RemoteTextServer` provides a real async network boundary without copying the legacy raw-socket implementation:

- `GET /health` returns `{ "ok": true }`;
- `/` serves a small browser editor and `/favicon.svg` serves its icon;
- `/ws` upgrades the authenticated browser/input connection;
- `/v1/realtime` upgrades the loopback-only Bearer-authenticated output connection;
- WebSocket messages are limited to 2 MiB;
- one input and one output writer task are retained around shared protocol state;
- generation tokens cancel stale debounce timers;
- graceful shutdown stops the listener and connection tasks;
- the standalone `remote-text-server` daemon command derives its port and credentials from the active remote provider.

Real local-socket tests cover HTTP assets, authorization failure, single-input ownership, session updates, debounce delivery, the committed/delta/completed event sequence, output disconnect notification, and the standalone command health endpoint.

## Daemon-owned lifecycle

`VinputDbusService` owns one `RemoteTextLifecycle` beside the ASR runtime. The normal `vinput-daemon --dbus` path reconciles it before requesting the D-Bus name, then keeps it aligned with the target config used by:

- `SetActiveAsrProvider`;
- `SetActiveAsrTarget`;
- `ReloadAsrBackend`.

An unchanged disabled or unchanged active configuration is a no-op. Changing port, debounce, API key, or provider stops the old listener before starting the desired one. Switching to a non-remote provider stops the service. A replacement bind failure returns an explicit D-Bus error and does not retain the stale listener or credentials. `SIGINT` and `SIGTERM` trigger graceful listener shutdown before process exit.

Deterministic evidence covers lifecycle start, no-op reconciliation, settings-driven restart, provider-selection enable/disable, bind-failure cleanup, and config-file reload. `scripts/run-remote-text-daemon-lifecycle-smoke.sh` additionally launches the normal daemon inside a private `dbus-run-session`, proves both `/health` and `GetStatus`, sends `SIGTERM`, and verifies that the listener is released.

## Remaining proof and diagnostics

Remote text parity remains **partial** only at the user-facing evidence boundary:

- expose useful redacted LAN endpoint diagnostics without leaking credentials;
- prove the browser flow from another real device on the desktop user's network;
- refine external-user setup and troubleshooting documentation from that real-session evidence.

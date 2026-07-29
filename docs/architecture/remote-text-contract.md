# Remote text service contract

The legacy remote text service combines two clients around one shared text buffer:

- a browser/input client on `/ws` that authenticates, reports `text_update`, and requests `finalize`;
- a loopback-only OpenAI Realtime-compatible output client on `/v1/realtime` that receives committed transcription events.

The Rust rewrite keeps protocol behavior separate from the network runtime. `vinput-daemon::remote` currently implements the deterministic settings and protocol core. It does not yet bind a TCP socket, serve browser assets, upgrade WebSocket requests, enumerate LAN endpoints, or synchronize service lifetime with daemon reload.

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

## Pending runtime boundary

The next independent slice should add a structured Rust HTTP/WebSocket runtime, not copy the legacy raw-socket implementation. It must provide:

- `GET /health`;
- browser HTML and favicon responses;
- WebSocket upgrades for `/ws` and `/v1/realtime`;
- frame size and connection limits;
- one input and one output task around the shared protocol state;
- debounce timer scheduling and cancellation;
- daemon config synchronization, shutdown, and endpoint diagnostics.

Until that runtime is wired, remote text service parity is **partial**: settings, authentication policy, and message/state semantics are deterministic, while no externally reachable service exists.

# Audio contract

`vinpst-audio` owns pure PCM data structures and deterministic byte-level audio helpers. Desktop capture backends such as PipeWire should feed this crate instead of duplicating PCM parsing or format policy.

## PCM layout

The canonical in-memory representation is signed 16-bit interleaved PCM carried by `PcmBuffer` with explicit `PcmSpec` metadata:

- `sample_rate_hz`: non-zero sample rate.
- `channels`: non-zero interleaved channel count, defaulting to mono when omitted from JSON.
- `samples`: raw `i16` samples whose length must align to the channel count.

Frame-oriented calculations, duration, and deterministic chunk planning use frames rather than raw sample count. Multi-channel buffers are preserved as complete interleaved frames and chunk helpers never split a frame across chunk boundaries.

## Byte formats

Raw PCM bytes are signed 16-bit little-endian. Use `PcmBuffer::from_pcm16le_bytes` to decode raw bytes with explicit `PcmSpec`, and `i16_samples_to_le_bytes` when command ASR helpers need raw PCM bytes. Odd byte counts are rejected before sample conversion.

WAV decoding supports uncompressed RIFF/WAVE PCM format tag 1 with 16-bit samples. The parser preserves sample rate and channel metadata, skips unknown chunks using RIFF padding rules, rejects odd data chunk byte counts, and validates `block_align` plus `byte_rate` against the parsed sample format.

## Capture device discovery

The runtime exposes additive typed D-Bus methods `GetCaptureDevice() -> s` and `SetCaptureDevice(s) -> b`. Selection is idle-only, passes through `CaptureTarget::from_config_value`, validates the updated config, atomically persists it when the daemon has an explicit or discovered config path, and changes the target used by the next recording without replacing the daemon owner. The boolean reply reports whether persistence occurred; runtime-only selection remains possible when no config path exists.

Desktop capture backends should expose `AudioDeviceEnumerator` for UI/CLI device lists. `AudioDeviceInfo` mirrors the legacy PipeWire discovery shape: backend-local `id`, backend object `name`, and human-readable `description`. Enumerators should return only capture sources, preserving backend discovery order. `AudioDeviceInfo::capture_target` maps a discovered source name to the concrete `CaptureTarget::Object` used by recording.

The optional `pipewire-backend` feature verifies that the Rust PipeWire bindings and system headers compile, link, and initialize, maps `PipeWire:Interface:Node` globals with `media.class=Audio/Source` into `AudioDeviceInfo`, and provides a `PipeWireDeviceEnumerator` implementation. `vinpst-cli audio-devices` and `vinpst-daemon audio-devices` use this enumerator when they are built with the feature; enumeration failures are reported in JSON as `enumeration_error` with `live: false` instead of making diagnostics fail. Live context and registry probes require a usable PipeWire client configuration, so they are guarded by `VINPST_TEST_PIPEWIRE_CONTEXT`, `VINPST_TEST_PIPEWIRE_ENUMERATE`, or `VINPST_TEST_PIPEWIRE_RECORD` instead of running in default CI. The recorder probe accepts `VINPST_TEST_PIPEWIRE_RECORD_MS` to extend the capture window and `VINPST_TEST_PIPEWIRE_MIN_PEAK` to reject silent or implausibly weak PCM; with `--nocapture` it prints source, frame count, duration, peak amplitude, and first-buffer latency without persisting samples. `VINPST_LIVE_NATIVE_WAV=/path/to/speech.wav scripts/live/niri/run-ime-fcitx-virtual-source-live.sh` adds an opt-in installed-profile proof through an isolated PipeWire sink/source, rejects silent preflight capture, and restores the original profile afterward. Direct desktop output-to-microphone pickup is environment-dependent and is not retained as proof. Default CI must compile and test the feature without requiring a live PipeWire daemon; live probes must only run when requested explicitly.

## Capture lifecycle

Desktop recorders should implement the stateful `AudioRecorder` contract instead of overloading `AudioSource`. The contract mirrors the legacy daemon lifecycle:

1. Parse `global.capture_device` with `CaptureTarget::from_config_value`; `default` maps to the backend default, any other non-empty value is passed as a concrete backend target object.
2. `begin_recording` starts a fresh capture session and rejects duplicate starts.
3. Optional chunk callbacks may receive interleaved `PcmBuffer` chunks for streaming ASR sessions.
4. `stop_and_get_buffer` stops capture and returns the accumulated PCM buffer.
5. `cancel_recording` stops capture and discards pending audio.

`RuntimeState` consumes `AudioRecorder` directly and selects delivery from the active backend descriptor. Recording startup follows the upstream cold-press ordering: capture begins before ASR session creation so backend setup cannot delay microphone opening. A `CaptureStartGate` is installed first; it buffers any early `PcmBuffer` chunks until session construction succeeds, then replays them in order through the real session callback. Recorder-start failure therefore creates no ASR session. Conversely, session-creation or gate-arming failure cancels the already-started capture, clears the callback, and leaves the runtime idle.

Buffered sessions keep the stop-time path: collect the final buffer, apply input gain and optional quiet-only peak normalization, push one processed `PcmBuffer` with explicit `PcmSpec`, drain pending events, finish, and merge final events. Chunked sessions apply input gain at the device boundary, combine arbitrary callback sizes into legacy-compatible 800-frame batches, push and poll each batch under shared session ownership, and flush only the final short batch on stop. The complete accumulated stop buffer is not replayed to chunked sessions. Callback errors and PCM metadata changes are retained and returned through the normal stop/cancel path. The existing `AudioSource` trait remains a one-shot source for deterministic tests and file-input demos. `SourceAudioRecorder` adapts those one-shot sources into the stateful runtime path, while `RecorderAudioSource` adapts stateful recorders back into legacy one-shot call sites.

ASR session ownership is explicit across recorder callbacks and the stop path. Chunked delivery shares the session through a mutex because live PipeWire callbacks run on the recorder worker thread; callbacks are detached before cancellation or drop. If recorder stop, PCM delivery, ASR polling, payload conversion, or text finishing fails, `RuntimeState` calls `RecognitionSession::cancel` before returning the error and resetting to idle. Dropping a runtime with an active recording also clears the callback, cancels the active ASR session, and then cancels the recorder.

`PipeWireStreamConfig` records the selected capture target plus the pinned `S16LE` 16 kHz mono PCM policy that live streams request. `PipeWireAudioRecorder` exists behind `pipewire-backend` as the live recorder seam. One long-lived PipeWire worker owns the loop, context, and connected stream. The stream is connected with `INACTIVE`, then normal stops use `set_active(false)` and the next recording with an identical target/PCM plan uses `set_active(true)` instead of reconnecting. Target changes rebuild the stream, and a failed reused activation retries once with a fresh stream. `VINPST_CAPTURE_REUSE` defaults to enabled; values beginning with `0`, `f`, `F`, `n`, or `N` restore destroy/create behavior. Cancellation and error cleanup always shut down the worker immediately rather than retaining an uncertain stream.

The worker command channel is attached directly to the PipeWire loop, so Start/Stop wakes the loop without timeout polling. Process callbacks accept samples only while the stream is armed; inactive streams have no sample path. The opt-in live callback test records twice with one recorder and asserts the first start creates a stream while the second reuses it. After a normal stop, a dedicated worker timer keeps the inactive stream warm for `VINPST_CAPTURE_IDLE_DESTROY_MS`, defaulting to 15,000 ms and capped at 600,000 ms; invalid or negative values fall back to the default, while `0` destroys immediately. Each begin/stop advances an idle generation, so a stale timeout cannot destroy a newly armed or more recently stopped stream. If timer scheduling is unavailable after capture completed, the recorder closes the warm worker instead of converting a successful recognition into an error. Cancellation, worker shutdown, reuse opt-out, and target changes still tear down or replace the stream immediately. The daemon `audio-devices` diagnostic prints the same recording target/format/rate/channel policy so desktop users can verify config before starting the daemon.

Capture startup diagnostics mirror the upstream cold-start instrumentation without changing capture policy. `PipeWireStartTiming` exposes `idle_gap_ms`, `create_stream_ms`, `set_active_ms`, `stream_reused`, `created_new_stream`, and `start_total_ms` for the most recent successful begin. The first non-empty process callback records `first_buffer_ms` exactly once through an atomic probe. `RuntimeState` separately emits `capture_open_ms`, `session_create_ms`, and its own `start_total_ms`, including failure-stage timing when capture or ASR session creation fails. These structured tracing events include the selected capture target and timing metadata only; they never include PCM samples, recognized text, provider credentials, or API keys.

When `global.duck_output_while_recording` is enabled, `RuntimeState` lowers the `WirePlumber` default sink after capture and ASR session startup have both succeeded. `OutputDucker` reads `wpctl get-volume @DEFAULT_AUDIO_SINK@`, multiplies the saved linear volume by the clamped `global.duck_output_volume`, and writes it with `wpctl set-volume`. Both commands have a hard two-second timeout, use direct argument passing without a shell, and are best-effort: missing `wpctl`, a missing default sink, parse failures, timeouts, or set failures never block recording. A second duck while already active is a no-op. Normal stop restores immediately after capture stops, while stop errors, ASR/text failures, reset, cancellation, and runtime drop all perform the same idempotent restore. Restore failure clears the saved state and is reported through tracing rather than leaving future recordings permanently marked as ducked. The opt-in `scripts/live/audio/run-output-ducking-live.sh` gate creates an isolated virtual PipeWire sink, temporarily selects it as the default, runs a mock recording through the real `wpctl` controller, and proves `0.80 -> 0.20 -> 0.80` for a `0.25` duck scale. It then restores the original default sink and verifies that its name and volume are unchanged, removes the virtual nodes, and retains evidence in `target/tmp/output-ducking-live/summary.json`. This proves real volume-control behavior without changing audible hardware output.

## Processing order

The runtime applies configured input gain at the capture boundary using the frozen `ApplyGainI16` algebra: multiply each sample, clamp to the `i16` range, then truncate toward zero. Streaming sessions use that gained PCM directly.

Buffered sessions use the same gained PCM. When `normalize_audio` is enabled, a non-zero peak below 0.1 full scale is amplified to full scale; louder audio is never attenuated. No fixed amplitude-threshold silence trimming is injected by this layer. Offline Silero VAD remains a separate ASR-backend concern and may trim speech according to its own configured policy.

`PcmBuffer::chunk_ranges_by_frames` can plan complete-frame chunk ranges without copying, and `PcmBuffer::chunks_by_frames` can materialize those ranges for deterministic tests or helper boundaries.

## Deterministic chunk callback seams

`MockAudioRecorder` and `SourceAudioRecorder` can use complete-frame chunk helpers for deterministic streaming callback tests. Runtime coverage intentionally uses recorder callback sizes that do not align with the ASR batch size and proves 1,700 mono samples become 800, 800, and 100 sample pushes without replaying the final accumulated buffer. This keeps chunked runtime behavior testable without requiring a live PipeWire daemon.

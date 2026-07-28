# ASR contract

`vinput-asr` owns the ASR backend/session contract. It keeps recognition behavior behind Rust trait boundaries while preserving the legacy daemon/frontend payload shape.

## Current crate responsibilities

`crates/vinput-asr` is split by responsibility while keeping the public trait boundary stable:

- `traits.rs`: `AudioDeliveryMode`, `BackendCapabilities`, `BackendDescriptor`, `RecognitionContext`, `RecognitionEvent`, `RecognitionSession`, and `AsrBackend`;
- `error.rs`: `AsrError`;
- `mock.rs`: deterministic buffered/streaming/early-final `MockAsrBackend`;
- `command.rs`: command provider specs, JSON request/response types, legacy batch and streaming runners, process runner helpers, and `CommandAsrBackend`;
- `factory.rs`: config-selected backend factory and config-derived `AsrBackendState`;
- `sherpa.rs`: local `sherpa-onnx` typed config parsing, model/hotwords path validation, SenseVoice and Qwen3 ASR runtime planning, and the feature-gated official runtime adapter;
- `payload.rs`: conversion from recognition events to the legacy recognition payload JSON model;
- `tests.rs`: behavior-preserving coverage for mock, command, factory, and payload contracts.

Command providers use legacy batch or `.streaming` runners through the factory, while the JSON helper seam remains available for explicit process-runner tests and small helper integrations. Local `sherpa-onnx` now has an explicit typed config seam, local model/hotwords path validation, offline SenseVoice layout inference, typed Qwen3 ASR metadata mapping, and an optional official runtime adapter behind the `sherpa-onnx-backend` Cargo feature. Default builds keep the runtime disabled so ordinary CI and command-demo installs do not download or link native ASR libraries. The validation seam accepts relative or absolute local model and hotwords paths, rejects empty values and URL-like paths, and verifies model directories plus regular hotwords files before any runtime is constructed.

## Daemon integration

`RuntimeState` owns a boxed `AsrBackend` and an active `RecognitionSession` while recording. The default daemon uses `MockAsrBackend`; explicit configured paths can build the active config-selected backend to exercise command ASR seams.

The current runtime flow is:

```text
StartRecording
  -> create_session
  -> begin audio recorder
StopRecording
  -> stop recorder and collect PCM
  -> apply deterministic audio processing
  -> push PCM to the active ASR session
  -> drain already-pending ASR events
  -> finish session
  -> poll and merge final/stop-time events
  -> emit stop-time partial through D-Bus when present
  -> events_to_payload
  -> text finishing
  -> reset Idle
```

This is a contract seam, not full legacy runtime parity. The feature-gated `sherpa-onnx` backend covers buffered offline SenseVoice and Qwen3 ASR plus online transducer and Zipformer2 CTC metadata/runtime layouts. SenseVoice, Qwen3 ASR, and Zipformer2 CTC are proven with real registry-model WAV samples; transducer metadata and asset mapping are contract-tested. Buffered offline recognition uses the migrated Silero VAD model when enabled, preserving the legacy 512-sample feed, threshold/duration/padding controls, no-speech fallback, and a 500 ms cold-start guard that prevents short push-to-talk startup gaps from clipping the first syllable. Online recognizers preserve the legacy endpoint defaults (`true`, `2.4`, `1.2`, `20.0`) unless typed metadata overrides them and run the legacy-compatible 200 ms silence warmup after creation. As in legacy, push-to-talk sessions still finalize on `StopRecording`; the runtime does not invent automatic multi-utterance finalization from `is_endpoint`. The daemon routes recorder callbacks to chunked ASR sessions in legacy-compatible 800-frame batches, applies input gain before delivery, polls native hypotheses after each push, and emits deduplicated `RecognitionPartial` D-Bus signals during recording through a generation-scoped 40 ms poller. Stop cancels the poller and suppresses a duplicate final partial. Full reload state, decode timeout enforcement, real desktop proof, and the remaining model families still belong to later phases.

## Command ASR provider contracts

A command ASR provider is configured with `type = "command"`, a `command`, optional `args`, `env`, `model`, `hotwords_file`, and `timeout_ms`. The config-selected factory preserves the legacy command behavior currently covered by tests:

1. provider ids that end with `.streaming` use `LegacyCommandStreamingRunner`, expose streaming/chunked capabilities, write one committed audio JSON line plus a finish line to stdin, parse JSON event lines from stdout, and suppress repeated partial text like the legacy C++ session;
2. other command providers use `LegacyCommandBatchRunner`, which writes raw signed 16-bit little-endian PCM to stdin and reads final text from stdout;
3. both runners honor configured args/env and process timeout/error handling.

`CommandAsrRequest` remains the internal buffered request type shared by these runners and explicit test seams. It carries provider metadata, recognition context, PCM layout, and interleaved signed 16-bit samples.

A JSON helper can return final text and optionally partial text:

```json
{"partial_text":"listening","text":"final text"}
```

A helper can also return an ASR-level error without a non-zero process exit:

```json
{"error":"asr failed"}
```

The deprecated `failure` response key is accepted as an alias for `error`. Non-zero exits, invalid JSON, missing final text, and timeout paths are surfaced as backend errors.

The repository also ships `scripts/command-asr-wav-helper.py` for external ASR CLIs that consume WAV files rather than the raw legacy PCM or vinput JSON request directly. The helper reads `CommandAsrRequest` JSON from stdin, writes the request PCM as a temporary WAV file, exposes that path as `VINPUT_ASR_WAV`, runs the command after `--`, and emits `{"text":...}` from trimmed stdout or `{"error":...}` for helper-level failures. It also exports `VINPUT_ASR_PROVIDER_ID`, `VINPUT_ASR_MODEL_ID`, `VINPUT_ASR_HOTWORDS_FILE`, `VINPUT_ASR_SAMPLE_RATE_HZ`, and `VINPUT_ASR_CHANNELS` for wrapper scripts. This keeps real command-ASR integration usable with tools such as whisper.cpp or sherpa CLIs without adding a hard runtime dependency.

For user-level live trials, `VINPUT_USER_PROFILE=real-command-asr-wav scripts/install-user-ime.sh` installs the WAV helper next to the daemon, generates a `real-command-asr-wav.json` config, and routes live PipeWire capture through the helper to `VINPUT_USER_COMMAND_ASR_WAV_COMMAND`. That profile is an interim real command-ASR path: `vinput doctor --config ...` should report a ready effective command backend, but this does not make the local `sherpa-onnx` runtime complete.


## Native `sherpa-onnx` backend contract

The native backend uses the official `sherpa-onnx` Rust crate only when built with `sherpa-onnx-backend`. SenseVoice-style offline recognition accepts `vinput-model.json` metadata and keeps directory inference as a compatibility fallback: the configured model directory must contain `model.int8.onnx` or `model.onnx`, plus `tokens.txt`. Qwen3 ASR requires typed metadata for its convolution frontend, encoder, decoder, tokenizer, and generation parameters; all declared assets are resolved under the model directory and validated before recognizer construction. Optional hotwords files are passed through to `OfflineRecognizerConfig`. Relative model paths are resolved under `VINPUT_SHERPA_MODEL_ROOT` when the feature is active; user install profiles generate absolute model paths to avoid environment-sensitive activation failures.

The runtime remains buffered: the daemon collects PCM, then `SherpaOnnxRecognitionSession` converts signed 16-bit samples to `f32`, calls `OfflineRecognizer::decode`, and emits one final-text payload. Timeout fields are preserved in config diagnostics but are not yet enforced around the native decode call.

A user-level live profile is available for real desktop trials:

```sh
VINPUT_USER_PROFILE=sherpa-sense-voice-live \
  VINPUT_USER_SHERPA_MODEL=/path/to/sense-voice-model-dir \
  scripts/install-user-ime.sh
```

This profile builds the daemon with `pipewire-backend,sherpa-onnx-backend`, writes `sherpa-sense-voice-live.json`, enables configured backends, and defaults the activation service to `--audio-backend pipewire`. It also runs `runtime-status` by default after install and during `VINPUT_USER_STATUS=1` checks so native model or library loading failures are visible before Fcitx5 restart. Set `VINPUT_USER_RUNTIME_STATUS=0` to skip that validation, or `VINPUT_USER_RUNTIME_STATUS=1` to opt into the same validation for another configured profile.

Before mutating the desktop profile, a local WAV can be used to validate the same native backend outside Fcitx5:

```sh
VINPUT_SHERPA_MODEL=/path/to/offline-model-dir \
  VINPUT_SHERPA_WAV=/path/to/input.wav \
  just sherpa-offline-local-smoke
```

`just sherpa-sense-voice-local-smoke` preserves the metadata-free SenseVoice compatibility path. `just sherpa-qwen3-local-smoke` requires the registry-generated `vinput-model.json` and verifies that its family is `qwen3_asr`. The generic smoke builds the feature-gated daemon, runs `runtime-status` to force model construction, and then runs `--once --wav` through the configured ASR/text pipeline.

The live registry Qwen3 model `model.sherpa-onnx.qwen3-asr-0.6b-int8` has been downloaded, SHA-256 verified, extracted, and recognized its bundled `test_wavs/es1.wav` as `Esta prenda es amplia. Recomiendo elegir una talla menor al habitual.` through `just sherpa-qwen3-local-smoke`.

The smoke prepends `target/debug` to `LD_LIBRARY_PATH` by default so the cargo-provided `libsherpa-onnx` and `libonnxruntime` are preferred over incompatible system libraries; override with `VINPUT_SHERPA_RUNTIME_LIB_DIR` when testing another runtime bundle.

## Diagnostics

Both `vinput-cli asr-state` and `vinput-daemon asr-state` serialize `AsrBackendState` from config only. They do not construct, reload, or probe the runtime backend. The daemon diagnostic remains usable with `--configured-backends` even when the selected runtime backend is unavailable.

## Known compatibility gaps

These gaps remain after the behavior-preserving ASR split:

- Native `sherpa-onnx` is feature-gated and supports offline SenseVoice/Qwen3 ASR plus online transducer/Zipformer2 CTC layouts. SenseVoice and Qwen3 ASR pass real WAV smokes with offline Silero VAD active; Zipformer2 CTC also has a real WAV smoke with the 200 ms recognizer warmup, while transducer construction remains metadata/feature-build tested. Legacy endpoint rule defaults and metadata overrides are forwarded to the official runtime. Moonshine, Dolphin, Paraformer, full reload state, and decode timeout enforcement are not implemented yet.
- Offline Silero VAD resolves `VINPUT_SHERPA_VAD_MODEL`, user/system XDG data paths, or the development asset; a missing or unloadable model degrades to untrimmed recognition. The native user-install profile installs the tracked MIT-licensed model under `fcitx-vinput/vad`.
- Runtime streaming delivers recorder callbacks to native online sessions in 800-frame batches. Callback-polled partials are emitted through D-Bus while recording; generation cancellation isolates recordings, and final/completed events remain available for stop processing. This path is session-bus tested but not yet proven in a real Fcitx desktop session.
- Command ASR is runtime-wired for configured command providers; remote ASR provider kinds remain contract-pinned but unavailable.

## Mock audio push observation

`MockAsrBackend` can attach a shared `MockAsrAudioLog` for deterministic tests. The log records each `push_audio` or `push_pcm` call, including sample length and optional `PcmSpec` metadata. Runtime tests use it to prove 800/800/tail chunk delivery, input metadata preservation, and no stop-time replay. A real session-bus integration test additionally proves that a partial signal arrives before `StopRecording` and is not repeated at stop. Native Zipformer2 CTC proves the recognizer/runtime path separately; real desktop Fcitx/PipeWire behavior remains unproven.

`MockAsrAudioPush` is serde/schema-ready so future diagnostics can expose recorded mock audio pushes without exposing the shared `MockAsrAudioLog` container itself.

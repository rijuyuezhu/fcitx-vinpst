# ASR contract

`vinput-asr` owns the ASR backend/session contract. It keeps recognition behavior behind Rust trait boundaries while preserving the legacy daemon/frontend payload shape.

## Current crate responsibilities

`crates/vinput-asr` is split by responsibility while keeping the public trait boundary stable:

- `traits.rs`: `AudioDeliveryMode`, `BackendCapabilities`, `BackendDescriptor`, `RecognitionContext`, `RecognitionEvent`, `RecognitionSession`, and `AsrBackend`;
- `error.rs`: `AsrError`;
- `mock.rs`: deterministic buffered/streaming/early-final `MockAsrBackend`;
- `command.rs`: command provider specs, JSON request/response types, legacy batch and streaming runners, process runner helpers, and `CommandAsrBackend`;
- `factory.rs`: config-selected backend factory and config-derived `AsrBackendState`;
- `sherpa.rs`: local `sherpa-onnx` typed config parsing, model/hotwords path validation, SenseVoice layout inference, and the feature-gated official runtime adapter;
- `payload.rs`: conversion from recognition events to the legacy recognition payload JSON model;
- `tests.rs`: behavior-preserving coverage for mock, command, factory, and payload contracts.

Command providers use legacy batch or `.streaming` runners through the factory, while the JSON helper seam remains available for explicit process-runner tests and small helper integrations. Local `sherpa-onnx` now has an explicit typed config seam, local model/hotwords path validation, offline SenseVoice layout inference, and an optional official runtime adapter behind the `sherpa-onnx-backend` Cargo feature. Default builds keep the runtime disabled so ordinary CI and command-demo installs do not download or link native ASR libraries. The validation seam accepts relative or absolute local model and hotwords paths, rejects empty values and URL-like paths, and verifies model directories plus regular hotwords files before any runtime is constructed.

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

This is a contract seam, not full legacy runtime parity. The feature-gated `sherpa-onnx` backend currently covers buffered offline SenseVoice recognition only; live PipeWire chunk delivery to streaming ASR, VAD trimming, warmup/reload state, broader sherpa model families, and real worker orchestration still belong to later phases.

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

The native backend uses the official `sherpa-onnx` Rust crate only when built with `sherpa-onnx-backend`. The first supported layout is SenseVoice-style offline recognition: the configured model directory must contain `model.int8.onnx` or `model.onnx`, plus `tokens.txt`. Optional hotwords files are passed through to `OfflineRecognizerConfig`. Relative model paths are resolved under `VINPUT_SHERPA_MODEL_ROOT` when the feature is active; user install profiles generate absolute model paths to avoid environment-sensitive activation failures.

The runtime remains buffered: the daemon collects PCM, then `SherpaOnnxRecognitionSession` converts signed 16-bit samples to `f32`, calls `OfflineRecognizer::decode`, and emits one final-text payload. Timeout fields are preserved in config diagnostics but are not yet enforced around the native decode call.

A user-level live profile is available for real desktop trials:

```sh
VINPUT_USER_PROFILE=sherpa-sense-voice-live \
  VINPUT_USER_SHERPA_MODEL=/path/to/sense-voice-model-dir \
  scripts/install-user-ime.sh
```

This profile builds the daemon with `pipewire-backend,sherpa-onnx-backend`, writes `sherpa-sense-voice-live.json`, enables configured backends, and defaults the activation service to `--audio-backend pipewire`.

Before mutating the desktop profile, a local WAV can be used to validate the same native backend outside Fcitx5:

```sh
VINPUT_SHERPA_MODEL=/path/to/sense-voice-model-dir \
  VINPUT_SHERPA_WAV=/path/to/input.wav \
  just sherpa-sense-voice-local-smoke
```

That smoke builds the feature-gated daemon, runs `runtime-status` to force model construction, and then runs `--once --wav` through the configured ASR/text pipeline.

## Diagnostics

Both `vinput-cli asr-state` and `vinput-daemon asr-state` serialize `AsrBackendState` from config only. They do not construct, reload, or probe the runtime backend. The daemon diagnostic remains usable with `--configured-backends` even when the selected runtime backend is unavailable.

## Known compatibility gaps

These gaps remain after the behavior-preserving ASR split:

- Native `sherpa-onnx` is feature-gated and currently limited to buffered offline SenseVoice-style models; broader sherpa model families, runtime VAD trimming, warmup, reload state, and decode timeout enforcement are not implemented yet.
- Runtime streaming has command-helper test seams, but live PipeWire chunk delivery to streaming ASR is not implemented.
- Command ASR is runtime-wired for configured command providers; remote ASR provider kinds remain contract-pinned but unavailable.

## Mock audio push observation

`MockAsrBackend` can attach a shared `MockAsrAudioLog` for deterministic tests. The log records each `push_audio` or `push_pcm` call, including sample length and optional `PcmSpec` metadata. This is a mock-only observation seam for future runtime streaming tests; it does not imply a real ASR runtime or live recorder is wired.

`MockAsrAudioPush` is serde/schema-ready so future diagnostics can expose recorded mock audio pushes without exposing the shared `MockAsrAudioLog` container itself.

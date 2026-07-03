# E2E capability matrix and CLI/daemon parity plan

Date: 2026-07-03

This is the detailed capability comparison for moving the Rust rewrite from a tested prototype into a user-usable replacement for legacy `fcitx5-vinput`. It focuses on **CLI and daemon experience** because those are now the shortest path to a real user workflow: discover/install a model, configure it, start the daemon, dictate through Fcitx, diagnose failures, and recover without hand-editing JSON.

## Evidence snapshot

- Rust repository: `/workspace/fcitx-vinput-rs`
- Legacy repository: `/workspace/fcitx5-vinput`
- Rust audited HEAD: `ced48b6 fix(asr): prefer local sherpa runtime libs`
- Local branch state at audit time: `main...origin/main [ahead 9, behind 1]`
- Rust CLI surface was collected from `target/debug/vinput --help` after `cargo clean -p vinput-cli -p vinput-daemon && cargo build -q -p vinput-cli -p vinput-daemon`.
- Rust daemon surface was collected from `target/debug/vinput-daemon --help` and `crates/vinput-daemon/src/dbus_service.rs`.
- Legacy CLI surface was collected from `src/cli/config/register_*.cpp` and `src/cli/control/register_*.cpp`.
- Legacy daemon surface was collected from `src/common/dbus/dbus_interface.h`, `src/daemon/runtime/dbus_service.cpp`, `src/daemon/runtime/daemon_runtime_controller.*`, `src/daemon/asr/**`, `src/daemon/audio/**`, and `src/daemon/postprocess/**`.
- Native sherpa evidence: registry model `model.sherpa-onnx.sense-voice-zh-en-ja-ko-yue-int8` was downloaded from live `xifan2333/vinput-registry`, verified by sha256, extracted, and recognized bundled `test_wavs/zh.wav` as `开放时间早上九点至下午五点` through `just sherpa-sense-voice-local-smoke`.

## Executive conclusion

Rust is **not functionally complete** versus legacy. The protocol spine is strong and a real offline SenseVoice ASR path has been proven, but the product still lacks the CLI/resource/config/runtime glue that makes legacy usable by normal users.

Current parity estimate:

| Area | Estimate | Meaning |
| --- | ---: | --- |
| D-Bus ABI and daemon facade | 80-90% | Core method/signal names and payload shapes are preserved, with Rust-only diagnostics added. |
| Deterministic E2E spine | 85-90% | Command-demo, user install smokes, activation, file-input tests, and adapter lifecycle are strong. |
| Native local ASR | 45-55% | One SenseVoice offline path is real and tested; streaming, model families, VAD, metadata mapping, and desktop runtime loading are incomplete. |
| CLI user experience | 20-30% | Rust CLI is mostly diagnostic and dry-run oriented; legacy has full config/model/provider/scene/control commands. |
| Registry/resource install | 20-30% | Rust has safe primitives and a dry-run `index.json` planner, but live registry `models.json/providers.json/adapters.json` install is not user-facing. |
| Real desktop readiness | 45-55% | Install/probe paths exist, but real Fcitx trigger/commit with native model still needs proof and runtime library handling. |
| Full user-visible parity | 55-65% | Enough pieces exist for targeted alpha work, but not enough for a normal replacement. |

The next project target should be: **replicate the legacy CLI and daemon experience well enough that a user can install a model from registry, choose it, run doctor/status, start/stop recording, and use the daemon through Fcitx without manual JSON editing.**

## User journeys

| Journey | Legacy behavior | Rust current behavior | Gap | Target acceptance |
| --- | --- | --- | --- | --- |
| J0: first-run init | `vinput init` creates config/dirs and default files. GUI/packaging also expect stable paths. | No equivalent CLI init. User install scripts can write selected profile configs. | Missing general first-run CLI. | `vinput init` creates default config, managed dirs, activation-service hint, and prints JSON/text summary. |
| J1: list and install local ASR model | `vinput model list/add/use/info/remove` fetches registry metadata, downloads assets, materializes model, updates config. | Manual download works; Rust registry CLI only validates/plans sample `index.json`; no live `models.json` install command. | Major. | `vinput model list`, `vinput model install <id|short_id>`, `vinput model use <id|path>`, `vinput model info`, `vinput model remove` work with live registry and sha256 checks. |
| J2: normal dictation with local model | Fcitx trigger starts PipeWire capture, ASR, optional postprocess, commit. | Native SenseVoice recognizes a WAV file; user profile install exists; live Fcitx/PipeWire/native model path not proven. | Major live proof and runtime library handling. | Real desktop checklist passes: trigger, preedit, capture, inference, commit into app, `doctor` green. |
| J3: command dictation over selected text | Fcitx command trigger captures selected text or fallback, ASR command scene, LLM/text transform, replace selection. | Surrounding-text command path and replacement logic exist; clipboard fallback and live proof incomplete. | Medium-major. | In two apps, selected text replacement works; fallback path has clear diagnostics when unavailable. |
| J4: command ASR provider | Legacy supports command batch and streaming providers. | Rust has command ASR, command WAV helper, streaming command partial tests, and user profile for real command WAV. | Mostly implemented, needs CLI config parity. | `vinput provider add/use/edit/remove` can configure command ASR without hand-editing JSON. |
| J5: LLM/text postprocess | Legacy supports OpenAI-compatible providers, command adapters, scenes, prompt files, candidate_count, command scene. | Rust has command text adapters, OpenAI-compatible provider tests, prompt/context pieces. | CLI/config UX incomplete and real provider validation limited. | `vinput llm`, `vinput adapter`, and `vinput scene` commands configure and validate postprocess paths. |
| J6: adapter lifecycle | Legacy `vinput adapter start/stop` and daemon D-Bus `StartAdapter/StopAdapter` supervise local adapters and PID files. | Rust daemon can start/stop supervised command adapters; CLI lacks user command. | CLI gap. | `vinput adapter start/stop/list` calls daemon and reports PID/running state. |
| J7: daemon control | Legacy `vinput daemon status/start/stop/restart/log` integrates D-Bus/systemd/logs. | Rust has `doctor`, activation service generation, runtime-status command, but not legacy lifecycle CLI. | Major CLI gap. | User can start/stop/restart/status/log daemon from CLI, using activation/systemd/user-mode strategy. |
| J8: recording control from CLI | Legacy `vinput recording start/stop/toggle`. | Rust daemon exposes D-Bus methods; Rust CLI does not expose them. | Major CLI gap. | `vinput recording start/stop/toggle [--scene] [--selected-text]` works against D-Bus service and prints result. |
| J9: device selection | Legacy `device list/use`, PipeWire device enumeration, config mutation. | Rust has `audio-devices` diagnostics; no `device use`. | Medium. | `vinput device list/use` maps PipeWire nodes to config and validates with doctor. |
| J10: diagnose and recover | Legacy has CLI, GUI, notifications, logs. | Rust `doctor`, `runtime-status`, `audio-devices`, live probe are better than legacy in several areas. | Mostly done, but needs user-facing commands. | One `vinput doctor` explains config, activation, model, runtime libs, audio, daemon owner, and next command. |

## CLI command surface comparison

### Legacy CLI commands

Legacy registers these user-facing commands:

```text
init
config get/set/edit
model list/add/remove/use/info
provider list/add/use/edit/remove
hotword get/set/clear/edit
device list/use
llm list/add/remove/edit/test
adapter list/add/start/stop
scene list/add/use/remove/edit
daemon status/start/stop/restart/log
recording start/stop/toggle
```

Legacy also supports a global `-j/--json` output mode.

### Rust CLI commands

Rust currently exposes:

```text
protocol
config validate/example
registry validate/plan/install-plan
asr-state
audio-devices
doctor
activation-service
mock-result
status
```

Rust CLI strengths:

- good structured diagnostics;
- stable D-Bus protocol inspection;
- activation service generation/removal/status;
- config validation and example export;
- registry safety primitives behind dry-run planning;
- deterministic test helpers.

Rust CLI weaknesses for a user:

- no first-run `init`;
- no config mutation commands;
- no live registry install command;
- no model/provider/scene/LLM/hotword/device management commands;
- no daemon lifecycle commands;
- no recording control commands;
- no global JSON/text output mode equivalent to legacy;
- no command aliases for short IDs from live registry.

## Daemon capability comparison

| Capability | Legacy daemon | Rust daemon | Status |
| --- | --- | --- | --- |
| D-Bus bus/interface/path | `org.fcitx.Vinput`, `/org/fcitx/Vinput`, `org.fcitx.Vinput.Service`. | Same. | Aligned. |
| Core methods | `StartRecording`, `StartCommandRecording`, `StopRecording`, `GetStatus`, `GetAsrBackendState`, `ReloadAsrBackend`, `StartAdapter`, `StopAdapter`. | Same core methods. | Mostly aligned. |
| Diagnostic extensions | Limited. | Adds `GetTextAdapterState`, `GetRuntimeStatus`. | Rust improved. |
| Signals | Recognition result/partial, status changed, daemon notification. | Same names preserved. | Mostly aligned. |
| Status strings | `idle`, `recording`, `inferring`, `postprocessing`, `error`. | Same strings. | Aligned. |
| Runtime state machine | Async/poll worker model with capture/infer/postprocess stages. | Deterministic runtime state with D-Bus facade, file input, reload deferral. | Partial; live async behavior still needs proof. |
| ASR reload | Legacy reload worker prepares backend and swaps later. | Rust has immediate/deferred reload skeleton and configured reload path. | Partial. |
| Audio capture | PipeWire capture with target object support, gain/normalization. | Feature-gated PipeWire recorder and diagnostics. | Partial; needs desktop proof. |
| File input | Not a first-class user path. | `--wav` and `--pcm16le` are first-class for smoke/debug. | Rust improved. |
| Command batch ASR | Implemented. | Implemented. | Mostly aligned. |
| Command streaming ASR | Implemented with partials and process protocol. | Implemented/tested in Rust command ASR path. | Mostly aligned, needs live CLI config. |
| Sherpa offline | Multiple families through C API metadata. | Feature-gated official Rust binding; SenseVoice layout works. | Partial. |
| Sherpa streaming | Implemented. | Not implemented. | Missing. |
| VAD | `vad_trimmer` with sherpa VAD model. | Config parses VAD but native trimming is not implemented. | Missing/partial. |
| Model metadata | Legacy reads registry/local `vinput_model` metadata and maps family-specific files. | Rust currently infers SenseVoice file layout from directory. | Major gap. |
| Text postprocess | OpenAI-compatible HTTP, prompt files/interpolation/context/candidates, command scene. | Command adapter and OpenAI-compatible paths exist; real UX/config incomplete. | Partial. |
| Adapter supervisor | Process supervision, PID files, stderr notifications. | Process supervision, PID files, D-Bus start/stop, diagnostics. | Mostly aligned. |
| Remote text service | Legacy has HTTP/WebSocket remote text service. | Not implemented. | Missing. |
| Notifications | Legacy classifies/forwards daemon notifications to frontend. | Error info and notification signal exist; coverage less mature. | Partial. |

## Registry/resource comparison

Legacy live registry layout:

```text
registry/models.json
registry/providers.json
registry/adapters.json
i18n/*.json
resources/providers/**/entry.py
resources/adapters/**/entry.py
```

Rust current registry fixture/layout:

```text
data/sample-registry-index.json
index.json-style AssetEntry list
```

Rust registry crate already has useful low-level safety pieces:

- mirror fallback boundary;
- checksum validation;
- archive extraction boundary;
- staging/materialization boundary;
- cache-related structure;
- dry-run install plan.

But to match legacy user experience, Rust needs a **live registry v2 layer** that understands `models.json`, `providers.json`, `adapters.json`, `short_id`, i18n, script resources, and `vinput_model` runtime metadata.

## Config comparison

The committed default JSON key shape is still close between legacy and Rust:

```text
version
registry.base_urls
global.default_language
global.capture_device
asr.active_provider
asr.normalize_audio
asr.input_gain
asr.vad.enabled
asr.providers[]
llm.providers[]
llm.adapters[]
scenes.active_scene
scenes.definitions[]
```

The gap is not the base schema; the gap is user-facing mutation and resource-aware configuration:

- legacy has JSON-pointer `config get/set/edit`;
- legacy model/provider/adapter/scene commands edit config safely;
- Rust validates and consumes config but cannot yet mutate most config from CLI;
- Rust install profiles generate specific configs, but they are not a general replacement for CLI config management.

## P0 plan: replicate usable CLI and daemon experience

This phase should avoid GUI and distro packaging. The goal is a terminal-first user flow that works in a real desktop session.

### P0.1 live registry v2 read/list layer

Implement live registry parsing in `vinput-registry` without replacing the existing safe asset primitives.

Acceptance:

- Parse `registry/models.json` into typed structs with `id`, `short_id`, `urls`, `sha256`, `size_bytes`, `language`, and raw/typed `vinput_model` metadata.
- Parse `registry/providers.json` and `registry/adapters.json` with script URLs and env specs.
- Fetch i18n maps and resolve title/description with fallback to id/short_id.
- Add fixtures copied from live registry with tests.
- `vinput model list --json` and text output show both `id` and `short_id`; legacy-compatible `model ls -a/--available` is accepted for the live/remote registry list.

### P0.2 model install/use/info/remove

Build the first user-facing model workflow.

Acceptance:

- `vinput model install <id-or-short-id>` downloads with mirror fallback, verifies sha256, extracts safely, and materializes under the managed model root; legacy-compatible `model add <id-or-short-id>` is accepted as the install alias.
- `vinput model info <id|short_id|path>` prints installed path, family, backend, language, model files, hotword support, and runtime readiness hints.
- `vinput model use <id|short_id|path>` updates config active provider/model in the current user config.
- `vinput model remove <id|short_id>` removes only managed installed model directories after safety checks.
- Install can optionally run `runtime-status` and reports native shared-library resolution failures.

### P0.3 config mutation core

Port enough config mutation to stop hand-editing JSON.

Acceptance:

- `vinput init` creates default config and managed directories idempotently.
- `vinput config get <json-pointer>` and `set <json-pointer> <value>` work with type-aware parsing and validation.
- `vinput config edit` opens the config path from `$EDITOR` and validates afterward.
- Config writes are atomic and preserve a backup or clear rollback behavior.
- All commands have `--json` and text output.

### P0.4 provider/hotword/device commands

Expose ASR provider UX before broad LLM UX.

Acceptance:

- `vinput provider list/add/use/edit/remove` can configure local, command batch, command streaming, and remote/cloud script providers.
- `vinput hotword get/set/clear/edit` manages the active provider hotwords path when supported.
- `vinput device list/use` uses Rust PipeWire diagnostics and updates `global.capture_device`.
- `vinput doctor` references these commands in remediation text.

### P0.5 daemon and recording control commands

Use the Rust D-Bus ABI instead of telling users to call low-level tools.

Acceptance:

- `vinput daemon status` calls `GetStatus`, `GetAsrBackendState`, `GetRuntimeStatus`, and activation status.
- `vinput daemon start` triggers D-Bus activation or starts the user service/profile strategy used by install scripts.
- `vinput daemon stop/restart` stops/restarts the known user daemon safely and reports stale owner details.
- `vinput daemon log` surfaces user service logs or clear fallback instructions.
- `vinput recording start`, `stop [--scene]`, and `toggle` call the D-Bus service and print result payloads.

### P0.6 native sherpa desktop runtime hardening

Convert the proven local smoke into a real desktop path.

Acceptance:

- Activation service can set or wrap `LD_LIBRARY_PATH` for the selected native runtime bundle, not only local smoke.
- `sherpa-sense-voice-live` install with the downloaded model passes `runtime-status` from the generated activation environment.
- Real Fcitx session proves normal trigger -> capture -> native ASR -> commit.
- Failure message identifies mismatched `libsherpa-onnx`/`libonnxruntime` before Fcitx restart.

## P1 plan: complete daemon parity slices

### P1.1 vinput_model metadata mapping

Acceptance:

- Rust reads registry/local `vinput_model` metadata and builds native backend config from it.
- SenseVoice behavior stops relying only on directory inference.
- Dolphin and Qwen3 metadata are parsed even before their recognizer runtime is fully enabled, with clear unsupported-family errors.

### P1.2 sherpa streaming backend

Acceptance:

- Add `sherpa-streaming` backend family support behind `sherpa-onnx-backend`.
- Streaming partial events map to `RecognitionPartial`.
- Stop/final behavior matches legacy signatures and status transitions.

### P1.3 VAD and timeout semantics

Acceptance:

- `asr.vad.enabled` loads the bundled or configured VAD model and trims buffered offline audio.
- Decode timeout fields are enforced or explicitly reported as unsupported per backend.
- `doctor` reports VAD model availability.

### P1.4 text/LLM CLI parity

Acceptance:

- `vinput llm list/add/remove/edit/test` manages OpenAI-compatible providers.
- `vinput adapter list/add/start/stop` manages command adapters through config and daemon D-Bus.
- `vinput scene list/add/use/remove/edit` manages scenes, prompt files, candidate count, model, provider, and context lines.
- Local mock-server tests cover OpenAI-compatible request and response behavior.

## P2 plan: frontend and release polish

- Scene menu, ASR menu, paging/search candidates, and persistent frontend trigger config.
- Clipboard fallback for selected text where Fcitx surrounding text is unavailable.
- User-facing install guide based on `vinput init`, `model install/use`, `doctor`, and `daemon start`.
- Distro packaging after P0 live desktop native path is proven.
- GUI can be deferred until CLI/daemon experience is usable.

## Immediate next implementation slices

Pick one focused slice at a time:

1. Add `vinput-registry` live `models.json` parser and tests using the current registry SenseVoice item.
2. Add `vinput model list --json` and text output with `id`, `short_id`, `language`, `size_bytes`, `family`, and supported/unsupported marker.
3. Add `vinput model install <id|short_id> --target-root ... --dry-run/--json`, then real download/materialize.
4. Add config mutation primitives for `vinput model use`.
5. Add D-Bus client commands for `vinput daemon status` and `vinput recording start/stop`.
6. Harden activation service runtime library environment for native sherpa.

## Stop conditions

Do not claim full parity until all of these pass:

- `vinput init`
- `vinput model list/install/use/info`
- `vinput doctor` reports configured native ASR ready
- `vinput daemon status/start/stop/restart`
- `vinput recording start/stop`
- live Fcitx normal dictation commit with registry-installed model
- live command-mode replacement with selected text
- no manual JSON edits in the documented happy path

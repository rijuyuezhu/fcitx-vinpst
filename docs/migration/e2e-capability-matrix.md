# E2E capability matrix and native runtime/frontend parity plan

Date: 2026-07-28

This is the detailed capability comparison for moving the Rust rewrite from a usable CLI/daemon alpha into a real desktop replacement for legacy `fcitx5-vinput`. The terminal management surface is now largely implemented, so the active focus is native desktop execution, ASR runtime breadth, frontend UX, packaging, and remote-service parity.

## Evidence snapshot

- Rust repository: `/workspace/fcitx-vinput-rs`
- Legacy repository: `/workspace/fcitx5-vinput`
- Rust audited HEAD: `fdd4a46 feat(asr): support qwen3 registry models`
- Audit branch: `feat/accelerate-port-refactor`, based on local `main` at `cab5e0d`; remote `origin/main` was `73e1418`.
- Rust CLI surface was collected from `target/debug/vinput --help` after `cargo clean -p vinput-cli -p vinput-daemon && cargo build -q -p vinput-cli -p vinput-daemon`.
- Rust daemon surface was collected from `target/debug/vinput-daemon --help` and `crates/vinput-daemon/src/dbus_service.rs`.
- Legacy CLI surface was collected from `src/cli/config/register_*.cpp` and `src/cli/control/register_*.cpp`.
- Legacy daemon surface was collected from `src/common/dbus/dbus_interface.h`, `src/daemon/runtime/dbus_service.cpp`, `src/daemon/runtime/daemon_runtime_controller.*`, `src/daemon/asr/**`, `src/daemon/audio/**`, and `src/daemon/postprocess/**`.
- Native sherpa evidence: registry model `model.sherpa-onnx.sense-voice-zh-en-ja-ko-yue-int8` was downloaded from live `xifan2333/vinput-registry`, verified by SHA-256, extracted, and recognized bundled `test_wavs/zh.wav` as `开放时间早上九点至下午五点` through `just sherpa-sense-voice-local-smoke`. Registry model `model.sherpa-onnx.qwen3-asr-0.6b-int8` was likewise verified and recognized bundled `test_wavs/es1.wav` as `Esta prenda es amplia. Recomiendo elegir una talla menor al habitual.` through `just sherpa-qwen3-local-smoke`. The live Moonshine Tiny int8 model also passed `just sherpa-moonshine-dbus-reload-smoke`: the daemon started on mock, reported `reload_in_progress` with mock still effective, re-read the changed config, swapped to native Moonshine, and recognized the expected English WAV through D-Bus.

## Executive conclusion

Rust is **not functionally complete** versus legacy. The protocol spine, CLI/resource/config management, deterministic E2E coverage, online partial delivery, and offline VAD are now strong enough for a usable alpha. The dominant gaps have moved to real desktop proof, remaining native runtime breadth, frontend UX, packaging, and remote services.

Current parity estimate:

| Area | Estimate | Meaning |
| --- | ---: | --- |
| D-Bus ABI and daemon facade | 80-90% | Core method/signal names and payload shapes are preserved, with Rust-only diagnostics added. |
| Deterministic E2E spine | 85-90% | Command-demo, user install smokes, activation, file-input tests, and adapter lifecycle are strong. |
| Native local ASR | 90% | SenseVoice, Qwen3 ASR, and Moonshine v1 are real-WAV tested with offline Silero VAD active; online Zipformer2 CTC is real-WAV tested with legacy endpoint-rule forwarding and 200 ms warmup; transducer mapping and live D-Bus partial emission are implemented. Command timeout enforcement and explicit native timeout diagnostics are implemented. Real desktop proof, reload parity, and remaining families remain incomplete. |
| CLI user experience | 75-85% | Init, config, model, provider, hotword, device, scene, LLM, adapter, daemon, and recording commands exist; remaining work is polish, live proof, edge cases, and continued module extraction. |
| Registry/resource install | 65-75% | Live model fetch/cache/checksum/extract/install/use/remove works; provider/adapter live install and GUI resource flows remain incomplete. |
| Real desktop readiness | 45-55% | Install/probe paths exist, but real Fcitx trigger/commit with native model still needs proof and runtime library handling. |
| Full user-visible parity | 70-75% | CLI/daemon alpha is usable, but native desktop, frontend, packaging, and remote-service parity are incomplete. |

The next project target should be: **prove and harden the real Fcitx -> PipeWire -> native ASR -> partial/preedit -> commit path, then add the remaining registry model families while completing frontend UX and packaging.**

## User journeys

| Journey | Legacy behavior | Rust current behavior | Gap | Target acceptance |
| --- | --- | --- | --- | --- |
| J0: first-run init | `vinput init` creates config/dirs and default files. GUI/packaging also expect stable paths. | Rust has `vinput init` for default config, managed model/cache dirs, dry-run/JSON output, and activation-service hints. User install scripts can still write selected profile configs. | Mostly implemented; still needs broader config get/set/edit. | `vinput init` creates default config, managed dirs, activation-service hint, and prints JSON/text summary. |
| J1: list and install local ASR model | `vinput model list/add/use/info/remove` fetches registry metadata, downloads assets, materializes model, updates config. | Rust live registry flow now lists, infos, installs, uses, and removes managed ASR models with dry-run JSON/text plans. | Mostly implemented; needs more live desktop proof and model-family metadata coverage. | `vinput model list`, `vinput model install <id|short_id>`, `vinput model use <id|path|installed-name>`, `vinput model info [--installed]`, and `vinput model remove [--installed]` work with live/installed metadata and sha256 checks. |
| J2: normal dictation with local model | Fcitx trigger starts PipeWire capture, ASR, optional postprocess, commit. | Native SenseVoice and Qwen3 ASR recognize bundled WAV files; user profile install exists; the live Fcitx/PipeWire/native model path remains unproven. | Major live proof and runtime library handling. | Real desktop checklist passes: trigger, preedit, capture, inference, commit into app, `doctor` green. |
| J3: command dictation over selected text | Fcitx command trigger captures selected text or fallback, ASR command scene, LLM/text transform, replace selection. | Surrounding-text selection, primary-selection clipboard fallback, command path, and replacement logic exist; multi-application live proof remains incomplete. | Medium. | In two apps, selected text replacement works; fallback path has clear diagnostics when unavailable. |
| J4: command ASR provider | Legacy supports command batch and streaming providers. | Rust has command ASR, command WAV helper, streaming command partial tests, user profile for real command WAV, and provider add/use/edit/remove CLI. | Mostly implemented; needs live provider proof and recovery testing. | `vinput provider add/use/edit/remove` configures command ASR without hand-editing JSON and a live helper completes recognition. |
| J5: LLM/text postprocess | Legacy supports OpenAI-compatible providers, command adapters, scenes, prompt files, candidate_count, command scene. | Rust has command text adapters, OpenAI-compatible provider tests, prompt/context pieces, and LLM/adapter/scene CLI management. | Mostly implemented; real-provider desktop validation remains limited. | `vinput llm`, `vinput adapter`, and `vinput scene` commands configure and validate postprocess paths, and a live provider completes one command-mode replacement. |
| J6: adapter lifecycle | Legacy `vinput adapter start/stop` and daemon D-Bus `StartAdapter/StopAdapter` supervise local adapters and PID files. | Rust daemon can start/stop supervised command adapters, `vinput adapter start/stop` calls the daemon D-Bus lifecycle methods, dry-run includes daemon owner-probe diagnostics and next steps, and `vinput adapter status` reads `GetTextAdapterState` for PID/running diagnostics. | Mostly implemented; live desktop proof can still improve. | `vinput adapter start/stop/status/list` calls daemon and reports PID/running state. |
| J7: daemon control | Legacy `vinput daemon status/start/stop/restart/log` integrates D-Bus/systemd/logs. | Rust has `daemon status/start/reload-asr`, real user-service `stop/restart/log` execution, dry-run owner-probe plans, activation service generation, D-Bus owner/PID/executable/cmdline probe diagnostics, and expanded runtime-status diagnostics for ASR/runtime/text-adapter state. | Mostly implemented; remaining work is live desktop/non-systemd proof rather than CLI probe coverage. | User can start/stop/restart/status/log daemon from CLI, including bounded daemon log output, using activation/systemd/user-mode strategy. |
| J8: recording control from CLI | Legacy `vinput recording start/stop/toggle`. | Rust CLI calls daemon D-Bus `StartRecording`, `StartCommandRecording`, `StopRecording`, toggle via `GetStatus`, and status diagnostics; dry-run JSON/text includes owner-probe next steps. | Mostly implemented; live desktop error handling can still improve. | `vinput recording start/stop/toggle/status [--scene] [--selected-text]` works against D-Bus service and prints result/status. |
| J9: device selection | Legacy `device list/use`, PipeWire device enumeration, config mutation. | Rust has `audio-devices` diagnostics plus `vinput device list/use` for JSON/text listing and guarded `global.capture_device` mutation. | Mostly implemented; live PipeWire selection still needs desktop proof. | `vinput device list/use` maps PipeWire nodes to config and validates with doctor. |
| J10: diagnose and recover | Legacy has CLI, GUI, notifications, logs. | Rust `doctor`, `runtime-status`, `audio-devices`, live probe, daemon owner/PID/procfs diagnostics, and bounded logs are better than legacy in several areas. | Mostly done; needs live validation and message polish. | One `vinput doctor` explains config, activation, model, runtime libs, audio, daemon owner, and next command. |

## CLI command surface comparison

### Legacy CLI commands

Legacy registers these user-facing commands:

```text
init
config get/set/edit
model list/add/remove/use/info
provider list/add/use/edit/remove
hotword get/set/clear/edit; scene list/ls/add/edit/use/remove; llm list/ls/add/edit/remove/test; adapter list/ls --configured/--available; adapter add/edit/install-plan/start/stop/status/remove
device list/use
llm list/add/remove/edit/test
adapter list/add/start/stop/status
scene list/add/use/remove/edit
daemon status/start/stop/restart/log
recording start/stop/toggle
```

Legacy also supports a global `-j/--json` output mode; Rust now accepts root/global `-j/--json` and trailing `-j/--json` aliases for JSON-capable subcommands.

### Rust CLI commands

Rust currently exposes:

```text
init
protocol
config validate/example/get/set/edit
registry validate/plan/install-plan
asr-state
audio-devices
doctor
activation-service
model list/ls/info/install/add/use/remove/rm
provider list/ls/add/edit/use/remove
hotword get/set/clear/edit; scene list/ls/add/edit/use/remove; llm list/ls/add/edit/remove/test; adapter list/ls --configured/--available; adapter add/edit/install-plan/start/stop/status/remove
daemon start/status/reload-asr/stop/restart/log
recording start/stop/toggle
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

- config `get/set/edit` exists, but broader resource-aware mutations are still incomplete;
- live registry install/use/remove exists for model flow, provider/scene/LLM/adapter config UX exists, but adapter live install UX polish and desktop proof are still missing; device list/use exists;
- daemon lifecycle commands include owner/PID/procfs diagnostics, activation-service fallback steps, and tool/env override metadata; remaining risk is live desktop/non-systemd proof rather than CLI probe coverage;
- recording start/stop/toggle/status exists;
- root and trailing `-j/--json` aliases work for JSON-capable subcommands; fully uniform legacy-style output mode is still incomplete;
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
| ASR reload | Legacy reload worker prepares a warmup session and swaps later. | Rust uses a single non-blocking worker, re-reads explicit config files, prepares outside the runtime mutex, reports queued/physical progress, coalesces generations, swaps only after warmup, preserves the old backend on failure, and waits for idle when busy. | Implemented and session-bus tested; real native-model desktop reload remains unproven. |
| Audio capture | PipeWire capture with target object support, gain/normalization. | Feature-gated PipeWire recorder and diagnostics. | Partial; needs desktop proof. |
| File input | Not a first-class user path. | `--wav` and `--pcm16le` are first-class for smoke/debug. | Rust improved. |
| Command batch ASR | Implemented. | Implemented. | Mostly aligned. |
| Command streaming ASR | Implemented with partials and process protocol. | Implemented/tested in Rust command ASR path. | Mostly aligned, needs live CLI config. |
| Sherpa offline | Multiple families through C API metadata. | Feature-gated official Rust binding; SenseVoice, Qwen3 ASR, and Moonshine v1 pass real registry-model WAV smokes. | Mostly implemented; Dolphin/Paraformer and other absent registry families remain. |
| Sherpa streaming | Implemented. | Native transducer and Zipformer2 CTC mappings exist; legacy endpoint defaults/overrides are forwarded; recognizers run a 200 ms warmup; recorder callbacks stream 800-frame batches, decode hypotheses, and emit deduplicated `RecognitionPartial` signals before stop. Zipformer2 CTC passes a real registry-model WAV smoke. Native timeout configuration is explicitly diagnostic-only because official decode is synchronous. | Mostly implemented; real desktop proof and reload parity remain. |
| VAD | `vad_trimmer` with sherpa VAD model for offline recognition; streaming disables it. | Implemented for buffered offline sherpa with the tracked Silero model, legacy thresholds/durations/padding, graceful fallback, a cold-start guard, user installation, and real SenseVoice/Qwen3 WAV regressions. | Mostly aligned; real microphone proof remains. |
| Model metadata | Legacy reads registry/local `vinput_model` metadata and maps family-specific files. | Rust classifies current and legacy registry families; maps SenseVoice, Qwen3 ASR, Moonshine v1, transducer, and Zipformer2 CTC assets/config; validates required files; and preserves unknown future family names. | Partial; Dolphin, Paraformer, and other families still need runtime mapping. |
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
asr.vad.threshold
asr.vad.min_speech_duration
asr.vad.min_silence_duration
asr.vad.speech_pad_ms
asr.providers[]
llm.providers[]
llm.adapters[]
scenes.active_scene
scenes.definitions[]
```

The base schema and terminal-facing mutation layer are implemented. Remaining configuration work is concentrated in frontend settings, provider/adapter resource installation breadth, and live validation:

- Rust has guarded JSON-pointer `get/set`, existence/default scripting helpers, type-aware parsing, validated editor-based `edit`, atomic writes, and backups;
- model/provider/hotword/device/scene/LLM/adapter commands perform resource-aware validated mutations;
- model registry install/use/remove is implemented with safe fetch, checksum, extraction, materialization, and config updates;
- user install profiles remain useful for desktop activation and native-runtime experiments, not as a substitute for CLI configuration.

## Completed M3 CLI/daemon slices and remaining desktop hardening

The terminal-first CLI/daemon alpha is implemented. P0.1 through P0.5 below retain their acceptance record; P0.6 remains active under the real desktop milestone.

### Completed P0.1 live registry v2 read/list layer

Live model registry parsing is implemented in `vinput-registry` on top of the existing safe asset primitives. Provider/adapter registry installation breadth remains incomplete.

Acceptance:

- Parse `registry/models.json` into typed structs with `id`, `short_id`, `urls`, `sha256`, `size_bytes`, `language`, and raw/typed `vinput_model` metadata.
- Parse `registry/providers.json` and `registry/adapters.json` with script URLs and env specs.
- Fetch i18n maps and resolve title/description with fallback to id/short_id.
- Add fixtures copied from live registry with tests.
- `vinput model list --json` and text output show both `id` and `short_id`; legacy-compatible `model ls -a/--available` is accepted for the live/remote registry list.

### P0.2 model install/use/info/remove

Build the first user-facing model workflow.

Acceptance:

- `vinput model list --installed` scans the managed model root for local `vinput-model.json` metadata, while `model list --available`/`model ls -a` keeps the live registry view.
- `vinput model install <id-or-short-id>` downloads with mirror fallback, verifies sha256, extracts safely, and materializes under the managed model root; legacy-compatible `model add <id-or-short-id>` is accepted as the install alias.
- `vinput model info <id|short_id|path>` prints installed path, family, backend, language, model files, hotword support, and runtime readiness hints; Rust now supports live registry `id`/`short_id`, installed path metadata, and managed installed names via `--installed --model-root`.
- `vinput model use <id|short_id|path>` updates config active provider/model in the current user config; Rust supports `--installed --model-root` for managed installed names, `--dry-run` config patch preview, guarded `--output <path>` writes, `--in-place` config mutation with a `<config>.bak` backup, and `--reload-daemon` after successful writes.
- `vinput model remove <id|short_id>` removes only managed installed model directories after safety checks; Rust supports `--installed --model-root` for managed installed names, `--dry-run` planning plus guarded `--yes` deletion with model-root containment, active-config protection, and an `rm` alias.
- Local workflow coverage exercises `install -> info <path> -> use --output -> active-remove guard -> remove --yes` with a local HTTP registry/archive fixture.
- Install can optionally run `runtime-status` and reports native shared-library resolution failures.

### P0.3 config mutation core

Port enough config mutation to stop hand-editing JSON.

Acceptance:

- `vinput init` creates default config and managed directories idempotently. **Done for default config, model/cache dirs, JSON/text output, and activation-service hints.**
- `vinput config get <json-pointer>` and `set <json-pointer> <value>` work with type-aware parsing, `get --exists`, `get --default`, `get --default-string`, `--string` literal values, and validation. **Done for existing JSON pointers, missing-pointer existence/default checks, dry-run, output writes, in-place backup, and validation guards.**
- `vinput config edit` opens the config path from `$EDITOR` and validates afterward.
- Config writes use same-directory temp files and rename; `model use --in-place` preserves a `<config>.bak` backup.
- All commands have `--json` and text output.

### Completed P0.4 provider/hotword/device commands

Expose ASR provider UX before broad LLM UX.

Acceptance:

- `vinput provider list` reads configured ASR providers and reports id/type/active status in JSON and text output. **Done for read-only list/ls, config fallback, active marker, and secret-minimizing provider diagnostics.**
- `vinput provider use <id>` switches `asr.active_provider` to an existing provider. **Done for dry-run/output/in-place writes, backup/validation guards, and JSON/text output.**
- `vinput provider remove <id>` removes inactive ASR providers. **Done for dry-run/output/in-place writes, backup/validation guards, active-provider protection, and JSON/text output.**
- `vinput provider add` can configure local, command, and remote provider entries. **Done for dry-run/output/in-place writes, backup/validation guards, duplicate checks, command args/env, remote endpoint, and JSON/text output.**
- `vinput provider edit` can update existing local, command, and remote provider entries. **Done for explicit field patches, clear flags, dry-run/output/in-place writes, backup/validation guards, JSON/text output, and invalid command/remote config rejection.**
- `vinput hotword get` reports the active or selected provider hotwords path and support marker. **Done for JSON/text output and provider override.**
- `vinput hotword set/clear` manages the active or selected provider hotwords path. **Done for supported local/command providers, dry-run/output/in-place writes, backup/validation guards, and JSON/text output.**
- `vinput hotword edit` opens the configured hotwords file in an editor. **Done for provider override, dry-run, editor resolution, and JSON/text output.**
- `vinput device list/use` uses Rust PipeWire diagnostics and updates `global.capture_device`. **Done with JSON/text list, dry-run/output/in-place writes, and backup/validation guards.**
- `vinput scene list/ls/add/edit/use/remove; llm list/ls/add/edit/remove/test; adapter list/ls --configured/--available; adapter add/edit/install-plan/start/stop/status/remove` inspects and selects configured recognition scenes. **Done for dry-run/output/in-place writes, backup/validation guards, JSON/text output, and README/just smoke.**
- `vinput doctor` references provider/hotword/device commands in remediation text. **Done for JSON `next_steps` covering provider list/use, hotword get, device list/use, daemon status previews, and daemon owner/procfs probe diagnostics.**

### Completed P0.5 daemon and recording control commands

Use the Rust D-Bus ABI instead of telling users to call low-level tools.

Acceptance:

- `vinput daemon reload-asr` calls the legacy `ReloadAsrBackend` D-Bus method, which rebuilds and swaps the configured backend after successful preparation; `--dry-run` prints the planned service/object/interface/method without contacting the daemon.
- `vinput daemon status` calls `GetStatus`, `GetAsrBackendState`, and `GetRuntimeStatus`; dry-run reports the planned D-Bus owner probe, and live JSON/text output includes bus owner unique name/PID plus procfs executable/cmdline when available. Text output now includes ASR target/effective models, reload error, remote endpoints, runtime status, uptime, active session, and text-adapter count. Activation status is still tracked separately.
- `vinput daemon start` triggers D-Bus activation or starts the user service/profile strategy used by install scripts; dry-run also reports the D-Bus owner and procfs probe used to diagnose stale bus owners. `activation-service --user-status/--remove-user` report follow-up next steps for activation and owner/procfs diagnostics.
- `vinput daemon stop/restart` executes `systemctl --user stop/restart fcitx-vinput.service`, reports argv/stdout/stderr/exit status, tool/env override metadata, activation-service fallback steps, plus owner-probe next diagnostics, and keeps dry-run CI-safe.
- `vinput daemon log` executes `journalctl --user -u fcitx-vinput.service`, reports argv/stdout/stderr/exit status, tool/env override metadata, activation-service fallback steps, plus owner-probe next diagnostics, and keeps dry-run CI-safe.
- `vinput recording start`, `stop [--scene]`, and `toggle` have CLI D-Bus paths; dry-run output includes owner-probe diagnostics and next steps as the stable CI-tested plan surface.

### Active P0.6 native sherpa desktop runtime hardening

Convert the proven local smoke into a real desktop path.

Acceptance:

- Activation service can set or wrap `LD_LIBRARY_PATH` for the selected native runtime bundle, not only local smoke.
- `sherpa-sense-voice-live` install with the downloaded model passes `runtime-status` from the generated activation environment.
- Real Fcitx session proves normal trigger -> capture -> native ASR -> commit.
- Failure message identifies mismatched `libsherpa-onnx`/`libonnxruntime` before Fcitx restart.

## P1 plan: complete daemon parity slices

### P1.1 vinput_model metadata mapping

Acceptance:

- Implemented: Rust reads registry/local `vinput_model` metadata and builds native backend config from it.
- Implemented: SenseVoice uses metadata with directory inference as a compatibility fallback.
- Implemented: Qwen3 ASR maps frontend, encoder, decoder, tokenizer, generation parameters, and optional hotwords to the official Rust binding.
- Proven: live-registry Qwen3 model install, native construction, and bundled WAV recognition pass.
- Implemented: known unsupported and unknown future families retain their exact family names in diagnostics.
- Implemented: online transducer and Zipformer2 CTC typed metadata/runtime mapping; Zipformer2 CTC is registry-model WAV-proven.
- Implemented: Moonshine v1 typed metadata/runtime mapping and a live-registry Tiny int8 WAV smoke with exact transcript assertion.
- Remaining acceptance: add native layouts for Dolphin, Paraformer, and other live-registry families.

### P1.2 sherpa streaming backend

Acceptance:

- Implemented: `sherpa-streaming` transducer and Zipformer2 CTC support behind `sherpa-onnx-backend`.
- Implemented: online session partial/final event generation and stop/final behavior through the runtime path.
- Implemented: generation-scoped polling emits callback-decoded partials during recording, retains final/completed events for stop, and suppresses duplicate stop-time partials.
- Proven: a real session-bus integration test receives a partial before `StopRecording` and no duplicate afterward.

### P1.3 VAD and timeout semantics

Acceptance:

- Implemented: `asr.vad` loads the explicit, XDG-installed, system-installed, or development Silero model and trims buffered offline audio with strict legacy-compatible parameters.
- Implemented: missing/unloadable VAD assets degrade to untrimmed recognition; no-speech output preserves the original recording; a 500 ms cold-start guard protects the first syllable.
- Implemented: command ASR helpers enforce `timeout_ms` by terminating the child; native synchronous sherpa decode reports configured values as `unsupported`/diagnostic-only.
- Implemented: `vinput doctor` reports VAD enablement, ready/missing status, resolved/requested model, source, offline-only scope, strict parameters, and a missing-model repair hint.
- Implemented: `vinput doctor` reports active-provider timeout value, provider kind, `not_configured`/`enforced`/`unsupported` classification, reason, and a command-provider isolation hint for unsupported native deadlines.

### P1.4 text/LLM CLI parity

Acceptance:

- Implemented: `vinput llm list/add/remove/edit/test` manages OpenAI-compatible providers.
- Implemented: `vinput adapter list/add/start/stop/status` manages command adapters through config and daemon D-Bus.
- Implemented: `vinput scene list/add/use/remove/edit` manages scenes, prompt files, candidate count, model, provider, and context lines.
- Implemented: local mock-server tests cover OpenAI-compatible request and response behavior.
- Remaining acceptance: validate one real provider in a desktop command-mode flow.

## P2 plan: frontend and release polish

- Implemented and deterministic-test proven: minimal scene menu with Right Shift default and installed-model-aware ASR menu with F8 default; both use typed D-Bus state, keyboard/paging/digit/mouse selection, and atomic explicit-config persistence. The daemon scans flat Rust and legacy engine/model layouts outside the runtime mutex; provider/model selection queues background reload and is proven through C++/sd-bus with subsequent recognition. Six legacy-named Fcitx KeyLists plus TriggerMode are persistent/configurable with immediate reload and temporary trigger overrides; both menus use configurable previous/next-page lists with main and keypad defaults. Tap/Hold/Both, 80 ms debounce, the 300 ms hold threshold, and 500 ms release tail are deterministic-test proven, while unknown legacy fields are preserved. Both menus also implement legacy slash filtering, multi-term matching, UTF-8/Ctrl editing, and two-stage Escape. Static menu/config/result labels use a real compiled and installed zh_CN gettext catalog with English fallback. Remaining: real desktop UI proof and dynamic registry-backed model title localization.
- Live validation of the implemented primary-selection clipboard fallback across applications where Fcitx surrounding text is unavailable.
- User-facing install guide based on `vinput init`, `model install/use`, `doctor`, and `daemon start`.
- Distro packaging after P0 live desktop native path is proven.
- GUI can be deferred until CLI/daemon experience is usable.

## Immediate next implementation slices

Pick one focused slice at a time:

1. Prove real desktop SenseVoice normal dictation from Fcitx trigger through PipeWire capture to application commit.
2. Port Dolphin, Paraformer, and other remaining metadata/runtime layouts in registry-priority order.
3. Prove localized searchable scene/ASR menus, persistent trigger/paging keys, and Tap/Hold/Both timing in a real Fcitx session, then add dynamic registry-backed model titles.
4. Advance packaging, remote-service breadth, and further feature-driven CLI module extraction.

## Stop conditions

Do not claim full parity until all of these pass:

- `vinput init`
- `vinput model list/install/use/info`
- `vinput doctor` reports configured native ASR ready
- `vinput daemon status/start/stop/restart/log --lines`
- `vinput recording start/stop`
- live Fcitx normal dictation commit with registry-installed model
- live command-mode replacement with selected text
- no manual JSON edits in the documented happy path

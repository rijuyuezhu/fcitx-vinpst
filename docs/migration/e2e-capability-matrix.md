# E2E capability matrix

Reviewed: 2026-07-29

This matrix describes user-visible parity and the evidence level for each path. Status labels are:

- **implemented**: code and focused tests exist;
- **deterministic**: integration is proven without a real desktop or microphone;
- **live-proven**: verified in a real desktop session;
- **partial**: important behavior is missing;
- **missing**: no implementation exists.

## Evidence baseline

- Rust implementation reviewed through `181b9b5`.
- Legacy reference is `/workspace/fcitx5-vinput`.
- `cargo test --workspace --all-targets`, D-Bus integration, and retained-addon tests pass at the reviewed baseline.
- Native registry models are validated by model-specific local WAV smokes.
- `sherpa-native-live` installation is validated in temporary HOME environments with a copied `libsherpa-onnx` and `libonnxruntime` bundle, wrapper activation through `vinput-daemon-with-vinput-env.sh`, and `user-ime-sherpa-native-activation-smoke`.

## User journeys

| Journey | State | Evidence | Remaining work |
| --- | --- | --- | --- |
| First-run initialization | implemented | `vinput init`, managed directories, config validation, dry-run/JSON tests | Install guide polish |
| Discover and install a model | implemented | live registry list/info/install, SHA-256, safe extraction, atomic materialization | Update/packaging polish |
| Discover, install, and remove an ASR provider | implemented | current `registry/providers.json`, short ids, batch/streaming validation, mirror download, executable publication, legacy timeout/env preservation, config backup, guarded managed update; local removal guard and active-clear semantics | registry i18n polish |
| Discover and install an adapter | implemented | current `registry/adapters.json`, short ids, mirror download, executable publication, config backup, guarded managed update | i18n and update/remove polish |
| Select and reload a model | deterministic | config persistence, background prepare-before-swap, C++/D-Bus selection smokes | Real desktop reload proof |
| Normal native dictation | deterministic | native WAV -> D-Bus -> addon partial preedit -> concrete `InputContext` commit | Live PipeWire and real application |
| Command native dictation | deterministic | selected text, ASR fallback candidate, candidate selection, deletion, replacement commit | Multi-application proof and real adapter flow |
| Scene and ASR menus | deterministic | typed D-Bus state, persistent keys, filtering, paging, i18n, localized model titles | Real desktop UI proof |
| Daemon lifecycle | implemented | activation, status, reload, stop/restart/log plans and owner diagnostics | Non-systemd and upgrade hardening |
| Recording control | implemented | start/stop/toggle/status D-Bus paths | Live error handling |
| Device selection | implemented | PipeWire enumeration seam and guarded config mutation | Real device-selection proof |
| Diagnose and recover | implemented | `doctor`, runtime status, owner/PID/procfs, activation and live probe | Message refinement from live failures |
| Provider-backed text processing | deterministic | command adapters and local OpenAI-compatible mock server | One real desktop provider flow |
| User installation | deterministic | temporary-HOME addon, activation, runtime bundle, exact recognition | Real profile and packaging |

## CLI command surface comparison

The Rust CLI covers the major legacy management groups:

```text
init
config validate/example/get/set/edit
model list/info/install/add/use/remove/rm
provider list/add/edit/use/remove
hotword get/set/clear/edit
device list/use
scene list/add/edit/use/remove
llm list/add/edit/remove/test
adapter list/add/edit/install/install-plan/start/stop/status/remove
daemon start/status/reload-asr/stop/restart/log
recording start/stop/toggle/status
doctor, asr-state, audio-devices, activation-service
```

Current CLI gaps are not command-group gaps. They are adapter/provider update/remove/i18n polish, output polish, non-systemd behavior, and further feature-driven extraction from the large CLI composition file.

## Daemon capability comparison

| Capability | State | Notes |
| --- | --- | --- |
| Legacy bus/interface/path | implemented | `org.fcitx.Vinput`, `/org/fcitx/Vinput`, `org.fcitx.Vinput.Service` |
| Core methods and signals | implemented | legacy methods, `RecognitionResult`, `RecognitionPartial`, `StatusChanged`, notification signal |
| Diagnostic extensions | implemented | runtime, adapter, scene, and ASR menu state |
| Runtime state machine | deterministic | normal/command lifecycle, chunk delivery, partials, final result, error cleanup |
| ASR reload | deterministic | one non-blocking prepare-before-swap worker, config reread, generation coalescing, old-backend preservation |
| Audio capture | partial | optional PipeWire recorder exists; live capture is not proven |
| File input | implemented | WAV and PCM paths are first-class deterministic seams |
| Command ASR | implemented | batch/streaming protocols, partials, timeouts, cancellation |
| Native offline ASR | deterministic | supported registry families pass real WAV smokes |
| Native online ASR | deterministic | online transducer and Zipformer2 CTC, 200 ms warmup, partial-before-stop |
| Offline VAD | deterministic | tracked Silero model, legacy controls, fallback and diagnostics |
| Text postprocess | deterministic | command and OpenAI-compatible paths; live provider proof missing |
| Adapter supervision | deterministic | process/PID lifecycle and D-Bus control |
| Notifications and recovery | deterministic | local notifications, daemon reload failure, owner loss, cross-client status reconciliation |
| Remote text service | missing | legacy HTTP/WebSocket service is not ported |

## Registry/resource comparison

### Models

Model workflow is implemented:

- parse live registry metadata and i18n;
- resolve full ids and short ids;
- fetch with mirror fallback;
- verify declared SHA-256;
- reject unsafe archive entries;
- stage and atomically materialize across filesystems;
- persist typed `vinput-model.json` and display metadata;
- discover flat Rust and legacy engine/model layouts;
- inspect, select, reload, and safely remove managed models.

### Providers and adapters

Current adapter script installation is implemented:

- parse the upstream `registry/adapters.json` shape and resolve full or short ids;
- derive the same managed relative paths as legacy;
- try ordered script mirrors and publish an executable file;
- add blank values for declared environment keys while preserving existing values;
- write config through output/in-place/backup policy;
- update only adapters already bound to the expected managed script and refuse user-defined replacements;
- keep dry-run free of script and config writes.

Current provider registry installation is implemented with full/short id lookup, batch/streaming validation, legacy-compatible managed paths, mirror-backed executable publication, 60000 ms default timeout, environment placeholders, existing timeout/env preservation, config backup, and user-defined provider protection. Provider removal also matches legacy: local providers are protected, active non-local removal clears the active selection, and registry short ids can be resolved from an explicit catalog. Adapter selector/remove and registry i18n UX remain partial.

## Native runtime coverage

| Family/path | Evidence |
| --- | --- |
| SenseVoice | real registry-model WAV smoke |
| Qwen3 ASR | real registry-model WAV smoke |
| Paraformer | real registry-model WAV smoke |
| Dolphin | real registry-model WAV smoke |
| Moonshine v1 | local WAV and D-Bus reload smoke |
| Offline transducer | real registry-model WAV smoke |
| Online transducer | real registry-model WAV smoke and activation/addon path |
| Zipformer2 CTC | real registry-model WAV smoke |
| Command batch/streaming | deterministic process protocol tests and user profile smokes |

### P1.2 sherpa streaming backend

Implemented through D-Bus and the retained frontend:

- recorder callbacks use legacy-compatible 800-frame batches;
- online hypotheses produce deduplicated `RecognitionPartial` signals;
- stop cancels the generation poller and preserves final/completed events;
- activation-safe owner tracking accepts signals only from the current daemon owner;
- partial text reaches concrete Fcitx preedit before stop;
- final commit remains the synchronous stop outcome.

The remaining streaming gap is live PipeWire behavior in a real application, not the deterministic backend path.

## Frontend capability

Implemented and deterministically tested:

- normal, command, scene-menu, ASR-menu, previous-page, and next-page persistent KeyLists;
- Tap/Hold/Both trigger mode with legacy timing;
- scene and installed-model-aware ASR menus;
- keyboard, paging, digit, enter, escape, mouse, and slash-filter behavior;
- UTF-8 editing and multi-term search;
- zh_CN gettext catalog and English fallback;
- localized installed-model titles with stable-id fallback;
- Fcitx notifications and stderr fallback;
- daemon signal monitoring, owner-loss recovery, and external-session reconciliation;
- selected-text replacement plus primary-selection clipboard fallback.

Remaining: real desktop rendering, focus transitions, candidate interaction, and cross-application selected-text behavior.

## Release and platform gaps

- distro packaging and repository integration;
- upgrade, rollback, and uninstall policy;
- runtime-library version selection;
- remote text service parity;
- adapter selector/remove and registry i18n lifecycle;
- external-user documentation;
- optional GUI strategy.

## Immediate next work

1. Prove real desktop SenseVoice or another supported native model from Fcitx trigger through PipeWire capture to application commit.
2. Prove command replacement and clipboard fallback in at least two application/toolkit combinations.
3. Record live menu, partial-preedit, notification, daemon-loss, and reload behavior.
4. Convert live findings into focused fixes and deterministic regressions.
5. Only then advance packaging, upgrade policy, remote services, and optional GUI work.

## Stop conditions

Do not claim full parity until all of these pass in a documented installation:

```sh
vinput init
vinput model list
vinput model install <id-or-short-id>
vinput model use <id-or-short-id>
vinput doctor
vinput daemon status
vinput recording start
vinput recording stop
```

The same profile must also prove real normal dictation, live partial/preedit, command replacement, restart/reload, and clean removal without manual JSON edits.

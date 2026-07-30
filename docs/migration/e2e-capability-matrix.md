# E2E capability matrix

Reviewed: 2026-07-30

This matrix describes user-visible parity and the evidence level for each path. Status labels are:

- **implemented**: code and focused tests exist;
- **deterministic**: integration is proven without a real desktop or microphone;
- **live-proven**: verified in a real desktop session;
- **partial**: important behavior is missing;
- **missing**: no implementation exists.

## Evidence baseline

- Rust implementation reviewed on the current branch through the checked Arch package slice.
- Legacy reference is `/workspace/fcitx5-vinput`.
- `cargo test --workspace --all-targets`, D-Bus integration, and retained-addon tests pass at the reviewed baseline.
- Native registry models are validated by model-specific local WAV smokes.
- `sherpa-native-live` installation is validated in temporary HOME environments with a copied `libsherpa-onnx` and `libonnxruntime` bundle, wrapper activation through `vinput-daemon-with-vinput-env.sh`, and `user-ime-sherpa-native-activation-smoke`. `sherpa-native-command-live` adds a checked local command adapter and has its own temporary-HOME install smoke.

## User journeys

| Journey | State | Evidence | Remaining work |
| --- | --- | --- | --- |
| First-run initialization | implemented | `vinput init`, managed directories, config validation, dry-run/JSON tests | Install guide polish |
| Discover and install a model | implemented | live registry list/info/install, SHA-256, safe extraction, atomic materialization | Update/packaging polish |
| Discover, install, update, edit, and remove an ASR provider | implemented | current `registry/providers.json`, short ids, localized title/description, batch/streaming validation, mirror download, executable publication, update-by-reinstall with legacy timeout/env preservation, config backup, guarded managed update; local removal guard, active-clear semantics, and legacy-compatible referenced-script editor | None for current script registry |
| Discover, install, update, remove, and control an adapter | implemented | current `registry/adapters.json`, short ids, localized title/description, mirror download, executable publication, update-by-reinstall with config backup and guarded managed update; short-id removal and in-place managed-script cleanup without deleting user-defined files; installed-selector validation before start/stop/status D-Bus calls | None for current script registry |
| Select and reload a model | deterministic; same-provider reload live-proven | config persistence and background prepare-before-swap are deterministic; `ime-fcitx-reload-live` preserves owner/provider/model and proves post-reload acoustic recognition | Real model/provider-switch reload proof |
| Normal native dictation | live-proven | real Fcitx client, F9, acoustic PipeWire capture, streaming native partial input-panel updates, and one final commit through `ime-fcitx-native-live`; GTK3 and Qt6 evidence collectors are implemented | Run and retain successful GTK3/Qt6 application evidence |
| Command native dictation | live-proven | real Fcitx client, F10, selected surrounding text, live partials, deletion, and an `adapter-backed:` direct replacement commit from the configured local command adapter; GTK3 and Qt6 command probes are implemented | Multi-toolkit evidence, clipboard fallback, and one external provider flow |
| Scene and ASR menus | live-proven for non-mutating interaction | real Fcitx client, F7/F8 candidates, slash-filter activation, first-Escape filter clearing, second-Escape close, and zero commits; typed D-Bus state, paging, i18n, and localized titles remain deterministic | Real selection/paging and reload proof |
| Daemon lifecycle | implemented | direct per-user activation, systemd-backed system activation, default user-config discovery with persistent D-Bus updates, status, reload, stop/restart/log plans and owner diagnostics | Non-systemd and upgrade hardening |
| Recording control | implemented | start/stop/toggle/status D-Bus paths | Live error handling |
| Device selection | implemented | PipeWire enumeration seam and guarded config mutation | Real device-selection proof |
| Diagnose and recover | implemented | `doctor`, runtime status, owner/PID/procfs, activation and live probe | Message refinement from live failures |
| Provider-backed text processing | live-proven for the configured local command adapter | `sherpa-native-command-live`, runtime adapter identity, acoustic command ASR, selected-text deletion, and an `adapter-backed:` direct commit; OpenAI-compatible transport remains deterministic | One external provider flow |
| User installation | deterministic | temporary-HOME activation/runtime recognition plus the checked Arch package, repository, signature, candidate-promotion, and explicit handoff gates; see [`../architecture/packaging-contract.md`](../architecture/packaging-contract.md) | Real profile, production repository/key operations, automatic package-manager handoff, incompatible-state rollback, and external-user regression |

## CLI command surface comparison

The Rust CLI covers the major legacy management groups:

```text
init
config validate/example/get/set/edit
model list/info/install/add/use/remove/rm
provider list/add/edit/edit-script/use/remove
hotword get/set/clear/edit
device list/use
scene list/add/edit/use/remove
llm list/add/edit/remove/test
adapter list/add/edit/install/install-plan/start/stop/status/remove
daemon start/status/reload-asr/stop/restart/log
recording start/stop/toggle/status
doctor, asr-state, audio-devices, activation-service
```

Current CLI gaps are not command-group gaps. They are output polish, non-systemd behavior, and further feature-driven extraction from the large CLI composition file.

## Daemon capability comparison

| Capability | State | Notes |
| --- | --- | --- |
| Legacy bus/interface/path | implemented | `org.fcitx.Vinput`, `/org/fcitx/Vinput`, `org.fcitx.Vinput.Service` |
| Core methods and signals | implemented | legacy methods, `RecognitionResult`, `RecognitionPartial`, `StatusChanged`, notification signal |
| Diagnostic extensions | implemented | runtime, adapter, scene, and ASR menu state |
| Runtime state machine | deterministic | normal/command lifecycle, capture-before-session startup, early-chunk gating, chunk delivery, partials, explicit inferring/postprocessing phases, final result, error cleanup |
| ASR reload | deterministic | unavailable-but-running configured startup, one non-blocking prepare-before-swap worker, config reread, generation coalescing, old-backend preservation |
| Audio capture | partial | deterministic lifecycle plus live acoustic capture from the configured PipeWire source through native recognition are proven; audible ducking and broader device combinations remain |
| File input | implemented | WAV and PCM paths are first-class deterministic seams |
| Command ASR | implemented | batch/streaming protocols, partials, timeouts, cancellation |
| Native offline ASR | deterministic | supported registry families pass real WAV smokes |
| Native online ASR | deterministic | online transducer and Zipformer2 CTC, 200 ms warmup, partial-before-stop |
| Offline VAD | deterministic | tracked Silero model, legacy controls, fallback and diagnostics |
| Text postprocess | deterministic | command and OpenAI-compatible paths; live provider proof missing |
| Adapter supervision | deterministic | process/PID lifecycle and D-Bus control |
| Notifications and recovery | partial; focus, owner loss, and same-provider reload live-proven | focus handoff keeps partials/final commit on the originating context; verified daemon loss surfaces an unavailable preedit with zero commit; D-Bus activation restores the configured profile; same-provider reload is followed by successful recognition | Live notification and model/provider-switch reload proof |
| Remote text service | partial | active-provider settings, API-key/loopback policy, single input/output ownership, debounce/finalize transitions, OpenAI Realtime-compatible event shapes, Axum `/health`/browser/`/ws`/`/v1/realtime` runtime, standalone diagnostics command, normal D-Bus daemon startup/provider-selection/reload ownership, bind-failure cleanup, `SIGTERM` shutdown, redacted LAN endpoint diagnostics, local-socket tests, and private-session process smoke | Live cross-device browser proof |

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

Current model, provider, and adapter lists resolve localized titles/descriptions from the shared root-level registry i18n map while retaining stable machine ids and short selectors. Localization detects and normalizes the process locale with legacy environment priority, then merges `en_US`, the requested locale, and the automatic user `vinput/i18n.local.json` override in increasing priority; unavailable locale/local layers remain nonfatal and visible in diagnostics. Reinstalling an existing managed entry is the registry update operation: provider timeout/model/environment values and adapter environment/forward-compatible fields are preserved while the executable is replaced through the guarded publication path. Provider removal also matches legacy: local providers are protected, active non-local removal clears the active selection, and registry short ids can be resolved from an explicit catalog. `provider edit-script` resolves an exact installed id or explicit registry short id, rejects non-command providers, locates the first existing regular file referenced by the command or its arguments, and launches the selected editor without mutating config. Adapter removal resolves explicit registry short ids, removes configuration through the normal backup policy, deletes a script only when its sole configured argument exactly matches the expected managed-root path, and preserves scripts for `--output` or user-defined adapters. Adapter start, stop, and filtered status resolve exact installed ids directly or explicit registry short ids, reject selectors that are not installed, and pass only the resolved machine id to D-Bus.

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

Implemented and deterministically tested, with normal/command outcome application additionally live-proven in a real Fcitx client input context:

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

GTK3 and Qt6 text-field probes remain opt-in evidence collectors and are not live-proven without real desktop F9/F10 events. The Fcitx focus-handoff, daemon-owner-loss, same-provider reload, scene/ASR menu, normal dictation, and local command-adapter summaries passed in an installed `sherpa-native-command-live` profile. Remaining behavior includes toolkit rendering, clipboard fallback, cross-application selected-text behavior, menu selection/paging, notifications, model/provider-switch reload, and an external provider.

## Release and platform gaps

- externally hosted repository publication, production signing-key custody/rotation/revocation and independent public-key/fingerprint distribution, and non-Arch package formats;
- automatic package-manager-triggered upgrade/removal handoff, incompatible-state rollback, and destructive direct-PID stale-owner policy (explicit conditional systemd-user handoff is implemented);
- runtime-library version selection;
- remote text live cross-device browser proof;
- external-user documentation;
- optional GUI strategy.

## Immediate next work

1. Run and retain normal/command evidence from the GTK3 and Qt6 probes, including clipboard fallback where surrounding text is unavailable.
2. Record live menu selection/paging, notification, and model/provider-switch reload behavior.
3. Validate one external provider-backed command transformation.
4. Convert live findings into focused fixes and deterministic regressions.
5. Only then advance upgrade/repository policy, additional package formats, remote live proof, and optional GUI work.

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

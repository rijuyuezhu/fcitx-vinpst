# E2E capability matrix

Reviewed: 2026-07-31

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
- `sherpa-native-live` installation is validated in temporary HOME environments with a copied `libsherpa-onnx` and `libonnxruntime` bundle, wrapper activation through `vinput-daemon-with-vinput-env.sh`, and `scripts/tests/install/run-user-ime-sherpa-native-activation-smoke.sh`. `sherpa-native-command-live` adds a checked local command adapter and has its own temporary-HOME install smoke.

## User journeys

| Journey | State | Evidence | Remaining work |
| --- | --- | --- | --- |
| First-run initialization | implemented | `vinput init`, managed directories, config validation, dry-run/JSON tests | Install guide polish |
| Discover and install a model | implemented | live registry list/info/install, SHA-256, safe extraction, atomic materialization | Update/packaging polish |
| Discover, install, update, edit, and remove an ASR provider | implemented | current `registry/providers.json`, short ids, localized title/description, batch/streaming validation, mirror download, executable publication, update-by-reinstall with legacy timeout/env preservation, config backup, guarded managed update; local removal guard, active-clear semantics, and legacy-compatible referenced-script editor | None for current script registry |
| Discover, install, update, remove, and control an adapter | implemented | current `registry/adapters.json`, short ids, localized title/description, mirror download, executable publication, update-by-reinstall with config backup and guarded managed update; short-id removal and in-place managed-script cleanup without deleting user-defined files; installed-selector validation before start/stop/status D-Bus calls | None for current script registry |
| Select and reload a model/provider | live-proven within `sherpa-onnx`, across command/Whisper/remote boundaries, and for remote prepare failure | real F8/Enter selection switches streaming Zipformer to offline Paraformer, compatibility command and independent Whisper providers, and `remote-http/fixture-remote-asr`; the remote success gate proves multipart WAV/Bearer/model/language/prompt transport plus a final-only application commit, the invalid-scheme gate preserves Zipformer, and deterministic one-shot daemon cases prove plain-HTTP proxy routing, Basic authentication for direct HTTP over plain-HTTP proxies and CONNECT through both plain-HTTP and TLS-protected HTTPS proxy endpoints, `NO_PROXY`, additional PEM roots through `SSL_CERT_FILE`, retained built-in `WebPKI` verification, one local CA-signed TLS interception relay with no retained plaintext, 429/503, fail-closed 3xx handling with an untouched redirect target, distinct request and response-body timeouts, a 1 MiB cap for success and error response bodies, untrusted self-signed TLS rejection, DNS failure, connection refusal, and secret redaction; a separate persistent-daemon gate proves atomic replacement of one CA-file path with mismatch rejection and idle recovery under unchanged ASR/text owner PIDs and endpoints; every live gate restores service/profile/backup/Fcitx/backend state exactly | Real hosted-provider operations, PAC, NTLM/Kerberos, enterprise TLS-interception policy and certificate deployment, provider-specific outage/rate-limit behavior, and provider credential lifecycle and production CA distribution/revocation operations |
| Normal native dictation | live-proven through isolated injection, the default physical microphone, and real applications | real Fcitx client, F9, a preflight-verified virtual PipeWire source, the default physical ALSA Digital Microphone without playback injection, streaming partials, final commits, three same-window/same-daemon GTK4 normal cycles, and real-key GTK3, GTK4, Qt6, sandbox-attested Chromium/Ozone and VS Code/Electron, GNOME Text Editor and VS Code saved-file evidence, and kitty terminal-output evidence | Additional physical-device switching breadth |
| Command native dictation | live-proven for surrounding text, an external HTTP process, primary selection, and the double-empty rejection boundary | real Fcitx client, F10, live partials, local `adapter-backed:` commits, a loopback OpenAI-compatible provider request carrying selected/ASR text, HTTP-candidate selection and deletion/replacement, zero-delete Wayland primary-selection fallback, exact `Please select text first.` rejection before recording when surrounding and primary selections are empty, plus GTK3, GTK4, Qt6, Chromium, GNOME Text Editor, kitty, and VS Code/Electron command paths and three same-window/same-daemon GTK4 command cycles; kitty, Chromium, and VS Code prove PRIMARY-selection fallback, while Chromium and VS Code use distinct application-selection/PRIMARY sentinels and restore the current-run PRIMARY bytes | Cross-application breadth and real cloud-provider behavior |
| Scene, ASR, configuration, and notification localization | legacy locale parity complete for English fallback plus zh_CN; selection/paging, cross-provider selection/failure preservation, zh_CN menu/configuration, and zh_CN scene-info/ASR-switch/error-summary notifications live-proven | real Fcitx clients prove F7/F8 display/filter/Escape, F7 Enter scene selection, F8 Enter same-provider and external-provider selection, unavailable-remote reload failure with old-backend preservation, configured-key scene paging, a 14-target ASR menu across `1/2 -> 2/2 -> 1/2`, installed-catalog `场景 /过滤` / `模型 /过滤` plus `当前：` status text, official configuration-form English/zh_CN labels and `Tap/Hold/Both` / `单击/长按/两者` choices without saving, `语音输入` summaries, `已切换场景到“Command”。`, `已请求切换语音识别到“remote-failure-fixture”。`, verbatim daemon error bodies, old-backend preservation plus nine recovered partials, and English/original-locale restoration; all gates reject unintended commits and restore profile/service/Fcitx/backend state exactly | Additional UI locales are optional expansion beyond legacy parity |
| Daemon lifecycle | implemented | direct per-user activation, systemd-backed activation, default user-config discovery with persistent D-Bus updates, status, reload, stop/restart/log plans, owner diagnostics, guarded old-systemd `daemon-reload`/restart, guarded idle same-user old-direct termination/reactivation, private-session direct replacement proof, real user-systemd replacement proof with changed `MainPID` and incremented `NRestarts`, plus guarded no-owner/systemd/direct removal preparation with active-session refusal | Actual package-installed upgrade and live production multi-user lifecycle proof |
| Recording control | implemented | start/stop/toggle/status D-Bus paths | Live error handling |
| Device selection | live-proven for isolated PipeWire sources | typed `GetCaptureDevice`/`SetCaptureDevice`, one daemon PID and recorder instance, two real source streams with target rebuilds, atomic persistence, and exact profile restoration | Additional physical-device switching breadth |
| Diagnose and recover | implemented | `doctor`, runtime status, owner/PID/procfs, activation and live probe | Message refinement from live failures |
| Provider-backed text processing | live-proven for both the local command adapter and an independent loopback OpenAI-compatible HTTP process, with deterministic production-client network semantics | the HTTP gate proves 404 preservation, Bearer/JSON request shape, selected/raw ASR transport, exact candidate replacement, no-selection rejection and restoration; `vinput llm test` additionally proves plain-HTTP proxy routing, Basic authentication for direct HTTP over plain-HTTP proxies and CONNECT through both plain-HTTP and TLS-protected HTTPS proxy endpoints, `NO_PROXY`, additional PEM roots through `SSL_CERT_FILE`, retained built-in `WebPKI` verification, same-daemon atomic replacement of one CA-file path with mismatch rejection and idle recovery, 429/503 bodies, fail-closed 3xx handling with an untouched redirect target, distinct request and response-body timeout diagnostics, the legacy 4000 ms default for omitted text-scene timeouts, a 1 MiB cap for success and error response bodies, untrusted self-signed TLS rejection, DNS failure, connection refusal, and credential redaction | Real hosted-provider credentials, PAC, NTLM/Kerberos, enterprise TLS-interception policy and certificate deployment, provider-specific outage/rate-limit behavior, provider credential lifecycle and production CA distribution/revocation operations, and cross-application cloud recovery |
| User installation | deterministic | temporary-HOME activation/runtime recognition, the checked external-user Arch lifecycle guide and isolated command smoke, checked release-time native-runtime bundle selection, the Arch package/repository/signature/candidate pipeline, automatic ownership-verified cross-user dispatch of existing owners into the guarded upgrade handoff with no-owner skip/failure propagation, private-session direct replacement/reactivation, real user-systemd replacement/restart/restore, guarded cross-user removal dispatch/activation rollback, and unsupported-schema refusal with byte-identical user config across install/upgrade/pkgrel rollback/removal; see [`../user/installation.md`](../user/installation.md) and [`../architecture/packaging-contract.md`](../architecture/packaging-contract.md) | Actual host package-installed upgrade, live production multi-user upgrade/removal, production repository/key operations, and regression on an unrelated external machine; schema-migration rollback waits for schema 2 |

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
| Diagnostic extensions | implemented | runtime, adapter, scene, and ASR menu state; shared provider-URL diagnostics remove userinfo/fragments and redact query values, ASR/text `Debug` hides prompt/body contents, and known echoed credentials are replaced without mutating requests |
| Runtime state machine | deterministic | normal/command lifecycle, capture-before-session startup, early-chunk gating, chunk delivery, partials, explicit inferring/postprocessing phases, final result, error cleanup |
| ASR reload | deterministic | unavailable-but-running configured startup, one non-blocking prepare-before-swap worker, config reread, generation coalescing, old-backend preservation |
| Audio capture | partial | deterministic lifecycle, live typed same-daemon and same-recorder target switching across two isolated PipeWire sources, live capture from a preflight-verified virtual source, default physical ALSA Digital Microphone recognition through native ASR, and real `wpctl` duck/restore against an isolated virtual sink are proven; audible hardware-output ducking and broader physical-device combinations remain |
| File input | implemented | WAV and PCM paths are first-class deterministic seams |
| Command ASR | implemented | batch/streaming protocols, partials, timeouts, cancellation |
| Native offline ASR | deterministic | supported registry families pass real WAV smokes |
| Native online ASR | deterministic | online transducer and Zipformer2 CTC, 200 ms warmup, partial-before-stop |
| Offline VAD | deterministic | tracked Silero model, legacy controls, fallback and diagnostics |
| Text postprocess | live-proven for local adapter and loopback OpenAI-compatible provider | deterministic command/OpenAI paths plus real F10 HTTP request, candidate selection, deletion, commit, and restoration; third-party cloud behavior remains |
| Adapter supervision | deterministic | process/PID lifecycle and D-Bus control |
| Notifications and recovery | live-proven for retained local cases | focus handoff keeps partials/final commit on the originating context; verified daemon loss surfaces an unavailable preedit with zero commit; information notifications are observed from the current Fcitx PID; daemon reload failure produces a matching 5-second error notification while preserving the old backend; same-provider reload and model switching are followed by successful recognition | Broader notification categories and cross-provider recovery |
| Remote text service | partial | active-provider settings, API-key/loopback policy, single input/output ownership, debounce/finalize transitions, OpenAI Realtime-compatible event shapes, Axum `/health`/browser/`/ws`/`/v1/realtime` runtime, standalone diagnostics command, normal D-Bus daemon startup/provider-selection/reload ownership, bind-failure cleanup, `SIGTERM` shutdown, redacted LAN endpoint diagnostics, local-socket tests, private-session process smoke, a real sandboxed Chromium same-host LAN page/WebSocket path, and a fail-closed external-device challenge collector requiring explicit physical-device confirmation | Successful collector proof from another physical network device |

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
- zh_CN gettext catalog, localized scene-info/error summaries, and English fallback;
- localized installed-model titles with stable-id fallback;
- Fcitx notifications and stderr fallback;
- daemon signal monitoring, owner-loss recovery, and external-session reconciliation;
- selected-text replacement plus primary-selection clipboard fallback.

The installed `sherpa-native-command-live` profile now has retained normal/command evidence for GTK3, GTK4, Qt6, Chromium/Ozone, GNOME Text Editor, kitty, and VS Code/Electron, including ten consecutive GTK4 normal cycles and ten consecutive GTK4 command cycles in one window and one daemon owner; Chromium and VS Code additionally have explicit renderer-sandbox evidence (`NoNewPrivs=1`, seccomp filter mode, zero effective capabilities, and a nested PID namespace). Chromium also proves no browser sandbox-disable flag, while the VS Code isolated process set rejects all supported sandbox-disable flags. The profile also retains real Fcitx-client evidence for default physical-microphone dictation, local-adapter and loopback OpenAI-compatible HTTP-provider surrounding-text replacement, Wayland primary-selection fallback, scene selection, configured-key scene and ASR paging, installed-catalog zh_CN Scene/ASR titles/status and scene-info/ASR-switch/error-summary notifications with English/original-locale restoration, F8 same-provider model and external command-provider selection/reload, persisted Tap/Hold/Both timing, and information/error notifications. The HTTP provider gate proves an independent failing server process that returns 404 after real F10/ASR and preserves the selected buffer with no delete/commit, followed by an independent successful server process proving Bearer/JSON transport, selected/raw ASR request content, exact provider-candidate commit, and local-adapter restoration; it explicitly does not claim third-party cloud-service proof. The compatibility ASR cross-provider gate proves an external child-process boundary and exact restoration while reusing the original sherpa/Zipformer recognizer; the companion Whisper gate proves an independent whisper.cpp v1.9.1 process and multilingual `ggml-base.bin` model with pinned hashes, a distinct final commit, and restoration to Zipformer partials. The invalid-scheme remote gate proves old-backend preservation, exact daemon/Fcitx error notification senders and payloads, profile/backup restoration, and subsequent streaming recognition. The successful remote gate proves the implemented OpenAI-compatible HTTP runtime against an independent loopback process: multipart WAV/Bearer/model/language/prompt, final-only commit, redacted evidence, and Zipformer restoration. It is not proof of a real hosted service. The physical gate uses no playback injection; fallback/localization/trigger/menu/model/provider gates restore the original addon config, scene, profile, activation service, Fcitx process, and effective backend. The bounded GTK4 soak retained 20 F9/F10 events per mode, ten completion events, nine ready transitions, at least seven partials per cycle, and exact restoration in 91-92 seconds per mode. The production remote-ASR and text-provider clients additionally have deterministic process coverage for plain-HTTP proxy routing, proxy-URL Basic authentication including CONNECT through TLS-protected HTTPS proxy endpoints, one local CA-signed TLS interception relay with no retained plaintext, same-daemon atomic replacement of one CA-file path with mismatch rejection and idle recovery, `NO_PROXY` bypass, 429/503 bodies, request and response-body timeouts, a 1 MiB cap for success and error response bodies, self-signed TLS rejection, DNS failure, connection refusal, and credential redaction. The text-provider path separately proves the legacy 4000 ms effective timeout when a scene omits `timeout_ms`. Shared URL diagnostics remove userinfo/fragments and preserve only query keys with redacted values while request URLs keep their real query values; ASR `Debug` hides prompts, text `Debug` hides body contents, and failure bodies replace exact known credentials rather than attempting arbitrary secret discovery. Remaining behavior includes additional terminal and sandbox-packaged/application selected-text behavior and hour-scale or longer soak coverage, real hosted-ASR/provider credential lifecycle and production CA distribution/revocation operations, PAC, NTLM/Kerberos, enterprise TLS-interception policy and certificate deployment, real hosted text-provider operations and cross-application recovery, and additional physical-device switching breadth. UI locales beyond the legacy English/zh_CN set are optional expansion.

## Release and platform gaps

- externally hosted repository publication, production signing-key custody/rotation/revocation and independent public-key/fingerprint distribution, and non-Arch package formats;
- actual host package-installed upgrade and live production multi-user upgrade/removal proof; deterministic ownership-verified upgrade/removal dispatch is complete, unsupported future-schema refusal/preservation is complete, and migration rollback waits for a second production schema;
- successful remote-text challenge collection from another physical network device; the real Chromium same-host LAN path and fail-closed collector are complete;
- deferred Rust `vinput-gui` implementation after its iced-first spike criteria are met; see [`../architecture/gui-contract.md`](../architecture/gui-contract.md).

## Immediate next work

1. Validate the implemented remote ASR client against a real hosted service, including third-party DNS/TLS, PAC, NTLM/Kerberos, enterprise TLS-interception policy and certificate deployment, provider-specific rate-limit/outage behavior, and provider credential rotation/custody and production CA distribution/revocation; validate equivalent real cloud text-provider operational behavior.
2. Validate additional physical-device switching breadth and audible hardware-output ducking.
3. Keep English fallback and zh_CN parity green; add further UI locales only as optional product expansion.
4. Convert live findings into focused fixes and deterministic regressions.
5. Only then advance upgrade/repository policy, additional package formats, remote live proof, and deferred Rust GUI work.

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

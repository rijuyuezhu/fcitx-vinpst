# Target architecture

This architecture keeps the risky C++ Fcitx5 addon boundary thin while moving daemon/backend logic to Rust crates with explicit test seams.

## Top-level shape

```text
fcitx-vinpst/
  crates/
    vinpst-protocol     # stable D-Bus/JSON ABI shared by all components
    vinpst-config       # config schema, migration, normalization, validation
    vinpst-process      # bounded Unix helper supervision shared by ASR/text
    vinpst-audio        # PipeWire capture and pure PCM transforms
    vinpst-asr          # ASR traits, mock backend, command backend, sherpa-onnx backend
    vinpst-text         # scene prompts, text adapters, command-mode text transforms
    vinpst-daemon-control # shared daemon user-service command and execution boundary
    vinpst-registry     # registry metadata, download, safe extraction, materialization
    vinpst-daemon       # async runtime, D-Bus service, orchestration actors
    vinpst-fcitx-core   # pure frontend payload/session/control decisions without Fcitx types
    vinpst-fcitx-dbus   # safe blocking zbus request transport and typed reply decoding
    vinpst-fcitx-ffi    # narrow static C ABI consumed by the retained addon
    vinpst-cli          # clap CLI over protocol/config/daemon APIs
    vinpst-gui          # Rust/Iced management GUI over typed crates and D-Bus
  cpp/
    fcitx5-addon        # retained thin AddonInstance and Fcitx API adapter
  data/
  docs/
```

The current workspace implements all listed Rust crates, including the packaged GUI baseline and the safe `vinpst-fcitx-core`, `vinpst-fcitx-dbus`, and narrow `vinpst-fcitx-ffi` frontend boundary. Backend, frontend, and management features should keep landing behind these seams without changing the top-level boundaries or expanding the retained C++ component.

## Runtime actors

```text
Fcitx5 addon (C++)
  ├─ request operations -> vinpst-fcitx-ffi -> vinpst-fcitx-dbus (zbus)
  └─ Fcitx Bus signal matches
      └─ D-Bus methods/signals using vinpst-protocol ABI
          └─ vinpst-daemon::dbus
              └─ Runtime actor
                  ├─ Audio capture task          -> vinpst-audio
                  ├─ ASR session task            -> vinpst-asr
                  ├─ Postprocess task            -> vinpst-text
                  ├─ Command helper supervisor   -> vinpst-process (used by vinpst-asr / vinpst-text)
                  ├─ Remote text service task    -> vinpst-daemon::remote
                  └─ Registry/install helpers    -> vinpst-registry
```

## State machine

The daemon should make state transitions explicit and testable:

```text
Idle
  ├─ StartRecording / StartCommandRecording
  ▼
Recording
  ├─ RecognitionPartial*      # streaming backends only
  ├─ StopRecording
  ▼
Inferring
  ├─ ASR final/error
  ▼
Postprocessing?               # scene/LLM/command mode
  ├─ RecognitionResult / Notification
  ▼
Idle
```

Every transition should have a unit test before it is wired to D-Bus or PipeWire.

## Compatibility contracts

`vinpst-protocol` owns the stable contract:

- bus name: `org.fcitx.Vinpst`
- object path: `/org/fcitx/Vinpst`
- interface: `org.fcitx.Vinpst.Service`
- status strings: `idle`, `recording`, `inferring`, `postprocessing`, `error`
- recognition result JSON: `{ "commit_text": string, "candidates": [{ "text": string, "source": string }] }`
- ASR backend state JSON fields matching the original frontend expectations
- Config file baseline and diagnostics behavior: see `docs/architecture/config-contract.md`
- Registry metadata and planning behavior: see `docs/architecture/registry-contract.md`

Any change to this crate must include compatibility tests.


## Current implementation boundary

The retained C++ Fcitx5 frontend bridge talks to the Rust daemon over the existing `vinpst-protocol` D-Bus ABI. It owns Fcitx API integration, persistent trigger and paging key configuration, Fcitx key-to-semantic classification, timer scheduling and modifier-release matching, Fcitx Bus owner/signal matches, the mechanical `callAsync` dispatch required to keep latency-sensitive Start/Stop calls on the Fcitx event loop, worker-thread scheduling plus `EventDispatcher` handoff for bounded Scene/ASR snapshot refreshes, gettext lookup of stable presentation fragments, `CommonCandidateList` construction, cursor calls and callbacks, preedit/status publication, selected-text collection and command-mode replacement, notifications, and mechanical execution of completed frontend presentations. Rust-owned result and generic menu projection handles remain alive through their Fcitx callback lifetime; C++ reads only the indexed row currently needed to construct or select an Fcitx candidate. It does not own semantic D-Bus operation selection, reply tuple decoding, Scene/ASR snapshot construction, recognition JSON parsing, recording-session state, semantic trigger gating, Tap/Hold/Both decision state, daemon availability/status/reconciliation policy, live status/partial/command-mode state, partial deduplication, preedit priority, result-kind fallback, candidate-source interpretation, candidate comment/commit/cursor policy, ASR label composition, menu open/page/filter state, menu query editing/matching, menu action priority, release handling, Escape/filter transitions, page targets, visible-row selection, page clamping, snapshot/control projection, specialized Scene/ASR projection types, or duplicate candidate/control vectors.

The Rust side owns runtime state, audio, ASR, text processing, registry operations, persistent scene/model/device selection, activation diagnostics, blocking session-bus transport and typed reply decoding for bounded status/menu operations, frontend D-Bus operation selection, recognition payload normalization, final result-kind fallback, candidate-menu decisions, candidate source interpretation, localized comment selection, LLM numbering, commit/cancel flags, preferred cursor selection, selection-replacement policy, frontend recording/command-mode/active-scene state, semantic trigger gating, cross-client recording adoption and stop, Tap/Hold/Both debounce plus pending/active/release/stop decisions, daemon signal presentation and control plans, complete menu-session state, semantic menu action decisions, Scene/ASR display snapshot storage and row ordering, stable base-label fallback, provider-kind/loading/current-backend label composition, page targets and clamping, digit/Enter visible-row selection, active-row exclusion, effective-ASR fallback selection, filtering, and visible-row control-command projection. The FFI exposes opaque completed frontend presentations, menu sessions, Scene/ASR menu controllers, one generic `VinpstFcitxMenuProjection`, and a narrow prepare/pending/complete boundary for Start/Stop. Rust prepares the exact operation and argument and later consumes transport success or the `RecognitionResult` payload; retained C++ merely sends that prepared operation through Fcitx `callAsync`. Daemon refresh calls still decode replies directly into the controllers; Scene/ASR menu refreshes perform that bounded blocking transport off the Fcitx input loop and move the completed Rust-owned controller back through `EventDispatcher`, while normal start, stop, and cross-client adoption borrow the Scene controller's current snapshot; result and menu projections remain Rust-owned and expose count/summary plus indexed final rows. ASR gettext fragments cross in one `VinpstFcitxAsrMenuTextView` and are converted atomically to the safe Rust text model. C++ supplies no active Scene ids, filter queries, candidate source kinds, current-page mirrors, or source snapshot indexes. All C++ ownership of Rust objects uses one move-only RAII wrapper; C++ byte conversion is centralized in `rust_string.h`, while Rust raw UTF-8 borrowing and view construction are confined to `ffi_string.rs`. `GetSceneState`/`SetActiveScene`, `GetCaptureDevice`/`SetCaptureDevice`, and `GetAsrDisplayMenuState`/`SetActiveAsrTarget` are additive frontend-facing extensions; older target-menu methods remain for compatibility. Installed model metadata persists the full registry id and locale titles so the frontend can fall back to stable ids without network access.

Multi-field C ABI inputs cross as semantic borrowed views rather than parallel pointer/length tuples. Menu key events use `VinpstFcitxMenuKeyInputView`; frontend candidate annotations use `VinpstFcitxFrontendPresentationTextView`; ASR provider/model selection uses `VinpstFcitxAsrTargetView`; and daemon control, status-preedit, and notification planning use `VinpstFcitxDaemonControlView`, `VinpstFcitxDaemonStatusView`, and `VinpstFcitxDaemonNotificationView`. Rust validates every string field before constructing the corresponding safe core model, so partially valid input cannot update state or output.

Both transport paths use the frozen 5-second D-Bus method-call deadline. Latency-sensitive recording Start/Stop calls use Fcitx `callAsync`, so recognition and post-processing never block the Fcitx event loop; successful Start dispatch immediately publishes the recording preedit like frozen upstream, while the later Start reply only confirms transport health. Status/menu calls retain the typed blocking Rust transport but are bounded to five seconds. Any connect, method, or async transport failure activates the frozen 1.5-second daemon-sync cooldown, and a successful call or owner recovery clears it. A private-session-bus regression proves that a deliberately slow blocking daemon call returns a typed timeout. Raw-pointer translation is confined to explicitly allowed `vinpst-fcitx-ffi` modules, exported functions contain panic barriers and typed sentinel/error fallbacks, and the deterministic ABI gate compares every published `vinpst_fcitx_*` C declaration with the symbols in the built static archive. C++ smoke tests continue to exercise ownership and view behavior across the same header.

Live partials and final results arrive through the Fcitx D-Bus monitor and are immediately applied to Rust-owned frontend state. Rust retains status, partial text, command-mode association, deduplication, reset epochs, recognition payload parsing, and the rule that a partial overrides status fallback; C++ retains only watched Fcitx input-context ownership and publishes the rendered presentation. Every key event remembers the latest live InputContext like frozen upstream. If another D-Bus client starts a session, unsolicited recording/inferring/postprocessing status automatically adopts a normal Rust frontend session onto that remembered context, so partials and final text remain visible without requiring a Vinpst trigger. `RecognitionResult` drives final commit/candidate projection through a dedicated completion path that does not require a pending Stop call; the asynchronous `StopRecording` method reply is only a transport-success/failure boundary. The pending Start and Stop Fcitx slots have independent lifetimes and are reclaimed by their own callbacks rather than by frontend session reset, matching frozen ordering when `RecognitionResult`/`idle` can precede the Stop reply. Deterministic evidence covers owner loss, automatic external-session following, explicit cross-client takeover, event-loop responsiveness after Stop dispatch, background ASR reload failures, menu behavior, i18n, and outcome application. The opt-in `scripts/live/niri/run-ime-fcitx-virtual-source-live.sh` gate additionally proves real Fcitx client paths through a preflight-verified isolated PipeWire source, partial input-panel updates, final commit, command replacement, focus handoff, owner loss, and same-provider reload. `scripts/live/niri/run-ime-fcitx-physical-microphone-live.sh` proves the same normal-dictation boundary through the default physical ALSA microphone without playback injection. `scripts/live/audio/run-pipewire-device-switch-live.sh` additionally proves typed `SetCaptureDevice` persistence and source A -> source B recording through one D-Bus owner and one recorder, with a live PipeWire stream rebuild for each target. Additional physical-device switching breadth and broader GUI-toolkit behavior remain tracked in `docs/migration/e2e-replication-plan.md`.

Do not replace the Fcitx5 addon with a Rust addon until mature Rust bindings and deployment integration are validated. Packaging and service artifacts remain separate from daemon, registry, and frontend logic. System installs stage `vinpst-daemon.service` beside the D-Bus activation service, whose `SystemdService=vinpst-daemon.service` hint routes activation through the user service while retaining an `Exec=` fallback. Per-user activation generated by the CLI remains direct-`Exec=` because that helper does not install a matching systemd unit. The checked Arch `x86_64` recipe packages this system boundary plus a private sherpa/ONNX Runtime bundle; see `docs/architecture/packaging-contract.md` for identity, rpath, asset, and release-gate rules.

## TDD migration order

1. **Protocol/config locked baseline**
   - Add golden JSON tests for existing daemon/frontend payloads.
   - Add config migration tests before editing defaults.

2. **D-Bus daemon shell**
   - Add a `zbus` service that exposes legacy methods/signals.
   - Test under `dbus-run-session` using a Rust proxy.
   - Keep mock runtime behind the service first.

3. **Runtime state machine**
   - Replace the demo runtime with an actor that accepts typed commands.
   - Test busy/error/cancel/reload races without audio or ASR.

4. **Audio and ASR seams**
   - Port pure PCM transforms first.
   - Add `AsrBackend`/`RecognitionSession` trait with mock implementation.
   - Add PipeWire and sherpa-onnx behind feature/integration tests.

5. **Postprocess and command mode**
   - Port prompt rendering and command-mode behavior with fixture tests.
   - Mock LLM adapter HTTP/process edges before real adapter supervision.

6. **Registry/CLI/GUI/addon tightening**
   - Port registry parsing/download with safe extraction tests.
   - Rebuild CLI commands against typed crates.
   - Reduce C++ addon to Fcitx API, native timers, signal matches, gettext, menus, preedit, and control execution.
   - Continue the standalone management GUI in Rust as `vinpst-gui`; do not port or restore the legacy Qt GUI in C++.
   - Keep GUI state and mutations behind typed library/D-Bus APIs instead of invoking CLI text interfaces.

## What not to port mechanically

- Raw HTTP/WebSocket code in `daemon/remote`: the deterministic protocol core, Axum runtime, normal-daemon reload/shutdown ownership, redacted endpoint diagnostics, and a real Chromium same-host LAN path are implemented; repeat the browser flow from another physical device rather than porting raw socket code.
- Generic path/file/process/string utilities: prefer well-maintained Rust crates.
- C++ daemon poll loop: replace with structured async tasks and explicit shutdown.
- Ad-hoc JSON parsing: use typed serde models and golden fixtures.

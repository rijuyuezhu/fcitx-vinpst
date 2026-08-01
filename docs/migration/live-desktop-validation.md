# Live desktop validation

Use this checklist only in a real user desktop session. Deterministic smokes are prerequisites, not substitutes for live proof.

## Safety boundary

- The non-mutating probe is safe to run first.
- Installation changes the real user profile only when explicitly requested.
- Do not stop an existing D-Bus owner until its PID, executable, and command line are understood.
- Keep exact failure output and do not mark a path live-proven after a partial success.

## 1. Deterministic preflight

```sh
cd /workspace/fcitx-vinput-rs
git status --porcelain=v1 -b
just ci
just user-ime-sherpa-native-activation-smoke
just ime-fcitx-live-probe-smoke
```

For the selected model, also run its matching local WAV smoke.

## 2. Session preflight

```sh
echo "$DBUS_SESSION_BUS_ADDRESS"
fcitx5-remote --check
fcitx5-remote -n
just ime-fcitx-live-probe
```

The probe should distinguish missing addon files, missing activation, an old daemon path, a stale bus owner, missing Fcitx environment, and a session without running Fcitx5.

A failed readiness probe is not automatically a code failure. Follow the reported next action and rerun it.

## 3. Native profile installation

This step mutates the real profile:

```sh
VINPUT_USER_PROFILE=sherpa-native-live \
  VINPUT_USER_SHERPA_MODEL=/path/to/registry-installed-model \
  VINPUT_USER_SHERPA_RUNTIME_LIB_DIR=/path/to/runtime/lib \
  scripts/install-user-ime.sh
```

The installer must:

- validate typed model metadata or the supported compatibility layout;
- copy `libsherpa-onnx` and `libonnxruntime` into the user data tree;
- generate `vinput-daemon-with-vinput-env.sh` with the matching library path;
- point user D-Bus activation at that wrapper;
- run `runtime-status` through the installed bundle;
- install the retained addon, metadata, translations, VAD asset when applicable, and Fcitx environment wrapper.

The `sherpa-sense-voice-live` name remains a compatibility alias. Use `VINPUT_USER_RUNTIME_STATUS=0` only for file-placement debugging. `VINPUT_USER_NATIVE_WAV` is a deterministic activation hook, not a live microphone configuration.

Inspect the result before restarting Fcitx5:

```sh
VINPUT_USER_PROFILE=sherpa-native-live VINPUT_USER_STATUS=1 scripts/install-user-ime.sh
just user-ime-status
```

Do not continue if `doctor` or `runtime-status` reports a model or native-library construction error.

## 4. Restart Fcitx5 with the generated environment

Prefer the installed wrapper:

```sh
"$HOME/.local/share/fcitx-vinput/fcitx5-with-vinput-env.sh" -dr
```

Then rerun:

```sh
just ime-fcitx-live-probe
```

The probe must see the addon module, addon metadata, activation service, current Rust daemon diagnostics, and the restarted Fcitx environment.

## 5. Live normal dictation

Run the repeatable isolated PipeWire client gate first:

```sh
VINPUT_LIVE_NATIVE_WAV=/path/to/validated-speech.wav \
  VINPUT_LIVE_NATIVE_MODES=normal \
  just ime-fcitx-virtual-source-live
```

This creates an isolated PipeWire sink/source pair, rejects a silent 16 kHz mono preflight capture, temporarily selects the virtual source, creates a real Fcitx client input context, sends the configured F9 trigger through Fcitx, and requires non-placeholder partial input-panel updates plus one final commit. It restores the original capture configuration and daemon afterward. Physical speaker and microphone behavior are outside this proof.

In a real application text field:

1. trigger normal recording;
2. confirm recording preedit appears;
3. speak while observing streaming partial preedit for an online model;
4. stop recording;
5. confirm one final commit reaches the application;
6. repeat once to catch stale-session and second-run failures.

Record failures separately for key handling, D-Bus activation, PipeWire setup, target selection, capture format, ASR, partial signal delivery, preedit rendering, final outcome, and application commit.

## 6. Live command dictation

The same repeatable client gate covers the surrounding-text candidate path:

```sh
VINPUT_LIVE_NATIVE_WAV=/path/to/validated-speech.wav \
  VINPUT_LIVE_NATIVE_MODES=command \
  VINPUT_LIVE_VIRTUAL_OUT_DIR=target/tmp/ime-fcitx-virtual-command-live \
  just ime-fcitx-virtual-source-live
```

It requires F10 handling, selected surrounding text, live partials, `delete-surrounding-text`, and a different replacement commit. A scene with multiple candidates must expose a candidate menu; a single-result adapter may commit directly. Evidence is written under the explicit `VINPUT_LIVE_VIRTUAL_OUT_DIR`.

To prove a configured command adapter rather than the raw-ASR fallback candidate, install the native command profile with the same model/runtime inputs:

```sh
VINPUT_USER_PROFILE=sherpa-native-command-live \
  VINPUT_USER_SHERPA_MODEL=/path/to/registry-installed-model \
  VINPUT_USER_SHERPA_RUNTIME_LIB_DIR=/path/to/runtime/lib \
  scripts/install-user-ime.sh
```

After restarting Fcitx5 through the generated environment wrapper, run:

```sh
VINPUT_LIVE_NATIVE_WAV=/path/to/validated-speech.wav \
  VINPUT_LIVE_NATIVE_MODES=command \
  VINPUT_LIVE_EXPECTED_TEXT_ADAPTER=native-command-live-adapter \
  VINPUT_LIVE_EXPECTED_COMMIT_PREFIX='adapter-backed:' \
  VINPUT_LIVE_VIRTUAL_OUT_DIR=target/tmp/ime-fcitx-virtual-command-live \
  just ime-fcitx-virtual-source-live
```

The gate requires `runtime-status` to contain `native-command-live-adapter` and the selected replacement commit to begin with `adapter-backed:`. This is repeatable proof of the configured command-adapter and frontend replacement path, not proof of a remote OpenAI-compatible provider.

To prove the Wayland primary-selection fallback independently of surrounding text:

```sh
VINPUT_LIVE_NATIVE_WAV=/path/to/validated-speech.wav \
  just ime-fcitx-primary-selection-live
```

This gate owns `primary fallback fixture` through `wl-copy --primary`, creates a command input context without calling `set_surrounding_text`, requires live partials, zero `delete-surrounding-text` events, and an `adapter-backed:` commit containing the primary fixture, then restores the exact previous primary text and live capture configuration. Evidence is written under `target/tmp/ime-fcitx-primary-selection-live`.

In at least two application/toolkit combinations:

1. select text and trigger command mode;
2. confirm selected text is acquired from surrounding text or primary-selection clipboard fallback;
3. speak a command;
4. inspect candidate behavior when no text adapter is configured;
5. select a replacement candidate;
6. confirm deletion occurs before the replacement commit;
7. repeat with a configured text provider or adapter when available;
8. verify failure is safe when no selection can be acquired.

### Repeatable toolkit probes

Use the GTK3 and Qt6 probes to capture toolkit-native preedit and commit evidence. They create real text widgets and wait for a real desktop shortcut; synthetic toolkit key events are intentionally forbidden because they are not reliable under Wayland. The GTK4 gate additionally supports a repeatable niri path: it verifies the exact focused window through `niri msg`, sends real kernel F9/F10 events through writable `/dev/uinput`, routes the validated WAV through an isolated PipeWire source, and never injects a GDK key event.

```sh
VINPUT_LIVE_TOOLKIT_WAV=/path/to/validated-speech.wav \
  just ime-gtk3-native-live normal
VINPUT_LIVE_TOOLKIT_WAV=/path/to/validated-speech.wav \
  just ime-gtk3-native-live command
VINPUT_LIVE_TOOLKIT_WAV=/path/to/validated-speech.wav \
  just ime-qt6-native-live normal
VINPUT_LIVE_TOOLKIT_WAV=/path/to/validated-speech.wav \
  just ime-qt6-native-live command
VINPUT_LIVE_NATIVE_WAV=/path/to/validated-speech.wav \
  just ime-gtk4-virtual-source-live normal
VINPUT_LIVE_NATIVE_WAV=/path/to/validated-speech.wav \
  just ime-gtk4-virtual-source-live command
VINPUT_LIVE_NATIVE_WAV=/path/to/validated-speech.wav \
  just ime-gnome-text-editor-virtual-source-live normal
VINPUT_LIVE_NATIVE_WAV=/path/to/validated-speech.wav \
  just ime-gnome-text-editor-virtual-source-live command
VINPUT_LIVE_NATIVE_WAV=/path/to/validated-speech.wav \
  just ime-kitty-virtual-source-live normal
VINPUT_LIVE_NATIVE_WAV=/path/to/validated-speech.wav \
  just ime-kitty-virtual-source-live command
VINPUT_LIVE_NATIVE_WAV=/path/to/validated-speech.wav \
  just ime-chromium-virtual-source-live normal
VINPUT_LIVE_NATIVE_WAV=/path/to/validated-speech.wav \
  just ime-chromium-virtual-source-live command
```

Each toolkit window prints JSONL and exits successfully only after the expected partial plus normal commit or command replacement. GTK4 additionally retains compositor-focus JSON, exactly two successful uinput-key events, isolated-audio preflight, and profile restoration under `target/tmp/ime-gtk4-virtual-source-live/{normal,command}`. The standalone GNOME Text Editor gate records exact niri PID/window focus, real Ctrl+A/F9/F10/Ctrl+S uinput sequences, daemon `RecognitionPartial` counts, isolated-audio evidence, and the saved temporary-file bytes under `target/tmp/ime-gnome-text-editor-virtual-source-live/{normal,command}`. The kitty gate records exact niri PID/window focus, F9/F10/Enter uinput sequences, daemon partial counts, terminal output bytes, PRIMARY-selection fallback, exact original PRIMARY-selection restoration, isolated-audio evidence, and profile restoration under `target/tmp/ime-kitty-virtual-source-live/{normal,command}`. The automatic Chromium/Ozone gate records exact niri browser PID/window focus, exactly two real F9/F10 events, an auto-trigger marker that precedes at least three partials, isolated-audio/profile restoration, and a non-extension renderer with `NoNewPrivs=1`, `Seccomp=2`, zero `CapEff`, nested `NSpid`, and no browser sandbox-disable flag under `target/tmp/ime-chromium-virtual-source-live/{normal,command}`. Its command phase uses distinct page-selection and PRIMARY sentinels, requires the replacement to contain the PRIMARY sentinel, and restores the PRIMARY snapshot after outer daemon/profile cleanup; it therefore proves fallback rather than Chromium surrounding-text transport. A compiled probe or an informal success report without its application/toolkit and output is not matrix evidence.

### Focus and owner-loss probes

```sh
VINPUT_LIVE_NATIVE_WAV=/path/to/validated-speech.wav VINPUT_LIVE_NATIVE_FOCUS_SWITCH=1 VINPUT_LIVE_VIRTUAL_OUT_DIR=target/tmp/ime-fcitx-virtual-focus-live just ime-fcitx-virtual-source-live
VINPUT_LIVE_NATIVE_WAV=/path/to/validated-speech.wav VINPUT_LIVE_NATIVE_OWNER_LOSS=1 VINPUT_LIVE_VIRTUAL_OUT_DIR=target/tmp/ime-fcitx-virtual-owner-loss-live just ime-fcitx-virtual-source-live
VINPUT_LIVE_NATIVE_WAV=/path/to/validated-speech.wav VINPUT_LIVE_RELOAD_BEFORE_PROBE=1 VINPUT_LIVE_VIRTUAL_OUT_DIR=target/tmp/ime-fcitx-virtual-reload-live just ime-fcitx-virtual-source-live
```

The focus probe requires partials and the final commit to remain on the input context that started recording even after another context receives focus and sends the stop trigger. The owner-loss probe resolves the current `org.fcitx.Vinput` PID, refuses to stop an unexpected executable, terminates only a verified `vinput-daemon`, requires the frontend to replace live partials with an unavailable error preedit, and rejects any final commit. The installed `sherpa-native-command-live` profile passed both checks on 2026-07-30; owner loss was followed by successful D-Bus activation back into the same profile and adapter identity.

### Scene menu selection and paging

```sh
just ime-fcitx-menu-selection-live
just ime-fcitx-menu-paging-live
just ime-fcitx-asr-menu-paging-live
just ime-fcitx-trigger-modes-live
just ime-fcitx-localization-live
just ime-fcitx-physical-microphone-live
just ime-fcitx-cross-provider-live
just ime-fcitx-cross-provider-failure-live
just ime-fcitx-remote-asr-live
just whisper-cpp-asr-live
just ime-fcitx-whisper-provider-live
just ime-fcitx-external-text-provider-live
```

The selection gate sends real F7 and Enter events, requires the first non-active scene to become active, accepts no text commit, and restores the original scene. The scene paging gate snapshots the active profile and any backup, temporarily adds 12 inert scenes, reads the persisted Fcitx `PageNextKeys`/`PagePrevKeys`, proves `1/2 -> 2/2 -> 1/2`, closes with Escape, and restores the profile bytes, backup state, daemon, and active scene. The ASR paging gate exposes 14 uniquely titled Paraformer metadata entries through an absolute temporary model root, proves the same two-page transition with F8 and the configured keys, changes neither target nor effective backend, commits no text, and restores the activation service, profile, Fcitx process, and backend. Evidence is written under `target/tmp/ime-fcitx-menu-selection-live`, `target/tmp/ime-fcitx-menu-paging-live`, and `target/tmp/ime-fcitx-asr-menu-paging-live`.

### ASR model selection and notifications

```sh
just ime-fcitx-model-switch-live
just ime-fcitx-notification-live
just ime-fcitx-error-notification-live
just ime-fcitx-notification-localization-live
```

The model-switch gate temporarily exposes one installed Paraformer through an absolute activation `--model-root`, restarts Fcitx to clear retained menu state, sends real F8 and Enter events, waits for target/effective reload completion, and proves an offline final commit. It then restores the exact original profile, reloads the streaming Zipformer, requires live partials plus another final commit, and restores the activation service, profile/backup, Fcitx process, and backend. Evidence is under `target/tmp/ime-fcitx-model-switch-live`.

The information-notification gate observes the real `org.freedesktop.Notifications.Notify` call emitted by the current Fcitx PID after scene selection. The error gate induces a recoverable ASR reload failure, requires the daemon `DaemonNotification` and the Fcitx 5-second `dialog-error` call to carry the same error, verifies both sender PIDs, preserves the old backend, and restores the profile. The localization companion proves zh_CN scene-information text and information/error summaries, preserves the daemon's technical error body verbatim, restores English, waits for Fcitx restart settlement, and restores the original locale environment. Evidence is under `target/tmp/ime-fcitx-notification-live`, `target/tmp/ime-fcitx-error-notification-live`, and `target/tmp/ime-fcitx-notification-localization-live`.

### Live evidence recorded on 2026-07-30 and 2026-07-31

The following installed-profile summaries reported `ok: true`:

- normal dictation: seven non-placeholder partials and one final commit under `target/tmp/ime-fcitx-virtual-source-live`;
- local command adapter: eight partials, one selected-text deletion, zero candidate rows for the configured single-result scene, and an `adapter-backed:` direct commit under `target/tmp/ime-fcitx-virtual-command-live`;
- primary-selection fallback: seven partials, no surrounding text, zero deletion events, an `adapter-backed:` commit containing `primary fallback fixture`, and exact primary-text restoration under `target/tmp/ime-fcitx-primary-selection-live`;
- focus handoff: focus moved to a second Fcitx context, while secondary partial and commit counts remained zero under `target/tmp/ime-fcitx-virtual-focus-live`;
- owner loss: an unavailable error preedit, zero final commit, and successful post-test D-Bus reactivation under `target/tmp/ime-fcitx-virtual-owner-loss-live`;
- same-provider reload: the owner PID and effective provider/model remained stable, reload completed without error, and a subsequent virtual-source recognition produced seven partials plus a final commit under `target/tmp/ime-fcitx-virtual-reload-live`;
- scene and ASR menus: candidate display, slash-filter activation, first-Escape filter clearing, second-Escape close, and zero commits under `target/tmp/ime-fcitx-menu-live`;
- scene menu selection: F7/Enter changed `raw` to `__command__`, emitted zero commits, and restored `raw` under `target/tmp/ime-fcitx-menu-selection-live`;
- scene menu paging: configured `equal`/`minus` keys proved `1/2 -> 2/2 -> 1/2`, four candidates on page 2, zero commits, Escape close, and exact profile/scene restoration under `target/tmp/ime-fcitx-menu-paging-live`;
- ASR menu paging: 14 temporary uniquely titled targets proved `1/2 -> 2/2 -> 1/2`, ten/four candidates, zero commits, unchanged configured/effective Zipformer state, and exact profile/service/Fcitx restoration under `target/tmp/ime-fcitx-asr-menu-paging-live`;
- trigger modes: persisted `Tap`, `Hold`, and `Both` each consumed real F9 press/release events against mock audio; Tap release preserved recording until a second tap, Hold short press stayed idle while long press crossed the 300 ms threshold and stopped after the 500 ms release tail, Both proved both paths, and addon config/service/Fcitx/backend restoration all reported true under `target/tmp/ime-fcitx-trigger-modes-live`;
- frontend localization: the user-installed addon loaded `~/.local/share/locale/zh_CN/LC_MESSAGES/fcitx5-vinput.mo` without build-tree fallback or `VINPUT_FCITX_LOCALEDIR`, F7/F8 exposed `场景 /过滤`, `模型 /过滤`, and `当前：...`, both menus closed with Escape and zero commits, English `Scenes /filter` was restored, and profile/service/addon/backend state remained unchanged under `target/tmp/ime-fcitx-localization-live`;
- notification localization: `just ime-fcitx-notification-localization-live` observed the current Fcitx PID emit `语音输入` with `已切换场景到“Command”。`, observed a daemon-originated error with the same localized summary while preserving its technical body verbatim, restored `Voice Input` / `Switched scene to 'Command'.`, restored the original locale environment, and left profile/service/addon/module/catalog/backend bytes unchanged under `target/tmp/ime-fcitx-notification-localization-live`;
- ASR notification localization: `just ime-fcitx-asr-notification-localization-live` opened the zh_CN `模型 /过滤` menu, selected the unique invalid remote target with real F8/Enter events, observed `语音输入` / `已请求切换语音识别到“remote-failure-fixture”。` plus the localized error summary with a verbatim technical body, preserved Zipformer, recovered with nine partials and a final commit, stopped the localized writer before restoring addon bytes, and restored the original locale under `target/tmp/ime-fcitx-asr-notification-localization-live`;
- physical microphone: `just ime-fcitx-physical-microphone-live` verified that `capture_device=default` resolved to `Raptor Lake-P/U/H cAVS Digital Microphone`, the source was a physical ALSA `Audio/Source`, real F9 events produced 25 streaming partials and a non-empty final commit without `pw-play`, and profile/service/addon/backend state remained unchanged under `target/tmp/ime-fcitx-physical-microphone-live`;
- cross-provider ASR compatibility: `just ime-fcitx-cross-provider-live` exposed `external-one-shot [Command]`, used real F8/Enter events to switch the unique candidate from internal `sherpa-onnx` to `external-command`, converted legacy raw PCM to a temporary WAV, launched a traced child one-shot daemon that deliberately reused the original sherpa/Zipformer model, produced a final-only commit, removed the WAV, restored profile and `.bak` bytes, then restored Zipformer and produced streaming partials plus a final commit under `target/tmp/ime-fcitx-cross-provider-live`;
- independent Whisper ASR: `just whisper-cpp-asr-live` pinned whisper.cpp v1.9.1 commit `f049fff95a089aa9969deb009cdd4892b3e74916`, binary/model/WAV hashes, and a non-empty multilingual recognition; `just ime-fcitx-whisper-provider-live` then exposed `whisper-cpp-base-multilingual [Command]`, selected `external-whisper` with real F8/Enter events, committed `對我做了介紹,我想說的是大家如果對我的研究感興趣` through the independent process/model, cleaned the temporary WAV, restored profile and `.bak`, and restored Zipformer with eight partials plus a final commit under `target/tmp/ime-fcitx-whisper-provider-live`;
- cross-provider failure preservation: `just ime-fcitx-cross-provider-failure-live` exposed one `remote-failure-fixture [Remote]` candidate with an unsupported `ftp://` endpoint, selected it with real F8/Enter events, observed the target change while Zipformer stayed effective, matched one `asr_backend_reload_failed` signal and one 5000 ms Fcitx `dialog-error` notification to the current daemon/Fcitx PIDs, restored profile and `.bak`, and then produced streaming partials plus a final commit under `target/tmp/ime-fcitx-cross-provider-failure-live`;
- remote HTTP ASR: `just ime-fcitx-remote-asr-live` exposed `fixture-remote-asr [Remote]`, selected `remote-http` through real F8/Enter events, sent one authenticated multipart `/v1/audio/transcriptions` request with a non-silent 16 kHz mono PCM WAV plus model/language/prompt fields, committed the exact final-only response `remote-http-final`, recorded only the Bearer scheme and prompt-match boolean, restored profile and `.bak`, and restored Zipformer with streaming partials plus a final commit under `target/tmp/ime-fcitx-remote-asr-live`; the endpoint is an independent loopback fixture rather than a hosted service;
- external text provider: `just ime-fcitx-external-text-provider-live` first runs configured F10 against a loopback endpoint that returns HTTP 404 after real surrounding-text capture and ASR. The probe requires an operation error, zero commits, zero surrounding-text deletions, an unchanged selected buffer, and an idle daemon. It then restarts configured text processing with a valid independent loopback OpenAI-compatible server, sends one authenticated `/v1/chat/completions` request containing the same real selected text and raw ASR text, exposes three Fcitx candidates, immediately selects the HTTP candidate, deletes the original text, and commits the exact provider result. A third phase snapshots and clears the Wayland primary selection while providing an empty surrounding selection, requires exact `Please select text first.` feedback, zero commits/deletions, and an idle daemon with no active recording or provider request, then restores the primary selection, profile/`.bak`, service, addon config, local adapter, ASR backend, and daemon ownership under `target/tmp/ime-fcitx-external-text-provider-live`; traces record only `Bearer` as the auth scheme and never store the token, and the fixtures are not third-party cloud-service proof;
- information notification: the real Fcitx PID called `org.freedesktop.Notifications.Notify` with `fcitx5-vinput`, `dialog-information`, the selected scene, and a 3000 ms timeout under `target/tmp/ime-fcitx-notification-live`;
- daemon error notification: a recoverable invalid-model reload emitted `asr_backend_reload_failed`, and the current Fcitx PID forwarded the exact runtime error through `dialog-error` with a 5000 ms timeout while the old backend remained effective under `target/tmp/ime-fcitx-error-notification-live`;
- ASR model switch: F8/Enter selected Paraformer, offline recognition committed `对我做了介绍啊那么我想说的是呢大家如果对我的研究感兴趣呢嗯`, then exact profile restoration reloaded Zipformer and produced eight partials plus `对我做了介绍那么我想说的是呢大家如果对我的研究感兴趣呢`; service, profile, Fcitx, and backend restoration all reported true under `target/tmp/ime-fcitx-model-switch-live`.

The real-key application matrix now reports `ok: true` for eight toolkit cases, two standalone GNOME Text Editor cases, and two kitty terminal cases:

- GTK3 normal and command under `target/tmp/ime-gtk3-native-live`;
- GTK4 normal and command under `target/tmp/ime-gtk4-virtual-source-live`, with niri focus and `/dev/uinput` evidence;
- Qt6 normal and command under `target/tmp/ime-qt6-native-live`;
- Chromium/Ozone normal and command under `target/tmp/ime-chromium-native-live`, plus automatic isolated-audio reruns with renderer-sandbox, niri-focus, uinput-trigger, partial-order, and current-run PRIMARY-restoration evidence under `target/tmp/ime-chromium-virtual-source-live`;
- GNOME Text Editor normal and command under `target/tmp/ime-gnome-text-editor-virtual-source-live`, with exact niri PID/window focus, real uinput shortcuts, daemon partial counts, and saved-file-byte evidence;
- kitty normal and command under `target/tmp/ime-kitty-virtual-source-live`, with exact niri PID/window focus, terminal output bytes, PRIMARY-selection fallback/restoration, daemon partial counts, and real uinput shortcuts.

Widget-backed command cases emit `selection-ready`, but selection transport is application-dependent. GTK/Qt and the standalone editor paths require the widget selection in the final replacement; kitty proves PRIMARY fallback directly. Chromium deliberately uses different page-selection and PRIMARY sentinels and requires the PRIMARY sentinel in the result, recording that Ozone page selection is not exposed as Fcitx surrounding text in this gate. All command cases observe same-run daemon partials and require the expected `adapter-backed:` replacement. The toolkit probes combine daemon `RecognitionPartial` evidence with the final text observed by the real application widget because Fcitx input-panel preedit is not exposed as client-side preedit in every toolkit. The current Chinese Zipformer model may render English abbreviations as `<unk>`; that is an ASR model limitation, not a toolkit transport failure.

## 7. Frontend behavior

Verify in the real session:

- scene and installed-model-aware ASR menus;
- keyboard, paging, digit, mouse, slash-filter, UTF-8 edit, and Escape behavior;
- persistent normal/command/menu/paging keys;
- Tap, Hold, and Both trigger modes;
- localized labels and installed-model titles;
- local notifications and daemon-originated reload failure;
- daemon owner loss during recording;
- cross-client busy-state reconciliation;
- model selection followed by background reload.

## 8. Live PipeWire diagnostics

```sh
just pipewire-check
VINPUT_TEST_PIPEWIRE_CONTEXT=1 VINPUT_TEST_PIPEWIRE_ENUMERATE=1 VINPUT_TEST_PIPEWIRE_RECORD=1 just pipewire-live
just addon-dbus-pipewire-live
just ime-configured-pipewire-live
VINPUT_LIVE_NATIVE_WAV=/path/to/validated-speech.wav just ime-fcitx-virtual-source-live
```

These checks are intentionally outside `just ci`. Capture the selected target, S16LE/16 kHz/mono plan, and the precise setup or record failure.

## Completion criteria

Real desktop native alpha requires one documented profile where:

- Fcitx5 loads the addon after restart;
- D-Bus activation starts the installed Rust daemon through `vinput-daemon-with-vinput-env.sh`;
- live PipeWire capture reaches a supported native model;
- streaming partials render as preedit when applicable;
- final text commits once into a real application;
- command mode replaces selected text safely;
- scene/ASR menus and persistent trigger behavior work;
- diagnostics explain install, owner, runtime, audio, and frontend failures;
- `just ci` remains green afterward.

Temporary-HOME `user-ime-sherpa-native-activation-smoke` evidence proves the runtime-library and activation boundary only. The installed-profile gates now prove normal dictation through both an isolated PipeWire source and the default physical ALSA Digital Microphone, surrounding-text replacement through both a local adapter and a loopback OpenAI-compatible HTTP process, Wayland primary-selection fallback, safe command rejection when both surrounding and primary selections are empty, scene/ASR display/filter, scene selection and configured-key scene paging, installed-catalog zh_CN Scene/ASR titles/status, official English/zh_CN configuration-form labels and trigger-mode choices, and scene-info/ASR-switch/error-summary notifications with English/original-locale restoration, F8 same-provider model selection and internal-to-command-provider switching with recognition roundtrips, information/error notifications, focus handoff, verified owner loss, and same-provider reload, plus GTK3, GTK4, Qt6, Chromium, GNOME Text Editor, and kitty normal/command application paths with real desktop key events; Chromium additionally has explicit Linux renderer-sandbox evidence. The gates restore the original capture target, primary text, active scene, profile and backup bytes, activation service, Fcitx process, local adapter, and effective backend; the physical gate uses no playback injection. The compatibility external-command child reuses the original Sherpa/Zipformer model, while the companion Whisper gate proves an independent local recognizer/model. Both the remote ASR endpoint and external text provider are independent loopback HTTP fixtures: their request/response runtimes are live-proven, but hosted-service DNS/TLS/proxy/rate-limit/outage behavior and credential rotation/custody are not. Additional physical-device switching breadth, additional locales, additional terminal/sandbox packaging formats, and repeated long-session behavior remain outside the proven boundary.

The opt-in `just systemd-upgrade-live` gate uses the current user systemd manager without installing a package. It requires the existing vinput owner to be idle, saves the original D-Bus activation entry and owner executable/command entry, starts a temporary `Type=dbus` runtime unit, atomically replaces a copied daemon, and requires `NRestarts >= 1`, a changed `MainPID`, and a healthy replacement owner. It then removes the temporary unit, restores the activation file byte-for-byte through rename, reloads D-Bus activation, reactivates the original profile, verifies the original executable and command entry, and rejects unit or backup residue. Evidence is retained in `target/tmp/systemd-upgrade-live/summary.json`; this is live supervision proof, not yet an actual package-installed upgrade.

The deterministic removal gates do not mutate the live desktop profile. `just daemon-removal-handoff-smoke` uses isolated session buses to prove no-owner cleanup, idle systemd disable-and-stop, exact idle direct-owner termination, activation-cache reload before every service/process mutation, and refusal to interrupt an active recording for either a systemd or direct owner. `just package-remove-handoff-smoke` proves a first all-session `--preflight` pass with zero owner mutation, a second execution pass only after all preflights succeed, trusted `/run/user/<uid>/bus` dispatch, and exact activation-file plus D-Bus-cache restoration when any session rejects removal. This is private-session process and rollback proof, not an actual package-installed uninstall on a production multi-user machine.


The opt-in `just output-ducking-live` gate uses a mock recording session with real WirePlumber control. It creates a temporary virtual PipeWire sink/source pair, records the original default sink identity and volume, sets the virtual sink to `0.80`, selects it as default, and requires the runtime to lower it to `0.20` for a `0.25` scale and restore it to `0.80` on normal stop. The gate then restores the original sink ID/name and unchanged volume, removes the virtual nodes, and retains `target/tmp/output-ducking-live/summary.json`. It proves real `wpctl` behavior without making an audible hardware sink the test target.

The opt-in `just pipewire-device-switch-live` gate creates two isolated PipeWire sink/source pairs and continuously injects distinct non-silent tones. First, one `PipeWireAudioRecorder` records source A and then source B; both captures report the exact selected source and non-silent PCM, and both starts report `created_new_stream=true` plus `stream_reused=false`. The same gate then starts one PipeWire-backed D-Bus daemon on source A, records successfully, calls typed `SetCaptureDevice(source B)`, verifies atomic config persistence and an unchanged owner PID, and records successfully from source B. Sanitized debug evidence requires a newly created stream for each target. The gate removes all four virtual nodes and retains `target/tmp/pipewire-device-switch-live/summary.json`. It proves same-owner hot switching across two isolated sources without claiming two physical microphones.
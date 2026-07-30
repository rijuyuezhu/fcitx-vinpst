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

Run the repeatable acoustic client gate first:

```sh
VINPUT_LIVE_NATIVE_WAV=/path/to/validated-speech.wav \
  VINPUT_LIVE_NATIVE_MODES=normal \
  just ime-fcitx-native-live
```

This creates a real Fcitx client input context, sends the configured F9 trigger through Fcitx, plays the WAV through the current output device, and requires non-placeholder partial input-panel updates plus one final commit.

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
  just ime-fcitx-native-live
```

It requires F10 handling, selected surrounding text, live partials, a candidate menu, selection of the transformed candidate, `delete-surrounding-text`, and a different replacement commit. Evidence is written under `target/tmp/ime-fcitx-native-live`.

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
  just ime-fcitx-native-command-adapter-live
```

The gate requires `runtime-status` to contain `native-command-live-adapter` and the selected replacement commit to begin with `adapter-backed:`. This is repeatable proof of the configured command-adapter and frontend replacement path, not proof of a remote OpenAI-compatible provider.

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

Use the GTK3 and Qt6 probes to capture toolkit-native preedit and commit evidence. They create real text widgets and wait for a real desktop shortcut; synthetic toolkit key events are intentionally forbidden because they are not reliable under Wayland.

```sh
VINPUT_LIVE_TOOLKIT_WAV=/path/to/validated-speech.wav \
  just ime-gtk3-native-live normal
VINPUT_LIVE_TOOLKIT_WAV=/path/to/validated-speech.wav \
  just ime-gtk3-native-live command
VINPUT_LIVE_TOOLKIT_WAV=/path/to/validated-speech.wav \
  just ime-qt6-native-live normal
VINPUT_LIVE_TOOLKIT_WAV=/path/to/validated-speech.wav \
  just ime-qt6-native-live command
```

Each window prints JSONL and exits successfully only after the expected partial plus normal commit or command replacement. Evidence is written under `target/tmp/ime-gtk3-native-live` or `target/tmp/ime-qt6-native-live`. A compiled probe or an informal success report without its application/toolkit and output is not matrix evidence.

### Focus and owner-loss probes

```sh
VINPUT_LIVE_NATIVE_WAV=/path/to/validated-speech.wav just ime-fcitx-focus-live
VINPUT_LIVE_NATIVE_WAV=/path/to/validated-speech.wav just ime-fcitx-owner-loss-live
```

The focus probe requires partials and the final commit to remain on the input context that started recording even after another context receives focus and sends the stop trigger. The owner-loss probe resolves the current `org.fcitx.Vinput` PID, refuses to stop an unexpected executable, terminates only a verified `vinput-daemon`, requires the frontend to replace live partials with an unavailable error preedit, and rejects any final commit. These are destructive, opt-in real-session checks and remain unproven until their JSONL summaries report `ok: true`.

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
VINPUT_LIVE_NATIVE_WAV=/path/to/validated-speech.wav just ime-fcitx-native-live
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

Temporary-HOME `user-ime-sherpa-native-activation-smoke` evidence proves the runtime-library and activation boundary only. The opt-in `ime-fcitx-native-live` gate proves one real Fcitx client application path. GTK3, Qt6, focus-transition, and owner-loss probes now provide repeatable evidence collection, but their existence is not a pass and the manual application/toolkit matrix remains required for full desktop parity.

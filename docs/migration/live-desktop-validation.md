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
"$HOME/.local/share/fcitx-vinput/fcitx5-with-vinput-env.sh" -r
```

Then rerun:

```sh
just ime-fcitx-live-probe
```

The probe must see the addon module, addon metadata, activation service, current Rust daemon diagnostics, and the restarted Fcitx environment.

## 5. Live normal dictation

In a real application text field:

1. trigger normal recording;
2. confirm recording preedit appears;
3. speak while observing streaming partial preedit for an online model;
4. stop recording;
5. confirm one final commit reaches the application;
6. repeat once to catch stale-session and second-run failures.

Record failures separately for key handling, D-Bus activation, PipeWire setup, target selection, capture format, ASR, partial signal delivery, preedit rendering, final outcome, and application commit.

## 6. Live command dictation

In at least two application/toolkit combinations:

1. select text and trigger command mode;
2. confirm selected text is acquired from surrounding text or primary-selection clipboard fallback;
3. speak a command;
4. inspect candidate behavior when no text adapter is configured;
5. select a replacement candidate;
6. confirm deletion occurs before the replacement commit;
7. repeat with a configured text provider or adapter when available;
8. verify failure is safe when no selection can be acquired.

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

Temporary-HOME `user-ime-sherpa-native-activation-smoke` evidence proves the runtime-library and activation boundary only. It does not prove a real desktop application frontend or live PipeWire capture.

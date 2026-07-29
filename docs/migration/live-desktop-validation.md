# Live desktop validation checklist

Use this checklist only inside a real desktop session. Deterministic smokes are required, but they do not prove live Fcitx behavior.

## Preconditions

```sh
cd /workspace/fcitx-vinput-rs
git status --porcelain=v1 -b
just user-ime-command-demo-smoke
just user-ime-real-command-asr-wav-smoke
just user-ime-sherpa-native-smoke
just user-ime-sherpa-sense-voice-smoke
just ime-fcitx-live-probe-smoke
```

Confirm the session has Fcitx5 and a user bus:

```sh
echo "$DBUS_SESSION_BUS_ADDRESS"
fcitx5-remote --check
fcitx5-remote -n
```

## Non-mutating probe

```sh
just ime-fcitx-live-probe
```

Expected outcomes:

- If no user install exists, the probe reports `addon-module-missing`, `addon-metadata-missing`, `daemon-missing`, and/or `activation-service-missing`.
- If the current shell has no user D-Bus session, the probe exits early with `user-dbus-session-missing`.
- If Fcitx5 is not running on the current session bus, the probe exits early with `fcitx5-not-running`.
- If the activation service points to a different daemon path, the probe reports `activation-service-old-daemon`.
- If `org.fcitx.Vinput` is already owned but does not expose the Rust diagnostic extension, the probe reports `runtime-status-unavailable` and `stale-bus-owner`, then prints the current owner PID/exe/cmdline when D-Bus can identify it. After confirming that process is safe to stop, rerun the probe with `VINPUT_LIVE_STOP_STALE_OWNER=1` to stop the stale owner before D-Bus activation.
- If installed files exist but the running Fcitx5 process was not restarted with the generated environment, the probe reports `fcitx-env-not-restarted`.
- A failed non-mutating probe is not a code failure by itself; it records readiness and the next corrective action.

## Explicit user install and probe

This mutates the real user profile. Run it only when that is intended.

```sh
just ime-fcitx-live-command-demo-setup
```

For a lower-level install/probe sequence, run:

```sh
VINPUT_LIVE_INSTALL_COMMAND_DEMO=1 just ime-fcitx-live-probe
```

The install writes a generated Fcitx environment wrapper and a managed user autostart override. Restart through the wrapper for the current session so the running Fcitx5 process sees the user addon module directory and metadata location:

```sh
"$HOME/.local/share/fcitx-vinput/fcitx5-with-vinput-env.sh" -r
```

If the desktop ignores XDG autostart or the wrapper is unavailable, source the environment manually before launching Fcitx5:

```sh
. "$HOME/.local/share/fcitx-vinput/fcitx-vinput.env"
fcitx5 -r
```

Re-run:

```sh
just ime-fcitx-live-probe
```

## Manual behavior checks

Open a text field in a normal application and check:

1. normal trigger press shows recording preedit;
2. normal trigger release stops recording and commits deterministic command-demo output;
3. command trigger without selected text shows a clear error preedit;
4. command trigger with selected text starts command mode;
5. command trigger release replaces selected text;
6. result candidate menu can show alternatives when payload includes candidates;
7. `just user-ime-status` reports addon metadata, addon module, daemon, activation service, environment file, Fcitx env wrapper, and managed autostart override paths.

## Real command-ASR WAV helper profile

Use this as an interim command-ASR path when testing an external recognizer that accepts a WAV file path. Native Rust `sherpa-onnx` is now available for the SenseVoice offline file-input smoke, but this profile is still useful for comparing command-provider behavior and for environments where native runtime libraries are not ready.

```sh
VINPUT_USER_PROFILE=real-command-asr-wav \
  VINPUT_USER_COMMAND_ASR_WAV_COMMAND='whisper-cli -m /path/to/model.bin -f "$VINPUT_ASR_WAV"' \
  scripts/install-user-ime.sh
```

The install copies `scripts/command-asr-wav-helper.py` to the user binary directory, writes `real-command-asr-wav.json`, enables configured backends, and defaults the activation service to `--audio-backend pipewire`. `VINPUT_USER_COMMAND_ASR_WAV_COMMAND` is executed by `sh -c`; it can use `VINPUT_ASR_WAV`, `VINPUT_ASR_SAMPLE_RATE_HZ`, `VINPUT_ASR_CHANNELS`, `VINPUT_ASR_PROVIDER_ID`, `VINPUT_ASR_MODEL_ID`, and `VINPUT_ASR_HOTWORDS_FILE`.

Check the generated profile before restarting Fcitx5:

```sh
VINPUT_USER_PROFILE=real-command-asr-wav VINPUT_USER_STATUS=1 scripts/install-user-ime.sh
```

Expected diagnostic shape: `doctor` reports `target_provider_id` and `effective_provider_id` as `real-command-asr-wav`, with `has_effective_backend: true` and an empty `last_error`. This proves a real command-ASR helper profile is configured; it does not prove native `sherpa-onnx` support.


## Generic native sherpa profile

Use this when you have a registry-installed model supported by the native `sherpa-onnx` backend. This mutates the real user profile and expects a real PipeWire desktop session, so run it only with explicit user approval.

Validate the model with its matching local smoke first, then install it:

```sh
VINPUT_USER_PROFILE=sherpa-native-live \
  VINPUT_USER_SHERPA_MODEL=/path/to/registry-installed-model \
  VINPUT_USER_SHERPA_RUNTIME_LIB_DIR=/path/to/runtime/bundle \
  scripts/install-user-ime.sh
```

Typed `vinput-model.json` may select an offline or online family. The legacy `sherpa-sense-voice-live` profile remains available for a metadata-free SenseVoice directory containing `model.int8.onnx` or `model.onnx` plus `tokens.txt`.

Optional knobs:

```sh
VINPUT_USER_SHERPA_HOTWORDS_FILE=/path/to/hotwords.txt
VINPUT_USER_SHERPA_TIMEOUT_MS=30000
```

The install builds with `pipewire-backend,sherpa-onnx-backend`, copies `libsherpa-onnx*.so*` and `libonnxruntime.so*` into `~/.local/share/fcitx-vinput/runtime/lib`, writes `fcitx-vinput.env`, creates `vinput-daemon-with-vinput-env.sh`, and points the user D-Bus activation service at the wrapper. Offline metadata enables the installed Silero VAD; online metadata disables it. `runtime-status` runs by default to force model construction through the same installed runtime bundle before Fcitx5 restart.

Check it before restarting Fcitx5:

```sh
VINPUT_USER_PROFILE=sherpa-native-live VINPUT_USER_STATUS=1 scripts/install-user-ime.sh
# Lightweight status without native model construction:
VINPUT_USER_PROFILE=sherpa-native-live VINPUT_USER_STATUS=1 VINPUT_USER_RUNTIME_STATUS=0 scripts/install-user-ime.sh
```

Expected diagnostic shape: `doctor` reports `target_provider_id` and `effective_provider_id` as `sherpa-onnx`, `has_effective_backend: true`, an empty `last_error`, and an activation-service `Exec` pointing at `vinput-daemon-with-vinput-env.sh`. If model or library loading fails, keep the exact `last_error`; do not mark native ASR ready.

For deterministic activation debugging without a microphone, set `VINPUT_USER_NATIVE_WAV=/path/to/known.wav`; this opt-in test hook adds `--wav` to the generated service only. Prefer `just user-ime-sherpa-native-activation-smoke` or the pinned `just sherpa-online-transducer-user-activation-smoke`, both of which use a temporary HOME and do not mutate the real profile.

## PipeWire live checks

Run only when live capture is expected to work:

```sh
just pipewire-check
just ime-configured-pipewire-live
```

Record whether failures are from PipeWire session, capture target, stream setup, ASR, text processing, or frontend commit.

## Completion criteria for real desktop alpha

A feature is not live-done until these are true in one real desktop session:

- addon is loaded by Fcitx5 after restart;
- normal and command triggers reach the addon;
- preedit is visible;
- normal commit reaches an application;
- command mode replaces selected text;
- diagnostics explain missing install/session/backend states, stale bus ownership, old activation daemon paths, unavailable `GetRuntimeStatus`, missing Fcitx env wrapper/autostart integration, and Fcitx restart/env mismatches;
- deterministic smokes and `just ime-fcitx-live-probe-smoke` still pass after the change.


## Native sherpa desktop note

A temporary-HOME install has proven that `sherpa-native-live` can materialize a typed online transducer config, copy the validated native runtime bundle, generate wrapper-based D-Bus activation, and construct the recognizer through `runtime-status`. `just sherpa-online-transducer-user-activation-smoke` now goes further: the first `daemon status` call auto-activates the installed daemon, reports that owner immediately, and a D-Bus `StartRecording`/`StopRecording` round trip returns the exact expected transcript from the installed runtime. The remaining desktop-specific checks are:

1. install into the explicitly approved real user profile;
2. restart Fcitx through the generated environment wrapper;
3. prove normal trigger -> PipeWire capture -> native ASR -> partial/preedit -> commit in a real application.

The runtime-library activation boundary is now deterministic; it is not yet real-desktop proof.

# Live desktop validation checklist

Use this checklist only inside a real desktop session. Deterministic smokes are required, but they do not prove live Fcitx behavior.

## Preconditions

```sh
cd /workspace/fcitx-vinput-rs
git status --porcelain=v1 -b
just user-ime-command-demo-smoke
just user-ime-real-command-asr-wav-smoke
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

Use this when you have a real local ASR CLI that accepts a WAV file path, but the native Rust `sherpa-onnx` runtime is still unavailable. This mutates the real user profile and expects a real PipeWire desktop session.

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


## Native sherpa SenseVoice profile

Use this when you have a local SenseVoice model directory supported by the native `sherpa-onnx` backend. This mutates the real user profile and expects a real PipeWire desktop session.

The model directory must contain `model.int8.onnx` or `model.onnx`, plus `tokens.txt`.

```sh
VINPUT_USER_PROFILE=sherpa-sense-voice-live \
  VINPUT_USER_SHERPA_MODEL=/path/to/sherpa-onnx-sense-voice-model \
  scripts/install-user-ime.sh
```

Optional knobs:

```sh
VINPUT_USER_SHERPA_HOTWORDS_FILE=/path/to/hotwords.txt
VINPUT_USER_SHERPA_TIMEOUT_MS=30000
```

Before mutating the real user profile, validate the model and a known-good speech WAV without Fcitx5:

```sh
VINPUT_SHERPA_MODEL=/path/to/sherpa-onnx-sense-voice-model \
  VINPUT_SHERPA_WAV=/path/to/input.wav \
  just sherpa-sense-voice-local-smoke
```

The local smoke prints `runtime-status` first, then the `--once --wav` recognition payload. If it fails, fix model layout, native library loading, WAV format, or ASR decode before debugging Fcitx5.

The install builds the daemon with `pipewire-backend,sherpa-onnx-backend`, writes `sherpa-sense-voice-live.json`, enables configured backends, and defaults the activation service to `--audio-backend pipewire`. It runs `runtime-status` by default to force native model construction before Fcitx5 restart. Use `VINPUT_USER_RUNTIME_STATUS=0` only when you deliberately want to skip this model-load check. Check it before restarting Fcitx5:

```sh
VINPUT_USER_PROFILE=sherpa-sense-voice-live VINPUT_USER_STATUS=1 scripts/install-user-ime.sh
# Lightweight status without native model construction:
VINPUT_USER_PROFILE=sherpa-sense-voice-live VINPUT_USER_STATUS=1 VINPUT_USER_RUNTIME_STATUS=0 scripts/install-user-ime.sh
```

Expected diagnostic shape: `doctor` reports `target_provider_id` and `effective_provider_id` as `sherpa-onnx`, with `has_effective_backend: true` and an empty `last_error`. If model loading fails, keep the exact `last_error`; do not mark native ASR ready.

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

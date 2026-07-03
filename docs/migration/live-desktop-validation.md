# Live desktop validation checklist

Use this checklist only inside a real desktop session. Deterministic smokes are required, but they do not prove live Fcitx behavior.

## Preconditions

```sh
cd /workspace/fcitx-vinput-rs
git status --porcelain=v1 -b
just user-ime-command-demo-smoke
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
- If `org.fcitx.Vinput` is already owned but does not expose the Rust diagnostic extension, the probe reports `runtime-status-unavailable` and `stale-bus-owner`.
- If installed files exist but the running Fcitx5 process was not restarted with the generated environment, the probe reports `fcitx-env-not-restarted`.
- A failed non-mutating probe is not a code failure by itself; it records readiness and the next corrective action.

## Explicit user install and probe

This mutates the real user profile. Run it only when that is intended.

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

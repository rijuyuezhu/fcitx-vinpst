# Network live validation

These opt-in gates exercise real network clients while keeping credentials and
user configuration out of tracked or retained evidence.

## Remote text Chromium LAN gate

`run-remote-text-chromium-lan-live.sh` starts the standalone remote-text server
on `0.0.0.0`, opens its browser page in a real Chromium-family browser through
an operational non-loopback IPv4 address, and connects the Realtime-compatible
output client through loopback as required by the service policy.

```sh
VINPUT_REMOTE_TEXT_BROWSER=/usr/bin/google-chrome-unstable \
  scripts/live/network/run-remote-text-chromium-lan-live.sh
```

Requirements:

- an operational non-loopback IPv4 address;
- a Chromium-family browser;
- `curl`, `ip`, `jq`, `python3`, and `ss`;
- no conflicting listener on the two temporary ports selected by the gate.

The gate requires:

- `/health` and the browser page to be reachable through the LAN address;
- browser authentication, output connection, and an enabled editor;
- one exact `input_audio_buffer.committed`, transcription `delta`, and
  transcription `completed` sequence;
- established non-loopback browser sockets plus a loopback output socket;
- a Chromium renderer with `NoNewPrivs=1`, seccomp filter mode, zero effective
  capabilities, a nested PID namespace, and no sandbox-disable browser flag;
- removal of the temporary Chrome profile and release of the listener;
- zero API-key bytes in retained evidence.

Evidence is written to `target/tmp/remote-text-chromium-lan-live/`.

This is **same-host LAN transport proof**: the browser uses the host's
non-loopback address rather than `127.0.0.1`, but the browser and server run on
the same physical machine. It is not proof from another phone, tablet, laptop,
or network peer. Cross-device validation remains a separate manual gate.

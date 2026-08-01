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


## External physical-device collector

`run-remote-text-external-device-live.sh` prepares the remaining cross-device
gate. It starts the same standalone server, prints a one-time URL and random
challenge only to the controlling terminal, and waits for another physical
device to submit that exact challenge. A non-interactive invocation without a
terminal is rejected so the one-time key cannot fall into redirected logs.

```sh
VINPUT_REMOTE_TEXT_CONFIRM_PHYSICAL_DEVICE=1 \
VINPUT_REMOTE_TEXT_EXTERNAL_TIMEOUT=180 \
  scripts/live/network/run-remote-text-external-device-live.sh
```

The collector succeeds only when all of the following are true:

- the committed/delta/completed event sequence contains the exact challenge;
- the Realtime output connection is loopback;
- at least one established input peer differs from every address assigned to
  the server host;
- the operator explicitly confirms that peer is another physical device rather
  than a VM/container or namespace on the server host;
- real `/usr/bin/ip` and `/usr/bin/ss` provide the address evidence;
- the one-time key is absent from retained files;
- the configuration is deleted and the listener is released.

Successful evidence is written to
`target/tmp/remote-text-external-device-live/summary.json` with
`same_host_lan_proof=false`, `distinct_network_peer_proof=true`,
`operator_confirmed_physical_device=true`, and `cross_device_proof=true`.

The wrapper refuses to start unless
`VINPUT_REMOTE_TEXT_CONFIRM_PHYSICAL_DEVICE=1` is set deliberately and a
controlling terminal is available. A no-device run must time out, return
nonzero, omit `summary.json`, remove its configuration, and stop the server.
The collector was verified to fail closed that way on 2026-08-01. It has not
yet completed successfully because no remote worker or Android device was
connected during that validation session.

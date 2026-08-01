# Scripts

The script tree is organized by execution boundary rather than filename prefix.
Run scripts from the repository root unless a script explicitly says otherwise.
All shell entry points locate the repository root by walking upward, so their
behavior does not depend on their current directory depth.

| Directory | Purpose | Expected environment |
| --- | --- | --- |
| `tests/` | Deterministic format, lint, unit/integration, and process smoke gates | Suitable for CI |
| `release/` | Arch package, repository, signing, manifest, and candidate operations | Release tooling; full package smoke may download pinned assets |
| `install/` | Per-user daemon/Fcitx installation and activation helpers | Mutates the current user's XDG directories |
| `fixtures/` | Standalone protocol, ASR, text-provider, and WAV fixtures | Called by tests and live gates |
| `live/audio/` | Opt-in PipeWire, WirePlumber, microphone, and recognizer probes | Real user audio session |
| `live/system/` | Opt-in host lifecycle probes | Real user systemd/session bus |
| `live/network/` | Opt-in HTTP/WebSocket/browser transport probes | Operational network interface and real client application |
| `live/niri/` | Local desktop automation for the project's niri/Wayland test host | niri, Fcitx, uinput, GUI applications |
| `tools/` | Developer diagnostics and benchmarks | Manual use |

The `justfile` intentionally exposes only broad workflows. Use `just check` for
the deterministic gate, `just package-smoke` for the complete Arch release
gate, and invoke specialized scripts directly. For example:

```sh
scripts/tests/daemon/run-daemon-handoff-smoke.sh
scripts/live/audio/run-output-ducking-live.sh
scripts/live/network/run-remote-text-chromium-lan-live.sh
scripts/live/niri/run-ime-gtk4-native-live.sh command
```

Generated `__pycache__` directories are not part of the tree and are removed by
the script lint/check entry points. No tracked script is treated as disposable:
a script should be removed only after its callers and documented behavior are
removed or replaced.

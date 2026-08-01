# Live validation

Live scripts are opt-in evidence gates. They are intentionally excluded from
`just check` because they require real user services, devices, applications, or
host lifecycle changes.

- `audio/` uses the current PipeWire/WirePlumber session or an isolated virtual
  graph and restores the original graph/configuration before exit.
- `system/` exercises the current user systemd/session-bus lifecycle and must
  restore the original owner and activation metadata.
- `network/` exercises real HTTP/WebSocket/browser boundaries without changing
  the installed user profile; each gate states whether it is loopback, same-host
  LAN, or true cross-device evidence.
- `niri/` contains desktop automation tied to the project's niri/Wayland test
  host. It is not a portable generic desktop test suite.

Each live runner must fail closed when its prerequisites or restoration checks
are not satisfied. Evidence under `target/tmp/` is diagnostic output, not a
tracked fixture.

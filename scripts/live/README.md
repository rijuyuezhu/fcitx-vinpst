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
  host. It includes complete-control, isolated desktop-opener/startup-notification,
  private-session XDG FileChooser portal and daemon/config-mutation gates, retained-Fcitx, and bounded-soak gates;
  it is not a portable generic desktop test suite. The GTK4 bounded-soak runner
  accepts 10-20 cycles and records bounded evidence separately from any future
  hour-scale soak.
- `x11/` forces the Rust management GUI onto X11 through the same host's
  xwayland-satellite instance. It verifies native X11 window properties,
  clipboard delivery, keyboard control, and Fcitx5 XIM while using Wayland only
  to focus the rootless client and restore the host clipboard exactly.

Each live runner must fail closed when its prerequisites or restoration checks
are not satisfied. Evidence under `target/tmp/` is diagnostic output, not a
tracked fixture.

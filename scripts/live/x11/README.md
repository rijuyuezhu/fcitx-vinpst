# X11 GUI live validation

This directory contains the management-GUI interaction gate for the X11 backend.
It runs inside the project's niri test session through xwayland-satellite, but the
GUI itself is forced onto X11 with `WINIT_UNIX_BACKEND=x11` and is launched with
`WAYLAND_DISPLAY` removed.

Run it after building the GUI:

```sh
cargo build -p vinput-gui
scripts/live/x11/run-gui-interaction-live.sh
```

The gate requires:

- a live niri session with a working Xwayland display;
- `xwininfo`, `xprop`, and `xclip`;
- writable `/dev/uinput` for repository-owned hardware-key probes;
- Fcitx5 with the configured Rime input method;
- `wl-copy` and `wl-paste` only for exact host clipboard restoration.

It proves that the forced-X11 process owns an X11 client window with a matching
`_NET_WM_PID`, observes UTF-8 dynamic titles through `_NET_WM_NAME`, exercises
complete keyboard traversal and activation, reads GUI clipboard output through
`xclip`, and commits non-ASCII text through Fcitx5's XIM path. English and zh_CN
instances use isolated XDG roots. Cleanup restores the original Fcitx state,
input method, and text clipboard and rejects any tracked GUI process residue.

Evidence is written under `target/tmp/gui-x11-interaction-live/` by default.
The summary records only booleans, titles, transport names, and committed UTF-8
byte length; it never stores the committed input text.

This is X11/Xwayland backend proof, not proof for a standalone non-composited Xorg
session or an assistive-technology accessibility tree.

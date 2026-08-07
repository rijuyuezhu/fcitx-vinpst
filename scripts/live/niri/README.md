# niri desktop gates

These runners automate the maintainer's niri/Wayland desktop. They may use
`niri msg`, kernel uinput, Fcitx input contexts, PRIMARY selection ownership,
PipeWire virtual nodes, and application-specific window matching.

`probes/` contains the small GTK, Qt, Chromium, and Fcitx clients compiled or
executed by the runners. The probes are test instruments rather than product
code.

Common entry points include:

```sh
scripts/live/niri/run-ime-fcitx-virtual-source-live.sh
scripts/live/niri/run-ime-gtk4-native-live.sh normal
scripts/live/niri/run-ime-chromium-virtual-live.sh command
scripts/live/niri/run-ime-kitty-live.sh command
```

Management-GUI collectors that discovered controls by counting `Tab` or `Shift+Tab`
focus steps are intentionally not retained. GUI behavior is covered by deterministic
state, widget-action, persistence, and protocol tests; any future desktop GUI
automation must target a stable semantic control identity instead of positional
focus order.

Do not claim portability from these results. A new compositor/backend needs its
own focus, key-injection, selection, and cleanup implementation or a shared
portable abstraction with equivalent evidence.

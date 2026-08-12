# Accessibility

## Support policy

Vinpst supports complete keyboard operation of the Rust management GUI. It does **not** support screen-reader navigation or expose a platform assistive-technology semantic tree.

This is an explicit support boundary, not an unverified claim. The current Iced 0.14 GUI has no stable platform accessibility-tree integration, so Vinpst does not claim Orca, AT-SPI, or equivalent screen-reader compatibility for the management window.

All ordinary management and diagnostic tasks remain available without the GUI through the `vinpst` command. Fcitx-only trigger keys and trigger mode remain available through the official Fcitx configuration tool.

## Supported keyboard operation

The management GUI supports:

- `Tab` and `Shift+Tab` to move through every enabled control;
- `Enter` or `Space` to activate buttons and checkboxes;
- arrow keys to change selectors and sliders;
- `Escape` to clear focus and restart traversal;
- `Ctrl+1` through `Ctrl+4` to open Control, Resources, LLM, and Hotwords;
- normal clipboard editing and Fcitx preedit/commit in text fields on Wayland and X11.

Disabled controls are omitted from the focus order. These keyboard paths have real niri/Wayland and X11/Xwayland evidence, but that evidence is not screen-reader proof.

## Non-GUI management paths

Start with:

```sh
vinpst --help
vinpst <command> --help
```

Use these command groups instead of the management GUI:

| Task | Command group |
| --- | --- |
| Validate or safely edit application configuration | `vinpst config` |
| Start, stop, restart, reload, or inspect the daemon | `vinpst daemon` |
| Start, stop, toggle, or inspect recording | `vinpst recording` |
| List or select capture devices | `vinpst device` |
| Install, inspect, select, or remove models | `vinpst model` |
| Add, install, edit, select, or remove ASR providers | `vinpst provider` |
| Configure or edit hotword files | `vinpst hotword` |
| Add, edit, select, or remove scenes | `vinpst scene` |
| Add, test, edit, or remove LLM providers | `vinpst llm` |
| Install, edit, start, stop, or remove text adapters | `vinpst adapter` |
| Diagnose configuration, audio, ASR, activation, and addon state | `vinpst doctor` |

Prefer `--json` for structured output and check the command exit status. Mutating commands generally provide `--dry-run`, explicit input/output paths, and guarded in-place replacement; review each subcommand's help before changing state.

## Fcitx frontend settings

Trigger keys, scene/ASR menu keys, paging keys, and Tap/Hold/Both mode belong to the Fcitx addon configuration rather than the daemon JSON configuration.

Run:

```sh
fcitx5-configtool
```

Then open **Addons** and select **Vinpst**. The accessibility behavior of that window is provided by the installed Qt/Fcitx desktop stack and is not part of Vinpst's own screen-reader support claim.

### Terminal-only frontend fallback

If the Fcitx configuration window is not usable, edit the addon file with a terminal editor. Do this only while no recording is active, and always keep a backup:

```sh
config_root="${XDG_CONFIG_HOME:-$HOME/.config}"
config="$config_root/fcitx5/conf/vinpst.conf"
test -f "$config" || { echo "Vinpst Fcitx configuration does not exist: $config" >&2; exit 1; }
backup="$config.bak.$(date +%Y%m%d-%H%M%S)"
cp --preserve=mode,timestamps -- "$config" "$backup"
"${EDITOR:-vi}" "$config"
fcitx5-remote --check -r
```

The key-list sections are `[TriggerKey]`, `[CommandKeys]`, `[SceneMenuKey]`, `[AsrMenuKey]`, `[PagePrevKeys]`, and `[PageNextKeys]`. Entries are numbered from zero, for example:

```ini
[CommandKeys]
0=F10

[PagePrevKeys]
0=minus
```

`TriggerMode` is a scalar and accepts exactly `Tap`, `Hold`, or `Both`:

```ini
TriggerMode=Both
```

Preserve unknown sections and keys. If Fcitx does not accept the edited file, restore the timestamped backup and run `fcitx5-remote --check -r` again. Direct editing is a last-resort accessibility fallback; the official Fcitx form remains safer for constructing key names.

## Inspecting the declared boundary

The packaged GUI reports the same policy without opening a window:

```sh
vinpst-gui --check --offline | jq '.interaction'
```

The snapshot reports the missing accessibility tree, the unsupported screen-reader policy, the supported keyboard capabilities, and the fallback command surfaces. It contains no provider secrets.

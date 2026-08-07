# Quick start

This guide assumes Vinpst is installed and the `vinpst` command is available.

## 1. Initialize user state

Preview the paths first:

```sh
vinpst init --dry-run --json
```

Create the default configuration and managed data/cache directories:

```sh
vinpst init
```

The main configuration file is:

```text
${XDG_CONFIG_HOME:-$HOME/.config}/fcitx-vinpst/config.json
```

`vinpst init` does not overwrite an existing configuration unless `--force` is explicitly passed.

## 2. Start Vinpst and reload Fcitx

```sh
systemctl --user enable --now vinpst-daemon.service
fcitx5 -r
```

Check the daemon and installation:

```sh
vinpst daemon status
vinpst doctor
```

A fresh configuration intentionally has no selected ASR model yet. The daemon remains available in this setup state so the GUI can install resources and reload the backend. Before model selection, `vinpst doctor` reports `"ok": false` with `"status": "setup-required"`; treat that as an incomplete setup rather than a daemon startup failure. Resolve any additional audio, activation, or addon errors before testing dictation.

## 3. Install and select an ASR model

The easiest path is the **Vinpst Configuration** desktop application:

```sh
vinpst-gui
```

Open **Resources**, choose a compatible model, install it, and make it active.

The equivalent CLI flow is:

```sh
vinpst model list --available
vinpst model install <model-id-or-short-id>
vinpst model use <model-id-or-short-id> --in-place --reload-daemon
```

Use `--dry-run --json` on install or selection commands when you want to inspect the planned paths and changes first.

## 4. Dictate

Default Fcitx keys in the current Vinpst configuration are:

| Action | Default key |
| --- | --- |
| Normal dictation | Right Control |
| Command editing | F10 |
| ASR provider/model menu | F8 |
| Scene menu | Right Shift |

The default trigger mode is **Both**:

- a short press toggles recording;
- holding the key uses push-to-talk behavior.

Focus a text field, press the normal dictation key, speak, and stop recording. Partial recognition appears as preedit when the active backend supports streaming; the final result is committed through Fcitx.

## 5. Try command editing

1. Select text in an application.
2. Press or hold `F10` according to the configured trigger mode.
3. Speak an instruction such as “translate this into English” or “make this more concise.”
4. Stop recording and choose a candidate when multiple results are returned.

Command mode requires a configured command scene and either a text adapter or an LLM provider. See [Scenes and text processing](scenes.md).

## 6. Customize keys

Open the Fcitx configuration tool, find the **Vinpst** addon, and edit the trigger, command, scene-menu, ASR-menu, paging, and trigger-mode options. The frontend settings are stored by Fcitx under its own `conf/vinpst.conf` path, separate from the daemon JSON configuration.

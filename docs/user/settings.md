# Settings

Vinpst has two configuration surfaces:

1. the daemon/application JSON configuration;
2. the Fcitx addon configuration for trigger keys and trigger mode.

They are intentionally separate.

## Main configuration

Default path:

```text
${XDG_CONFIG_HOME:-$HOME/.config}/fcitx-vinpst/config.json
```

Initialize it with:

```sh
vinpst init
```

Validate it at any time:

```sh
vinpst config validate \
  "${XDG_CONFIG_HOME:-$HOME/.config}/fcitx-vinpst/config.json"
```

Read or update an existing value with a JSON pointer:

```sh
vinpst config get /global/default_language
vinpst config set /global/default_language '"en"' --in-place
```

Preview mutations before writing:

```sh
vinpst config set /global/duck_output_while_recording true \
  --dry-run --json
```

`--in-place` writes an adjacent `.bak` when replacing an existing file. The GUI also detects external file changes and refuses to overwrite a configuration that changed after it was loaded.

## Common settings

### Capture device

```sh
vinpst device list
vinpst device use <target> --in-place
vinpst daemon restart
```

The default target is `default`. Device names come from the active PipeWire environment.

### Input gain and normalization

- `asr.input_gain` multiplies captured samples before recognition.
- `asr.normalize_audio` enables peak normalization for completed audio.

Excessive gain can clip audio and reduce recognition quality. Start near `1.0` and change it gradually.

### VAD

VAD settings live under `asr.vad`:

- `enabled`;
- `threshold`;
- `min_speech_duration`;
- `min_silence_duration`;
- `speech_pad_ms`.

Use the GUI for ordinary adjustment. Validate manual JSON edits before restarting the daemon.

### Output ducking

Set `global.duck_output_while_recording` to reduce output volume while recording. `global.duck_output_volume` is a scale from `0.0` to `1.0`.

Vinpst records the previous default-sink volume and restores it after recording. Ducking is best-effort and depends on the desktop audio control path being available.

### Language

`global.default_language` is the default recognition language hint. Provider/model capabilities still determine which languages are actually supported.

## Fcitx addon settings

Open the Fcitx configuration tool and select the **Vinpst** addon. Available settings include:

- normal dictation keys;
- command dictation keys;
- scene-menu keys;
- ASR-menu keys;
- previous/next-page keys;
- Tap/Hold/Both trigger mode.

Fcitx stores these values under its own package configuration root as `conf/vinpst.conf`. They are not part of `fcitx-vinpst/config.json`.

## GUI

Run:

```sh
vinpst-gui
```

The GUI provides Control, Resources, LLM, and Hotwords pages. It uses the same typed configuration and resource-management libraries as the CLI. A display-independent check is available for package validation:

```sh
vinpst-gui --check --offline
```

## Environment variables

Remote provider HTTP clients use standard proxy environment variables such as `HTTP_PROXY`, `HTTPS_PROXY`, and `NO_PROXY`. `SSL_CERT_FILE` may point to an additional PEM CA bundle.

Set daemon-specific environment through a systemd user-service drop-in rather than exporting secrets globally. Keep environment files mode `0600` and do not include them in bug reports.

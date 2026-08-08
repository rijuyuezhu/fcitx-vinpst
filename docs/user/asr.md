# ASR models and providers

Vinpst supports three ASR provider types:

- **Local**: native sherpa-onnx models managed by Vinpst.
- **Command**: an external executable that receives audio and returns recognition output.
- **Remote**: an OpenAI-compatible HTTP transcription endpoint.

## Local models

List models available from the configured registry:

```sh
vinpst model list --available
```

Inspect and install a model:

```sh
vinpst model info <model-id-or-short-id>
vinpst model install <model-id-or-short-id> --dry-run --json
vinpst model install <model-id-or-short-id>
```

Select it for the active local provider:

```sh
vinpst model use <model-id-or-short-id> --in-place --reload-daemon
```

Managed models are stored under:

```text
${XDG_DATA_HOME:-$HOME/.local/share}/fcitx-vinpst/models
```

Model installation downloads into a staging area, verifies registry metadata, rejects unsafe archive paths, and publishes the completed model into the managed root.

## Provider registry

List configured providers or registry providers:

```sh
vinpst provider list
vinpst provider list --available
```

Install a command provider from registry metadata:

```sh
vinpst provider install <provider-id-or-short-id> --dry-run --json
vinpst provider install <provider-id-or-short-id> --in-place
```

Select a provider:

```sh
vinpst provider use <provider-id> --in-place
vinpst daemon reload-asr
```

You can also select providers/models from the Fcitx ASR menu or the management GUI.

## Custom command providers

Use `vinpst provider create --type command` for a manually configured executable. Command providers run under bounded process supervision: Vinpst applies a deadline, drains stdout and stderr independently, limits output size, and terminates the helper process group on timeout or overflow.

Review the full command syntax before creating one:

```sh
vinpst provider create --help
```

## Remote providers

Use `vinpst provider create --type remote` for an OpenAI-compatible transcription endpoint. Remote recognition sends a WAV multipart request with the configured model, language, prompt, authentication, and timeout fields.

Provider redirects are disabled. TLS verification remains enabled, response bodies are bounded, and known credentials are redacted from generic diagnostics.

Standard `HTTP_PROXY`, `HTTPS_PROXY`, and `NO_PROXY` environment variables are supported by the provider HTTP client. Set `SSL_CERT_FILE` to an absolute PEM bundle path when the daemon must trust an additional private CA. The additional bundle augments rather than replaces the built-in roots.

## Hotwords

Show or update the hotwords file for the active provider:

```sh
vinpst hotword get
vinpst hotword set /absolute/path/to/hotwords.txt --in-place
vinpst hotword edit
vinpst hotword clear --in-place
```

Hotword support depends on the selected provider/model family. Vinpst rejects URL-like hotword paths and unsafe relative-path combinations whose meaning would depend on the daemon's working directory.

## Diagnostics

```sh
vinpst doctor
vinpst daemon status
vinpst daemon log --lines 100
```

A provider switch that fails during preparation should leave the previous backend usable. Report a bug when the daemon becomes stuck active or loses the previous working backend after a recoverable reload failure.

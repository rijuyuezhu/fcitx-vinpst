# Scenes and text processing

A **scene** controls how recognized text is post-processed. The raw scene commits ASR text directly; other scenes can call an OpenAI-compatible LLM provider or a command text adapter.

## List and select scenes

```sh
vinpst scene list
vinpst scene use <scene-id> --in-place
```

The active scene can also be changed from the Fcitx scene menu or the management GUI.

## Add a scene

```sh
vinpst scene add my-scene \
  --label "My scene" \
  --prompt "Rewrite the input clearly." \
  --provider-id my-llm \
  --candidate-count 1 \
  --timeout-ms 10000 \
  --in-place
```

Important scene fields:

- `prompt`: instruction/template supplied to the text processor;
- `provider_id`: explicit LLM provider for the scene;
- `model`: optional model override;
- `candidate_count`: number of alternatives requested;
- `timeout_ms`: processing deadline;
- `context_lines`: recent committed-input lines included as context.

Use `vinpst scene edit --help` to change an existing explicit scene. Built-in scene identities are retained by normalization and cannot be removed like ordinary custom scenes.

## LLM providers

Add an OpenAI-compatible chat provider:

```sh
vinpst llm add my-llm \
  --base-url https://provider.example/v1 \
  --api-key '$MY_LLM_API_KEY' \
  --model example-model \
  --in-place
```

Test it before selecting it in a scene:

```sh
MY_LLM_API_KEY=secret vinpst llm test my-llm
```

Do not paste real API keys into issue reports or retained command output. Prefer an environment-reference expression or another deployment-specific secret source instead of storing a literal key in shared configuration.

## Text adapters

A text adapter is an external command managed separately from an LLM HTTP provider.

List registry adapters and install one:

```sh
vinpst adapter list --available
vinpst adapter install <adapter-id-or-short-id> --dry-run --json
vinpst adapter install <adapter-id-or-short-id> --in-place
```

Control configured adapter processes through the daemon:

```sh
vinpst adapter start <adapter-id>
vinpst adapter status <adapter-id>
vinpst adapter stop <adapter-id>
```

Manually configured adapters are added with `vinpst adapter add`; registry resources use `vinpst adapter install`.

## Command scene

The built-in command scene combines selected text and recognized speech. It is used by command mode even when another ordinary scene is active.

Command processing must be fail-safe:

- processing failure does not delete the selected text;
- zero available adapters/providers returns an explicit configuration error;
- multiple implicit choices are rejected rather than guessed;
- the original selection is replaced only after a candidate succeeds and is selected.

See [Dictation and command mode](usage.md) for the desktop workflow.

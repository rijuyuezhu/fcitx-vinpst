# CLI overview

`vinpst` is the management and diagnostics command for the daemon, configuration, registries, and managed resources.

```sh
vinpst --help
vinpst <command> --help
```

Most commands that can emit structured data accept `--json`; `-j/--json` is also available as a global option.

## Main command groups

| Command | Purpose |
| --- | --- |
| `init` | Create the default user configuration and managed directories. |
| `config` | Validate, read, update, or safely edit configuration. |
| `daemon` | Start, inspect, reload, restart, or stop the daemon. |
| `recording` | Start, stop, toggle, or inspect recording. |
| `device` | List or select capture devices. |
| `model` | List, inspect, install, select, or remove managed models. |
| `provider` | Manage local, command, and remote ASR providers. |
| `hotword` | Inspect or edit provider hotword-file configuration. |
| `scene` | Manage post-processing scenes. |
| `llm` | Manage and test OpenAI-compatible LLM providers. |
| `adapter` | Manage command text adapters and their daemon processes. |
| `registry` | Validate registry metadata and inspect installation plans. |
| `doctor` | Run combined configuration, ASR, audio, activation, and addon diagnostics. |

## Safe mutation pattern

Commands that change configuration generally support:

- `--dry-run` to preview;
- `--json` for a machine-readable plan/result;
- `--config <path>` for an explicit input;
- `--output <path>` for a separate result;
- `--in-place` for a validated replacement with an adjacent backup.

A typical workflow is:

```sh
vinpst scene use my-scene --dry-run --json
vinpst scene use my-scene --in-place
```

Use `--reload-daemon` where provided, or reload/restart the daemon after changing active ASR settings.

## Registry install versus custom entries

The CLI distinguishes managed registry resources from custom configuration:

- `model install`, `provider install`, and `adapter install` resolve registry metadata and publish managed files;
- `provider add`, `adapter add`, and `llm add` create explicit custom configuration entries.

Review the subcommand help before running a mutation. Pre-release Vinpst does not promise command-line compatibility with another project.

## Exit behavior

Commands return a non-zero exit status for invalid input, failed validation, unavailable services, unsafe paths, failed provider/resource operations, or incomplete mutations. Do not parse human-readable output in automation; use `--json` and still check the process exit status.

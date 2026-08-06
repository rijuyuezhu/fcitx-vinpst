# fcitx-vinpst

Voice input for Fcitx 5, built around a Rust daemon, CLI, and management GUI with a thin C++ Fcitx addon.

Vinpst supports local, command, and OpenAI-compatible remote ASR; normal dictation; selected-text command editing; scenes and LLM post-processing; model/provider/adapter registries; configurable audio and VAD behavior; and English/zh_CN frontend localization.

> **Release status:** the project is preparing its first `0.1.0` release. Package and desktop behavior are already exercised extensively, but public release artifacts and their final support matrix are not published yet.

## Get started

- [Installation](docs/user/installation.md)
- [Quick start](docs/user/quick-start.md)
- [Dictation and command mode](docs/user/usage.md)
- [ASR models and providers](docs/user/asr.md)
- [Scenes and text processing](docs/user/scenes.md)
- [Settings](docs/user/settings.md)
- [Troubleshooting](docs/user/troubleshooting.md)
- [Known limitations](docs/user/limitations.md)

The documentation is also built as a MkDocs site. Run `just docs` for a strict local build or `just docs-serve` for a preview server.

## Vinpst identity

Vinpst is an independent project. Its executables, package names, addon, D-Bus service, systemd unit, configuration, data, and cache paths use `vinpst` or `fcitx-vinpst`. It does not replace or migrate another voice-input package.

## Development

The [documentation map](docs/development-index.md) separates user guides from architecture, development, migration, and evidence records. Contributors should start with [AGENTS.md](AGENTS.md) and [docs/development.md](docs/development.md).

```sh
just fmt-check
just lint
just test
just ci
```

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

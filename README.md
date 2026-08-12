# fcitx-vinpst

Voice input for Fcitx 5, built around a Rust daemon, CLI, and management GUI with a thin C++ Fcitx addon.

Vinpst supports local, command, and OpenAI-compatible remote ASR; normal dictation; selected-text command editing; scenes and LLM post-processing; model/provider/adapter registries; configurable audio and VAD behavior; and English/zh_CN frontend localization.

Documentation: <https://project.rijuyuezhu.top/fcitx-vinpst/>

## Get started

- [Installation](docs/user/installation.md)
- [Quick start](docs/user/quick-start.md)
- [Dictation and command mode](docs/user/usage.md)
- [ASR models and providers](docs/user/asr.md)
- [Scenes and text processing](docs/user/scenes.md)
- [Settings](docs/user/settings.md)
- [Troubleshooting](docs/user/troubleshooting.md)
- [Known limitations](docs/user/limitations.md)
- [0.1.0 release notes](RELEASE_NOTES.md)

Run `just docs` for a strict local documentation build or `just docs-serve` for local development.

## Vinpst identity

Vinpst is an independent project. Its executables, package names, addon, D-Bus service, systemd unit, configuration, data, and cache paths use `vinpst` or `fcitx-vinpst`. It does not replace or migrate another voice-input package.

## Development

Contributors should start with [AGENTS.md](AGENTS.md), [docs/development.md](docs/development.md), and the [architecture contracts](docs/architecture/README.md).

```sh
just fmt-check
just lint
just test
just ci
```

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

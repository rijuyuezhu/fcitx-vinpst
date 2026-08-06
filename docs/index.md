# Vinpst

Vinpst adds voice input and voice-driven editing to Fcitx 5.

The project combines:

- a Rust daemon for audio capture, ASR, post-processing, and runtime state;
- a Rust CLI for setup, resource management, diagnostics, and automation;
- a Rust/Iced management GUI for common configuration tasks;
- a thin C++ Fcitx addon for key events, preedit, candidates, commits, selected text, and desktop notifications.

## What you can do

- Dictate into Fcitx-compatible applications.
- Select text, speak an instruction, and replace the selection with the processed result.
- Use local sherpa-onnx models, external command providers, or OpenAI-compatible remote ASR.
- Switch ASR providers/models and post-processing scenes from Fcitx menus.
- Configure capture devices, gain, VAD, output ducking, hotwords, scenes, LLM providers, and text adapters.
- Install managed models, providers, and adapters from registry metadata.
- Diagnose configuration, daemon, audio, ASR, activation, and addon problems with `vinpst doctor`.

## Start here

1. Read [Installation](user/installation.md).
2. Follow the [Quick start](user/quick-start.md).
3. Learn the [dictation and command workflows](user/usage.md).
4. Use [Troubleshooting](user/troubleshooting.md) when setup does not work.

## Release status

Vinpst is preparing its first `0.1.0` release. The repository already contains deterministic package checks and extensive real-desktop evidence, but final public artifacts and the supported release matrix are still being completed.

## Independent project identity

All user-visible identities and paths use `vinpst` or `fcitx-vinpst`. Vinpst does not replace, import, or migrate another voice-input package. Feature comparisons with other implementations are development input only; they do not change Vinpst package names, commands, D-Bus names, services, or XDG paths.

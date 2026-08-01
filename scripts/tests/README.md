# Deterministic tests

This directory contains CI-safe orchestration and smoke tests. The main entry
points are:

- `check.sh`: complete deterministic gate used by `just check` and `just ci`;
- `format.sh`: Rust/C++ formatting, with `--check` for verification;
- `scripts-lint.sh`: shell syntax/ShellCheck, Python Ruff/bytecode checks, and source-layout limits;
- `lint.sh`: Rust Clippy and retained C++ clang-tidy;
- `test.sh`: Rust workspace/D-Bus tests and retained-addon CTest suite;
- `addon-install-smoke.sh`: staged addon, D-Bus, and systemd metadata install;
- `toolkit-probe-check.sh`: compile-only validation of desktop probe sources.

Subdirectories group process smokes by the component they exercise:

- `asr/`: command/remote/native ASR and fixture behavior;
- `cpp/`: retained Fcitx bridge and D-Bus process behavior;
- `daemon/`: activation, handoff, removal, and remote-service lifecycle;
- `install/`: temporary-HOME per-user installation and activation.

These scripts may create files only under `target/` or temporary directories.
Real desktop, audio, and host-system mutation belongs under `../live/` instead.

Production Rust/C++ files are limited to 1200 lines, while fixture-heavy test files have a 3000-line ceiling. These are regression guards against monolithic modules, not targets to fill.

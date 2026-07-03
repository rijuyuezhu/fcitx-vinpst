# fcitx-vinput-rs

Rust-oriented rewrite workspace for [`fcitx5-vinput`](https://github.com/xifan2333/fcitx5-vinput).

The early refactor milestones have produced stable Rust protocol/config/audio/ASR/text/registry/daemon/CLI seams and a deterministic retained-addon product spine. The current milestone is real desktop alpha: prove user install, Fcitx addon load, trigger/preedit/commit, command replacement, and a first real recognition path in a live desktop session.

## Current layout

- `crates/vinput-protocol`: D-Bus names, status strings, ASR state, text adapter state, and recognition result JSON.
- `crates/vinput-config`: typed config model for the legacy `data/default-config.json` plus validation.
- `crates/vinput-audio`: pure PCM buffers, capture traits, and deterministic audio transforms.
- `crates/vinput-asr`: ASR backend/session traits, recognition events, command backend seam, and deterministic mock backend.
- `crates/vinput-text`: scene post-processing, prompt rendering, text adapter traits, and command adapter seam.
- `crates/vinput-registry`: registry metadata parsing, validation, and dry-run asset/install planning.
- `crates/vinput-daemon`: mock/configured daemon runtime, diagnostics, and `zbus` service facade for the legacy daemon ABI.
- `crates/vinput-cli`: bootstrap CLI named `vinput` for protocol/config/registry/payload inspection.
- `data/default-config.json`: copied from the original project as the compatibility baseline.
- `AGENTS.md`: required short instruction file for coding agents.
- `docs/README.md`: documentation map and required reading order.
- `docs/development.md`: project style, commit message style, and `just` command guide.
- `docs/migration/function-gap-audit.md`: tracked Rust-vs-legacy parity baseline.
- `docs/migration/e2e-replication-plan.md`: active plan for real desktop alpha and full E2E replication.
- `docs/migration/live-desktop-validation.md`: live Fcitx desktop validation checklist.
- `docs/migration/agent-kickoff.md`: copyable context for a fresh implementation agent.
- `docs/architecture/README.md`: tracked architecture contract index.
- `docs/legacy/`: tracked original-source annotations.

Local planning notes under `docs/plan/` are intentionally ignored by the root `.gitignore`. Do not manually track them.

## Tooling

The repo pins shared project tooling in:

- `rust-toolchain.toml`: stable Rust with `rustfmt` and `clippy` components.
- `rustfmt.toml`: formatting policy.
- `clippy.toml` plus workspace lints in `Cargo.toml`: lint policy.
- `.pre-commit-config.yaml`: local pre-commit hooks for format and lint checks.
- `justfile`: common commands used locally and mirrored by CI.

Install optional local hooks with:

```sh
pre-commit install
```

## Smoke checks

```sh
just ci
just smoke
```

`just ci` mirrors the GitHub Actions checks, including C++ addon format/lint/test coverage and the D-Bus integration feature lint.

Equivalent raw commands:

```sh
clang-format --dry-run --Werror {{addon-sources}}
cargo fmt --all -- --check
cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON
ln -sfn target/cpp/fcitx5-addon/compile_commands.json compile_commands.json
clang-tidy -p target/cpp/fcitx5-addon {{addon-lint-sources}}
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
dbus-run-session -- cargo test -p vinput-daemon --features dbus-integration --test dbus_integration
cargo clippy -p vinput-daemon --all-targets --features dbus-integration -- -D warnings
cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DVINPUT_FCITX_BRIDGE_ENABLE_FCITX_DEPS=OFF
ln -sfn target/cpp/fcitx5-addon/compile_commands.json compile_commands.json
cmake --build target/cpp/fcitx5-addon --parallel
ctest --test-dir target/cpp/fcitx5-addon --output-on-failure
scripts/run-cpp-dbus-smoke.sh
scripts/run-command-asr-wav-helper-smoke.sh
scripts/run-user-ime-real-command-asr-wav-smoke.sh
scripts/run-user-ime-sherpa-sense-voice-smoke.sh
cargo run -q -p vinput-cli -- protocol
cargo run -q -p vinput-cli -- init --dry-run --json
cargo run -q -p vinput-cli -- config
cargo run -q -p vinput-cli -- config validate data/default-config.json --summary-only
cargo run -q -p vinput-cli -- config get /global/default_language --config data/default-config.json --json
cargo run -q -p vinput-cli -- config set /global/default_language en --config data/default-config.json --dry-run --json
cargo run -q -p vinput-cli -- config edit --dry-run --editor true --json
cargo run -q -p vinput-cli -- asr-state
cargo run -q -p vinput-cli -- asr-state --config data/default-config.json
cargo run -q -p vinput-cli -- audio-devices
cargo run -q -p vinput-cli -- device list --json
cargo run -q -p vinput-cli -- device use default --dry-run --json
cargo run -q -p vinput-cli -- provider list --json
cargo run -q -p vinput-cli -- provider use sherpa-onnx --dry-run --json
cargo run -q -p vinput-cli -- provider edit sherpa-onnx --model sherpa-onnx --dry-run --json
cargo run -q -p vinput-cli -- scene list --json
cargo run -q -p vinput-cli -- scene use __raw__ --dry-run --json
cargo run -q -p vinput-cli -- scene add scratch --label Scratch --dry-run --json
cargo run -q -p vinput-cli -- scene edit __raw__ --label __label_raw__ --dry-run --json
cargo run -q -p vinput-cli -- scene remove __command__ --dry-run --json
cargo run -q -p vinput-cli -- llm list --json
cargo run -q -p vinput-cli -- llm add scratch --base-url https://llm.example.test/v1 --dry-run --json
cargo run -q -p vinput-cli -- adapter list --json
cargo run -q -p vinput-cli -- adapter add scratch --command true --dry-run --json
cargo run -q -p vinput-cli -- adapter start scratch --dry-run --json
cargo run -q -p vinput-cli -- adapter stop scratch --dry-run --json
cargo run -q -p vinput-cli -- hotword get --json
cargo run -q -p vinput-cli -- hotword set /tmp/hotwords.txt --dry-run --json
cargo run -q -p vinput-cli -- hotword clear --dry-run --json
cargo run -q -p vinput-cli -- hotword edit --dry-run --editor true --json
cargo run -q -p vinput-cli -- registry
cargo run -q -p vinput-cli -- registry validate data/sample-registry-index.json
cargo run -q -p vinput-cli -- registry plan data/sample-registry-index.json --summary-only
cargo run -q -p vinput-cli -- model list --registry crates/vinput-registry/tests/fixtures/live-models-sensevoice.json --json
cargo run -q -p vinput-cli -- daemon start --dry-run --json
cargo run -q -p vinput-cli -- daemon status --dry-run --json
cargo run -q -p vinput-cli -- daemon reload-asr --dry-run --json
cargo run -q -p vinput-cli -- daemon stop --dry-run --json
cargo run -q -p vinput-cli -- daemon restart --dry-run --json
cargo run -q -p vinput-cli -- daemon log --dry-run --json
cargo run -q -p vinput-cli -- model use onnx-sv-zh-int8-off --registry crates/vinput-registry/tests/fixtures/live-models-sensevoice.json --model-root /tmp/vinput-models --reload-daemon --dry-run --json
cargo run -q -p vinput-cli -- recording start --dry-run --json
cargo run -q -p vinput-cli -- recording start --selected-text demo --dry-run --json
cargo run -q -p vinput-cli -- recording stop --scene demo --dry-run --json
cargo run -q -p vinput-cli -- recording toggle --dry-run --json
cargo run -q -p vinput-cli -- mock-result '你好'
cargo run -q -p vinput-daemon -- print-config
cargo run -q -p vinput-daemon -- asr-state
cargo run -q -p vinput-daemon -- text-adapters
cargo run -q -p vinput-daemon -- audio-devices
cargo run -q -p vinput-daemon -- --once
```

Use `cargo run -p vinput-cli -- asr-state --config path/to/config.json` to inspect ASR diagnostics for a custom config without starting daemon runtime backends. Use `cargo run -p vinput-cli -- audio-devices` or `cargo run -p vinput-daemon -- audio-devices` to inspect capture-device config and, when built with the optional PipeWire feature, live source enumeration.

`data/default-config.json` and `data/sample-registry-index.json` are stable smoke fixtures for explicit config and registry CLI paths. See [`docs/architecture/config-contract.md`](docs/architecture/config-contract.md) and [`docs/architecture/registry-contract.md`](docs/architecture/registry-contract.md) for their fixture contracts.

## Local E2E demo

Run the deterministic file-input demo with:

```sh
just e2e-demo
```

The recipe generates `target/tmp/vinput-demo.wav`, then runs `vinput-daemon --configured-backends --once --wav` with `data/e2e-command-demo-config.json`. This exercises the current product spine end to end: WAV input, command ASR, command text adapter, and final recognition JSON. The demo ASR reports the input byte count instead of performing real speech recognition, which keeps the path deterministic until the concrete ASR backend lands.

Stage the Rust daemon, Fcitx addon module, addon metadata, and D-Bus activation service together with:

```sh
just ime-install-smoke
just ime-configured-install-smoke
```

`just ime-configured-install-smoke` additionally stages `data/e2e-command-demo-config.json` plus a deterministic demo WAV, and wires D-Bus activation to `vinput-daemon --dbus --configured-backends --config /usr/local/share/fcitx-vinput/e2e-command-demo-config.json --wav /usr/local/share/fcitx-vinput/e2e-command-demo.wav`.

This staged install shape is the current local packaging spine for the input method: Fcitx loads `fcitx5-vinput.so`, the addon talks to `org.fcitx.Vinput`, and the D-Bus service activates `vinput-daemon --dbus` from the same install prefix. To activate configured command ASR/text backends from Fcitx, configure the addon CMake build with `-DVINPUT_DAEMON_ARGS="--dbus --configured-backends --config /path/to/config.json"`. For live desktop capture builds that enable the `pipewire-backend` Cargo feature, include `--audio-backend pipewire` in those activation args.

For a real per-user desktop install, run `just ime-fcitx-live-command-demo-setup` for the guided deterministic command-demo setup, `just user-ime-command-demo` for the lower-level deterministic command-demo profile, or `just user-ime-pipewire-live` for configured command backends plus live PipeWire capture. These recipes install the daemon, retained Fcitx addon module, addon metadata, per-user D-Bus activation service, generated Fcitx environment wrapper, and a managed user autostart override. Use `just user-ime-status` and `just user-ime-clear` to inspect or remove them. For the current session, restart Fcitx5 through the generated wrapper shown by the installer, for example `~/.local/share/fcitx-vinput/fcitx5-with-vinput-env.sh -r`; on next login the managed autostart override starts Fcitx5 with the same `FCITX_ADDON_DIRS` environment. The retained addon defaults to `Right Ctrl` for normal dictation and `F10` for command dictation; set `VINPUT_FCITX_NORMAL_TRIGGER` or `VINPUT_FCITX_COMMAND_TRIGGER` before launching Fcitx5 to override them with portable Fcitx key strings such as `F8` or `Control+space`.

Run `just addon-dbus-smoke` to verify both the C++ bridge client and retained `FcitxVinputAddon` trigger path against a manually started Rust daemon. Run `just addon-dbus-activation-smoke` to verify that a staged D-Bus service file can activate the Rust daemon for those bridge/addon trigger smokes without manually starting `vinput-daemon` first. Run `just addon-dbus-configured-activation-smoke` to exercise the same activation path with `--configured-backends`, the command ASR demo config, and deterministic demo WAV input. Run `just addon-dbus-adapter-lifecycle-smoke` to verify configured text adapter start/duplicate-start/stop diagnostics over DBus. Run `just ime-configured-activation-smoke` to repeat the configured activation path from a staged install tree containing the daemon, addon, config, and demo WAV. Run `just ime-e2e-smoke` to combine that staged activation shape with fake outcome sink coverage for preedit, commit, command-mode selected-text deletion, candidate menus, and fallback commit behavior; this is deterministic and CI-friendly, but it is not a live desktop `fcitx::InputContext` mutation test.

Run the mock D-Bus service inside an existing session bus with:

```sh
cargo run -p vinput-daemon -- --dbus
```

The daemon accepts `--wav` or `--pcm16le` with `--dbus` for deterministic file-input service demos, and accepts `--audio-backend mock|pipewire` for long-running D-Bus sessions. `mock` remains the default for deterministic CI and staged demos. `pipewire` is feature-gated behind `--features pipewire-backend` and selects the live PipeWire recorder worker for desktop capture.

## Development route

The current route is real desktop alpha. Start with `AGENTS.md`, then read `docs/README.md`, `docs/development.md`, `docs/migration/function-gap-audit.md`, and `docs/migration/e2e-replication-plan.md`.

1. Keep `vinput-protocol` compatible with the legacy Fcitx5 addon contract.
2. Keep the retained C++ Fcitx frontend thin; backend logic belongs in Rust crates and `vinput-daemon`.
3. Keep deterministic checks such as `just ime-e2e-smoke` and `just user-ime-command-demo-smoke` green.
4. Prioritize live desktop proof: addon load, trigger/preedit/commit, command replacement, PipeWire capture, and a first real recognition path.
5. Defer broad GUI polish, full resource orchestration, and release packaging until real desktop alpha and real ASR alpha are proven.

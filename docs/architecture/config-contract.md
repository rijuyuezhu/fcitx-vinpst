# Config contract

`vinpst-config` owns config parsing, field defaults, validation, secret-safe diagnostics, and persistence. CLI, daemon, and GUI callers consume the same typed config so file-backed behavior stays deterministic. Vinpst has no released historical config format to migrate: parsed user values are never silently repaired or rewritten.

## Baseline fixture

`data/default-config.json` is the committed Vinpst baseline aligned with the current upstream defaults. It is also the stable smoke fixture for explicit config CLI paths:

```sh
cargo run -q -p vinpst-cli -- config validate data/default-config.json --summary-only
cargo run -q -p vinpst-cli -- asr-state --config data/default-config.json
```

Daemon config resolution uses the canonical Vinpst XDG path. An explicit `--config` path has highest priority. Without it, the daemon reads `$XDG_CONFIG_HOME/fcitx-vinpst/config.json`, falling back to `$HOME/.config/fcitx-vinpst/config.json`; only a missing user file falls back to the bundled default. CLI `doctor`, `asr-state`, and `audio-devices` use the same explicit/discovered/bundled priority so diagnostics describe the configuration the normal daemon will consume. A discovered user file is retained as the runtime persistence path, so D-Bus scene/provider selection and config reload update the same file. `scripts/tests/daemon/run-daemon-default-config-smoke.sh` starts the daemon on a private session bus without `--config`, switches the active scene, and verifies the discovered file is atomically updated.

Integration tests consume the same committed fixture directly, so changes to config parsing or defaults must keep the CLI summary and ASR diagnostics contracts stable.

The committed baseline intentionally fixes these fields:

- output ducking disabled by default, with a `duck_output_volume` multiplier of `0.25` when enabled;
- ASR provider `sherpa-onnx` as the active local provider placeholder.
- active scene `__raw__`, with `__command__` using the current upstream scoped interpolation prompt. The prompt places selected text and ASR text in `<vinput-selected>` and `<vinput-asr>` blocks through `{{selected}}` and `{{asr}}`, so request assembly does not append a second copy of either input.
- empty `llm.providers` and `llm.adapters`, so text-adapter diagnostics report no configured adapters.

Runtime availability is not implied by the fixture; local `sherpa-onnx` requires the feature-gated native backend and a compatible installed model.

## Strict config policy

Vinpst v0.1.0 is the first release, so the config parser does not carry migration or repair code for unpublished development snapshots. The contract is fail-closed:

- `version` must exactly equal `CURRENT_CONFIG_VERSION`. Older, newer, or zero versions are rejected rather than upgraded or partially interpreted.
- Serde defaults apply only when a field is intentionally defined as optional/defaulted by the current schema. After parsing, Vinpst does not clamp, rename, insert, deduplicate, or otherwise repair user-supplied values.
- Both built-in scenes, `__raw__` and `__command__`, must be present. Missing built-ins are rejected instead of synthesized. A blank or unknown `active_scene` is rejected.
- Omitted scene `candidate_count` uses the current upstream default `1`. An explicit `0` is preserved and disables post-processing; it is never rewritten to `1`.
- Numeric values such as output ducking volume, input gain, VAD controls, scene candidate count/context limits, and explicit timeouts are range-checked. Out-of-range values are errors, not values to clamp or repair.
- Duplicate/blank registry mirrors, provider ids, adapter ids, malformed provider definitions, and invalid scene references are rejected rather than dropped or normalized.
- An existing malformed or invalid user config is an error. Only an absent user config file falls back to the bundled default.

The strict behavior is pinned by `crates/vinpst-config/tests/strict_config.rs`. If the schema changes after a public release, any future migration policy must be designed explicitly for that released version; pre-release Vinpst snapshots do not justify compatibility shims.

Some defaults and wire behavior intentionally match the current upstream C++ project, including the 4000 ms effective scene timeout when `timeout_ms` is omitted, the empty-string "no ASR provider selected" state, and the built-in scene ids. Those are current product semantics, not migration repairs.

Provider removal retains local providers; removing the active non-local provider clears `asr.active_provider`, and the resulting config remains valid with no runtime backend selected. Configured runtime construction reports that state as unavailable instead of choosing another provider implicitly.

## Offline VAD fields

`asr.vad` preserves the legacy offline Silero controls: `enabled`, `threshold`, `min_speech_duration`, `min_silence_duration`, and `speech_pad_ms`. Defaults are `true`, `0.45`, `0.15`, `0.5`, and `300` respectively. The native runtime applies them only to buffered offline sherpa recognition; online/streaming recognition does not use this trimmer.

## Diagnostics behavior

Config diagnostics parse local JSON only. They do not construct runtime ASR backends, launch helpers, download registry assets, or require the daemon to be running.

`VinpstConfig::summary()` is the compact config diagnostic surface. It reports validation status, schema version, active scene/provider ids, and counts only. It must not serialize secret-bearing config fields such as LLM API keys, provider or adapter environment values, command arguments, working directories, provider base URLs, or provider `extra_body` objects. `redact_url_for_diagnostics` is the shared URL diagnostic boundary for ASR and text providers: it removes userinfo and fragments, preserves scheme/host/port/path plus query-key order and duplicates, and replaces every query value with `REDACTED`. Invalid URLs become the fixed marker `<invalid-url>`. This helper never mutates the configured URL used for an HTTP request.

`vinpst-daemon --config data/default-config.json print-config`, `asr-state`, `text-adapters`, and `audio-devices` are covered by integration tests to keep daemon diagnostics aligned with the same committed fixture. `audio-devices` reports the parsed capture target without constructing the runtime. In default builds it reports `backend: "unavailable"`; with `pipewire-backend` it may enumerate live PipeWire sources, but still succeeds with `live: false` and an `enumeration_error` when PipeWire client configuration or a server is unavailable.

# New agent kickoff context

Use this as the copyable startup context for an implementation agent that will continue the complete E2E replication work.

## Mission

Continue the `fcitx-vinput-rs` migration from the completed **usable CLI/daemon alpha** toward real desktop native alpha and real ASR parity. The project already has the retained Fcitx addon, Rust daemon, broad management CLI, live model registry install/use/remove, user activation profiles, deterministic E2E smokes, a real SenseVoice WAV smoke, typed model-family classification, and native Qwen3 ASR recognizer configuration. Do not reimplement those surfaces. Focus on the current M4/native-runtime gaps from [`e2e-capability-matrix.md`](e2e-capability-matrix.md) and [`e2e-replication-plan.md`](e2e-replication-plan.md).

## Repositories

- Rust rewrite: `/workspace/fcitx-vinput-rs`
- Legacy C++ reference: `/workspace/fcitx5-vinput`
- Rust remote: `git@github.com:rijuyuezhu/fcitx-vinput-rs.git`
- Legacy upstream: `https://github.com/xifan2333/fcitx5-vinput`

If the legacy repository is missing, clone it:

```sh
git clone https://github.com/xifan2333/fcitx5-vinput /workspace/fcitx5-vinput
```

## Start-of-session checks

Run these before editing:

```sh
cd /workspace/fcitx-vinput-rs
git status --porcelain=v1 -b
git log -8 --oneline --decorate
gh run list --repo rijuyuezhu/fcitx-vinput-rs --limit 12
```

If `gh run list` is blocked, use:

```sh
gh api repos/rijuyuezhu/fcitx-vinput-rs/actions/runs \
  --jq '.workflow_runs[:12][] | [.status, .conclusion, .head_branch, .head_sha[0:7], .display_title] | @tsv'
```

Then read, in order:

1. `docs/README.md`
2. `docs/development.md`
3. `docs/migration/function-gap-audit.md`
4. `docs/migration/e2e-capability-matrix.md`
5. `docs/migration/e2e-replication-plan.md`
6. `docs/migration/e2e-port-plan.md`
7. the relevant `docs/architecture/*` contract for the files you will touch
8. `docs/legacy/source-annotations.md` when comparing legacy behavior

## Current parity baseline

- Overall legacy feature parity: approximately **70-75%** as a planning estimate.
- CLI/daemon alpha: usable and broadly covered by deterministic tests.
- Real desktop readiness: **prototype usable / early alpha**; the full native desktop chain is not yet proven.
- Native SenseVoice file-input path: proven with a registry-downloaded model and bundled WAV.
- Native desktop install: `sherpa-native-live` accepts supported typed offline/online models, copies the validated `libsherpa-onnx`/`libonnxruntime` bundle, uses wrapper-based D-Bus activation, and passes real temporary-HOME online-transducer readiness. The old `sherpa-sense-voice-live` alias remains tested. Real Fcitx commit proof remains.
- Native offline transducer: the registry Zipformer multi-Chinese int8 model is SHA-256 verified and recognizes bundled `test_wavs/0.wav` as `对我做了介绍那么我想说的是大家如果对我的研究感兴趣` through `just sherpa-offline-transducer-local-smoke`.
- Native Dolphin: the registry multilingual int8 model is SHA-256 verified and recognizes bundled `test_wavs/0.wav` as `对我做了介绍哈那么我想说的是呢大家如果对我的研究感兴趣呢。` through `just sherpa-dolphin-local-smoke`.
- Native Paraformer: the registry small model is SHA-256 verified and recognizes bundled `test_wavs/0.wav` as `对我做了介绍啊那么我想说的是呢大家如果对我的研究感兴趣呢嗯` through `just sherpa-paraformer-local-smoke`.
- Native Qwen3 ASR: proven with the live registry model and its bundled `test_wavs/es1.wav` through `just sherpa-qwen3-local-smoke`.
- Selected-text primary-selection fallback: implemented in the retained addon; live multi-application proof remains.
- Daemon chunked delivery: implemented with 800-frame batching, callback event polling, error propagation, and no final-buffer replay.
- Native online ASR: transducer and Zipformer2 CTC metadata/runtime mappings are implemented and both pass real registry-model WAV smokes with the 200 ms warmup. Online transducer recognizes bundled `test_wavs/0.wav` as `THE YELLOW LAMPS WOULD LIGHT UP HERE AND THERE THE SQUALID QUARTER OF THE BRAFFLEL` through `just sherpa-online-transducer-local-smoke`. Offline transducer is separately registry-WAV proven.
- Live partial signals: generation-scoped D-Bus emission is implemented and session-bus tested before stop, with stop-time deduplication. The retained addon now consumes `StatusChanged` and `RecognitionPartial` through the Fcitx bus and maps partial-first status to active-context preedit; real desktop rendering remains unproven.
- Offline VAD: the tracked Silero model, strict legacy-compatible config, native trimming, user install, no-speech fallback, and real SenseVoice/Qwen3 WAV regressions are implemented.
- Online endpoint/warmup: legacy endpoint defaults and metadata overrides are forwarded, and every native online recognizer runs the legacy-compatible 200 ms silence warmup.
- Timeout semantics: command helpers enforce configured deadlines; native synchronous sherpa decode is explicitly classified as unsupported/diagnostic-only in `vinput doctor` with an isolation hint.
- Reload semantics: the legacy D-Bus method re-reads explicit daemon config files and queues one non-blocking worker; startup/readiness/reload paths share prepare-before-swap, physical progress is observable, stale generations are discarded, and failure preserves the previous effective backend.
- Frontend menus: a minimal Right-Shift scene menu and installed-model-aware F8 ASR menu are implemented with typed D-Bus state and atomic explicit-config persistence. ASR provider/model selection queues background reload and is proven through the C++ client; real desktop menu proof is still missing.
- Frontend config: normal, command, scene-menu, ASR-menu, previous-page, and next-page keys are persistent legacy-named Fcitx KeyLists with immediate reload; both menus consume the configured paging lists, including keypad defaults. TriggerMode implements Tap/Hold/Both with legacy debounce/hold/release-tail timing, temporary trigger overrides remain, and unknown legacy fields are preserved. Both menus implement legacy slash filtering, multi-term matching, UTF-8/Ctrl editing, and two-stage Escape. Static menu/config/result labels use a compiled and installed zh_CN gettext catalog with English fallback.
- Model titles: registry installs persist full ids and the selected locale title; the additive display-menu D-Bus row is C++/session-bus tested, and old installs fall back to stable ids.
- Frontend notifications/recovery: local errors and scene/ASR switch confirmations use translated Fcitx notifications with legacy icons/timeouts and stderr fallback. The retained addon subscribes to daemon signals and uses race-free service-owner tracking; owner loss during recording or status-only recovery clears the frontend state with a localized error. Trigger-time `GetStatus` reconciliation also adopts and stops externally started normal recordings while external busy states become tracked preedit instead of conflicting Start calls. Current-generation background ASR reload failures are emitted and session-bus tested; broader notification categories and live desktop presentation remain.
- Biggest blockers: real Fcitx -> PipeWire -> native ASR -> partial/preedit -> commit proof, broader legacy sherpa families, distro packaging, and remote services.

## First recommended implementation slices

Pick one focused M4 or native-runtime slice:

1. Prove real desktop SenseVoice dictation from Fcitx trigger through PipeWire capture to application commit.
2. Broaden legacy sherpa family coverage according to concrete demand; current registry families are already supported, and Dolphin, Paraformer, and Moonshine v1 are registry-installed and WAV-proven.
3. Prove localized searchable scene/ASR menus, persisted registry titles, persistent trigger/paging keys, and Tap/Hold/Both timing live.
4. Advance packaging and remote-service breadth only where they unblock the native desktop path.

Do not start broad GUI polish or distro packaging before real desktop native alpha is proven. Keep refactors feature-driven and scoped to the next migration slice.

## Implementation rules

- Communicate with the user in Chinese.
- Keep code, comments, test names, file paths, and commit messages in English.
- Preserve legacy service names, method names, status strings, config semantics, and recognition payload shape.
- Do not count deterministic smokes as live desktop proof.
- Keep environment overrides as temporary development escapes over persistent frontend KeyLists.
- Keep user-profile mutations explicit and opt-in.
- Keep commits small and scoped.

## Validation tiers

For code changes, run the narrowest relevant tier plus any affected integration checks.

### Minimum for docs-only changes

```sh
git status --porcelain=v1 -b
git diff --check
```

### Rust/core changes

```sh
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

### C++ addon/frontend changes

```sh
just addon-format-check
just addon-test
```

### Deterministic IME path changes

```sh
just ime-e2e-smoke
just user-ime-command-demo-smoke
```

### User install changes

Run in a temporary `HOME` unless the user explicitly wants to mutate the real profile:

```sh
tmp_home="$(mktemp -d)"
HOME="$tmp_home" VINPUT_USER_PROFILE=command-demo scripts/install-user-ime.sh
HOME="$tmp_home" VINPUT_USER_STATUS=1 scripts/install-user-ime.sh
rm -rf "$tmp_home"
```

### Optional live desktop checks

Only run in a real desktop session:

```sh
just ime-fcitx-live-probe
VINPUT_LIVE_INSTALL_COMMAND_DEMO=1 just ime-fcitx-live-probe
just ime-configured-pipewire-live
```

If live checks fail, record exactly what failed and do not mark the feature as done.

## Commit style

Use English Conventional Commit style:

```text
<type>(optional-scope): <imperative summary>
```

Examples:

```text
docs(migration): track feature parity audit
fix(ime): improve live probe diagnostics
test(addon): cover selected text fallback
feat(asr): add initial sherpa runtime
```

Do not mix audit docs, tests, and feature implementation in one commit unless the change is intentionally tiny and inseparable.

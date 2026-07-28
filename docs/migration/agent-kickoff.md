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
- Native Qwen3 ASR: proven with the live registry model and its bundled `test_wavs/es1.wav` through `just sherpa-qwen3-local-smoke`.
- Selected-text primary-selection fallback: implemented in the retained addon; live multi-application proof remains.
- Daemon chunked delivery: implemented with 800-frame batching, callback event polling, error propagation, and no final-buffer replay.
- Native online ASR: transducer and Zipformer2 CTC metadata/runtime mappings are implemented; Zipformer2 CTC passes a real registry-model WAV smoke.
- Live partial signals: generation-scoped D-Bus emission is implemented and session-bus tested before stop, with stop-time deduplication.
- Biggest blockers: real Fcitx -> PipeWire -> native ASR -> partial/preedit -> commit proof, VAD/endpoint/timeout/warm-reload semantics, remaining sherpa families, frontend menus/configuration, packaging, and remote services.

## First recommended implementation slices

Pick one focused M4 or native-runtime slice:

1. Prove real desktop SenseVoice dictation from Fcitx trigger through PipeWire capture to application commit.
2. Add native VAD/endpoint, timeout, warmup, and warm reload behavior.
3. Port Moonshine, Dolphin, Paraformer, and other remaining families in registry-priority order.
4. Add scene/ASR menus and persistent frontend trigger configuration where they directly support live validation.

Do not start broad GUI polish or distro packaging before real desktop native alpha is proven. Keep refactors feature-driven and scoped to the next migration slice.

## Implementation rules

- Communicate with the user in Chinese.
- Keep code, comments, test names, file paths, and commit messages in English.
- Preserve legacy service names, method names, status strings, config semantics, and recognition payload shape.
- Do not count deterministic smokes as live desktop proof.
- Keep environment overrides for development even if persistent frontend config is added.
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

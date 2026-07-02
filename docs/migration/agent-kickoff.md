# New agent kickoff context

Use this as the copyable startup context for an implementation agent that will continue the complete E2E replication work.

## Mission

Continue the `fcitx-vinput-rs` migration toward **real desktop alpha**. The project is no longer just architecture scaffolding: deterministic command-demo, retained Fcitx addon, Rust daemon, user install, activation, doctor, staged E2E, and user install smokes already exist. Do not reimplement them. Focus on the remaining P0 gaps from [`e2e-replication-plan.md`](e2e-replication-plan.md).

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
4. `docs/migration/e2e-replication-plan.md`
5. `docs/migration/e2e-port-plan.md`
6. the relevant `docs/architecture/*` contract for the files you will touch
7. `docs/legacy/source-annotations.md` when comparing legacy behavior

## Current parity baseline

- Overall legacy feature parity: about **60-65%**.
- Real desktop readiness: **prototype usable / early alpha**.
- Deterministic product spine: strong and tested.
- Biggest blockers: real local ASR, live desktop verification, frontend config/menus, selected-text fallback, model/resource install.

## First recommended implementation slices

Pick one focused P0 slice:

1. Improve `just ime-fcitx-live-probe` diagnostics and document the explicit opt-in user install path.
2. Add `docs/migration/live-desktop-validation.md` with a real desktop alpha checklist.
3. Implement selected-text fallback for command mode.
4. Add the first real ASR path, preferably the smallest `sherpa-onnx` path compatible with current config.
5. Add a real or local-mock text provider validation test.

Do not start distro packaging, GUI polish, or broad refactors before real desktop alpha is proven.

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

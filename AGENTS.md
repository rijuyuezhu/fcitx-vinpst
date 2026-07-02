# AGENT

Before doing any work in this repository, read these files in order:

1. `docs/README.md` — documentation map and required reading order.
2. `docs/development.md` — project style, commit message style, and test commands.
3. `docs/migration/function-gap-audit.md` — tracked Rust-vs-legacy parity baseline.
4. `docs/migration/e2e-replication-plan.md` — active plan for real desktop alpha and full E2E replication.
5. `docs/migration/live-desktop-validation.md` — live desktop validation checklist.
6. `docs/migration/agent-kickoff.md` — copyable context for a fresh implementation agent.
7. `docs/architecture/README.md` — tracked architecture contract index; then read the contract document for the area you will touch.
8. `docs/legacy/README.md` and `docs/legacy/source-annotations.md` — legacy C++ source map when comparing behavior with `fcitx5-vinput`.

Rules for agents:

- Communicate with the user in Chinese; keep code, comments, tests, and commit messages in English unless existing code requires otherwise.
- Current priority: reach real desktop alpha, then real ASR alpha, then legacy feature parity.
- Prefer product-spine implementation over generic cleanup.
- Preserve public wire formats and frontend expectations with focused tests.
- Keep the retained Fcitx frontend thin. Backend logic belongs in Rust crates and `vinput-daemon`.
- Never manually track files under `docs/plan/`; it is local scratch and must remain ignored.
- Use test commands from `docs/development.md`. Prefer focused checks while iterating, and broader checks before handoff.

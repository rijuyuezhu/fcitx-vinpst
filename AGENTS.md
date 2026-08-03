# AGENT

Before working in this repository, read:

1. `docs/README.md` — documentation map and source-of-truth rules.
2. `docs/development.md` — workflow, validation tiers, and commit style.
3. `docs/migration/function-gap-audit.md` — current Rust-versus-legacy status.
4. `docs/migration/e2e-capability-matrix.md` — detailed capability and evidence matrix.
5. `docs/migration/e2e-replication-plan.md` — active milestones and next work.
6. `docs/migration/live-desktop-validation.md` — real-session validation procedure.
7. `docs/architecture/README.md` — stable contract index; then read the contract for the touched subsystem.
8. `docs/legacy/README.md` and `docs/legacy/source-annotations.md` — legacy source map when behavior comparison is needed.

Rules:

- Communicate with the user in Chinese; keep code, comments, tests, docs identifiers, and commit messages in English unless surrounding content requires otherwise.
- Current priority: advance the Rust management GUI baseline while keeping the live desktop and release evidence green; defer new packaging expansion unless required by a regression or the user.
- Prefer product-spine work over generic cleanup.
- Preserve public wire formats and frontend expectations with focused behavior and compatibility tests.
- Do not add tests that assert source declarations, interface text, exact documentation wording, docstrings, recipe names, or other implementation prose.
- Keep the retained Fcitx frontend thin. Backend logic belongs in Rust crates and `vinput-daemon`.
- Keep the standalone GUI in Rust; do not port or restore the legacy Qt GUI as a C++ product component.
- Distinguish `implemented`, `deterministic`, and `live-proven`; never count a smoke as desktop proof.
- Never track files under ignored `docs/plan/`.
- Use the validation tiers in `docs/development.md`; run focused checks while iterating and the full relevant gate before handoff.

# AGENT

Before working in this repository, read:

1. `docs/development-index.md` — documentation map and source-of-truth rules.
2. `docs/development.md` — workflow, validation tiers, and commit style.
3. `docs/migration/function-gap-audit.md` — current Rust-versus-legacy status.
4. `docs/migration/e2e-capability-matrix.md` — detailed capability and evidence matrix.
5. `docs/migration/e2e-replication-plan.md` — active milestones and next work.
6. `docs/migration/live-desktop-validation.md` — real-session validation procedure.
7. `docs/architecture/README.md` — stable contract index; then read the contract for the touched subsystem.
8. `docs/legacy/README.md` and `docs/legacy/source-annotations.md` — legacy source map when behavior comparison is needed.

Rules:

- Communicate with the user in Chinese; keep code, comments, tests, docs identifiers, and commit messages in English unless surrounding content requires otherwise.
- Current priority: complete the 0.1.0 user-capability audit, close meaningful functional gaps, publish user-facing documentation, and expand the checked release pipeline while keeping desktop and package evidence green.
- Vinpst is an independent product. Preserve `vinpst` / `fcitx-vinpst` identities and paths; do not add upstream package replacement, old-name aliases, automatic migration, or pre-0.1.0 internal compatibility unless the user explicitly changes that product decision.
- Prefer product-spine work over generic cleanup.
- Preserve intentionally stable Vinpst wire formats and frontend expectations with focused behavior tests. Before 0.1.0, improve internal and user interfaces when that makes the product clearer or safer.
- Do not add tests that assert source declarations, interface text, exact documentation wording, docstrings, recipe names, or other implementation prose.
- Keep the retained Fcitx frontend thin. Backend logic belongs in Rust crates and `vinpst-daemon`.
- Keep the standalone GUI in Rust; do not port or restore the legacy Qt GUI as a C++ product component.
- Distinguish `implemented`, `deterministic`, and `live-proven`; never count a smoke as desktop proof.
- Never track files under ignored `docs/plan/`.
- Use the validation tiers in `docs/development.md`; run focused checks while iterating and the full relevant gate before handoff.

# AGENT

Before working in this repository, read:

1. `docs/development.md` — workflow, validation tiers, and commit style.
2. `docs/migration/function-gap-audit.md` — current Rust-versus-legacy status.
3. `docs/migration/e2e-capability-matrix.md` — detailed capability and evidence matrix.
4. `docs/migration/live-desktop-validation.md` — real-session validation procedure.
5. `docs/architecture/README.md` — stable contract index; then read the contract for the touched subsystem.
6. `docs/legacy/README.md` and `docs/legacy/source-annotations.md` — legacy source map when behavior comparison is needed.

Rules:

- Communicate with the user in Chinese; keep code, comments, tests, docs identifiers, and commit messages in English unless surrounding content requires otherwise.
- Current priority: keep released user workflows, documentation, packaging, and publication paths clear and green while developing new capabilities behind focused tests.
- Vinpst is an independent product. Preserve `vinpst` / `fcitx-vinpst` identities and paths; do not add upstream package replacement, old-name aliases, or automatic migration unless a released compatibility requirement calls for it.
- Prefer product-spine work over generic cleanup.
- Preserve published Vinpst wire formats and frontend expectations with focused behavior tests. Make incompatible public changes only through an explicit versioned migration or release decision.
- Do not add tests that assert source declarations, interface text, exact documentation wording, docstrings, recipe names, or other implementation prose.
- Keep the retained Fcitx frontend thin. Backend logic belongs in Rust crates and `vinpst-daemon`.
- Keep the standalone GUI in Rust; do not port or restore the legacy Qt GUI as a C++ product component.
- Distinguish `implemented`, `deterministic`, and `live-proven`; never count a smoke as desktop proof.
- Use the validation tiers in `docs/development.md`; run focused checks while iterating and the full relevant gate before handoff.

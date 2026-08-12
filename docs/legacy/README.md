# Legacy source analysis

This directory contains tracked analysis of the upstream `fcitx5-vinput/src` tree. It is development input for practical feature parity, not a request to reuse upstream names, paths, packages, D-Bus identities, CLI spelling, or implementation structure.

- [`upstream-source-inventory.json`](upstream-source-inventory.json): generated inventory of every production C/C++ file and every Ctags function, prototype, signal, and slot occurrence at the recorded upstream commit.
- [`source-annotations.md`](source-annotations.md): review-oriented file map that assigns each source file a Vinpst implementation area and behavior note.

Regenerate the inventory from a clean upstream checkout when refreshing the audit baseline:

```sh
scripts/tools/generate-upstream-inventory.py \
  --upstream-root /path/to/fcitx5-vinput
```

The generated file is reviewed as audit evidence rather than enforced by a source-path/layout CI checker.

The scheduled `Upstream parity drift` workflow checks the latest upstream default branch against the tracked inventory. A failure means the review baseline must be refreshed; it does not imply that every new low-level function requires a one-to-one Rust port.

These files are intentionally tracked because they are part of the migration record.

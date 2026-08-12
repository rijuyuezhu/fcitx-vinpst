# Legacy source analysis

This directory contains tracked analysis of the upstream `fcitx5-vinput/src` tree. It is development input for practical feature parity, not a request to reuse upstream names, paths, packages, D-Bus identities, CLI spelling, or implementation structure.

- [`upstream-sync.json`](upstream-sync.json): the last upstream commit intentionally reviewed for Vinpst.
- [`upstream-source-inventory.json`](upstream-source-inventory.json): generated inventory of every production C/C++ file and every Ctags function, prototype, signal, and slot occurrence at the frozen audit commit.
- [`source-annotations.md`](source-annotations.md): review-oriented file map that assigns each source file a Vinpst implementation area and behavior note.

Regenerate the inventory from a clean upstream checkout when refreshing the audit baseline:

```sh
scripts/tools/generate-upstream-inventory.py \
  --upstream-root /path/to/fcitx5-vinput
```

The generated file is reviewed as audit evidence rather than enforced by a source-path/layout CI checker.

The inventory is a frozen review baseline. `Upstream sync watch` compares only the recorded reviewed commit with the upstream default-branch HEAD and opens or updates a normal GitHub issue when they differ. It is not a required CI gate. When an upstream refresh is intentionally reviewed, classify meaningful deltas by user-visible capability, regenerate the inventory only when useful, and update `upstream-sync.json` in the PR that records the completed review.

These files are intentionally tracked because they are part of the migration record.

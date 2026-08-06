# Release scripts

This directory owns checked source archives, safe release-source materialization,
Arch/Debian/RPM/Nix/Flatpak package gates, and signed release-candidate
boundaries.

Lightweight checks, also exposed through `just package-check`:

```sh
scripts/release/check-arch-install-script.sh
scripts/release/check-arch-pkgbuild.sh
scripts/release/check-deb-package.sh
scripts/release/check-flatpak-manifest.sh
scripts/release/check-nix-flake.sh
scripts/release/check-rpm-spec.sh
scripts/release/check-source-archive.sh
scripts/release/check-release-manifest.sh
scripts/release/check-release-signature.sh
scripts/release/check-arch-release-candidate.sh
```

The complete release gate is:

```sh
scripts/release/run-arch-package-smoke.sh
scripts/release/run-deb-package-smoke.sh
scripts/release/run-flatpak-package-smoke.sh
scripts/release/run-nix-package-smoke.sh
scripts/release/run-rpm-package-smoke.sh
```

The tag workflow first calls the same reusable CI workflow used for pull
requests, retaining strict docs, complete Rust/FFI/integration, and locked Nix
checks while skipping its duplicate Debian matrix. It then creates one
deterministic source archive. Arch, Debian, and Flatpak jobs download that
artifact, materialize it through
`extract-source-archive.py`, and run package construction from the extracted
tree rather than the Actions checkout. Arch and Flatpak record/recheck the
consumed archive SHA-256 before publication selection; Flatpak additionally
copies the exact archive bytes into its local build inputs.

The Arch path builds the checked runtime bundle, formal package, and synthetic
upgrade archive, then runs isolated pacman transaction/repository/signing tests
and promotes a verified release candidate. The tag workflow selects only the
formal unsigned `pkgrel=1` package; temporary signatures, keys, repository
metadata, and `pkgrel=2` remain test evidence. The Debian Docker matrix builds release
1 and synthetic release 2 for Debian 12 and Ubuntu 24.04, performs real `dpkg`
install/upgrade/removal transactions, and publishes only release 1. The Nix
path evaluates the lock file and builds the immutable closure. The Flatpak path
uses locked Fcitx/KDE/Rust/LLVM runtimes and Cargo/native sources. It materializes
the checked native runtime and every crates.io archive outside Flatpak Builder,
using bounded transport, concurrent Cargo downloads, host-cache reuse only after
exact SHA-256 verification, and local manifest-relative sources. It then builds
the product once, verifies a real extension install/update/remove transaction,
and publishes only the revision-1 bundle; revision 2 changes only a test marker
in the exported OSTree tree. Test keys and synthetic upgrade artifacts are never
production release inputs.

The package lifecycle contract installs three cooperating files:

- `package-session-common.sh`: ownership-verified session-bus discovery and minimal user environment construction;
- `package-upgrade-handoff.sh`: existing-owner-only dispatch into `vinpst daemon handoff`;
- `package-remove-handoff.sh`: all-session removal preflight, guarded mutation, and activation rollback.

They are product lifecycle code, not developer convenience scripts.

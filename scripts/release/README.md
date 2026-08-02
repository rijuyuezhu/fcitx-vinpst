# Release scripts

This directory owns checked source archives, Arch/Debian/RPM/Nix/Flatpak package gates,
and signed release-candidate boundaries.

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

The Arch path builds the checked runtime bundle, package, and synthetic upgrade
archive, then runs isolated pacman transaction/repository/signing tests and
promotes a verified release candidate. The Debian Docker matrix builds release
1 and synthetic release 2 for Debian 12 and Ubuntu 24.04, performs real `dpkg`
install/upgrade/removal transactions, and publishes only release 1. The Nix
path evaluates the lock file and builds the immutable closure. The Flatpak path
uses locked Fcitx/KDE/Rust/LLVM runtimes and Cargo/native sources. It prefetches
the checked native runtime with bounded transport and exact SHA-256 verification,
builds the product once, verifies a real extension install/update/remove
transaction, and publishes only the revision-1 bundle; revision 2 changes only a
test marker in the exported OSTree tree. Test keys and synthetic upgrade
artifacts are never production release inputs.

The package lifecycle contract installs three cooperating files:

- `package-session-common.sh`: ownership-verified session-bus discovery and minimal user environment construction;
- `package-upgrade-handoff.sh`: existing-owner-only dispatch into `vinput daemon handoff`;
- `package-remove-handoff.sh`: all-session removal preflight, guarded mutation, and activation rollback.

They are product lifecycle code, not developer convenience scripts.

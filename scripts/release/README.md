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
scripts/release/check-release-metadata.sh
scripts/release/check-release-signature.sh
scripts/release/check-github-release-publish.sh
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

After bundle assembly, the release workflow runs
`run-release-bundle-install-smoke.sh` in a fresh Ubuntu 24.04 container. It
verifies the final manifest and checksums, satisfies the selected Ubuntu
package's declared runtime dependencies, exercises the bundled tarball before
the native package is installed, then installs the selected Ubuntu package,
runs the installed CLI and offline GUI check, initializes a new user profile,
removes the package, and requires that user configuration remain byte-identical.

The tag workflow first calls the same reusable CI workflow used for pull
requests, retaining strict docs, complete Rust/FFI/integration, and locked Nix
checks while skipping its duplicate Debian matrix. It then creates one
deterministic source archive. Arch, Debian, Fedora RPM, openSUSE RPM, Flatpak,
and release Nix jobs download that artifact, materialize it through
`extract-source-archive.py`, and run package construction from the extracted
tree rather than the Actions checkout. Arch, both RPM jobs, and Flatpak
record/recheck the consumed archive SHA-256 before publication selection;
Flatpak additionally copies the exact archive bytes into its local build inputs.
The bundled Linux tarball is assembled from the already transaction-tested
Ubuntu release-1 `/usr` payload and normalizes tar/gzip metadata instead of
compiling a duplicate payload.

The Arch path builds the checked runtime bundle, formal package, and synthetic
upgrade archive, then runs isolated pacman transaction/repository/signing tests
and promotes a verified release candidate. The tag workflow selects only the
formal unsigned `pkgrel=1` package; temporary signatures, keys, repository
metadata, and `pkgrel=2` remain test evidence. The Debian Docker matrix builds release
1 and synthetic release 2 for Debian 12 and Ubuntu 24.04, performs real `dpkg`
install/upgrade/removal transactions, and publishes only release 1. Fedora 43
and openSUSE Leap 16.0 each run the shared RPM release gate, build formal release
1 plus synthetic release 2, and publish only release 1 after the isolated
transaction gate. The Nix release path
evaluates the lock file, validates the immutable closure, and publishes it to
the public Cachix cache before bundle assembly may continue. The Flatpak path
uses locked Fcitx/KDE/Rust/LLVM runtimes and Cargo/native sources. It materializes
the checked native runtime and every crates.io archive outside Flatpak Builder,
using bounded transport, concurrent Cargo downloads, host-cache reuse only after
exact SHA-256 verification, and local manifest-relative sources. It then builds
the product once, verifies a real extension install/update/remove transaction,
and publishes only the revision-1 bundle; revision 2 changes only a test marker
in the exported OSTree tree. Test keys and synthetic upgrade artifacts are never
production release inputs.

Package smokes share only immutable/downloaded inputs through
`target/package-source-cache`: Cargo registry/git data and native runtime assets
whose SHA-256 values come from the checked runtime manifest. Set
`VINPST_PACKAGE_SOURCE_CACHE` to relocate that cache; relative values resolve
from the repository root and are normalized to an absolute path before package
builders change directories. Cargo target directories, CMake trees, and package
outputs remain under `target/tmp` and are intentionally excluded from this
cross-run source cache.

The package lifecycle contract installs three cooperating files:

- `package-session-common.sh`: ownership-verified session-bus discovery and minimal user environment construction;
- `package-upgrade-handoff.sh`: existing-owner-only dispatch into `vinpst daemon handoff`;
- `package-remove-handoff.sh`: all-session removal preflight, guarded mutation, and activation rollback.

They are product lifecycle code, not developer convenience scripts.

## Publication boundary

The release-bundle job creates GitHub/Sigstore provenance attestations for every
checked GitHub asset. A version tag generates release notes from conventional
commits with git-cliff, creates or reuses only a draft GitHub Release, uploads the complete checked bundle, compares the remote
asset names, sizes, and GitHub-reported SHA-256 digests with the local bundle,
and publishes only after they match. The workflow refuses to replace assets on
an already-public release. Nix is distributed through Cachix rather than as a
file in that flat GitHub bundle.

Run the workflow with `workflow_dispatch` for a non-publishing release-candidate
rehearsal. The operator procedure and incident policy are documented in
`docs/release/publishing.md`.

# Packaging contract

The first supported distro recipe is the checked Arch Linux `x86_64` template at `packaging/arch/PKGBUILD.in`. Release automation renders it with `scripts/render-arch-pkgbuild.py`; generated `PKGBUILD` and `.SRCINFO` files are build artifacts and are not the source of truth.

## Package identity

The package is named `fcitx-vinput-rs`. It provides and conflicts with `fcitx5-vinput` because both projects own the same Fcitx addon name, D-Bus service, and systemd user unit. They must not be installed together.

The recipe is currently `x86_64`-only because it uses the official sherpa-onnx 1.13.3 Linux x64 shared-library archive. Adding another architecture requires a pinned upstream runtime archive, checksums, runtime-link validation, and an updated package smoke; silently falling back to an ABI-incompatible system sherpa library is not allowed.

## Runtime contents

The package builds `vinput` and `vinput-daemon` in release mode with both `pipewire-backend` and `sherpa-onnx-backend`. It installs:

- `/usr/bin/vinput` and `/usr/bin/vinput-daemon`;
- `/usr/lib/fcitx5/fcitx5-vinput.so` and `/usr/share/fcitx5/addon/vinput.conf`;
- `/usr/lib/systemd/user/vinput-daemon.service` and `/usr/share/dbus-1/services/org.fcitx.Vinput.service`;
- the default config reference under `/usr/share/fcitx-vinput/default-config.json`;
- the Silero VAD model under `/usr/share/fcitx-vinput/vad/silero_vad.onnx`;
- project, Silero VAD, sherpa-onnx, and ONNX Runtime license material.

No ASR language model is bundled. A fresh configured daemon therefore starts with an unavailable ASR backend instead of exiting or fabricating mock text; installing/selecting a model and reloading activates the real backend without replacing the package.

## Native runtime isolation

The package does not link against the independently packaged system sherpa-onnx runtime. It installs the exact sherpa-onnx C API and ONNX Runtime libraries used by the Rust crate under `/usr/lib/fcitx-vinput`, then applies private relative rpaths:

- Rust binaries use `$ORIGIN/../lib/fcitx-vinput`;
- `libsherpa-onnx-c-api.so` uses `$ORIGIN`.

The unused sherpa C++ API library is not installed. This boundary prevents a system sherpa or ONNX Runtime upgrade from silently changing the package ABI.

## Activation and configuration

The installed systemd user unit starts:

```text
/usr/bin/vinput-daemon --dbus --configured-backends --audio-backend pipewire --exit-when-executable-replaced
```

The D-Bus activation file and the installed systemd user unit both start the daemon with `--exit-when-executable-replaced`. A later atomic package replacement changes the executable inode; the daemon detects the change, shuts down owned services, and exits unsuccessfully. A systemd-owned daemon is restarted immediately because the unit uses `Restart=on-failure` with a one-second delay. A daemon started directly by `dbus-daemon` is not supervised after exit, but the next D-Bus request activates the newly installed executable. The daemon discovers `$XDG_CONFIG_HOME/fcitx-vinput/config.json` or `$HOME/.config/fcitx-vinput/config.json`; the packaged reference config is not copied over user state during upgrades.

Staged CMake installs preserve their real runtime prefix and use `DESTDIR` as the filesystem staging root. A staging directory must never be passed as `cmake --install --prefix`: doing so changes generated runtime paths and can incorrectly move the absolute systemd user-unit directory under the temporary prefix. With the default `/usr/local` prefix, addon, locale, and D-Bus files stage under `DESTDIR/usr/local`, while the pkg-config-provided systemd unit remains under `DESTDIR/usr/lib/systemd/user`. The Arch recipe uses `/usr` plus `DESTDIR`, so all package files land under one `/usr` tree.

## Package transaction messages

The template attaches `packaging/arch/fcitx-vinput-rs.install` as the package `.INSTALL` script. Pacman scriptlets run in a root package transaction and cannot safely select or control every logged-in user's session bus. The hooks therefore execute no `systemctl --user`, `fcitx5`, or `vinput` command; they only print explicit per-user follow-up:

- `post_install` tells each desktop user to enable/start `vinput-daemon.service` and reload Fcitx5;
- `post_upgrade` explains that daemons started by current systemd or direct D-Bus activation metadata exit after executable replacement; systemd restarts immediately and direct activation resumes on the next request. If an owner from older metadata remains, each affected desktop user can run the guarded `vinput daemon handoff`, followed by `fcitx5 -r` when the addon changed;
- `post_remove` states that user config, models, and cache are preserved, then shows how each logged-in user can stop a still-running daemon and reload Fcitx5.

`scripts/render-arch-pkgbuild.py` copies the checked install script beside every rendered PKGBUILD. Generated release directories are therefore complete makepkg inputs rather than a PKGBUILD that silently depends on a file left in the source tree.

## Reproducibility and validation

The template pins source archives and third-party license checksums. Arch makepkg LTO is disabled because linker-plugin LTO breaks the `ring` build script's C/assembly objects; normal Rust release optimization remains enabled. Debug splitting is disabled for the already stripped release artifacts.

Development/test builds may resolve the repository VAD asset through a build-script-provided path. Release builds do not embed that source-tree path and resolve only explicit, user XDG, or system XDG VAD locations.

The following gates cover the recipe:

- `just arch-install-script-check` syntax-checks every lifecycle hook, executes each hook with an intentionally empty `PATH`, verifies the exact guidance, and rejects command-shaped script lines. This proves the scriptlets are message-only. It is part of `just ci`.
- `just arch-pkgbuild-check` renders the template, parses it with `makepkg --printsrcinfo`, and verifies identity, dependencies, conflicts, architecture, options, and pinned checksums. It is part of `just ci`.
- `just arch-package-smoke` performs a clean `makepkg` build, verifies the package archive carries the checked `.INSTALL` lifecycle hooks, extracts the package, validates the complete file set, private rpaths and dynamic linkage, confirms no build-tree path remains, runs the packaged binaries, and checks systemd/D-Bus activation commands. It then uses `makepkg --repackage` to create a `pkgrel=2` archive without recompiling and invokes the direct transaction, local repository, signed repository, and release-artifact bundle smokes. It is an explicit release gate because it downloads fixed upstream assets and recompiles release binaries.
- `just arch-package-transaction-smoke` reuses the `pkgrel=1` and `pkgrel=2` archives under a fakeroot-isolated pacman root. It proves package database registration, file ownership and integrity, install, upgrade, same-version `pkgrel=2` to `pkgrel=1` rollback, and removal. A pre-existing `$HOME/.config/fcitx-vinput/config.json` sentinel remains byte-identical throughout, and the package owns no `/etc` or `/home` paths.
- `just arch-repository-smoke` creates a local `repo-add` database containing `pkgrel=1`, synchronizes it through a `file://` pacman repository, and installs the package with `pacman -S`. It then replaces the repository entry with `pkgrel=2`, refreshes the sync database, upgrades through the repository, verifies both downloaded package archives reached pacman's cache, checks package integrity and user-config preservation, and removes the package. The repository intentionally uses `SigLevel = Never`; signing and trust policy remain external-publication work.
- `just arch-signing-smoke` generates an ephemeral Ed25519 signing key under `target/tmp`, signs both package archives and the `repo-add` database, imports only the public key into a fakeroot-isolated pacman keyring, and requires `SigLevel = Required DatabaseRequired`. It proves signed install and upgrade, verifies pacman reports `SHA-256 Sum  Signature`, rejects the signed database when the signer is absent from another isolated keyring, and rejects a same-size byte-flipped package as an invalid PGP signature. No private key, fingerprint, or trust database is checked into the repository. Production key custody, rotation, revocation, and public-key distribution remain release-operations work.

## Release artifact inventory

`scripts/release_manifest.py` assembles a flat release-artifact directory through a temporary sibling and verifies it before publication. `manifest.json` uses an exact schema: package name/version/architecture, one unique role per artifact, byte size, and lowercase SHA-256. `SHA256SUMS` is sorted by artifact name and must exactly match the manifest. The verifier rejects duplicate roles or basenames, unsafe names, symlinks, nested directories, non-regular entries, missing files, digest/size changes, and any unlisted extra file.

`--force` is deliberately narrow: it may replace only an existing bundle that already passes the same verifier. It cannot recursively delete an arbitrary directory, and an input artifact inside the output directory is rejected before any replacement. Assembly occurs in a new temporary directory so a copy, serialization, or validation error never publishes a partial bundle.

`just release-manifest-check` is the lightweight deterministic gate in `just ci`. It exercises successful assembly/verification and negative extra-file, same-size mutation, symlink, nested-directory, schema-drift, unsafe-force, in-output-source, and duplicate-role cases with tiny fixtures.

`manifest.json.sig` is optional metadata rather than a manifest artifact. Excluding the detached signature from `manifest.json` and `SHA256SUMS` avoids a recursive digest/signature dependency while retaining strict inventory checks: if the file is present it must be a regular top-level file with the reserved name, and rebuilding a bundle replaces the old directory without carrying a stale signature forward.

`scripts/sign-release-manifest.sh` accepts an already verified bundle, a caller-supplied GPG home, and an exact primary fingerprint. It requires the matching secret key, writes the detached signature to a sibling temporary file, verifies that temporary signature, publishes it atomically as mode `0644`, and never exports key material. `scripts/verify-release-bundle-signature.sh` requires a public-key file from outside the bundle plus the independently pinned fingerprint. It imports exactly one primary key into a temporary isolated GPG home, disables automatic key retrieval, requires a matching `VALIDSIG`, and only then runs the full artifact/checksum verifier. It never trusts a key merely because the same bundle contains a copy.

`just release-signature-check` is the lightweight deterministic signature gate in `just ci`. It proves signing, explicit re-signing, external-key verification, and the unsigned-after-rebuild boundary. It rejects a missing or modified signature, modified manifest, modified artifact, wrong key, wrong fingerprint, bundled-key trust, and an unavailable secret key. All generated keys live only under `target/tmp`.

`just arch-release-bundle-smoke` is part of the complete Arch release gate. After package and repository signing, it assembles exactly 13 release-gate artifacts: the source archive, rendered `PKGBUILD`, `.SRCINFO`, lifecycle script, `pkgrel=1` package/signature, synthetic `pkgrel=2` package/signature, signed repository database/files archives and signatures, and the ephemeral public key. It rechecks `SHA256SUMS`, verifies all four artifact signatures, signs `manifest.json` with the ephemeral release-gate key, verifies that signature using the public key outside the bundle and its pinned fingerprint, and proves extra, artifact-mutated, and signature-mutated bundle copies are rejected.

The roles `package-pkgrel2-test`, `package-signature-pkgrel2-test`, and `signing-public-key-test` explicitly mark test-only evidence. This directory is a release-gate bundle, not a public release set. The detached-manifest signing mechanism is deterministic, but production publication must select actual release artifacts, sign with the managed production key, distribute the expected fingerprint through an independent trusted channel, and exclude all ephemeral test assets.

## Release candidate promotion

`scripts/prepare-arch-release-candidate.sh` is the explicit promotion boundary between the broad release-gate evidence bundle and a publication-shaped candidate. It first verifies the gate's detached manifest signature against the external public key and pinned fingerprint, then requires the exact 13-role gate policy. It selects only `package-pkgrel1` and its signature, rebuilds fresh `repo-add` database/files metadata containing that package alone, signs those repository indexes with the caller-supplied signing home, copies the external public key under a stable name, assembles a new bundle, signs its manifest, and invokes the candidate verifier before reporting success. The repository builder supports both pacman 6, which automatically embeds a sibling package signature, and pacman 7, which requires the explicit `--include-sigs` option; both paths still sign the database/files indexes and are subject to the same candidate verification.

The candidate uses exactly 11 production roles: source archive, rendered Arch metadata, lifecycle script, package/signature, repository database/files archives and signatures, and signing public key. Role and file names containing `test` or `synthetic` are rejected. The candidate verifier checks the package name/base version/pkgrel/architecture, all three artifact signatures, the exact external public-key bytes, and the single repository entry's name/version/architecture/filename. It therefore prevents the synthetic `pkgrel=2` upgrade fixture or its repository metadata from leaking into a publication candidate.

Existing output is never replaced implicitly. `--force` first verifies the old candidate cryptographically and structurally; arbitrary directories, invalid candidates, symlink outputs, and descendants of the signed gate are refused. The lightweight `just release-candidate-check` builds minimal packages, a signed synthetic gate, and a promoted candidate entirely under `target/tmp`, then proves normal promotion, safe force replacement, gate-as-candidate rejection, output-path protection, invalid-force preservation, and mutation rejection.

The complete Arch gate promotes the real package outputs after all signing checks. Because that test still uses an ephemeral key, its candidate under `target/tmp` is not itself a public release. A production invocation must supply the managed signing home plus independently distributed public key and fingerprint, then publish only the verified candidate directory.

The current recipe proves package construction, extracted-runtime behavior, isolated pacman install/upgrade/same-version-rollback/uninstall transactions, local repository metadata/install/upgrade behavior, ephemeral signed-repository trust/tamper enforcement, strict release-gate artifact inventory/checksums, detached manifest signing against an external pinned key, test-role-free release-candidate promotion with rebuilt repository metadata, and safe lifecycle guidance at each package transition. `vinput daemon status` detects a running D-Bus owner from a different executable path or an executable inode unlinked by package replacement. The explicit `vinput daemon handoff` command first identifies whether the exact stale D-Bus owner is the systemd unit `MainPID`. A systemd owner is handled through `daemon-reload` followed by restart. A direct owner is sent `SIGTERM` only when procfs and runtime evidence prove an idle same-user `vinput-daemon`, a safe exact PID, no active session, and no systemd cgroup ownership; D-Bus activation metadata is reloaded before the signal, and a fresh matching owner is required before success. Current owners are a strict no-op, and guard/control failures leave the owner untouched. Rust behavior tests prove same-inode startup identity, atomic executable replacement detection, and asynchronous watcher completion. CLI, CMake staging, addon-install, and package checks prove that both current direct D-Bus activation metadata and the systemd unit pass the watcher flag, while the unit additionally enables failure restart. `run-direct-activation-upgrade-smoke.sh` proves direct-owner exit and next-request reactivation against a private session bus. The opt-in `just systemd-upgrade-live` gate temporarily replaces an idle real-profile owner with a runtime user unit, atomically replaces its copied daemon, verifies that `NRestarts` increments and `MainPID` changes, then restores the original activation file, executable, command entry, and idle owner without residue; its summary is retained at `target/tmp/systemd-upgrade-live/summary.json`. This proves both subsequent-upgrade supervision paths at process level. An actual package-installed upgrade, automatic cross-user invocation of the guarded old-metadata handoff, package removal, rollback across versions with incompatible config or state, externally hosted repository publication, production signing-key custody/rotation/revocation, independent fingerprint/public-key distribution, and external-user live validation remain separate release-readiness work.

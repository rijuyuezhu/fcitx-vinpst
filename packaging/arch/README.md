# Arch Linux package

External users should follow [`../../docs/user/installation.md`](../../docs/user/installation.md). This file documents package construction and release validation.

`PKGBUILD.in` is the release template for the Rust rewrite. Render it with a
release source archive whose top-level directory matches `--source-dir`:

```sh
scripts/release/render-arch-pkgbuild.py \
  --version 0.1.0 \
  --source-url https://example.invalid/fcitx-vinpst-0.1.0.tar.gz \
  --source-sha256 <sha256> \
  --source-dir fcitx-vinpst-0.1.0 \
  --output packaging/arch/PKGBUILD
```

The renderer selects the default entry from `runtime-bundles.json`. A release
may select another checked entry with `--runtime-bundle <id>` or point at a
different manifest with `--runtime-bundles <path>`. Every entry must provide a
safe package architecture and Rust target, sherpa archive/version/root, ONNX Runtime license
version, and lowercase SHA-256 values for every downloaded runtime/license
asset. Unknown bundle ids, duplicate ids, malformed checksums, unsafe tokens,
or unresolved template values fail before a PKGBUILD is written. Adding a
production runtime version therefore requires a reviewed manifest entry and a
successful package smoke; the application does not switch C libraries at
runtime.

The package builds the PipeWire and sherpa-onnx features, installs the retained
Fcitx addon plus D-Bus/systemd activation files, and bundles the exact sherpa
1.13.3/ONNX Runtime 1.24.4 shared libraries under `/usr/lib/fcitx-vinpst`.
Private rpaths prevent those pinned libraries from replacing unrelated system
copies. The package uses only Vinpst identities and deliberately declares no
replacement, conflict, or compatibility relationship with another voice-input
package.

Run `just package-smoke` to render a local-source PKGBUILD, execute
`makepkg`, inspect the package archive, run both extracted Rust binaries, create
a `pkgrel=2` repackage, and prove pacman install, upgrade, same-version
rollback, and uninstall in a
fakeroot-isolated temporary root. After a full build,
`scripts/release/run-arch-package-transaction-smoke.sh` reruns only the fast direct-package
transaction checks. `scripts/release/run-arch-repository-smoke.sh` creates a local `repo-add`
database from the same two archives and proves `pacman -S` installation and
upgrade through a `file://` repository.
`scripts/release/run-arch-signing-smoke.sh` adds an ephemeral signing key, package/database
signatures, an isolated trusted pacman keyring, unknown-signer rejection, and
same-size package tamper rejection. `scripts/release/run-arch-release-bundle-smoke.sh` then
assembles the source archive, rendered Arch metadata, package/repository
artifacts, signatures, and ephemeral public key into a strict `manifest.json`
plus `SHA256SUMS` inventory, revalidates every package/database signature,
signs `manifest.json`, and verifies `manifest.json.sig` using the public key from
outside the bundle plus a pinned fingerprint. The synthetic `pkgrel=2` and test
key are explicitly labeled test-only; this bundle is release-gate evidence, not
the public release set. The generated private key lives only under `target/tmp`;
the gate then promotes only `pkgrel=1` into a test-role-free candidate and
rebuilds repository metadata around that package. Production key custody and
independent fingerprint/public-key distribution are not part of the repository.

The renderer also copies `fcitx-vinpst.install` beside the generated PKGBUILD. Package lifecycle hooks do not inspect user sessions, contact D-Bus, or start/stop daemon services; they only print short guidance. D-Bus activation routes daemon startup through the packaged systemd user unit, so package installation does not enable or start that unit explicitly. Reload Fcitx5 in an active desktop session after installing or upgrading the addon:

```sh
fcitx5 -r
```

Upgrades rely on the daemon's executable-replacement watcher and normal D-Bus/systemd activation behavior; package hooks do not perform a daemon handoff. Removal similarly does not run a pre-remove daemon operation. User config, models, and cache remain outside package ownership.

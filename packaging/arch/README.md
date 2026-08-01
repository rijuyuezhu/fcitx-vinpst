# Arch Linux package

External users should follow [`../../docs/user/installation.md`](../../docs/user/installation.md). This file documents package construction and release validation.

`PKGBUILD.in` is the release template for the Rust rewrite. Render it with a
release source archive whose top-level directory matches `--source-dir`:

```sh
scripts/release/render-arch-pkgbuild.py \
  --version 0.1.0 \
  --source-url https://example.invalid/fcitx-vinput-rs-0.1.0.tar.gz \
  --source-sha256 <sha256> \
  --source-dir fcitx-vinput-rs-0.1.0 \
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
1.13.3/ONNX Runtime 1.24.4 shared libraries under `/usr/lib/fcitx-vinput`.
Private rpaths prevent those pinned libraries from replacing unrelated system
copies. `fcitx-vinput-rs` conflicts with and provides `fcitx5-vinput` because
both projects own the same addon, bus name, and user service.

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

The renderer also copies `fcitx-vinput-rs.install` beside the generated
PKGBUILD. The package installs a shared trusted-session helper plus guarded
upgrade and removal dispatchers. After installing a local package, each desktop
user should run:

```sh
systemctl --user enable --now vinput-daemon.service
fcitx5 -r
```

After an upgrade, the package scans only ownership-verified live session buses.
Sessions without an existing daemon owner are skipped. For each existing owner,
it runs the guarded `vinput daemon handoff` as that user: current owners are
unchanged, while stale systemd/direct owners are handled only after the CLI's
identity and idle checks. A failed session causes the package hook to report an
error and that user can retry `vinput daemon handoff`. Removal uses the separate
two-phase guarded preflight and leaves user config, models, and cache intact;
reload Fcitx5 afterward with `fcitx5 -r`.

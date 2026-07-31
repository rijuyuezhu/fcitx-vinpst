# Arch Linux package

`PKGBUILD.in` is the release template for the Rust rewrite. Render it with a
release source archive whose top-level directory matches `--source-dir`:

```sh
scripts/render-arch-pkgbuild.py \
  --version 0.1.0 \
  --source-url https://example.invalid/fcitx-vinput-rs-0.1.0.tar.gz \
  --source-sha256 <sha256> \
  --source-dir fcitx-vinput-rs-0.1.0 \
  --output packaging/arch/PKGBUILD
```

The package builds the PipeWire and sherpa-onnx features, installs the retained
Fcitx addon plus D-Bus/systemd activation files, and bundles the exact sherpa
1.13.3/ONNX Runtime 1.24.4 shared libraries under `/usr/lib/fcitx-vinput`.
Private rpaths prevent those pinned libraries from replacing unrelated system
copies. `fcitx-vinput-rs` conflicts with and provides `fcitx5-vinput` because
both projects own the same addon, bus name, and user service.

Run `just arch-package-smoke` to render a local-source PKGBUILD, execute
`makepkg`, inspect the package archive, run both extracted Rust binaries, create
a `pkgrel=2` repackage, and prove pacman install, upgrade, same-version
rollback, and uninstall in a
fakeroot-isolated temporary root. After a full build,
`just arch-package-transaction-smoke` reruns only the fast direct-package
transaction checks. `just arch-repository-smoke` creates a local `repo-add`
database from the same two archives and proves `pacman -S` installation and
upgrade through a `file://` repository.
`just arch-signing-smoke` adds an ephemeral signing key, package/database
signatures, an isolated trusted pacman keyring, unknown-signer rejection, and
same-size package tamper rejection. `just arch-release-bundle-smoke` then
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
PKGBUILD. Its hooks are message-only because a root pacman transaction cannot
safely target every user's session bus. After installing a local package, each
desktop user should run:

```sh
systemctl --user enable --now vinput-daemon.service
fcitx5 -r
```

After an upgrade, current activation metadata hands off automatically. When an
owner from older metadata remains, `vinput daemon handoff` identifies whether
the exact D-Bus owner belongs to the systemd user unit or to direct activation.
It reloads and restarts the former; it terminates the latter only after proving
that it is an idle same-user `vinput-daemon` outside the systemd unit, then
verifies the newly activated owner. After removal, the package leaves user
config, models, and cache intact; a still-running user daemon can be stopped
with `systemctl --user stop vinput-daemon.service`, followed by `fcitx5 -r`.

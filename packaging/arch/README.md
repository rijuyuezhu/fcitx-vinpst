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

The renderer also copies `fcitx-vinput-rs.install` beside the generated
PKGBUILD. Its hooks are message-only because a root pacman transaction cannot
safely target every user's session bus. After installing a local package, each
desktop user should run:

```sh
systemctl --user enable --now vinput-daemon.service
fcitx5 -r
```

After an upgrade, `vinput daemon handoff` conditionally restarts only a stale
daemon and verifies the new owner. After removal, the package leaves user
config, models, and cache intact; a still-running user daemon can be stopped
with `systemctl --user stop vinput-daemon.service`, followed by `fcitx5 -r`.

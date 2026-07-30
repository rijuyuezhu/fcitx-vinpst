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
/usr/bin/vinput-daemon --dbus --configured-backends --audio-backend pipewire
```

The D-Bus activation file carries the same `Exec=` fallback plus `SystemdService=vinput-daemon.service`. The daemon discovers `$XDG_CONFIG_HOME/fcitx-vinput/config.json` or `$HOME/.config/fcitx-vinput/config.json`; the packaged reference config is not copied over user state during upgrades.

Staged CMake installs preserve their real runtime prefix and use `DESTDIR` as the filesystem staging root. A staging directory must never be passed as `cmake --install --prefix`: doing so changes generated runtime paths and can incorrectly move the absolute systemd user-unit directory under the temporary prefix. With the default `/usr/local` prefix, addon, locale, and D-Bus files stage under `DESTDIR/usr/local`, while the pkg-config-provided systemd unit remains under `DESTDIR/usr/lib/systemd/user`. The Arch recipe uses `/usr` plus `DESTDIR`, so all package files land under one `/usr` tree.

## Package transaction messages

The template attaches `packaging/arch/fcitx-vinput-rs.install` as the package `.INSTALL` script. Pacman scriptlets run in a root package transaction and cannot safely select or control every logged-in user's session bus. The hooks therefore execute no `systemctl --user`, `fcitx5`, or `vinput` command; they only print explicit per-user follow-up:

- `post_install` tells each desktop user to enable/start `vinput-daemon.service` and reload Fcitx5;
- `post_upgrade` explains that the package transaction cannot restart every user session and points logged-in users to `vinput daemon handoff` plus `fcitx5 -r`;
- `post_remove` states that user config, models, and cache are preserved, then shows how each logged-in user can stop a still-running daemon and reload Fcitx5.

`scripts/render-arch-pkgbuild.py` copies the checked install script beside every rendered PKGBUILD. Generated release directories are therefore complete makepkg inputs rather than a PKGBUILD that silently depends on a file left in the source tree.

## Reproducibility and validation

The template pins source archives and third-party license checksums. Arch makepkg LTO is disabled because linker-plugin LTO breaks the `ring` build script's C/assembly objects; normal Rust release optimization remains enabled. Debug splitting is disabled for the already stripped release artifacts.

Development/test builds may resolve the repository VAD asset through a build-script-provided path. Release builds do not embed that source-tree path and resolve only explicit, user XDG, or system XDG VAD locations.

Two gates cover the recipe:

- `just arch-install-script-check` syntax-checks every lifecycle hook, executes each hook with an intentionally empty `PATH`, verifies the exact guidance, and rejects command-shaped script lines. This proves the scriptlets are message-only. It is part of `just ci`.
- `just arch-pkgbuild-check` renders the template, parses it with `makepkg --printsrcinfo`, and verifies identity, dependencies, conflicts, architecture, options, and pinned checksums. It is part of `just ci`.
- `just arch-package-smoke` performs a clean `makepkg` build, verifies the package archive carries the checked `.INSTALL` lifecycle hooks, extracts the package, validates the complete file set, private rpaths and dynamic linkage, confirms no build-tree path remains, runs the packaged binaries, and checks systemd/D-Bus activation commands. It then uses `makepkg --repackage` to create a `pkgrel=2` archive without recompiling and invokes the pacman transaction smoke. It is an explicit release gate because it downloads fixed upstream assets and recompiles release binaries.
- `just arch-package-transaction-smoke` reuses the `pkgrel=1` and `pkgrel=2` archives under a fakeroot-isolated pacman root. It proves package database registration, file ownership and integrity, install, upgrade, same-version `pkgrel=2` to `pkgrel=1` rollback, and removal. A pre-existing `$HOME/.config/fcitx-vinput/config.json` sentinel remains byte-identical throughout, and the package owns no `/etc` or `/home` paths.
- `just arch-repository-smoke` creates a local `repo-add` database containing `pkgrel=1`, synchronizes it through a `file://` pacman repository, and installs the package with `pacman -S`. It then replaces the repository entry with `pkgrel=2`, refreshes the sync database, upgrades through the repository, verifies both downloaded package archives reached pacman's cache, checks package integrity and user-config preservation, and removes the package. The repository intentionally uses `SigLevel = Never`; signing and trust policy remain external-publication work.
- `just arch-signing-smoke` generates an ephemeral Ed25519 signing key under `target/tmp`, signs both package archives and the `repo-add` database, imports only the public key into a fakeroot-isolated pacman keyring, and requires `SigLevel = Required DatabaseRequired`. It proves signed install and upgrade, verifies pacman reports `SHA-256 Sum  Signature`, rejects the signed database when the signer is absent from another isolated keyring, and rejects a same-size byte-flipped package as an invalid PGP signature. No private key, fingerprint, or trust database is checked into the repository. Production key custody, rotation, revocation, and public-key distribution remain release-operations work.

The current recipe proves package construction, extracted-runtime behavior, isolated pacman install/upgrade/same-version-rollback/uninstall transactions, local repository metadata/install/upgrade behavior, ephemeral signed-repository trust/tamper enforcement, and safe lifecycle guidance at each package transition. `vinput daemon status` detects a running D-Bus owner from a different executable path or an executable inode unlinked by package replacement. The explicit `vinput daemon handoff` command restarts the systemd user service only for those stale states and requires a fresh matching owner before reporting success; current owners are a strict no-op and service-control failures leave the owner untouched. Automatic package-manager invocation inside user sessions, rollback across versions with incompatible config or state, destructive direct-PID stale-owner cleanup, externally hosted repository publication, production signing-key operations, and live installed-desktop validation remain separate release-readiness work.

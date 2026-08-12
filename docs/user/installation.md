# Installation

## Release packages

Vinpst publishes:

- a bundled Linux x86_64 tarball for manual integration;
- Arch Linux x86_64;
- Debian 12 amd64;
- Ubuntu 24.04 amd64;
- Fedora 43 x86_64;
- openSUSE Leap 16.0 x86_64;
- a public x86_64 Nix/Cachix channel;
- an x86_64 Flatpak extension for `org.fcitx.Fcitx5//stable`.

Download the package for your distribution from [GitHub Releases](https://github.com/rijuyuezhu/fcitx-vinpst/releases).

### Arch Linux

```sh
sudo pacman -U ./fcitx-vinpst-*-x86_64.pkg.tar.zst
```

### Debian 12

```sh
sudo apt install ./fcitx-vinpst_*_debian12_amd64.deb
```

### Ubuntu 24.04

```sh
sudo apt install ./fcitx-vinpst_*_ubuntu24.04_amd64.deb
```

### Fedora 43

```sh
sudo dnf install ./fcitx-vinpst-*-fedora43-x86_64.rpm
```

### openSUSE Leap 16.0

```sh
sudo zypper install ./fcitx-vinpst-*-opensuse16.0-x86_64.rpm
```

### Nix / Cachix

The release workflow publishes the locked `x86_64-linux` closure to the public
`fcitx-vinpst` Cachix cache. `cachix use fcitx-vinpst` configures the
substituter and its managed signing key. Set `release_tag` to a tag shown on
GitHub Releases (for example `v0.1.0`), then build the flake:

```sh
release_tag=v0.1.0
cachix use fcitx-vinpst
nix build "github:rijuyuezhu/fcitx-vinpst/${release_tag}#fcitx-vinpst"
```

### Bundled Linux tarball

The bundled Linux tarball contains the same `/usr`
payload as the transaction-tested Ubuntu release package, including the private
sherpa-onnx/ONNX Runtime libraries. It does not bundle ordinary host libraries
or desktop services: a compatible Linux system still needs the normal Fcitx,
PipeWire, systemd, Wayland/X11, fontconfig, and related runtime dependencies.
It is primarily for manual integration, inspection, or environments that cannot
consume a native package; prefer the native package for normal desktops because
tarball extraction is not tracked by a package manager.

### Flatpak

The Flatpak build extends the Fcitx Flatpak; it does not attach to a system-installed Fcitx. If you use the system Fcitx package, prefer the native Vinpst package for your distribution.

```sh
flatpak info --user org.fcitx.Fcitx5
flatpak install --user --bundle ./fcitx-vinpst-*-x86_64.flatpak
```

Grant the Fcitx Flatpak the runtime paths used by audio capture, the per-user systemd service, and Vinpst caches, then restart that Flatpak instance so the new permissions take effect:

```sh
flatpak override --user --filesystem=xdg-run/pipewire-0 org.fcitx.Fcitx5
flatpak override --user --filesystem=xdg-config/systemd:create org.fcitx.Fcitx5
flatpak override --user --filesystem=xdg-cache org.fcitx.Fcitx5
flatpak kill org.fcitx.Fcitx5
```

The extension binaries live inside the Fcitx Flatpak rather than on the host `PATH`. Use the packaged CLI once to install the host user-service unit and initialize configuration, then enable the service on the host:

```sh
flatpak run --user \
  --command=/app/addons/Vinpst/bin/vinpst \
  org.fcitx.Fcitx5 daemon install-service
flatpak run --user \
  --command=/app/addons/Vinpst/bin/vinpst \
  org.fcitx.Fcitx5 init
systemctl --user enable --now vinpst-daemon.service
```

`vinpst doctor` reports the same Flatpak permission requirements as `remediation_commands` when troubleshooting; run it through the same `flatpak run --command=/app/addons/Vinpst/bin/vinpst org.fcitx.Fcitx5 ...` boundary.

## Development checkout

Use this only when intentionally testing or developing the current source tree.

You need Rust 1.88 or newer, Cargo, CMake, a C++ compiler, Fcitx 5 development files, PipeWire development files, gettext, and `just`.

Build and install for the current user:

```sh
just build
just addon-fcitx-build
just install-user
```

The installer prints the files it writes and the commands needed to refresh the current session. Remove this per-user installation with:

```sh
just user-remove
```

## After installation

For native Arch, Debian, Ubuntu, Fedora, or openSUSE packages, initialize the user configuration, start the daemon, and reload Fcitx:

```sh
vinpst init
systemctl --user enable --now vinpst-daemon.service
fcitx5 -r
```

The Flatpak steps above perform the equivalent initialization and service bootstrap. Then follow the [Quick start](quick-start.md) to install a model and try dictation.

## Uninstall

Finish any active recording and remove Vinpst through the same package manager that installed it. Package removal stops the daemon safely but keeps your configuration, downloaded models, provider/adapter scripts, hotwords, and caches.

Delete those user files manually only when you intentionally want to discard all Vinpst state.

For release integrity/provenance details and maintainer procedures, see the **Development → Publishing and rollback** documentation rather than the normal installation guide.

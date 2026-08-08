# Installation

Vinpst is preparing its first `0.1.0` release. Public release packages are not available yet, so ordinary users should not install files produced under `target/` or copied from CI test fixtures.

## Supported identity and paths

Vinpst uses its own names throughout:

- package: `fcitx-vinpst`;
- commands: `vinpst`, `vinpst-daemon`, and `vinpst-gui`;
- Fcitx addon: `vinpst` / `fcitx5-vinpst.so`;
- D-Bus service: `org.fcitx.Vinpst`;
- systemd user service: `vinpst-daemon.service`;
- configuration root: `${XDG_CONFIG_HOME:-$HOME/.config}/fcitx-vinpst`;
- data root: `${XDG_DATA_HOME:-$HOME/.local/share}/fcitx-vinpst`;
- cache root: `${XDG_CACHE_HOME:-$HOME/.cache}/fcitx-vinpst`.

Vinpst does not replace or migrate another voice-input package. Do not rename its installed files or configuration directories to match another project.

## Release packages

The current `0.1.0` release workflow selects an Arch Linux x86_64 package, Debian 12 and Ubuntu 24.04 amd64 packages, and an x86_64 Flatpak extension bundle. These artifacts are still pre-release: the repository is validating the complete workflow before publishing them. RPM and Nix remain validated build paths rather than selected public artifacts.

When release artifacts are published, download the complete checked release directory rather than an isolated package. From that directory, verify every listed file before installation:

```sh
sha256sum -c SHA256SUMS
```

The checksum file detects corruption or mismatched artifacts. The release workflow also creates signed GitHub/Sigstore provenance attestations for every asset. Verify a downloaded artifact against the release workflow identity with:

```sh
gh attestation verify ./fcitx-vinpst-0.1.0.tar.gz \
  --repo rijuyuezhu/fcitx-vinpst \
  --signer-workflow rijuyuezhu/fcitx-vinpst/.github/workflows/release.yml
```

### Arch Linux

Install the native x86_64 package with pacman:

```sh
sudo pacman -U ./fcitx-vinpst-0.1.0-1-x86_64.pkg.tar.zst
```

### Debian 12

Install the Debian 12 amd64 package with APT so dependencies are resolved:

```sh
sudo apt install ./fcitx-vinpst_0.1.0-1_debian12_amd64.deb
```

### Ubuntu 24.04

Install the Ubuntu 24.04 amd64 package with APT:

```sh
sudo apt install ./fcitx-vinpst_0.1.0-1_ubuntu24.04_amd64.deb
```

### Flatpak extension preview

The Flatpak artifact extends `org.fcitx.Fcitx5//stable`; it does not attach to a system-installed Fcitx. Check that the matching Fcitx Flatpak is installed before installing the bundle:

```sh
flatpak info --user org.fcitx.Fcitx5
flatpak install --user --bundle ./fcitx-vinpst-0.1.0-x86_64.flatpak
```

The Flatpak path remains a preview until host-session Fcitx discovery, PipeWire capture, service lifecycle, and GUI interaction are validated on an unrelated desktop. System Fcitx users should use the native package for their distribution.

Do not use a package produced for another distribution merely because it contains Linux binaries. After native package installation, continue with [Quick start](quick-start.md).

## Development checkout installation

This is a contributor and early-testing path, not the final end-user installation method.

Required build tools include Rust 1.88 or newer, `cargo`, CMake, a C++ compiler, Fcitx 5 development files, PipeWire development files, gettext, and `just`. Exact package names vary by distribution.

Build the Rust workspace and real Fcitx addon:

```sh
just build
just addon-fcitx-build
```

Install the current checkout into the active user's XDG/Fcitx directories:

```sh
just install-user
```

The installer prints the exact files it writes and the commands required to restart the daemon and Fcitx. It does not install a system package.

Remove that per-user checkout installation with:

```sh
just user-remove
```

## After installation

Initialize user state, start the daemon, and reload Fcitx:

```sh
vinpst init
systemctl --user enable --now vinpst-daemon.service
fcitx5 -r
```

Then follow [Quick start](quick-start.md) to install an ASR model and verify the setup.

## Uninstalling a release package

Before removing a release package, finish any active recording, then remove the package through the package manager that installed it. The package hook performs the guarded daemon shutdown/removal handoff automatically; users should not need to invoke the internal maintenance command themselves.

Package removal must not delete user configuration, downloaded models, provider scripts, adapter scripts, hotword files, or caches. Remove those directories manually only when you intentionally want to discard all Vinpst user state.

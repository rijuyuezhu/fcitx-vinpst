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

The repository currently validates Debian 12, Ubuntu 24.04, Flatpak, Arch, RPM, and Nix packaging paths at different evidence levels. The final `0.1.0` publication matrix will be listed here after release CI is complete.

When release artifacts are published:

1. download the artifact for your distribution from the matching Vinpst release;
2. verify the supplied checksums and manifest;
3. install the package with your distribution package manager;
4. continue with [Quick start](quick-start.md).

Do not use a package produced for another distribution merely because it contains Linux binaries.

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

Before removing a release package, finish any active recording and stop the user service:

```sh
vinpst daemon prepare-remove
```

Remove the package through the package manager that installed it. Package removal must not delete user configuration, downloaded models, provider scripts, adapter scripts, hotword files, or caches. Remove those directories manually only when you intentionally want to discard all Vinpst user state.

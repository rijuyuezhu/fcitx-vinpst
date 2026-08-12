# Project identity contract

`fcitx-vinpst` is the only supported project identity. This is an explicit product boundary: Vinpst does not reuse or migrate upstream identities, and identifiers that were never part of a public Vinpst release do not receive compatibility aliases.

## Canonical names

- Repository and distribution package: `fcitx-vinpst`.
- Rust packages and libraries: `vinpst-*`.
- Executables: `vinpst`, `vinpst-daemon`, and `vinpst-gui`.
- Fcitx addon: `vinpst`, with module `fcitx5-vinpst.so`.
- D-Bus service: `org.fcitx.Vinpst` at `/org/fcitx/Vinpst`, using `org.fcitx.Vinpst.Service`.
- User service: `vinpst-daemon.service`.
- Desktop application and icon: `vinpst-gui`.
- User configuration, data, and cache roots: `fcitx-vinpst` below the corresponding XDG homes.
- Product-owned environment variables: `VINPST_*`.

## No compatibility aliases

The project must not install old executable symlinks, old D-Bus activation files, old systemd unit aliases, package `Provides`/`Conflicts`/`Replaces`/`Obsoletes` entries for other identities, environment-variable fallbacks, or automatic old-path migration. Runtime code reads and writes only the canonical identity.

Historical upstream names may appear only in migration/source-analysis documentation or in URLs for genuinely external resources that have not been renamed. They must not be exposed as supported runtime identities.

## Change rule

Any future identity change must update code, build targets, package metadata, install paths, D-Bus contracts, desktop metadata, documentation, fixtures, and deterministic checks atomically. Compatibility behavior requires a separate explicit product decision; it must not be added opportunistically during a rename.

# Debian package baseline

The checked Debian package is assembled by
`scripts/release/build-deb-package.sh`. It builds the Rust CLI, daemon, and
Iced GUI plus the retained Fcitx addon, then installs the same private
checksum-pinned sherpa-onnx/ONNX Runtime bundle used by the Arch and RPM
packages. Maintainer scripts only print short lifecycle guidance; they do not
inspect or mutate live desktop sessions.

The release targets currently proved by Docker are:

- Debian 12 (`debian:12`)
- Ubuntu 24.04 (`ubuntu:24.04`)

Run one target with:

```bash
scripts/release/run-deb-package-smoke.sh \
  --image debian:12 \
  --distribution debian12
```

The smoke builds package releases 1 and 2, validates metadata, payload,
linkage, private rpaths, maintainer scripts, and offline GUI startup, then
performs a real `dpkg` install, upgrade, verification, removal, and purge
inside the target container. A pre-existing unsupported-future-schema user
configuration must remain byte-identical and unowned throughout.

These packages are release artifacts, unlike the Nix flake expression itself.
Nix normally publishes a derivation result through a binary cache rather than
attaching a `.nix` file as a binary package.

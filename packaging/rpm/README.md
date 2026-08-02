# RPM packaging baseline

`fcitx-vinput-rs.spec.in` is the checked RPM-family source-package template. It is rendered by `scripts/release/render-rpm-spec.py` from the same strict native-runtime bundle manifest used by the Arch package.

The current baseline targets x86_64 and packages:

- `vinput`, `vinput-daemon`, and `vinput-gui`;
- the retained Fcitx 5 addon, D-Bus activation metadata, and systemd user unit;
- desktop entry, icons, configuration/VAD data, and zh_CN translations;
- the checksum-pinned sherpa-onnx C API and ONNX Runtime libraries under `/usr/lib/fcitx-vinput` with private rpaths;
- the shared upgrade/removal session helpers.

The spec declares Fedora/RHEL-family dependency names and uses `/usr/lib64/fcitx5` for the addon while keeping the systemd user unit under `/usr/lib/systemd/user`.

Run the lightweight deterministic metadata gate with:

```sh
just package-check
```

Run the explicit release gate with:

```sh
just rpm-package-smoke
```

The release gate renders two package releases from a clean source archive, builds both with `rpmbuild`, validates metadata/scriptlets/payload/rpaths/linkage and the display-independent GUI check, then uses an unprivileged user namespace to perform `--noscripts` install, upgrade, verification, and removal against an isolated chroot/rpmdb while proving an unsupported future user config is byte-preserved.

The isolated transaction intentionally does not execute package scriptlets because the synthetic rpm root does not contain a complete Fedora userspace or live desktop sessions. Scriptlet metadata and argument guards are checked deterministically; executing upgrade/removal handoff inside a real Fedora-family package transaction remains a live release requirement. Repository metadata, package signing, DNF installation, SELinux policy review, and builds inside supported Fedora/openSUSE distributions also remain.

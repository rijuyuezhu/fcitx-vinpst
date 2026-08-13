# Fedora and openSUSE RPM release packages

`fcitx-vinpst.spec.in` is the checked RPM-family source-package template. `scripts/release/render-rpm-spec.py` renders Fedora 43 and openSUSE Leap 16.0 variants from the same strict native-runtime bundle manifest used by the Arch package.

The current release targets are Fedora 43 x86_64 and openSUSE Leap 16.0 x86_64. Both package:

- `vinpst`, `vinpst-daemon`, and `vinpst-gui`;
- the retained Fcitx 5 addon, D-Bus activation metadata, and systemd user unit;
- desktop entry, icons, configuration/VAD data, and zh_CN translations;
- the checksum-pinned sherpa-onnx C API and ONNX Runtime libraries under `/usr/lib/fcitx-vinpst` with private rpaths.

The renderer keeps one shared payload and portable runtime command dependencies while selecting distribution-specific `BuildRequires` and release suffixes. Private sherpa-onnx/ONNX Runtime libraries under `/usr/lib/fcitx-vinpst` are excluded from RPM automatic provides/requires. The addon stays under `/usr/lib64/fcitx5` and the systemd user unit under `/usr/lib/systemd/user` on both release targets.

Run the lightweight deterministic metadata gate with:

```sh
just package-check
```

Run the explicit release gate with:

```sh
just rpm-package-smoke
```

The release gate accepts the checked release source archive, renders two package releases, builds both with `rpmbuild`, validates metadata/hint-only scriptlets/payload/rpaths/linkage and the display-independent GUI check, then uses an unprivileged user namespace to perform `--noscripts` install, upgrade, verification, and removal against an isolated chroot/rpmdb while proving an unsupported future user config is byte-preserved. `.github/workflows/release.yml` runs this gate separately in Fedora 43 and openSUSE Leap 16.0 and publishes only release 1 from each job into the checked GitHub release bundle.

The isolated transaction intentionally does not execute package scriptlets because the synthetic rpm root does not contain a complete desktop session. The checked scriptlets only print lifecycle guidance and do not inspect or mutate user sessions; their metadata and removal argument guard are checked deterministically. Hosted DNF/Zypper repositories, RPM signing, SELinux policy review, and real desktop lifecycle-script execution remain separate follow-on distribution work.

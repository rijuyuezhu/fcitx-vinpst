# Vinpst 0.1.0

Vinpst 0.1.0 is the first public release of the independent `fcitx-vinpst` voice-input project for Fcitx 5.

## Highlights

- Normal dictation with streaming preedit and final Fcitx commits.
- Selected-text command editing with failure-safe replacement and candidate selection.
- Local sherpa-onnx, external command, and OpenAI-compatible remote ASR providers.
- Scene-based text processing through command adapters or OpenAI-compatible LLM providers.
- Rust CLI and Rust/Iced management GUI for configuration, diagnostics, models, providers, adapters, scenes, devices, VAD, and hotwords.
- Tap, Hold, and Both trigger modes; scene and ASR menus; English fallback and Simplified Chinese frontend localization.
- Guarded package upgrade/removal, mode-0600 configuration and recovery files, typed diagnostics, redacted provider failures, checksum-verified resources, and staged publication.

## Release artifacts

The selected 0.1.0 publication matrix is:

- source archive;
- bundled Linux x86_64 tarball derived from the tested Ubuntu payload;
- Arch Linux x86_64 native package;
- Debian 12 amd64 native package;
- Ubuntu 24.04 amd64 native package;
- Fedora 43 x86_64 RPM package;
- openSUSE Leap 16.0 x86_64 RPM package;
- x86_64 Flatpak extension bundle for `org.fcitx.Fcitx5//stable`.

The release also publishes the locked x86_64 Nix closure to the public
`fcitx-vinpst` Cachix binary cache. Nix is a release channel rather than a
GitHub Release file, so it is gated by the same release workflow but is not
listed in `manifest.json` or `SHA256SUMS`.

## Verify downloads

Download the complete release asset set, then verify the checked inventory:

```sh
sha256sum -c SHA256SUMS
```

The release workflow also creates signed GitHub/Sigstore provenance attestations for every asset. Verify an asset against this repository with:

```sh
gh attestation verify ./fcitx-vinpst-0.1.0.tar.gz \
  --repo rijuyuezhu/fcitx-vinpst \
  --signer-workflow rijuyuezhu/fcitx-vinpst/.github/workflows/release.yml
```

## Important support boundaries

- Vinpst uses its own package, executable, addon, D-Bus, service, and XDG identities. It does not replace, import, or migrate another voice-input installation.
- Native packages are the primary 0.1.0 desktop path. The Flatpak extension requires the matching Fcitx Flatpak runtime.
- The management GUI supports keyboard operation, but 0.1.0 does not claim screen-reader or assistive-technology semantic-tree support. CLI and Fcitx configuration fallbacks are documented.
- English and Simplified Chinese are the supported interface locales for 0.1.0.
- OpenAI-compatible provider behavior is covered by deterministic and loopback tests, but compatibility with every hosted vendor, enterprise proxy, credential policy, or outage mode is not guaranteed.
- Selected-text replacement depends on application surrounding-text support or an available primary selection.

## Start here

For a native Arch, Debian, Ubuntu, Fedora, or openSUSE package, initialize the user configuration, start the user daemon, and reload Fcitx:

```sh
vinpst init
systemctl --user enable --now vinpst-daemon.service
fcitx5 -r
vinpst doctor
```

Then install and select an ASR model through `vinpst-gui` or the `vinpst model` commands. Flatpak has a different host-service bootstrap boundary and does not put these commands on the host `PATH`; follow the Flatpak steps on the Installation page instead. See the Quick start, Troubleshooting, and Known limitations pages in the user guide.

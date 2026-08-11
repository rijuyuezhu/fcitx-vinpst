# 0.1.0 release readiness

This page is the release-oriented view of issue #27. It tracks only work that can still block the first public release; broader evidence and post-0.1.0 improvements remain in the migration documents.

## Frozen comparison baseline

The source-parity comparison remains frozen at the reviewed baseline:

```text
6cdcac8b4300ff347ad3157bf61cd09a5302f7a9
```

That baseline remains the 164-file, 28,168-line, 1,559-callable capability inventory used for 0.1.0 parity. Release/build infrastructure was separately refreshed against current upstream in August 2026; newer upstream workflow changes are being adopted without silently changing the frozen product-behavior baseline.

## Completed release foundations

- Practical user-capability mapping is complete for the frozen upstream baseline.
- Normal dictation, selected-text command replacement, Fcitx menus, provider switching, localization, owner recovery, and representative applications have real desktop coverage.
- GUI Control, Resources, LLM, and Hotwords workflows have deterministic tests. Positional focus-order GUI desktop collectors are retired and are not release evidence.
- The user guide covers installation, initialization, dictation, command mode, ASR, scenes, settings, accessibility, CLI usage, troubleshooting, removal, and limitations.
- The selected GitHub artifact matrix is source, bundled Linux x86_64 tarball, Arch x86_64, Debian 12 amd64, Ubuntu 24.04 amd64, Fedora 43 x86_64 RPM, openSUSE Leap 16.0 x86_64 RPM, and Flatpak x86_64; the same release also publishes the locked x86_64 Nix closure through the public `fcitx-vinpst` Cachix channel.
- Selected package jobs build from one checked source archive and pass install, upgrade, rollback or removal transactions appropriate to each format. The Nix release job builds that same extracted source and publishes only after the locked closure smoke succeeds.
- CLI-created configuration and recovery files are private mode 0600 on Unix.
- The release bundle contains a strict machine-readable manifest and `SHA256SUMS`.
- The release workflow checks tag/workspace version consistency, runs the reusable docs/Rust/integration/Nix quality gates, publishes the checked Nix closure to Cachix, and generates GitHub release notes from conventional commits with git-cliff.
- Release assets receive GitHub/Sigstore build-provenance attestations.
- Publication uses a draft release, verifies every remote asset name, size, and GitHub-reported SHA-256 digest, and publishes only after the complete inventory matches.

## Required before creating `v0.1.0`

1. Merge all release-blocking fixes and require the normal `main` checks to pass from a clean checkout.
2. Run `.github/workflows/release.yml` with `workflow_dispatch` on `main` after confirming that `main` points to the exact reviewed commit.
3. Download `checked-release-bundle`, run the manifest verifier and `sha256sum -c SHA256SUMS`, and verify at least the source archive attestation with `gh attestation verify`.
4. Confirm the workflow's clean GitHub-hosted package jobs completed real install/upgrade/removal transactions for the selected native packages and Flatpak extension, and that the Nix closure was published successfully to Cachix.
5. Install the appropriate release-candidate native package in an unrelated clean user environment and run the artifact-installed smoke:

   ```sh
   scripts/release/run-release-bundle-install-smoke.sh \
     --bundle-dir ./fcitx-vinpst-0.1.0-release \
     --image ubuntu:24.04

   vinpst --version
   vinpst-gui --check --offline
   vinpst init --dry-run --json
   vinpst doctor
   vinpst daemon status
   ```

6. Confirm a final native desktop check for normal dictation, command replacement, restart/reload, provider or model switching, and package removal with user state preserved.
7. Review the git-cliff generated release notes, the selected support matrix, known limitations, and the publication/rollback procedure. `RELEASE_NOTES.md` remains the curated first-release guide rather than a per-version publication input.

## Post-publication check

After the release is public, download the assets again from the GitHub Release rather than reusing workflow artifacts. Re-run checksum, manifest, provenance, installation, diagnostics, normal dictation, command replacement, and removal checks. Do not move or reuse `v0.1.0` if a defect is found; correct it in a new version.

## Explicitly non-blocking for 0.1.0

- Semantic-identity GUI automation beyond the deterministic management coverage.
- Screen-reader semantic-tree support.
- Additional application, microphone, Bluetooth, USB, and long-duration soak breadth beyond the representative evidence already retained.
- PAC, NTLM/Kerberos, hosted-provider-specific operations, and enterprise certificate deployment.
- RPM repository signing plus DNF/Zypper channel publication, Flathub publication, additional architectures, and long-term Nix cache retention policy.
- Migration from or package replacement of another voice-input project.

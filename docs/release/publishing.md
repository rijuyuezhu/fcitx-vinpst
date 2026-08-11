# Publishing and rollback

This procedure publishes a checked Vinpst release without rebuilding or replacing assets after they become public.

## Preconditions

- The release commit is on protected `main` and all required checks are successful.
- `Cargo.toml` package versions, package templates, and the intended tag all use the same version.
- The upstream comparison baseline and user-capability audit are current.
- A non-publishing `workflow_dispatch` run of `.github/workflows/release.yml` succeeds on the exact release commit.
- The downloaded `checked-release-bundle` passes manifest, checksum, provenance, package-install, diagnostics, and required desktop checks.
- The repository secret `CACHIX_AUTH_TOKEN` is configured for the public `fcitx-vinpst` cache, and `cliff.toml` is present for generated GitHub release notes.

## Rehearse without publishing

Confirm that protected `main` points to the reviewed commit, then run the **Release** workflow manually with `main` as the dispatch ref. The dispatch runs every build, native package transaction, the Flatpak transaction, the locked Nix closure smoke plus Cachix publication, bundle assembly, installation, and provenance step, but the GitHub `publish` job is skipped because the ref is not a version tag.

With the GitHub CLI:

```sh
gh workflow run release.yml --ref main
gh run list --workflow release.yml --branch main --limit 5
```

After the run succeeds, download the resulting `checked-release-bundle` and verify it:

```sh
gh run download <run-id> \
  --name checked-release-bundle \
  --dir ./fcitx-vinpst-0.1.0-release
scripts/release/release_manifest.py verify ./fcitx-vinpst-0.1.0-release
cd ./fcitx-vinpst-0.1.0-release
sha256sum -c SHA256SUMS
```

Verify representative assets against the release workflow identity:

```sh
gh attestation verify ./fcitx-vinpst-0.1.0.tar.gz \
  --repo rijuyuezhu/fcitx-vinpst \
  --signer-workflow rijuyuezhu/fcitx-vinpst/.github/workflows/release.yml
```

The rehearsal is the release candidate. Do not create the tag when any selected job, package transaction, Nix/Cachix publication, manifest check, attestation, installation check, or required desktop check is incomplete.

## Publish

Create and push an annotated tag for the reviewed `main` commit:

```sh
git switch main
git pull --ff-only origin main
git tag -a v0.1.0 -m "Vinpst 0.1.0"
git push origin v0.1.0
```

The tag workflow:

1. rejects a tag that does not match the workspace version;
2. runs the reusable quality gates;
3. builds the bundled Linux tarball, Arch, Debian 12, Ubuntu 24.04, Fedora 43 RPM, openSUSE Leap 16.0 RPM, and Flatpak release artifacts from one checked source archive or its already transaction-tested Ubuntu payload;
4. builds that same extracted source through the locked Nix package smoke and publishes the resulting closure to Cachix;
5. verifies package transactions and assembles the checked GitHub release bundle;
6. creates signed GitHub/Sigstore provenance attestations;
7. generates the release notes from conventional commits with git-cliff;
8. creates or reuses only a draft GitHub Release;
9. uploads the exact checked bundle and compares every remote asset name, size, and GitHub-reported SHA-256 digest with the local bundle;
10. publishes the draft only after the inventory matches.

A rerun may replace assets only while the release is still a draft. The workflow refuses to mutate an already-public release.

Nix is intentionally published as a binary-cache channel rather than as a `.nix` file in the GitHub Release. The GitHub bundle therefore records the source archive, bundled Linux tarball, native packages, and Flatpak bundle, while the release DAG separately requires the Cachix closure publication to succeed.

## Post-publication verification

Download a fresh copy from the public GitHub Release and repeat:

```sh
mkdir -p ./fcitx-vinpst-0.1.0-public
gh release download v0.1.0 \
  --repo rijuyuezhu/fcitx-vinpst \
  --dir ./fcitx-vinpst-0.1.0-public
cd ./fcitx-vinpst-0.1.0-public
../scripts/release/release_manifest.py verify .
sha256sum -c SHA256SUMS
gh attestation verify ./fcitx-vinpst-0.1.0.tar.gz \
  --repo rijuyuezhu/fcitx-vinpst \
  --signer-workflow rijuyuezhu/fcitx-vinpst/.github/workflows/release.yml
```

Install the appropriate native package on the final clean test environment and check:

```sh
vinpst --version
vinpst-gui --check --offline
vinpst doctor
vinpst daemon status
```

Complete normal dictation, selected-text command replacement, restart/reload, one model or provider switch, and package removal while confirming that user configuration and managed data remain present.

## Rollback and incident handling

### Before the draft is published

Fix the release commit and use a new tag/version when the release contents or code changed. A failed draft can be deleted:

```sh
gh release delete v0.1.0 --repo rijuyuezhu/fcitx-vinpst --yes
```

Delete the remote tag only when it has never represented a public release and the team has explicitly abandoned that candidate:

```sh
git push origin :refs/tags/v0.1.0
```

Do not move a tag to a different commit.

### After publication

Do not clobber assets, move the tag, or reuse the version. Mark the release as a prerelease and add a visible warning when immediate mitigation is necessary, then publish a corrected patch release:

```sh
gh release edit v0.1.0 \
  --repo rijuyuezhu/fcitx-vinpst \
  --prerelease
```

For a compromised or unsafe artifact, remove the affected public assets or release only as an emergency containment measure, record what was removed and why, and publish a new version with fresh checksums and attestations. Consumers should be directed to the fixed version rather than to a rewritten `v0.1.0`.

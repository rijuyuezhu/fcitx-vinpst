# Flatpak extension package

The checked Flatpak target is an extension of `org.fcitx.Fcitx5`, matching the legacy package identity and install root:

- extension ID: `org.fcitx.Fcitx5.Addon.Vinput`;
- branch/runtime version: `stable`;
- SDK: `org.kde.Sdk//6.10` plus the Rust stable and LLVM 20 SDK extensions;
- install prefix: `/app/addons/Vinput`.

`manifest-base.json` is the stable metadata source of truth. `scripts/release/render-flatpak-manifest.py` combines it with:

- the exact native runtime selected from `packaging/arch/runtime-bundles.json`;
- a local source directory or checksum-verified source archive;
- all crates.io packages from `Cargo.lock`, rendered into `cargo-sources.json` with their lock-file checksums.

The LLVM extension supplies `libclang` at `/usr/lib/sdk/llvm20/lib` and its C resource headers under `/usr/lib/sdk/llvm20/lib/clang/20/include` for PipeWire bindgen generation; those paths are fixed through `LIBCLANG_PATH` and `BINDGEN_EXTRA_CLANG_ARGS`. Cargo writes `$ORIGIN/../lib` into the packaged executables at link time, while the checked Sherpa and ONNX Runtime libraries already carry `$ORIGIN`; the Flatpak SDK therefore does not need a mutable ELF post-processing tool. The Flatpak build is offline after source acquisition. The renderer rejects non-crates.io Cargo sources, missing or malformed checksums, unsafe release tokens, unknown runtime bundles, source digest mismatches, and non-x86_64 runtime selection. Regenerate the checked Cargo source list after any lock-file change:

```sh
scripts/release/generate-flatpak-cargo-sources.py \
  Cargo.lock \
  --output packaging/flatpak/cargo-sources.json
```

Run the lightweight metadata gate with:

```sh
scripts/release/check-flatpak-manifest.sh
```

The complete release gate first materializes all checked network archives outside Flatpak Builder. Sherpa and third-party license downloads use bounded connection, total-time, and low-speed limits. Cargo archives are copied from a checksum-valid host Cargo cache when available; missing crates are downloaded concurrently with the same bounded transport. Every file must match the shared runtime manifest or Cargo source list before the renderer replaces network sources with local relative paths. This prevents Flatpak Builder's internal serial fetcher from hanging indefinitely while preserving the exact source digests. The gate then builds a minimal Debian 12 Flatpak Builder container and uses the same privileged builder boundary and KDE 6.10/Fcitx SDK declared by the legacy release workflow. Because the locked remote is not enumerable, the gate installs the exact `org.kde.Platform//6.10` dependency before asking Flatpak Builder to install the Fcitx runtime plus Rust and LLVM SDK extensions:

```sh
scripts/release/run-flatpak-package-smoke.sh
```

That gate compiles the product once, installs revision 1, runs the packaged CLI/daemon/GUI self-check through the Fcitx Flatpak application, and creates the publication bundle from that exact commit. It then changes only a checked package-revision marker in the exported build tree, creates a synthetic revision-2 OSTree commit, performs a real update, and verifies that the commit and marker changed. Finally it removes the extension, installs the revision-1 bundle, proves the synthetic update did not enter the publication artifact, and removes it again. A rendered manifest without that build/install/update/bundle/remove transaction is not Flatpak release evidence.

The outer gate takes an exclusive lock before reading the container id or cleaning its isolated HOME, Flatpak source cache, OSTree repository, and build tree, so a second invocation fails without killing the active transaction. A checksum-valid host-side runtime source cache may be reused across retries; invalid or partial files are discarded before use. `VINPUT_FLATPAK_REUSE_HOME=1` is reserved for direct inner-gate retries while developing the recipe; it retains only the isolated Flatpak runtime/source cache and is not used by the release workflow.

Optional transport overrides do not change the locked refs or checksums:

- `VINPUT_FLATPAK_BUILDER_IMAGE` selects a prebuilt builder image, including the legacy `ghcr.io/flathub-infra/flatpak-github-actions:kde-6.10` image;
- `VINPUT_FLATPAK_APT_MIRROR` and `VINPUT_FLATPAK_APT_SECURITY_MIRROR` select Debian package mirrors for the local builder image;
- `VINPUT_FLATPAK_REMOTE_URL` selects a Flathub OSTree mirror and is asserted after remote registration;
- `VINPUT_FLATPAK_RETRY_ATTEMPTS` controls bounded dependency/source transaction retries;
- `VINPUT_FLATPAK_CARGO_DOWNLOAD_JOBS` controls concurrent checked Cargo archive materialization;
- `VINPUT_FLATPAK_CARGO_DOWNLOAD_ATTEMPTS` controls per-archive Cargo download attempts;
- `VINPUT_FLATPAK_DEPENDENCY_TIMEOUT_SECONDS` bounds each exact Flatpak runtime/SDK installation attempt;
- `VINPUT_FLATPAK_BUILD_TIMEOUT_SECONDS` bounds each complete Flatpak Builder attempt;
- `VINPUT_FLATPAK_TRANSACTION_TIMEOUT_SECONDS` bounds each extension install, update, bundle reinstall, and removal attempt.

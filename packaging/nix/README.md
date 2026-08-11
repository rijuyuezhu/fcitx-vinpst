# Nix flake baseline

The repository root flake builds the Rust CLI, D-Bus daemon, Iced GUI, and
retained Fcitx 5 addon against a source-built sherpa-onnx runtime. The flake
uses the same sherpa-onnx input family as the legacy project and follows the
repository's pinned nixpkgs input.

```bash
nix build .#fcitx-vinpst
nix run .#cli -- --version
nix run . -- --check --offline
```

The Nix result contains the binaries, addon, D-Bus activation file, systemd
user unit, translations, desktop entry, icons, default config, VAD model, and
project/asset licenses. Package-manager upgrade and removal scriptlets are not
part of the Nix result: Nix store paths are immutable and profile generations
provide activation and rollback semantics.

The CI Nix job evaluates `flake.lock`, runs `nix flake check`, builds the
package, executes the display-independent GUI self-check, and validates the
installed closure layout. A Nix release is normally distributed through a
binary cache rather than attached to GitHub as a `.nix` binary file. The flake
and lock file remain in the source archive.

The Release workflow materializes the same checked source archive used by the
native packages, runs the full locked Nix package smoke from that tree, and
publishes the resulting closure to the public `fcitx-vinpst` Cachix cache
before the checked GitHub release bundle may proceed. `.github/workflows/nix-cache.yml`
remains as a manual cache-refresh escape hatch and runs the same package smoke.
Both paths require only the repository secret `CACHIX_AUTH_TOKEN`; use a
per-cache write token rather than a personal account token. Public read access
does not require that secret. Add the cache substituter and trusted public key
to `flake.nix` only after the cache has been created and its Cachix-managed
public key is known.

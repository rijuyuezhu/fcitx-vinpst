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
and lock file remain in the source archive, while cache publication can be
added once project cache credentials and retention policy are defined.

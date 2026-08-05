{
  description = "Rust voice-input addon, daemon, CLI, and GUI for Fcitx 5";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    sherpa-onnx = {
      url = "github:xifan2333/sherpa-onnx-flake";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      sherpa-onnx,
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      version = (builtins.fromTOML (builtins.readFile ./crates/vinpst-cli/Cargo.toml)).package.version;
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
          inherit (pkgs) lib;
          sherpa = sherpa-onnx.packages.${system}.sherpa-onnx;
          sherpaRuntime = pkgs.symlinkJoin {
            name = "vinpst-sherpa-onnx-runtime";
            paths = [
              sherpa
              pkgs.onnxruntime
            ];
          };
          source = builtins.path {
            name = "fcitx-vinpst-source";
            path = ./.;
            filter = path: type:
              let
                name = builtins.baseNameOf path;
              in
              !(builtins.elem name [
                ".git"
                ".cache"
                ".ruff_cache"
                "__pycache__"
                "dist"
                "target"
              ]);
          };
          package = pkgs.rustPlatform.buildRustPackage {
            pname = "fcitx-vinpst";
            inherit version;
            src = source;
            cargoLock.lockFile = ./Cargo.lock;
            strictDeps = true;

            nativeBuildInputs = with pkgs; [
              autoPatchelfHook
              clang
              cmake
              gettext
              libclang
              makeWrapper
              ninja
              pkg-config
            ];

            buildInputs = with pkgs; [
              bzip2
              fcitx5
              fontconfig
              libx11
              libxkbcommon
              onnxruntime
              pipewire
              sherpa
              systemd
              wayland
            ];

            SHERPA_ONNX_LIB_DIR = "${sherpaRuntime}/lib";
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";

            buildPhase = ''
              runHook preBuild
              cargo build --offline --frozen --release \
                -p vinpst-cli --features pipewire-backend,sherpa-onnx-backend \
                -p vinpst-daemon --features pipewire-backend,sherpa-onnx-backend \
                -p vinpst-gui
              cmake -S cpp/fcitx5-addon -B build/fcitx-addon -G Ninja \
                -DBUILD_TESTING=OFF \
                -DCMAKE_BUILD_TYPE=Release \
                -DCMAKE_INSTALL_PREFIX="$out" \
                -DCMAKE_INSTALL_LIBDIR=lib \
                -DVINPST_DAEMON_EXECUTABLE="$out/bin/vinpst-daemon" \
                -DVINPST_DAEMON_ARGS='--dbus --configured-backends --audio-backend pipewire' \
                -DVINPST_FCITX_BRIDGE_ENABLE_TESTS=OFF \
                -DVINPST_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON \
                -DVINPST_FCITX_MODULE_INSTALL_DIR=lib/fcitx5 \
                -DVINPST_FCITX_ADDON_INSTALL_DIR=share/fcitx5/addon \
                -DVINPST_FCITX_RUNTIME_BUILD_LOCALEDIR= \
                -DVINPST_SYSTEMD_USER_UNIT_DIR=lib/systemd/user
              cmake --build build/fcitx-addon \
                --target fcitx5_vinpst_addon --parallel "$NIX_BUILD_CORES"
              runHook postBuild
            '';

            installPhase = ''
              runHook preInstall
              install -Dm755 target/release/vinpst "$out/bin/vinpst"
              install -Dm755 target/release/vinpst-daemon "$out/bin/vinpst-daemon"
              install -Dm755 target/release/vinpst-gui "$out/bin/vinpst-gui"
              cmake --install build/fcitx-addon
              install -Dm644 data/vinpst-gui.desktop \
                "$out/share/applications/vinpst-gui.desktop"
              for size in 16 22 24 32 48 64 128 256 512; do
                install -Dm644 \
                  "data/icons/hicolor/''${size}x''${size}/apps/vinpst-gui.png" \
                  "$out/share/icons/hicolor/''${size}x''${size}/apps/vinpst-gui.png"
              done
              install -Dm644 data/default-config.json \
                "$out/share/fcitx-vinpst/default-config.json"
              install -Dm644 data/vad/silero_vad.onnx \
                "$out/share/fcitx-vinpst/vad/silero_vad.onnx"
              install -Dm644 LICENSE \
                "$out/share/licenses/fcitx-vinpst/LICENSE"
              install -Dm644 data/vad/LICENSE \
                "$out/share/licenses/fcitx-vinpst/silero-vad-LICENSE"
              runHook postInstall
            '';

            postFixup = ''
              wrapProgram "$out/bin/vinpst" \
                --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath [ sherpaRuntime ]}"
              wrapProgram "$out/bin/vinpst-daemon" \
                --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath [ sherpaRuntime ]}"
            '';

            doCheck = false;

            meta = {
              description = "Rust voice-input addon and management application for Fcitx 5";
              homepage = "https://github.com/rijuyuezhu/fcitx-vinpst";
              license = lib.licenses.gpl3Plus;
              mainProgram = "vinpst-gui";
              platforms = systems;
            };
          };
        in
        {
          fcitx-vinpst = package;
          default = package;
        }
      );

      apps = forAllSystems (
        system:
        let
          package = self.packages.${system}.fcitx-vinpst;
        in
        {
          default = {
            type = "app";
            program = "${package}/bin/vinpst-gui";
          };
          cli = {
            type = "app";
            program = "${package}/bin/vinpst";
          };
          daemon = {
            type = "app";
            program = "${package}/bin/vinpst-daemon";
          };
        }
      );

      checks = forAllSystems (system: {
        package = self.packages.${system}.fcitx-vinpst;
      });

      devShells = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.mkShell {
            inputsFrom = [ self.packages.${system}.fcitx-vinpst ];
            packages = with pkgs; [
              cargo
              clippy
              rustc
              rustfmt
            ];
          };
        }
      );
    };
}

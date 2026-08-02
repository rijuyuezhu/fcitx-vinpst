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
      version = (builtins.fromTOML (builtins.readFile ./crates/vinput-cli/Cargo.toml)).package.version;
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
          inherit (pkgs) lib;
          sherpa = sherpa-onnx.packages.${system}.sherpa-onnx;
          sherpaRuntime = pkgs.symlinkJoin {
            name = "vinput-sherpa-onnx-runtime";
            paths = [
              sherpa
              pkgs.onnxruntime
            ];
          };
          source = builtins.path {
            name = "fcitx-vinput-rs-source";
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
            pname = "fcitx-vinput-rs";
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
                -p vinput-cli --features pipewire-backend,sherpa-onnx-backend \
                -p vinput-daemon --features pipewire-backend,sherpa-onnx-backend \
                -p vinput-gui
              cmake -S cpp/fcitx5-addon -B build/fcitx-addon -G Ninja \
                -DBUILD_TESTING=OFF \
                -DCMAKE_BUILD_TYPE=Release \
                -DCMAKE_INSTALL_PREFIX="$out" \
                -DCMAKE_INSTALL_LIBDIR=lib \
                -DVINPUT_DAEMON_EXECUTABLE="$out/bin/vinput-daemon" \
                -DVINPUT_DAEMON_ARGS='--dbus --configured-backends --audio-backend pipewire' \
                -DVINPUT_FCITX_BRIDGE_ENABLE_TESTS=OFF \
                -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON \
                -DVINPUT_FCITX_MODULE_INSTALL_DIR=lib/fcitx5 \
                -DVINPUT_FCITX_ADDON_INSTALL_DIR=share/fcitx5/addon \
                -DVINPUT_FCITX_RUNTIME_BUILD_LOCALEDIR= \
                -DVINPUT_SYSTEMD_USER_UNIT_DIR=lib/systemd/user
              cmake --build build/fcitx-addon \
                --target fcitx5_vinput_addon --parallel "$NIX_BUILD_CORES"
              runHook postBuild
            '';

            installPhase = ''
              runHook preInstall
              install -Dm755 target/release/vinput "$out/bin/vinput"
              install -Dm755 target/release/vinput-daemon "$out/bin/vinput-daemon"
              install -Dm755 target/release/vinput-gui "$out/bin/vinput-gui"
              cmake --install build/fcitx-addon
              install -Dm644 data/vinput-gui.desktop \
                "$out/share/applications/vinput-gui.desktop"
              for size in 16 22 24 32 48 64 128 256 512; do
                install -Dm644 \
                  "data/icons/hicolor/''${size}x''${size}/apps/vinput-gui.png" \
                  "$out/share/icons/hicolor/''${size}x''${size}/apps/vinput-gui.png"
              done
              install -Dm644 data/default-config.json \
                "$out/share/fcitx-vinput/default-config.json"
              install -Dm644 data/vad/silero_vad.onnx \
                "$out/share/fcitx-vinput/vad/silero_vad.onnx"
              install -Dm644 LICENSE \
                "$out/share/licenses/fcitx-vinput-rs/LICENSE"
              install -Dm644 data/vad/LICENSE \
                "$out/share/licenses/fcitx-vinput-rs/silero-vad-LICENSE"
              runHook postInstall
            '';

            postFixup = ''
              wrapProgram "$out/bin/vinput" \
                --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath [ sherpaRuntime ]}"
              wrapProgram "$out/bin/vinput-daemon" \
                --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath [ sherpaRuntime ]}"
            '';

            doCheck = false;

            meta = {
              description = "Rust voice-input addon and management application for Fcitx 5";
              homepage = "https://github.com/rijuyuezhu/fcitx-vinput-rs";
              license = lib.licenses.gpl3Plus;
              mainProgram = "vinput-gui";
              platforms = systems;
            };
          };
        in
        {
          fcitx-vinput-rs = package;
          default = package;
        }
      );

      apps = forAllSystems (
        system:
        let
          package = self.packages.${system}.fcitx-vinput-rs;
        in
        {
          default = {
            type = "app";
            program = "${package}/bin/vinput-gui";
          };
          cli = {
            type = "app";
            program = "${package}/bin/vinput";
          };
          daemon = {
            type = "app";
            program = "${package}/bin/vinput-daemon";
          };
        }
      );

      checks = forAllSystems (system: {
        package = self.packages.${system}.fcitx-vinput-rs;
      });

      devShells = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.mkShell {
            inputsFrom = [ self.packages.${system}.fcitx-vinput-rs ];
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

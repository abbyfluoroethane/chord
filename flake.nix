{
  description = "Chord: an XMPP client with circles, channels, and DMs";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
              overlays = [ rust-overlay.overlays.default ];
            }
          )
        );
    in
    {
      packages = forAllSystems (pkgs: rec {
        default = chord-cli;

        # The command-line client. chord-core and chord-ffi build with it.
        # chord-desktop is not packaged: it also needs the npm front end. Use the
        # dev shell and `npx tauri build` for the desktop app.
        chord-cli = pkgs.rustPlatform.buildRustPackage {
          pname = "chord-cli";
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version;

          src = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./chord-core
              ./chord-cli
              ./chord-ffi
              # A workspace member: cargo reads its manifest even for `-p chord-cli`.
              ./chord-desktop/src-tauri
            ];
          };

          cargoLock.lockFile = ./Cargo.lock;

          # rusqlite builds its own SQLite ("bundled"), so it needs a C compiler only.
          nativeBuildInputs = [ pkgs.pkg-config ];

          cargoBuildFlags = [
            "-p"
            "chord-cli"
          ];
          # The integration tests are #[ignore] and need a live server, so `cargo test`
          # here runs the unit tests only.
          cargoTestFlags = [
            "-p"
            "chord-cli"
            "-p"
            "chord-core"
          ];

          meta = {
            description = "Command-line XMPP client built on chord-core";
            homepage = "https://bigaouette.com/chord-site/";
            license = pkgs.lib.licenses.mit;
            mainProgram = "chord-cli";
          };
        };
      });

      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          inherit (self.packages.${pkgs.stdenv.hostPlatform.system}.chord-cli) meta;
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.chord-cli}/bin/chord-cli";
        };
      });

      devShells = forAllSystems (
        pkgs:
        let
          # Edition 2024 needs Rust 1.85 or later. rust-src helps the editor, and the
          # wasm32 target builds chord-core as CI does.
          toolchain = pkgs.rust-bin.stable.latest.default.override {
            extensions = [
              "rust-src"
              "rust-analyzer"
              "clippy"
              "rustfmt"
            ];
            targets = [ "wasm32-unknown-unknown" ];
          };

          # The Tauri libraries on Linux. Darwin uses the system WebKit.
          tauriLibraries = with pkgs; [
            webkitgtk_4_1
            gtk3
            glib
            glib-networking
            cairo
            pango
            gdk-pixbuf
            atkmm
            libsoup_3
            librsvg
            libayatana-appindicator
            openssl
            dbus
          ];
        in
        {
          default = pkgs.mkShell {
            packages = [
              toolchain
            ]
            ++ (with pkgs; [
              nodejs_22
              pkg-config
              # dev/ffi-bindgen-check.sh downloads kotlinc itself. It needs a JDK.
              jdk21
              # dev/prosody/setup.sh starts the test server and makes its certificates.
              docker-compose
              mkcert
              # A small SQLite shell for the stores in chord-core.
              sqlite
            ])
            ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux (tauriLibraries ++ [ pkgs.patchelf ]);

            env = {
              RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";
              # The wasm32 build compiles SQLite C code. cc-rs reads the CC that the
              # shell sets, which is the host gcc, so name a wasm compiler per target.
              # It is the unwrapped clang: the wrapper adds hardening flags that
              # wasm32 does not accept.
              CC_wasm32_unknown_unknown = "${pkgs.llvmPackages.clang-unwrapped}/bin/clang";
              AR_wasm32_unknown_unknown = "${pkgs.llvmPackages.bintools-unwrapped}/bin/llvm-ar";
            }
            // pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
              # WebKitGTK falls back to software rendering on many Nix setups.
              # Unset this if your driver handles DMA-BUF.
              WEBKIT_DISABLE_DMABUF_RENDERER = "1";
              LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath tauriLibraries;
            };

            shellHook = pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
              # GTK reads its icons and schemas from these paths.
              export XDG_DATA_DIRS="${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}:$XDG_DATA_DIRS"
              export GIO_MODULE_DIR="${pkgs.glib-networking}/lib/gio/modules"
            '';
          };
        }
      );

      formatter = forAllSystems (pkgs: pkgs.nixfmt-tree);
    };
}

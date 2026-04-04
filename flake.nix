{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-parts.url = "github:hercules-ci/flake-parts";
  };

  outputs =
    inputs@{
      nixpkgs,
      rust-overlay,
      flake-parts,
      ...
    }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      perSystem =
        { system, lib, ... }:
        let
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs { inherit system overlays; };
          inherit (pkgs.stdenv.hostPlatform) isLinux;
        in
        {
          formatter = pkgs.nixfmt-rfc-style;
          devShells.default = pkgs.mkShell rec {
            packages = [
              # pkgs.rust-bin.beta.latest.default
              pkgs.rust-analyzer
              pkgs.ffmpeg_6

              (pkgs.rust-bin.beta.latest.default.override {
                targets = [ "wasm32-unknown-unknown" ];
              })
            ];

            # For EGUI
            buildInputs = [
              pkgs.trunk

              pkgs.openssl
              pkgs.pkg-config
            ]
            ++ (
              if isLinux then
                [
                  pkgs.libxkbcommon
                  pkgs.libGL
                  pkgs.fontconfig
                  pkgs.wayland
                  pkgs.xorg.libXcursor
                  pkgs.xorg.libXrandr
                  pkgs.xorg.libXi
                  pkgs.xorg.libX11
                ]
              else
                [ ]
            );

            LD_LIBRARY_PATH = "${lib.makeLibraryPath buildInputs}";
            JAMON_MAIN_FONT_PATH = "${pkgs.iosevka}/share/fonts/truetype/Iosevka-Regular.ttf";
          };
        };
    };
}

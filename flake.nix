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
        { system, ... }:
        let
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs { inherit system overlays; };
        in
        {
          formatter = pkgs.nixfmt-rfc-style;
          devShells.default = pkgs.mkShell {
            packages = [
              pkgs.rust-bin.beta.latest.default
              pkgs.rust-analyzer
              pkgs.ffmpeg_6
            ];
          };
        };
    };
}

# TODO: Maybe you need this for egui
# # Rust
# pkgs.rust-bin.beta.latest.default
# pkgs.trunk
# # development
# pkgs.jujutsu
# pkgs.rust-analyzer
# # misc. libraries
# pkgs.openssl
# pkgs.pkg-config
# # GUI libs
# pkgs.libxkbcommon
# pkgs.libGL
# pkgs.fontconfig
# # wayland libraries
# pkgs.wayland
# # x11 libraries
# pkgs.xorg.libXcursor
# pkgs.xorg.libXrandr
# pkgs.xorg.libXi
# pkgs.xorg.libX11
# LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath buildInputs}";

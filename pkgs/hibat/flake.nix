{
  description = "hibat - Battery history tracker and visualizer";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default;

        runtimeLibs = with pkgs; [
          wayland
          libxkbcommon
          libGL
          libx11
          libxcursor
          libxrandr
          libxi
          vulkan-loader
        ];
      in
      {
        packages = {
          default = pkgs.callPackage ./default.nix { };
        };

        checks = {
          inherit (self.packages.${system}) default;
        }
        // import ./nix/checks.nix { inherit pkgs; };

        devShells.default = pkgs.mkShell {
          RUSTFLAGS = "-C link-arg=-fuse-ld=mold";
          RUSTC_WRAPPER = "${pkgs.sccache}/bin/sccache";

          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibs;

          buildInputs = [
            rustToolchain
          ]
          ++ (with pkgs; [
            openssl
            pkg-config

            mold
            sccache
          ])
          ++ runtimeLibs;
        };
      }
    );
}

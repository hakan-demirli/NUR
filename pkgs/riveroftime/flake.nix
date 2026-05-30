{
  description = "riveroftime - Wayland layer countdown timer";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs =
    {
      nixpkgs,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          config.allowUnfree = true;
        };
        riveroftime = pkgs.callPackage ./package.nix { };

      in
      {
        packages.riveroftime = riveroftime;
        packages.default = riveroftime;
        devShells.default = pkgs.mkShell {
          packages = [
            pkgs.gcc
            pkgs.pkg-config
            pkgs.wayland
            pkgs.libxkbcommon
            pkgs.fontconfig
            pkgs.rustc
            pkgs.cargo
            pkgs.rustfmt
            pkgs.clippy
          ];
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
            pkgs.wayland
            pkgs.libxkbcommon
            pkgs.fontconfig
          ];
        };
        checks = import ./nix/checks.nix { inherit pkgs; };
      }
    );
}

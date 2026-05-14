{
  description = "dap template";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
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
        dap = pkgs.callPackage ./package.nix { };

      in
      {
        packages.dap = dap;
        packages.default = dap;
        devShells.default = pkgs.mkShell {
          packages = [
            pkgs.gcc
            pkgs.gdb
            pkgs.cmake
            pkgs.rustc
            pkgs.cargo
            pkgs.rustfmt
          ];
        };
        checks = import ./nix/checks.nix { inherit pkgs; };
      }
    );
}

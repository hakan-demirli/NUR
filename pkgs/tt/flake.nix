{
  description = "Inline terminal UI for task tracking";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs =
    { nixpkgs, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
        "x86_64-darwin"
      ];
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
              config.allowUnfree = true;
            }
          )
        );
    in
    {
      packages = forAllSystems (pkgs: {
        default = pkgs.callPackage ./nix/package.nix { };
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ (pkgs.callPackage ./nix/package.nix { }) ];
          packages = with pkgs.python3Packages; [
            mypy
            pytest
          ];
        };
      });

      checks = forAllSystems (pkgs: import ./nix/checks.nix { inherit pkgs; });

      formatter = forAllSystems (pkgs: import ./nix/formatters.nix { inherit pkgs; });
    };
}

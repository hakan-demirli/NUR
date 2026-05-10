{
  description = "A collection of small applications";

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
    let
      lib = nixpkgs.lib;
      findPackageDirs = fpath: lib.filterAttrs (name: type: type == "directory") (builtins.readDir fpath);
      allPackageNames = lib.attrNames (findPackageDirs ./pkgs);

      perSystem =
        flake-utils.lib.eachSystem
          (lib.filter (s: !(lib.hasInfix "darwin" s)) flake-utils.lib.defaultSystems)
          (
            system:
            let
              pkgs = import nixpkgs {
                inherit system;
                config.allowUnfree = true;
              };

              allMyPackages = lib.genAttrs allPackageNames (
                name: pkgs.callPackage (./pkgs + "/${name}/default.nix") { }
              );

              validPackages = lib.filterAttrs (name: pkg: !(pkg.meta.broken or false)) allMyPackages;
            in
            {
              packages = validPackages // {
                default = pkgs.buildEnv {
                  name = "small-apps-bundle-${system}";
                  paths = lib.attrValues validPackages;
                  meta.description = "Build environment containing all enabled small-apps";
                };
              };
            }
          );

    in
    perSystem
    // {
      overlays.default =
        final: _:
        lib.genAttrs allPackageNames (name: final.callPackage (./pkgs + "/${name}/default.nix") { });
    };
}

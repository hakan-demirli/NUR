{
  description = "Orbit watchface for Pebble Round 2 (gabbro)";

  inputs = {
    dotfiles.url = "github:hakan-demirli/dotfiles";
    nixpkgs.follows = "dotfiles/nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      dotfiles,
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

        pebbleosSource = dotfiles.packages.${system}.pebble-round-2-pebbleos-source;
        pebbleosSdk = dotfiles.packages.${system}.pebble-round-2-pebbleosSdk;

        pythonEnv = import "${dotfiles}/modules/devices/pebble-round-2/nix/python-env.nix" {
          inherit pkgs;
        };

        appSdk = pkgs.callPackage ./nix/app-sdk.nix {
          inherit pebbleosSource pebbleosSdk pythonEnv;
        };

        orbit = pkgs.callPackage ./nix/watchface.nix {
          inherit appSdk pebbleosSdk pythonEnv;
          src = ./.;
        };
      in
      {
        packages = {
          inherit appSdk orbit;
          default = orbit;
        };

        devShells.default = pkgs.mkShell {
          packages = [
            pythonEnv
            pebbleosSdk
          ];
          shellHook = ''
            export PEBBLE_APP_SDK="${appSdk}"
            echo "build: \$PEBBLE_APP_SDK/waf configure && \$PEBBLE_APP_SDK/waf build"
          '';
        };
      }
    );
}

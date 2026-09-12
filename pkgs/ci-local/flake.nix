{
  description = "ci-local: local nix-based CI runner daemon";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        packages.default = pkgs.callPackage ./default.nix { };

        checks = {
          inherit (self.packages.${system}) default;
        }
        // import ./nix/checks.nix { inherit pkgs; };

        devShells.default = pkgs.mkShell {
          RUSTFLAGS = "-C link-arg=-fuse-ld=mold";
          RUSTC_WRAPPER = "${pkgs.sccache}/bin/sccache";
          buildInputs = with pkgs; [
            openssl
            pkg-config
            rustc
            cargo
            clippy
            mold
            sccache
          ];
        };
      }
    );
}

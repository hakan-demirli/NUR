{
  description = "A rust devShell example";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
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
        packages = {
          default = pkgs.callPackage ./default.nix { };
          static = pkgs.pkgsStatic.callPackage ./default.nix { };
        };

        checks = {
          inherit (self.packages.${system}) default;
        }
        // import ./nix/checks.nix { inherit pkgs; };

        formatter = import ./nix/formatters.nix { inherit pkgs; };

        devShells.default = pkgs.mkShell {
          # RUSTFLAGS = ["-Ctarget-feature=+crt-static"];
          # cargo build --release --target x86_64-unknown-linux-gnu
          # RUSTC_WRAPPER = "sccache";
          RUSTFLAGS = "-C link-arg=-fuse-ld=mold";
          RUSTC_WRAPPER = "${pkgs.sccache}/bin/sccache";
          buildInputs = with pkgs; [
            # glibc
            # glibc.static
            # pkgsStatic.openssl
            openssl
            pkg-config
            # pkgsStatic.pkg-config
            # pkgsStatic.cargo
            rustc
            cargo
            clippy
            # pkgsStatic.rust-bin.beta.latest.default
            #
            # (pkgs.vscode-with-extensions.override {
            #   vscodeExtensions = [
            #     pkgs.vscode-extensions.vadimcn.vscode-lldb
            #     pkgs.vscode-extensions.llvm-org.lldb-vscode
            #   ];
            # })

            mold
            sccache
          ];
        };
      }
    );
}

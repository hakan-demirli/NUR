{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
  };

  outputs =
    {
      nixpkgs,
      ...
    }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        config.allowUnfree = true;
      };
    in
    {
      devShells.${system}.default =
        let
          pythonEnv = pkgs.python312.withPackages (
            ps: with ps; [
              setuptools
              requests

              flask
              flask-cors

              flask-socketio
              eventlet
            ]
          );
        in
        pkgs.mkShell {
          packages = [
            pythonEnv
          ];
        };
    };
}

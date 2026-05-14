{ pkgs }:

let
  formatter = import ../formatters.nix { inherit pkgs; };
in
pkgs.runCommand "check-formatting"
  {
    nativeBuildInputs = [ formatter ];
    src = ../..;
  }
  ''
    cp -r $src ./src
    chmod -R +w ./src
    cd ./src

    nur-fmt --ci

    touch $out
  ''

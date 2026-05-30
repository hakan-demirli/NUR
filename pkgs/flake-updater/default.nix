{ pkgs }:

pkgs.writeShellApplication {
  name = "flake-updater";

  runtimeInputs = [
    pkgs.git
    pkgs.gawk
    pkgs.gnused
    pkgs.nix
    pkgs.coreutils
    pkgs.util-linux
    pkgs.findutils
  ];

  text = builtins.readFile ./flake-updater.sh;
}

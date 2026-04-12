{ pkgs }:

pkgs.writeShellApplication {
  name = "umu-fzf";

  runtimeInputs = [
    pkgs.curl
    pkgs.jq
    pkgs.fzf
    pkgs.wl-clipboard
  ];

  text = builtins.readFile ./umu-fzf.sh;
}

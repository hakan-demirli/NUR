{ pkgs }:

{
  formatting = import ./checks/formatting.nix { inherit pkgs; };
  shebangs = import ./checks/shebangs.nix { inherit pkgs; };
  shellcheck = import ./checks/shellcheck.nix { inherit pkgs; };
}

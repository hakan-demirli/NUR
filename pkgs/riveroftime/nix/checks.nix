{
  pkgs,
}:

{
  deadnix = (import ./checks/deadnix.nix { inherit pkgs; }).lint;
  statix = (import ./checks/statix.nix { inherit pkgs; }).lint;
  shebangs = (import ./checks/shebangs.nix { inherit pkgs; }).check;
  shellcheck = (import ./checks/shellcheck.nix { inherit pkgs; }).lint;
  clippy = (import ./checks/clippy.nix { inherit pkgs; }).lint;
}

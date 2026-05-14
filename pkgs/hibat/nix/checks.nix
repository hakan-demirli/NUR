{ pkgs }:

let
  lintChecks = {
    "lint-deadnix" = (import ./checks/lint/deadnix.nix { inherit pkgs; }).lint;
    "lint-statix" = (import ./checks/lint/statix.nix { inherit pkgs; }).lint;
    "lint-shebang" = (import ./checks/lint/shebangs.nix { inherit pkgs; }).check;
    "lint-shellcheck" = (import ./checks/lint/shellcheck.nix { inherit pkgs; }).lint;
    "lint-clippy" = (import ./checks/lint/clippy.nix { inherit pkgs; }).lint;
    "lint-machete" = (import ./checks/lint/machete.nix { inherit pkgs; }).lint;
  };

in
lintChecks

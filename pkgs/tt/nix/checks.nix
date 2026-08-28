{ pkgs }:

let
  lintChecks = {
    "lint-deadnix" = (import ./checks/lint/deadnix.nix { inherit pkgs; }).lint;
    "lint-statix" = (import ./checks/lint/statix.nix { inherit pkgs; }).lint;
    "lint-formatting" = (import ./checks/lint/formatting.nix { inherit pkgs; }).fmt;
    "lint-shebang" = (import ./checks/lint/shebangs.nix { inherit pkgs; }).check;
    "lint-shellcheck" = (import ./checks/lint/shellcheck.nix { inherit pkgs; }).lint;
    "lint-mypy" = (import ./checks/lint/mypy.nix { inherit pkgs; }).lint;
  };

  testChecks = {
    "test-pytest" = (import ./checks/test/pytest.nix { inherit pkgs; }).test;
  };

in
lintChecks // testChecks

{ pkgs }:
{
  # buildPythonApplication runs pytest as part of its own check phase, so the
  # package build is the test run.
  test = pkgs.callPackage ../../package.nix { };
}

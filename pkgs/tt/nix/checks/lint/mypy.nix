{ pkgs }:
let
  package = pkgs.callPackage ../../package.nix { };
  pythonEnv = pkgs.python3.withPackages (
    ps:
    [
      ps.mypy
      ps.pytest
    ]
    ++ package.dependencies
  );
in
{
  lint =
    pkgs.runCommand "mypy"
      {
        nativeBuildInputs = [ pythonEnv ];
        src = ./../../..;
      }
      ''
        cp -r $src ./src
        chmod -R +w ./src
        cd ./src
        echo "Running mypy..."
        MYPY_CACHE_DIR=$TMPDIR/mypy mypy
        touch $out
      '';
}

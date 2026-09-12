{
  pkgs,
  lib,
}:
let
  realDerivation = pkgs.callPackage ./package.nix { };
  checks = import ./nix/checks.nix {
    inherit pkgs;
  };

in
pkgs.stdenv.mkDerivation {
  inherit (realDerivation) pname version;

  dontUnpack = true;
  dontBuild = true;

  doCheck = true;
  checkPhase = ''
    echo "Running checks..."
    ${builtins.concatStringsSep "\n" (builtins.map (c: "echo ${c}") (builtins.attrValues checks))}
  '';

  installPhase = ''
    mkdir -p $out/bin
    cp ${realDerivation}/bin/dap $out/bin/dap
  '';

  meta = {
    description = "Diff-fenced style diff apply tool.";
    license = lib.licenses.mit;
    maintainers = [ lib.maintainers.hakan-demirli ];
    mainProgram = "dap";
  };
}

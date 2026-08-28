{ pkgs }:

let
  treefmt = import ./formatters/treefmt.nix { inherit pkgs; };
in
pkgs.writeShellScriptBin "custom-formatter" ''
  failed=0
  echo "[Formatter] Running treefmt..."
  ${treefmt}/bin/treefmt --ci -v "$@" || failed=1

  if [ $failed -ne 0 ]; then
    echo "[Formatter] Formatting failed."
    exit 1
  fi

  echo "[Formatter] Done."
''

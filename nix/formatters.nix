{ pkgs }:

let
  treefmt = import ./formatters/treefmt.nix { inherit pkgs; };
in
pkgs.writeShellScriptBin "nur-fmt" ''
  mode="format"
  args=()

  for arg in "$@"; do
    case "$arg" in
      --check|--ci)
        mode="check"
        ;;
      *)
        args+=("$arg")
        ;;
    esac
  done

  if [ "$mode" = "check" ]; then
    exec ${treefmt}/bin/treefmt --ci -v "''${args[@]}"
  fi

  exec ${treefmt}/bin/treefmt -v "''${args[@]}"
''

{ pkgs }:

let
  statix-wrapper = pkgs.writeShellScriptBin "statix-fix" ''
    for path in "$@"; do
      ${pkgs.statix}/bin/statix fix "$path"
    done
  '';
in
pkgs.treefmt.withConfig {
  runtimeInputs = with pkgs; [
    nixfmt-rfc-style
    deadnix
    statix
    shfmt
    rustfmt
  ];

  settings = {
    on-unmatched = "info";
    tree-root-file = "flake.nix";

    formatter = {
      deadnix = {
        command = "deadnix";
        options = [ "--edit" ];
        includes = [ "*.nix" ];
      };

      statix = {
        command = "${statix-wrapper}/bin/statix-fix";
        includes = [ "*.nix" ];
      };

      nixfmt = {
        command = "nixfmt";
        includes = [ "*.nix" ];
      };

      shfmt = {
        command = "shfmt";
        options = [
          "-i"
          "2"
          "-ln"
          "bash"
          "-s"
          "-ci"
          "-bn"
          "-sr"
          "-w"
        ];
        includes = [
          "*.sh"
          "*.bash"
        ];
      };

      rustfmt = {
        command = "rustfmt";
        options = [
          "--edition"
          "2021"
        ];
        includes = [ "*.rs" ];
      };
    };
  };
}

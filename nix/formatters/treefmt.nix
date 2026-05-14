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
    deadnix
    nixfmt-rfc-style
    nodePackages.prettier
    ruff
    rustfmt
    shfmt
    statix
    taplo
  ];

  settings = {
    on-unmatched = "info";
    tree-root-file = "flake.nix";

    formatter = {
      deadnix = {
        command = "deadnix";
        options = [ "--edit" ];
        includes = [ "*.nix" ];
        excludes = [ "flake.lock" ];
        priority = 1;
      };

      statix = {
        command = "${statix-wrapper}/bin/statix-fix";
        includes = [ "*.nix" ];
        excludes = [ "flake.lock" ];
        priority = 2;
      };

      nixfmt = {
        command = "nixfmt";
        includes = [ "*.nix" ];
        priority = 3;
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
          "*.bash"
          "*.sh"
        ];
      };

      ruff-check = {
        command = "ruff";
        options = [
          "check"
          "--fix"
          "--select"
          "E,W,F,I,B,C4,UP,SIM,RUF"
          "--ignore"
          "E501,W191,E111,E114,E117"
        ];
        includes = [
          "*.py"
          "*.pyi"
        ];
        priority = 1;
      };

      ruff-format = {
        command = "ruff";
        options = [ "format" ];
        includes = [
          "*.py"
          "*.pyi"
        ];
        priority = 2;
      };

      rustfmt = {
        command = "rustfmt";
        options = [
          "--edition"
          "2021"
        ];
        includes = [ "*.rs" ];
      };

      prettier = {
        command = "prettier";
        options = [ "--write" ];
        includes = [
          "*.css"
          "*.html"
          "*.js"
          "*.json"
          "*.jsonc"
          "*.md"
          "*.ts"
          "*.tsx"
          "*.yaml"
          "*.yml"
        ];
      };

      taplo = {
        command = "taplo";
        options = [ "format" ];
        includes = [ "*.toml" ];
      };
    };
  };
}

{ pkgs }:
let
  target = pkgs.pkgsCross.aarch64-multiplatform-musl.pkgsStatic;

  fontsConf = pkgs.makeFontsConf {
    fontDirectories = [ pkgs.dejavu_fonts ];
  };

  src = pkgs.lib.cleanSourceWith {
    src = ./.;
    filter =
      path: _type:
      let
        p = toString path;
        root = toString ./.;
        rel = pkgs.lib.removePrefix (root + "/") p;
      in
      p == root || rel == "Cargo.toml" || rel == "Cargo.lock" || pkgs.lib.hasPrefix "crates" rel;
  };
in
target.rustPlatform.buildRustPackage {
  pname = "r01-ui";
  version = "0.1.0";

  inherit src;
  cargoLock.lockFile = ./Cargo.lock;

  cargoBuildFlags = [
    "-p"
    "r01-ui"
    "--no-default-features"
    "--features"
    "router"
  ];

  doCheck = false;

  nativeBuildInputs = with pkgs; [
    fontconfig
    dejavu_fonts
    pkg-config
  ];

  FONTCONFIG_FILE = fontsConf;

  preBuild = ''
    export LD_LIBRARY_PATH="${pkgs.fontconfig.lib}/lib''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
  '';

  meta = {
    description = "Touchscreen UI for the GL-BE10000 LCD (cross-built aarch64-musl static)";
    mainProgram = "r01-ui";
    platforms = pkgs.lib.platforms.linux;
  };
}

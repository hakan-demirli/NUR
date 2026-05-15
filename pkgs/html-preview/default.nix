{ pkgs, ... }:
let
  htmlPreviewLsp = pkgs.rustPlatform.buildRustPackage {
    pname = "html-preview-lsp";
    version = "0.1.0";

    src = ./lsp;
    cargoHash = "sha256-UTdOk/DMxTGhHbyTiIPz1V/wI6bTHDSGfO5n3b8NVBc=";

    nativeBuildInputs = [ pkgs.pkg-config ];

    buildInputs = [ pkgs.openssl ];

    meta = {
      description = "LSP Server for HTML Preview";

    };
  };

  htmlPreviewServerStdenv = pkgs.stdenv.mkDerivation {
    name = "html-preview-server-script";
    version = "0.1.0";

    src = ./server;

    nativeBuildInputs = [ pkgs.makeWrapper ];

    propagatedBuildInputs = [
      (pkgs.python3.withPackages (
        ps: with ps; [
          requests
          flask
          flask-cors
          flask-socketio
          eventlet
        ]
      ))
    ];

    dontUnpack = true;

    installPhase = ''
      runHook preInstall
      mkdir -p $out/bin $out/libexec/html-preview-server
      cp $src/main.py $out/libexec/html-preview-server/

      makeWrapper ${pkgs.python3}/bin/python $out/bin/html-preview-server \
        --add-flags $out/libexec/html-preview-server/main.py \
        --prefix PYTHONPATH : "$PYTHONPATH" 

      runHook postInstall
    '';
    meta = {
      description = "Server component for HTML Preview";
    };
  };

in

pkgs.buildEnv {
  name = "html-preview";
  paths = [
    htmlPreviewLsp
    htmlPreviewServerStdenv
  ];

  meta = {
    description = "HTML Preview tools (LSP + Server)";
    broken = true;
  };
}

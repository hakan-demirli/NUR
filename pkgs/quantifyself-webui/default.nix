{ pkgs, ... }:
pkgs.stdenv.mkDerivation {
  pname = "quantifyself-webui";
  version = "0.1.0";

  propagatedBuildInputs = [
    (pkgs.python3.withPackages (
      pythonPackages: with pythonPackages; [
        setuptools
        flask-cors
        flask
        duckdb
        psutil
        requests
      ]
    ))
  ];
  dontUnpack = true;

  src = ./.;

  installPhase = ''
    mkdir -p $out/bin
    cp -r $src/static                    $out/bin/static
    cp -r $src/quantifyself-webui.py     $out/

    ln -s $out/quantifyself-webui.py     $out/bin/quantifyself-webui
    chmod +x $out/bin/quantifyself-webui
  '';
}

{ pkgs, ... }:
pkgs.stdenv.mkDerivation {
  pname = "quantifyself";
  version = "0.1.0";

  propagatedBuildInputs = [
    (pkgs.python3.withPackages (
      pythonPackages: with pythonPackages; [
        setuptools
        flask
        flask-cors
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
    cp -r $src/quantifyself-system.py    $out/
    cp -r $src/quantifyself-server.py    $out/
    cp -r $src/quantifyself-netstatus.py $out/
    cp -r $src/quantifyself-window.py    $out/

    ln -s $out/quantifyself-server.py    $out/bin/quantifyself-server
    ln -s $out/quantifyself-system.py    $out/bin/quantifyself-system
    ln -s $out/quantifyself-netstatus.py $out/bin/quantifyself-netstatus
    ln -s $out/quantifyself-window.py    $out/bin/quantifyself-window

    chmod +x $out/bin/quantifyself-server
    chmod +x $out/bin/quantifyself-system
    chmod +x $out/bin/quantifyself-netstatus
    chmod +x $out/bin/quantifyself-window
  '';
}

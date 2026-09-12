{ pkgs, ... }:
pkgs.stdenv.mkDerivation {
  pname = "prometheus-exporters";
  version = "0.1.0";

  propagatedBuildInputs = [
    (pkgs.python3.withPackages (
      pythonPackages: with pythonPackages; [
        poetry-core
        setuptools
        psutil
        requests
        prometheus-client
      ]
    ))
  ];
  dontUnpack = true;

  src = ./.;

  installPhase = ''
    mkdir -p $out/bin

    cp -r $src/prometheus-exporter-window.py  $out/
    cp -r $src/prometheus-exporter-network.py $out/
    cp -r $src/prometheus-exporter-system.py  $out/

    ln -s $out/prometheus-exporter-window.py  $out/bin/prometheus-exporter-window
    ln -s $out/prometheus-exporter-network.py $out/bin/prometheus-exporter-network
    ln -s $out/prometheus-exporter-system.py  $out/bin/prometheus-exporter-system

    chmod +x $out/bin/prometheus-exporter-window
    chmod +x $out/bin/prometheus-exporter-network
    chmod +x $out/bin/prometheus-exporter-system
  '';
}

{ pkgs, ... }:
pkgs.stdenv.mkDerivation {
  pname = "notify-scheduler";
  version = "0.1.0";

  propagatedBuildInputs = [
    (pkgs.python3.withPackages (pythonPackages: with pythonPackages; [ requests ]))
  ];
  dontUnpack = true;
  installPhase = ''
    install -Dm755 ${./notify-scheduler.py} $out/bin/notify-scheduler;
  '';
}

{ pkgs, ... }:
pkgs.stdenv.mkDerivation {
  pname = "clipboard-tts";
  version = "0.1.0";

  propagatedBuildInputs = [
    pkgs.piper-tts
    (pkgs.python3.withPackages (
      pythonPackages: with pythonPackages; [
        pyclip
      ]
    ))
  ];

  dontUnpack = true;

  installPhase = ''
    install -Dm755 ${./clipboard-tts.py} $out/bin/clipboard-tts;
  '';

  meta.broken = true;
}

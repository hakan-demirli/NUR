{ pkgs, ... }:
pkgs.stdenv.mkDerivation {
  pname = "youtube-sync";
  version = "0.1.0";
  propagatedBuildInputs = [
    (pkgs.python3.withPackages (pythonPackages: with pythonPackages; [ yt-dlp ]))
    pkgs.ffmpeg
  ];
  dontUnpack = true;
  installPhase = ''
    install -Dm755 ${./youtube-sync.py} $out/bin/youtube-sync;
  '';
}

{ pkgs, ... }:
pkgs.stdenv.mkDerivation {
  pname = "waybar-timer";
  version = "0.1.0";

  nativeBuildInputs = with pkgs; [ zenity ];

  propagatedBuildInputs = [
    pkgs.python3
    pkgs.ffmpeg-full # full version for ffplay
  ];

  dontUnpack = true;

  installPhase = ''
    install -Dm755 ${./waybar-timer.py} $out/bin/waybar-timer;
  '';

  postFixup = ''
    substituteInPlace $out/bin/waybar-timer \
      --replace 'ZENITY = "zenity"' 'ZENITY = "${pkgs.zenity}/bin/zenity"'
  '';
}

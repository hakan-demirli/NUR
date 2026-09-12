{ pkgs, ... }:
pkgs.stdenv.mkDerivation {
  pname = "auto-refresh";
  version = "0.1.0";

  nativeBuildInputs = with pkgs; [
    wrapGAppsHook3
    gobject-introspection
    libgudev
    libnotify
    gnused
  ];
  propagatedBuildInputs = [
    (pkgs.python3.withPackages (pythonPackages: with pythonPackages; [ pygobject3 ]))
  ];
  dontUnpack = true;
  installPhase = ''
    install -Dm755 ${./auto-refresh.py} $out/bin/auto-refresh;
  '';
}

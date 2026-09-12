{ pkgs, ... }:
pkgs.stdenv.mkDerivation {
  pname = "gtk-indicator";
  version = "0.1.0";

  nativeBuildInputs = with pkgs; [
    wrapGAppsHook3
    gobject-introspection
    libappindicator
  ];
  propagatedBuildInputs = [
    pkgs.gtk4-layer-shell
    pkgs.gtk4

    pkgs.gvfs

    (pkgs.python3.withPackages (pythonPackages: with pythonPackages; [ pygobject3 ]))
  ];
  dontUnpack = true;

  src = ./.;

  installPhase = ''
    mkdir -p $out/bin
    cp -r $src/* $out/
    rm $out/default.nix
    makeWrapper $out/gtk-indicator.py $out/bin/gtk-indicator \
      --set LD_PRELOAD ${pkgs.gtk4-layer-shell}/lib/libgtk4-layer-shell.so
    makeWrapper $out/gtk-indicator-client.py $out/bin/gtk-indicator-client \
      --set LD_PRELOAD ${pkgs.gtk4-layer-shell}/lib/libgtk4-layer-shell.so
    makeWrapper $out/gtk-indicator-client-volume.py $out/bin/gtk-indicator-client-volume \
      --set LD_PRELOAD ${pkgs.gtk4-layer-shell}/lib/libgtk4-layer-shell.so
    makeWrapper $out/gtk-indicator-client-mic.py $out/bin/gtk-indicator-client-mic \
      --set LD_PRELOAD ${pkgs.gtk4-layer-shell}/lib/libgtk4-layer-shell.so
    makeWrapper $out/gtk-indicator-client-brightness.py $out/bin/gtk-indicator-client-brightness \
      --set LD_PRELOAD ${pkgs.gtk4-layer-shell}/lib/libgtk4-layer-shell.so
    chmod +x $out/bin/gtk-indicator
    chmod +x $out/bin/gtk-indicator-client
    chmod +x $out/bin/gtk-indicator-client-volume
    chmod +x $out/bin/gtk-indicator-client-mic
    chmod +x $out/bin/gtk-indicator-client-brightness
  '';
}

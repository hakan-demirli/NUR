{ pkgs, ... }:
pkgs.stdenv.mkDerivation {
  pname = "gtk-applet";
  version = "0.1.0";

  nativeBuildInputs = with pkgs; [
    wrapGAppsHook3
    gobject-introspection
    libappindicator
  ];
  propagatedBuildInputs = [
    pkgs.activate-linux # for update_wp
    (pkgs.python3.withPackages (
      pythonPackages: with pythonPackages; [
        pygobject3
        requests
      ]
    ))
  ];
  dontUnpack = true;

  src = ./.;

  installPhase = ''
    mkdir -p $out/bin
    cp -r $src/gtk-applet-power-menu.py $out/
    cp -r $src/gtk-applet-script-menu.py $out/
    ln -s $out/gtk-applet-power-menu.py $out/bin/gtk-applet-power-menu
    ln -s $out/gtk-applet-script-menu.py $out/bin/gtk-applet-script-menu
    chmod +x $out/bin/gtk-applet-script-menu
    chmod +x $out/bin/gtk-applet-power-menu
  '';
}

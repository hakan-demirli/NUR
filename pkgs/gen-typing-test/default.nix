{ pkgs, ... }:
pkgs.stdenv.mkDerivation {
  pname = "gen-typing-test";
  version = "0.1.0";
  propagatedBuildInputs = [
    pkgs.awww
    (pkgs.python3.withPackages (
      pythonPackages: with pythonPackages; [
        #
      ]
    ))
  ];
  dontUnpack = true;

  src = ./.;

  installPhase = ''
    mkdir -p $out/bin
    cp -r $src/* $out/
    rm $out/default.nix
    ln -s $out/gen-typing-test.py $out/bin/gen-typing-test
    chmod +x $out/bin/gen-typing-test
  '';
}

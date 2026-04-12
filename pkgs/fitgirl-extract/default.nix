{ pkgs }:

let
  mingw = pkgs.pkgsCross.mingw32.buildPackages;

  windows-helpers = pkgs.stdenv.mkDerivation {
    name = "fitgirl-extract-helpers";

    src = ./src;

    nativeBuildInputs = [
      mingw.gcc
    ];

    buildPhase = ''
      ${mingw.gcc}/bin/i686-w64-mingw32-g++ -static -o unarc.exe unarc.cpp
      ${mingw.gcc}/bin/i686-w64-mingw32-gcc -shared -static -o CLS-srep.dll \
        cls-srep.c -lkernel32 -luser32
    '';

    installPhase = ''
      mkdir -p $out
      cp unarc.exe CLS-srep.dll $out/
    '';
  };

in
pkgs.stdenv.mkDerivation {
  name = "fitgirl-extract";

  src = ./.;
  dontUnpack = true;

  nativeBuildInputs = [ pkgs.makeWrapper ];

  buildInputs = [
    pkgs.wine64
    pkgs.innoextract
    pkgs.coreutils
    pkgs.findutils
    pkgs.gnused
    pkgs.gnugrep
  ];

  installPhase = ''
    mkdir -p $out/bin $out/libexec/fitgirl-extract

    cp ${windows-helpers}/unarc.exe $out/libexec/fitgirl-extract/
    cp ${windows-helpers}/CLS-srep.dll $out/libexec/fitgirl-extract/

    substitute ${./fitgirl-extract.sh} $out/bin/fitgirl-extract \
      --replace-fail '@libexec@' "$out/libexec/fitgirl-extract"
    chmod +x $out/bin/fitgirl-extract

    wrapProgram $out/bin/fitgirl-extract \
      --prefix PATH : ${
        pkgs.lib.makeBinPath [
          pkgs.wine64
          pkgs.innoextract
          pkgs.coreutils
          pkgs.findutils
          pkgs.gnused
          pkgs.gnugrep
        ]
      }
  '';
}

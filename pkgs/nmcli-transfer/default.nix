{ pkgs, ... }:
pkgs.stdenv.mkDerivation {
  pname = "nmcli-transfer";
  version = "0.1.0";
  propagatedBuildInputs = [ ];
  dontUnpack = true;
  installPhase = ''
    install -Dm755 ${./nmcli-transfer.sh} $out/bin/nmcli-transfer;
  '';
}

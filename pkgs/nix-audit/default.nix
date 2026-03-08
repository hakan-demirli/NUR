{
  pkgs,
}:
pkgs.stdenv.mkDerivation {
  pname = "nix-audit";
  version = "1.0.0";

  src = ./.;

  nativeBuildInputs = [ pkgs.makeWrapper ];

  installPhase = ''
    runHook preInstall
    mkdir -p $out/bin
    cp $src/nix-audit.sh $out/bin/nix-audit
    chmod +x $out/bin/nix-audit
    runHook postInstall
  '';

  dontBuild = true;
  dontCheck = true;
}

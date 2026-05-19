{ pkgs }:

pkgs.pkgsStatic.rustPlatform.buildRustPackage {
  pname = "raider";
  version = "0.5.0";

  src = pkgs.lib.cleanSourceWith {
    src = ./.;
    filter =
      path: _type:
      let
        p = toString path;
        root = toString ./.;
        rel = pkgs.lib.removePrefix (root + "/") p;
      in
      p == root || rel == "Cargo.toml" || rel == "Cargo.lock" || pkgs.lib.hasPrefix "crates" rel;
  };
  doCheck = false;

  cargoLock.lockFile = ./Cargo.lock;

  nativeBuildInputs = with pkgs; [
    pkg-config
  ];

  buildInputs = with pkgs; [
    openssl
  ];
}

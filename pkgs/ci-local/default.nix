{ pkgs }:

pkgs.rustPlatform.buildRustPackage {
  pname = "ci-local";
  version = "0.1.0";

  src = pkgs.lib.cleanSourceWith {
    src = ./.;
    filter =
      path: _type:
      let
        p = toString path;
        root = toString ./.;
        rel = pkgs.lib.removePrefix (root + "/") p;
      in
      p == root
      || rel == "Cargo.toml"
      || rel == "Cargo.lock"
      || pkgs.lib.hasPrefix "src" rel
      || pkgs.lib.hasPrefix "tests" rel;
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

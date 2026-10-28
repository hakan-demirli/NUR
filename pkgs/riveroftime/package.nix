{
  pkgs,
}:
pkgs.rustPlatform.buildRustPackage {
  pname = "riveroftime";
  version = "0.1.0";

  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;

  nativeBuildInputs = [
    pkgs.pkg-config
  ];

  buildInputs = [
    pkgs.wayland
    pkgs.libxkbcommon
    pkgs.fontconfig
  ];

  meta = {
    description = "Wayland layer countdown timer";
    license = pkgs.lib.licenses.mit;
    mainProgram = "riveroftime";
  };
}

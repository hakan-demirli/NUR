{ pkgs }:

pkgs.rustPlatform.buildRustPackage {
  pname = "hibat";
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
      p == root || rel == "Cargo.toml" || rel == "Cargo.lock" || pkgs.lib.hasPrefix "src" rel;
  };
  doCheck = false;

  cargoLock.lockFile = ./Cargo.lock;

  nativeBuildInputs = with pkgs; [
    pkg-config
    makeWrapper
  ];

  buildInputs = with pkgs; [
    openssl
    wayland
    libxkbcommon
    libGL
    libx11
    libxcursor
    libxrandr
    libxi
  ];

  postInstall = ''
    wrapProgram $out/bin/hibat \
      --prefix LD_LIBRARY_PATH : ${
        pkgs.lib.makeLibraryPath (
          with pkgs;
          [
            wayland
            libxkbcommon
            libGL
            libx11
            libxcursor
            libxrandr
            libxi
            vulkan-loader
          ]
        )
      }
  '';

  meta.platforms = pkgs.lib.platforms.linux;
}

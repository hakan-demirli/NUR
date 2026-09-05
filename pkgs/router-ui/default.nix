{ pkgs }:
let
  target = pkgs.pkgsCross.aarch64-multiplatform-musl.pkgsStatic;

  fontsConf = pkgs.makeFontsConf {
    fontDirectories = [ pkgs.dejavu_fonts ];
  };

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
      || pkgs.lib.hasPrefix "crates" rel
      || pkgs.lib.hasPrefix "fixtures" rel;
  };
  desktopLibs = with pkgs; [
    fontconfig.lib
    libxkbcommon
    wayland
    libGL
    libx11
    libxcursor
    libxi
    libxrandr
  ];

  desktop = pkgs.rustPlatform.buildRustPackage {
    pname = "router-ui-desktop";
    version = "0.1.0";

    inherit src;
    cargoLock.lockFile = ./Cargo.lock;

    cargoBuildFlags = [
      "-p"
      "router-ui"
    ];

    doCheck = false;

    nativeBuildInputs = with pkgs; [
      pkg-config
      makeWrapper
      dejavu_fonts
    ];
    buildInputs = desktopLibs;

    FONTCONFIG_FILE = fontsConf;

    preBuild = ''
      export LD_LIBRARY_PATH="${pkgs.fontconfig.lib}/lib''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    '';

    postInstall = ''
      mkdir -p $out/share/router-ui
      cp -r fixtures $out/share/router-ui/fixtures

      wrapProgram $out/bin/router-ui \
        --set FONTCONFIG_FILE ${fontsConf} \
        --set-default ROUTER_UI_FIXTURES $out/share/router-ui/fixtures \
        --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath desktopLibs}
    '';

    meta = {
      description = "router-ui desktop preview (windowed, fixture-backed)";
      mainProgram = "router-ui";
      platforms = pkgs.lib.platforms.linux;
    };
  };
  buildRouter =
    {
      pname,
      features,
      passthru ? { },
    }:
    target.rustPlatform.buildRustPackage {
      inherit pname passthru;
      version = "0.1.0";

      inherit src;
      cargoLock.lockFile = ./Cargo.lock;

      cargoBuildFlags = [
        "-p"
        "router-ui"
        "--no-default-features"
        "--features"
        (pkgs.lib.concatStringsSep "," features)
      ];

      doCheck = false;

      nativeBuildInputs = with pkgs; [
        fontconfig
        dejavu_fonts
        pkg-config
      ];

      FONTCONFIG_FILE = fontsConf;

      preBuild = ''
        export LD_LIBRARY_PATH="${pkgs.fontconfig.lib}/lib''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
      '';

      meta = {
        description = "Touchscreen UI for the GL-BE10000 LCD (cross-built aarch64-musl static)";
        mainProgram = "router-ui";
        platforms = pkgs.lib.platforms.linux;
      };
    };
  touchDebug = buildRouter {
    pname = "router-ui-touch-debug";
    features = [
      "router"
      "touch-debug"
    ];
  };
in
buildRouter {
  pname = "router-ui";
  features = [
    "router"
  ];
  passthru = {
    inherit desktop touchDebug;
  };
}

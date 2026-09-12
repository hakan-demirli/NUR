{
  lib,
  stdenv,
  src,
  appSdk,
  pebbleosSdk,
  pythonEnv,
}:
let
  info = lib.importJSON "${src}/package.json";
in
stdenv.mkDerivation {
  pname = info.name;
  inherit (info) version;

  inherit src;

  nativeBuildInputs = [
    pythonEnv
    pebbleosSdk
  ];

  dontConfigure = true;
  hardeningDisable = [ "all" ];

  buildPhase = ''
    runHook preBuild

    export HOME="$TMPDIR/home"
    mkdir -p "$HOME"
    export PATH="${pebbleosSdk}/arm-none-eabi/bin:$PATH"
    unset CC CXX AR AS LD RANLIB NM STRIP OBJDUMP OBJCOPY READELF SIZE HOSTCC HOSTCXX

    # The SDK's waf self-extracts its waflib next to the waf binary and derives
    # the SDK root from that location, so it cannot run from the read-only store.
    cp -r --no-preserve=ownership ${appSdk} "$TMPDIR/sdk"
    chmod -R u+w "$TMPDIR/sdk"

    "$TMPDIR/sdk/waf" configure
    "$TMPDIR/sdk/waf" build

    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    shopt -s nullglob
    bundles=(build/*.pbw)
    if [ ''${#bundles[@]} -ne 1 ]; then
      echo "ERROR: expected exactly one .pbw, got: ''${bundles[*]:-none}" >&2
      exit 1
    fi
    install -Dm644 "''${bundles[0]}" "$out/${info.name}.pbw"
    runHook postInstall
  '';

  passthru.sdk = appSdk;

  meta = {
    description = "${info.pebble.displayName} watchface for Pebble Round 2";
    platforms = lib.platforms.linux;
  };
}

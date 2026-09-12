{
  lib,
  stdenv,
  runCommand,
  pebbleosSource,
  pebbleosSdk,
  pythonEnv,
  binutils,
  clang,
  dash,
  gcc,
  gettext,
  git,
  gnumake,
  librsvg,
  nodejs,
  pkg-config,
  protobuf,
  which,
  board ? "getafix@dvt2",
}:
let
  clangBinsOnly = runCommand "pebble-clang-bins" { } ''
    mkdir -p $out/bin
    ln -sf ${clang}/bin/clang $out/bin/clang
    ln -sf ${clang}/bin/clang++ $out/bin/clang++
  '';
in
stdenv.mkDerivation {
  pname = "pebble-app-sdk";
  version = pebbleosSource.passthru.upstreamTag;

  src = pebbleosSource;

  nativeBuildInputs = [
    pythonEnv
    pebbleosSdk
    clangBinsOnly
    binutils
    dash
    gcc
    gettext
    git
    gnumake
    librsvg
    nodejs
    pkg-config
    protobuf
    which
  ];

  dontConfigure = true;
  hardeningDisable = [ "all" ];

  unpackPhase = ''
    runHook preUnpack
    cp -r --no-preserve=ownership ${pebbleosSource} ./pebbleos
    chmod -R u+w ./pebbleos
    sourceRoot=$(pwd)/pebbleos
    runHook postUnpack
  '';

  postPatch = ''
    patchShebangs .
  '';

  buildPhase = ''
    runHook preBuild
    cd $sourceRoot

    export HOME="$TMPDIR/home"
    mkdir -p "$HOME"
    export XDG_CACHE_HOME="$HOME/.cache"

    # The SDK's arm-none-eabi must win over the nixpkgs host gcc, otherwise
    # waf's find_program(['gcc']) picks the wrong toolchain version.
    export PATH="${pebbleosSdk}/arm-none-eabi/bin:$PATH"
    unset CC CXX AR AS LD RANLIB NM STRIP OBJDUMP OBJCOPY READELF SIZE HOSTCC HOSTCXX

    # tools/waf/gitinfo.py stamps the build from `git describe`; the fetched
    # source has no .git, so bootstrap a tagged empty commit.
    export GIT_AUTHOR_NAME=nix GIT_AUTHOR_EMAIL=nix@localhost
    export GIT_COMMITTER_NAME=nix GIT_COMMITTER_EMAIL=nix@localhost
    git init -q -b main .
    git commit -q --allow-empty -m "nix pin: ${pebbleosSource.passthru.upstreamRev}"
    git tag -a "${pebbleosSource.passthru.upstreamTag}" -m "nix pin"

    ./waf configure \
      --board=${board} \
      --variant=normal \
      --relax_toolchain_restrictions \
      -DCONFIG_MODDABLE_XS=n
    ./waf build --onlysdk

    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    if [ ! -f build/sdk/gabbro/lib/libpebble.a ]; then
      echo "ERROR: SDK generation produced no libpebble.a" >&2
      ls -la build/sdk >&2 || true
      exit 1
    fi
    cp -r build/sdk $out
    runHook postInstall
  '';

  passthru = { inherit board; };

  meta = {
    description = "Pebble app SDK (pebble.h + libpebble.a) for the gabbro platform";
    homepage = "https://github.com/coredevices/PebbleOS";
    license = lib.licenses.asl20;
    platforms = lib.platforms.linux;
  };
}

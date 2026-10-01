{
  lib,
  stdenv,
  fetchFromGitHub,
  cmake,
  ninja,
  pkg-config,
  icu,
  curl,
  libpng,
  libjpeg,
  openssl,
  ffmpeg-headless,
  rocmPackages,
}:

let
  revision = "fd1710b5fd090880722e0681a868df2006595c73";
  shortRevision = builtins.substring 0 7 revision;
  gpuPackages = rocmPackages.gfx1151.overrideScope (
    final: prev: {
      rocblas = prev.rocblas.override {
        buildTests = false;
        buildBenchmarks = false;
      };
      hipblas = prev.hipblas.overrideAttrs (oldAttrs: {
        buildInputs = lib.subtractLists [
          final.rocsolver
          final.rocsparse
        ] oldAttrs.buildInputs;
        cmakeFlags = lib.remove "-DBUILD_WITH_SOLVER=ON" oldAttrs.cmakeFlags ++ [
          "-DBUILD_WITH_SOLVER=OFF"
        ];
      });
    }
  );
  mediaTools = ffmpeg-headless.override {
    withHeadlessDeps = false;
    withX264 = true;
    withHardcodedTables = true;
    withSafeBitstreamReader = true;
    withPixelutils = true;
    buildFfmpeg = true;
    buildFfprobe = true;
    buildAvcodec = true;
    buildAvdevice = true;
    buildAvfilter = true;
    buildAvformat = true;
    buildAvutil = true;
    buildSwresample = true;
    buildSwscale = true;
  };

in
stdenv.mkDerivation (finalAttrs: {
  pname = "gufo";
  version = "0.3.0";

  src = fetchFromGitHub {
    owner = "gufo-org";
    repo = "gufo";
    rev = revision;
    hash = "sha256-nqbCT/pgsOJD/AmmgE2o+CpEriosf2chRUFTfermvtE=";
  };

  patches = [ ./default-cache.patch ];
  patchFlags = [
    "-p1"
    "--fuzz=0"
  ];

  nativeBuildInputs = [
    cmake
    ninja
    pkg-config
    gpuPackages.clr
  ];

  buildInputs = [
    icu
    curl
    libpng
    libjpeg
    openssl
    gpuPackages.clr
    gpuPackages.hipblas
    gpuPackages.hipblaslt
    gpuPackages.hipcub
    gpuPackages.rocprim
    gpuPackages.rocwmma
    gpuPackages.rocblas
  ];

  cmakeFlags = [
    "-DCMAKE_BUILD_TYPE=RelWithDebInfo"
    "-DBUILD_TESTING=OFF"
    "-DGUFO_BUILD_TOOLS=OFF"
    "-DENGINE_ENABLE_HIP=ON"
    "-DCMAKE_HIP_ARCHITECTURES=gfx1151"
    "-DGUFO_RELEASE_VERSION=${finalAttrs.version}"
    "-DGUFO_REVISION=${shortRevision}"
    "-DGUFO_FFMPEG_EXECUTABLE=${lib.getBin mediaTools}/bin/ffmpeg"
    "-DGUFO_FFPROBE_EXECUTABLE=${lib.getBin mediaTools}/bin/ffprobe"
    "-DCMAKE_HIP_COMPILER=${gpuPackages.llvm.clang}/bin/clang"
  ];

  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck

    test "$("$out/bin/gufo" --version)" = "gufo version ${finalAttrs.version} (${shortRevision})"
    "$out/bin/gufo" --help >/dev/null
    test "$("$out/bin/gufo-server" --version)" = "gufo version ${finalAttrs.version} (${shortRevision})"
    "$out/bin/gufo-server" --help >/dev/null
    test ! -e "$out/bin/gufo-kernel-bench"
    test -f "$out/share/gufo/eval/antirez-ds4.json"
    test -f "$out/share/gufo/models/minimax_h3/MINIMAX_H3_FL2VA_BF16.source-manifest.json"
    test -f "$out/share/licenses/gufo/LICENSE"
    test -f "$out/share/licenses/gufo/third-party/LICENSE.ds4"

    c++ -std=c++20 -UNDEBUG -I "$NIX_BUILD_TOP/$sourceRoot" \
      ${./cache-directory-test.cpp} -o cache-directory-test
    ./cache-directory-test

    runHook postInstallCheck
  '';

  passthru = {
    inherit revision;
    rocmPackages = gpuPackages;
    ffmpeg = mediaTools;
  };

  meta = {
    description = "Local inference engine for AMD Strix Halo (gfx1151 GPU)";
    homepage = "https://github.com/gufo-org/gufo";
    changelog = "https://github.com/gufo-org/gufo/releases/tag/v${finalAttrs.version}";
    license = lib.licenses.mit;
    platforms = [ "x86_64-linux" ];
    mainProgram = "gufo";
  };
})

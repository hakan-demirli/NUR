{
  lib,
  stdenvNoCC,
  bubblewrap,
  claude-code,
  esbuild,
  fetchFromGitHub,
  python3Packages,
}:

let
  opencode-claude-auth = stdenvNoCC.mkDerivation (finalAttrs: {
    pname = "opencode-claude-auth";
    version = "1.5.3";

    src = fetchFromGitHub {
      owner = "griffinmartin";
      repo = "opencode-claude-auth";
      rev = "v${finalAttrs.version}";
      hash = "sha256-q5H2p3mwHH9Uh3BWDC/AJaj1+jxGYEthm7YNfy7MEPQ=";
    };

    patches = [
      ./plugins/claude-auth/patches/extra-homes.patch
      ./plugins/claude-auth/patches/auto-switch.patch
    ];

    nativeBuildInputs = [ esbuild ];

    buildPhase = ''
      runHook preBuild
      esbuild src/index.ts \
        --bundle \
        --format=esm \
        --platform=node \
        --target=node22 \
        --outfile=opencode-claude-auth.js
      runHook postBuild
    '';

    installPhase = ''
      runHook preInstall

      mkdir -p "$out"
      cp opencode-claude-auth.js "$out/opencode-claude-auth.js"
      cp LICENSE "$out/LICENSE"
      cp README.md "$out/README.md"

      runHook postInstall
    '';

    meta = {
      description = "OpenCode Claude auth plugin with extra Claude home support";
      homepage = "https://github.com/griffinmartin/opencode-claude-auth";
      license = lib.licenses.mit;
      platforms = lib.platforms.all;
    };
  });

  opencode-goal = python3Packages.buildPythonApplication {
    pname = "opencode-goal";
    version = "0.1.0";
    pyproject = true;

    src = ./plugins/goal/runtime;

    build-system = [ python3Packages.setuptools ];

    meta = {
      description = "OpenCode /goal and /loop orchestrator daemon";
      license = lib.licenses.mit;
      platforms = lib.platforms.linux;
    };
  };
in
stdenvNoCC.mkDerivation {
  pname = "opencode-plugins";
  version = "0.1.0";

  src = ./.;

  installPhase = ''
    runHook preInstall

    mkdir -p "$out"
    mkdir -p "$out/bin"
    substitute ${./plugins/claude-auth/scripts/claude2.sh} "$out/bin/claude2" \
      --subst-var-by bwrap "${bubblewrap}/bin/bwrap" \
      --subst-var-by claude "${claude-code}/bin/claude"
    chmod +x "$out/bin/claude2"

    mkdir -p "$out/plugins"
    cp plugins/claude-auth/opencode/opencode-claude-auth-multi.js "$out/plugins/opencode-claude-auth-multi.js"
    cp -R "${opencode-claude-auth}" "$out/plugins/opencode-claude-auth-multi"
    substituteInPlace "$out/plugins/opencode-claude-auth-multi/opencode-claude-auth.js" \
      --replace-fail "__OPENCODE_CLAUDE_AUTH_CLAUDE__" \
      "${claude-code}/bin/claude" \
      --replace-fail "__OPENCODE_CLAUDE_AUTH_CLAUDE2__" \
      "$out/bin/claude2"

    mkdir -p "$out/plugins/goal"
    cp plugins/goal/goal.lua "$out/plugins/goal/goal.lua"
    cp plugins/goal/loop.lua "$out/plugins/goal/loop.lua"
    substituteInPlace "$out/plugins/goal/goal.lua" \
      --replace-fail "__OPENCODE_GOAL_BIN__" "${opencode-goal}/bin/opencode-goal"
    substituteInPlace "$out/plugins/goal/loop.lua" \
      --replace-fail "__OPENCODE_GOAL_BIN__" "${opencode-goal}/bin/opencode-goal"

    runHook postInstall
  '';

  passthru.pluginsDir = "${placeholder "out"}/plugins";
  passthru.claude2 = "${placeholder "out"}/bin/claude2";
  passthru.opencode-claude-auth = opencode-claude-auth;
  passthru.opencode-goal = opencode-goal;
  passthru.goalLua = "${placeholder "out"}/plugins/goal/goal.lua";
  passthru.loopLua = "${placeholder "out"}/plugins/goal/loop.lua";

  meta = {
    description = "Personal OpenCode plugin collection";
    license = lib.licenses.mit;
    platforms = lib.platforms.all;
  };
}

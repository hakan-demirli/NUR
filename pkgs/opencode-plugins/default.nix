{
  lib,
  stdenvNoCC,
  bubblewrap,
  claude-code,
  fetchFromGitHub,
  fetchPnpmDeps,
  nodejs,
  pnpmConfigHook,
  pnpm_10,
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

    patches = [ ./plugins/claude-auth/patches/extra-homes.patch ];

    pnpmDeps = fetchPnpmDeps {
      inherit (finalAttrs) pname version src;
      fetcherVersion = 2;
      hash = "sha256-iRR8J3eX9qqVcKEKCFRX5ghvw3JxoalMO4V1G/GUGX4=";
    };

    nativeBuildInputs = [
      nodejs
      pnpmConfigHook
      pnpm_10
    ];

    buildPhase = ''
      runHook preBuild
      pnpm run build
      runHook postBuild
    '';

    installPhase = ''
      runHook preInstall

      mkdir -p "$out"
      cp -R dist "$out/dist"
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

  opencode-office = python3Packages.buildPythonApplication {
    pname = "opencode-office";
    version = "0.1.0";
    pyproject = true;

    src = ./plugins/office/runtime;

    build-system = [ python3Packages.setuptools ];

    meta = {
      description = "OpenCode worker/judge office daemon";
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
    cp -R plugins/office/opencode "$out/plugins/office"
    cp plugins/claude-auth/opencode/opencode-claude-auth-multi.js "$out/plugins/opencode-claude-auth-multi.js"
    substituteInPlace "$out/plugins/office/tui.ts" \
      --replace-fail "__OPENCODE_OFFICE_BIN__" "${opencode-office}/bin/opencode-office"
    cp -R "${opencode-claude-auth}" "$out/plugins/opencode-claude-auth-multi"
    substituteInPlace "$out/plugins/opencode-claude-auth-multi/dist/credentials.js" \
      --replace-fail 'process.env.OPENCODE_CLAUDE_AUTH_REFRESH_WRAPPER ?? "claude"' \
      'process.env.OPENCODE_CLAUDE_AUTH_REFRESH_WRAPPER ?? "'"$out"'/bin/claude2"'

    runHook postInstall
  '';

  passthru.pluginsDir = "${placeholder "out"}/plugins";
  passthru.claude2 = "${placeholder "out"}/bin/claude2";
  passthru.opencode-claude-auth = opencode-claude-auth;
  passthru.opencode-office = opencode-office;

  meta = {
    description = "Personal OpenCode plugin collection";
    license = lib.licenses.mit;
    platforms = lib.platforms.all;
  };
}

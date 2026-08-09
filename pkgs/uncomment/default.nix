{
  lib,
  fetchFromGitHub,
  fetchurl,
  git,
  nix-update-script,
  rustPlatform,
  versionCheckHook,
}:

let
  parserSources = fetchurl {
    url = "https://github.com/xberg-io/tree-sitter-language-pack/releases/download/v1.13.7/parser-sources-1.13.7.tar.zst";
    hash = "sha256-9tJTNlVDZzFTyd46lfN7Rhflv1YgZdn9+DT0ggNRrLI=";
  };
in
rustPlatform.buildRustPackage (finalAttrs: {
  pname = "uncomment";
  version = "3.5.2";

  src = fetchFromGitHub {
    owner = "Goldziher";
    repo = "uncomment";
    tag = "v${finalAttrs.version}";
    hash = "sha256-in/5ptO4WHRquDusDzg6cG2VAl1+4x1/ihohR6LRwrA=";
  };

  postPatch = ''
    substituteInPlace tests/init_integration_test.rs \
      --replace-fail 'fn get_binary_path() -> std::path::PathBuf {
        let build_output = Command::new("cargo")
            .args(["build", "--bin", "uncomment"])
            .output()
            .expect("Failed to build binary");

        if !build_output.status.success() {
            panic!(
                "Failed to build binary: {}",
                String::from_utf8_lossy(&build_output.stderr)
            );
        }

        let mut binary_path = std::env::current_dir().expect("Failed to get current directory");
        binary_path.push("target/debug/uncomment");
        binary_path
    }' 'fn get_binary_path() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_BIN_EXE_uncomment"))
    }'
  '';

  cargoHash = "sha256-nkwMWctuJ4NItPGmyAIKUAFcMIBSkEOoLoHCEZjIAX0=";

  nativeCheckInputs = [ git ];

  env = {
    TSLP_LANGUAGES = "all";
    TSLP_LINK_MODE = "static";
    TSLP_SOURCE_BUNDLE_URL = "file://${parserSources}";
  };

  doInstallCheck = true;
  nativeInstallCheckInputs = [ versionCheckHook ];

  passthru.updateScript = nix-update-script { };

  meta = {
    description = "CLI to remove comments from code using tree-sitter grammars";
    homepage = "https://github.com/Goldziher/uncomment";
    changelog = "https://github.com/Goldziher/uncomment/releases/tag/v${finalAttrs.version}";
    license = lib.licenses.mit;
    maintainers = with lib.maintainers; [ hakan-demirli ];
    mainProgram = "uncomment";
  };
})

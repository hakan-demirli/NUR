{
  pkgs,
}:
pkgs.rustPlatform.buildRustPackage {
  pname = "dap";
  version = "0.1.0";

  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;

  cargoBuildFlags = [
    "-p"
    "dap-cli"
  ];
  cargoTestFlags = [
    "-p"
    "dap-core"
    "-p"
    "dap-cli"
  ];

  meta = {
    description = "Diff-fenced style diff apply tool.";
    license = pkgs.lib.licenses.mit;
    maintainers = [ pkgs.lib.maintainers.hakan-demirli ];
    mainProgram = "dap";
  };
}

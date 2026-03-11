{
  pkgs,
  testName,
  cargoTestArgs,
}:
let
  ciLocalSrc = (pkgs.callPackage ../../../default.nix { }).src;
  cargoDeps = pkgs.rustPlatform.importCargoLock {
    lockFile = ../../../Cargo.lock;
  };
  cargoConfig = pkgs.writeText "cargo-config.toml" ''
    [source.crates-io]
    replace-with = 'nix-sources'

    [source.nix-sources]
    directory = '${cargoDeps}'
  '';
in
pkgs.testers.runNixOSTest {
  name = testName;

  nodes.machine =
    { pkgs, ... }:
    {
      virtualisation = {
        diskSize = 25600;
        memorySize = 4096;
        cores = 4;
      };

      environment = {
        systemPackages = with pkgs; [
          cargo
          rustc
          pkg-config
          openssl
          gcc
          git
        ];
        variables = {
          RUST_BACKTRACE = "1";
        };
      };
    };

  testScript = ''
    start_all()

    machine.succeed("cp -r ${ciLocalSrc} /build")
    machine.succeed("chmod -R +w /build")
    machine.succeed("mkdir -p /build/.cargo")
    machine.succeed("cp ${cargoConfig} /build/.cargo/config.toml")
    machine.succeed("cd /build && git init && git config user.email 'test@test.com' && git config user.name 'Test' && git config commit.gpgsign false && git add . && git commit --no-gpg-sign -m 'init'")
    machine.succeed("cd /build && cargo build --release --offline")
    machine.succeed("cd /build && cargo test --release --offline ${cargoTestArgs}")
  '';
}

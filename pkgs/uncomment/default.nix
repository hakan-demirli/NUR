{
  pkgs,
}:

pkgs.rustPlatform.buildRustPackage {
  pname = "uncomment";
  version = "2.11.0";

  src = pkgs.fetchFromGitHub {
    owner = "Goldziher";
    repo = "uncomment";
    rev = "fa0f93d90e70c0655da261fa269d5bbcea8c6b3c";
    hash = "sha256-nNjl1RC6A7CMt2ow/Z5K++iPDda3GmBcPlWphmTEiPA=";
  };

  cargoHash = "sha256-Yjm2yOpMDMCVtdQrzed1zG67z2ASxjgUX52AqQ8/pGA=";
  doCheck = false;
}

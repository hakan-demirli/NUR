{
  pkgs,
}:

pkgs.rustPlatform.buildRustPackage {
  pname = "uncomment";
  version = "3.0.2";

  src = pkgs.fetchFromGitHub {
    owner = "Goldziher";
    repo = "uncomment";
    rev = "3ee8bb325a17b63f16d682219ee5b8ab123da767";
    hash = "sha256-AzwOBDlAylF5gLiKOot1iBhgFbWpm8bKl+1rAGUs7Zg=";
  };

  cargoHash = "sha256-ccJ1jhNfqrZSEf42nZyzLIIO7aD74LCNHalePRCDgZQ=";
  doCheck = false;
}

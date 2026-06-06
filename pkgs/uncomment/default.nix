{
  pkgs,
}:

pkgs.rustPlatform.buildRustPackage {
  pname = "uncomment";
  version = "3.0.3";

  src = pkgs.fetchFromGitHub {
    owner = "Goldziher";
    repo = "uncomment";
    rev = "a2a898c556a503922017cffa9a4f96ca46ef098b";
    hash = "sha256-A76V1XA0aPsBGBMDVfjyOHLcf/6HOAK8AtXvqOVSw7E=";
  };

  cargoHash = "sha256-vwBIiwN2SMkIeEQBYc2BoC04mHxtfs4oKM6LKa1qdUA=";
  doCheck = false;
}

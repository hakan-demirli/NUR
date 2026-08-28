{
  lib,
  python3Packages,
}:

python3Packages.buildPythonApplication {
  pname = "tt";
  version = "0.1.0";
  pyproject = true;

  src = lib.fileset.toSource {
    root = ./..;
    fileset = lib.fileset.unions [
      ./../pyproject.toml
      ./../src
      ./../tests
    ];
  };

  build-system = [ python3Packages.hatchling ];

  dependencies = with python3Packages; [
    duckdb
    pytz # DuckDB needs it to hand TIMESTAMPTZ values back to Python
    rich
  ];

  nativeCheckInputs = [ python3Packages.pytestCheckHook ];

  meta = {
    description = "Inline terminal UI for task tracking";
    mainProgram = "tt";
  };
}

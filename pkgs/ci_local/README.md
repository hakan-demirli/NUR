# ci-local

Local CI daemon. Polls git repos, reads their `.github/workflows/*.yml`, runs everything with nix. No docker, no cloud.

## Requirements

- **nix** (hard dependency)
- git

## Quick start

The included `ci-local.toml` watches the current repo:

```toml
base_dir = "/tmp/ci-local"

[[repo]]
name = "ci-local"
source = "$PWD"
branch = "main"
```

```
ci-local start
```

## Config

Config values support `$VAR` and `${VAR}` environment variable expansion.

```toml
base_dir = "$HOME/.cache/ci-local"

[[repo]]
name = "my-project"
source = "$PWD"
branch = "main"

[[repo]]
name = "other"
source = "owner/repo"
```

`source` accepts local paths, `owner/repo` GitHub slugs, or full git URLs.

Jobs and steps come from the repo's `.github/workflows/*.yml` files. Every `run:` step must start with `nix`. `uses:` steps are skipped.

## Directory layout

```
~/.cache/ci-local/
  my-project-a1b2c3d4/
    summary.md
    00000_deadbeef/
      attempt-1/
        fast-checks/
          00_Run_Fast_Checks_stdout.log
          00_Run_Fast_Checks_stderr.log
        checks__unit-tests_/
          00_Run_Check_stdout.log
          00_Run_Check_stderr.log
        summary.json
      attempt-2/
        ...
    00001_cafebabe/
      attempt-1/
        ...
```

## Usage

```
ci-local start
ci-local status [--repo NAME]
ci-local cancel <sha> [--repo NAME]
ci-local cancel-all
ci-local retry --repo NAME <sha>
ci-local shutdown
```

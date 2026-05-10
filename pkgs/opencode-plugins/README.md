# opencode-plugins

Personal OpenCode plugin collection packaged by Nix.

The package installs one directory:

```text
$out/plugins
```

Home Manager can symlink that directory to:

```text
~/.config/opencode/plugins
```

## Source Layout

Each plugin owns its OpenCode entrypoint and all support code under one directory:

```text
plugins/<plugin-name>/
  opencode/  # files installed into $out/plugins
  runtime/   # optional packaged daemon/helper source
  scripts/   # optional packaged helper scripts
  patches/   # optional patches for fetched upstreams
```

Current plugins:

- `plugins/office/opencode/` - OpenCode TUI/server plugin for `/judge`.
- `plugins/office/opencode.ts` - Top-level loader installed as `$out/plugins/office.ts` so OpenCode discovers the plugin.
- `plugins/office/runtime/` - Python package for the office daemon and CLI.
- `plugins/claude-auth/opencode/` - Loader for patched Claude auth plugin.
- `plugins/claude-auth/patches/extra-homes.patch` - Upstreamable patch applied to `griffinmartin/opencode-claude-auth`.
- `plugins/claude-auth/scripts/claude2.sh` - Claude Code refresh wrapper installed as `$out/bin/claude2`.

The package output is intentionally flat for OpenCode:

```text
$out/plugins/office/
$out/plugins/office.ts
$out/plugins/opencode-claude-auth-multi.js
$out/plugins/opencode-claude-auth-multi/
$out/bin/claude2
```

## Claude Extra Homes

The patched Claude auth plugin supports extra Linux Claude Code homes via:

```sh
OPENCODE_CLAUDE_AUTH_EXTRA_HOMES=/path/to/work-home:/path/to/other-home
```

Each home is expected to contain:

```text
.claude/.credentials.json
```

Authenticate an extra account with:

```sh
HOME=/path/to/work-home claude
```

Then switch inside OpenCode with:

```sh
opencode auth login
```

The patched auth plugin uses `bin/claude2` by default for CLI fallback refresh. The plugin passes the selected account home in `OPENCODE_CLAUDE_AUTH_ACCOUNT_HOME`; `claude2` bind-mounts that profile home over the real home path before launching the packaged Claude Code binary.

Override the refresh wrapper if needed with:

```sh
OPENCODE_CLAUDE_AUTH_REFRESH_WRAPPER=/path/to/wrapper
```

## Adding Plugins

Add new plugins under `plugins/<plugin-name>/`. Keep support code inside that plugin's directory. If a plugin needs helper binaries or daemons, package them from its own `runtime/` or `scripts/` directory and substitute store paths during `installPhase`; do not hardcode mutable host paths.

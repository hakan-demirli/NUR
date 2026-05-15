# opencode-plugins

Personal opencode plugins, packaged by Nix. Home Manager symlinks `$out/plugins` to `~/.config/opencode/plugins`.

- `office/` — `/judge` slash command driving a Python daemon; plugin source in `office/opencode/`, daemon in `office/runtime/`.
- `claude-auth/` — patched Claude auth with extra-home support and 429-cooldown auto-switch across accounts; patch in `claude-auth/patches/`, `claude2` refresh wrapper in `claude-auth/scripts/`.

Extra Claude homes: `OPENCODE_CLAUDE_AUTH_EXTRA_HOMES=/h1:/h2` (each holds `.claude/.credentials.json`). Auto-switch order in `${XDG_CONFIG_HOME:-~/.config}/opencode-plugins/claude-auth/config.json`: `{"autoSwitch":{"order":["home:~/path","file"]}}`. Cooldowns are in-memory; restart `opencode-serve` to reload config.

New plugins go under `plugins/<name>/` with their own `runtime/`, `scripts/`, or `patches/`. Substitute store paths in `installPhase`; never hardcode host paths.

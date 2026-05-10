// Server-side facet of the office plugin. Intentionally a no-op: all real
// logic lives in `./tui.ts` and is registered by the TUI plugin runtime.
//
// We have to expose a server entry because the path-plugin loader resolves
// per-kind entrypoints from this package's `exports` map. Without it, the
// server-side `loadExternal({kind: "server"})` walk would fail to find an
// entry and surface a hard error.
export default {
  id: "office",
  server: async () => ({}),
}

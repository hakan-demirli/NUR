import path from "node:path"
import { createHash } from "node:crypto"
import { homedir } from "node:os"
import { readdir, readFile } from "node:fs/promises"
import type { TuiPlugin, TuiPluginApi, TuiCommand, TuiDialogSelectOption } from "@opencode-ai/plugin/tui"

const OPENCODE_OFFICE_BIN = process.env.OPENCODE_OFFICE_BIN ?? "__OPENCODE_OFFICE_BIN__"
const OFFICE_CACHE_ROOT = path.join(homedir(), ".cache", "opencode_office_py")

// ----- daemon socket helpers --------------------------------------------------

function runtimeKey(directory: string, sessionID?: string): string {
  const resolved = path.resolve(directory)
  return sessionID ? `${resolved}\0${sessionID}` : resolved
}

function projectSocketPath(directory: string, sessionID?: string): string {
  const hash = createHash("md5").update(runtimeKey(directory, sessionID)).digest("hex")
  return path.join(OFFICE_CACHE_ROOT, hash, "daemon.sock")
}

async function runtimeSessionIDFor(directory: string, sessionID: string): Promise<string> {
  const resolved = path.resolve(directory)
  try {
    const entries = await readdir(OFFICE_CACHE_ROOT, { withFileTypes: true })
    for (const entry of entries) {
      if (!entry.isDirectory()) continue
      let state: any
      try {
        state = JSON.parse(await readFile(path.join(OFFICE_CACHE_ROOT, entry.name, "state.json"), "utf8"))
      } catch {
        continue
      }
      if (path.resolve(String(state?.directory ?? "")) !== resolved) continue
      if (state?.worker_session_id === sessionID || state?.judge_session_id === sessionID || state?.session_id === sessionID) {
        return typeof state?.worker_session_id === "string" && state.worker_session_id
          ? state.worker_session_id
          : sessionID
      }
    }
  } catch {
    // No runtime cache yet; the current session will become the daemon key.
  }
  return sessionID
}

function currentSessionID(api: TuiPluginApi): string | undefined {
  const route = api.route.current
  if (route.name !== "session") return
  const sessionID = (route.params as any)?.sessionID
  return typeof sessionID === "string" && sessionID ? sessionID : undefined
}

function headerValue(headers: unknown, name: string): string | undefined {
  const lower = name.toLowerCase()
  if (!headers) return
  if (typeof Headers !== "undefined" && headers instanceof Headers) return headers.get(name) ?? undefined
  if (Array.isArray(headers)) {
    const match = headers.find((entry) => String(entry?.[0] ?? "").toLowerCase() === lower)
    return match?.[1] === undefined ? undefined : String(match[1])
  }
  if (typeof headers === "object") {
    for (const [key, value] of Object.entries(headers)) {
      if (key.toLowerCase() !== lower || value === undefined || value === null) continue
      return Array.isArray(value) ? String(value[0]) : String(value)
    }
  }
}

function basicCredentials(header?: string): { username: string; password: string } | undefined {
  const match = /^basic\s+(.+)$/i.exec(header ?? "")
  if (!match) return
  try {
    const decoded = Buffer.from(match[1], "base64").toString("utf8")
    const index = decoded.indexOf(":")
    if (index < 0) return
    return { username: decoded.slice(0, index), password: decoded.slice(index + 1) }
  } catch {
    return
  }
}

function serverConnection(api: TuiPluginApi) {
  // OpencodeClient (from @opencode-ai/sdk/v2) wraps the underlying hey-api
  // client on `.client`; that inner client exposes getConfig().
  const inner: any = (api.client as any)?.client
  const cfg = typeof inner?.getConfig === "function" ? inner.getConfig() : {}
  const auth = basicCredentials(headerValue(cfg?.headers, "authorization"))
  return {
    baseUrl: typeof cfg?.baseUrl === "string" ? cfg.baseUrl : undefined,
    password: process.env["OPENCODE_SERVER_PASSWORD"] ?? auth?.password,
    username: process.env["OPENCODE_SERVER_USERNAME"] ?? auth?.username ?? "opencode",
  }
}

function externalServeRequired(sessionID: string, baseUrl?: string) {
  return {
    kind: "alert" as const,
    title: "Judge requires external serve",
    message: [
      "opencode is running in internal-serve mode; the office daemon",
      "is a separate process and cannot reach this TUI's in-process API.",
      "Restart this session with an external auto-assigned port:",
      "",
      `    opencode -s ${sessionID} --port 0`,
      "",
      "The OS resolves --port 0 to a real free port, so multiple sessions",
      "do not collide. The plugin passes that resolved URL to this",
      "session-specific daemon.",
      "",
      `Detected baseUrl: ${baseUrl ?? "(none)"}`,
    ].join("\n"),
  }
}

async function daemonRequest<T = any>(
  directory: string,
  sessionID: string | undefined,
  method: "GET" | "POST",
  endpoint: string,
  body?: unknown,
): Promise<T> {
  const sock = projectSocketPath(directory, sessionID)
  const init: { method: string; unix: string; headers?: Record<string, string>; body?: string } = {
    method,
    unix: sock,
  }
  if (body !== undefined) {
    init.headers = { "Content-Type": "application/json" }
    init.body = JSON.stringify(body)
  }
  // Bun's fetch supports `unix:` to talk to a Unix domain socket.
  const response = await fetch(`http://localhost${endpoint}`, init as any)
  const raw = await response.text()
  if (!response.ok) {
    throw new Error(`daemon ${method} ${endpoint} -> ${response.status}: ${raw || "<empty>"}`)
  }
  if (!raw.length) return {} as T
  try {
    return JSON.parse(raw) as T
  } catch {
    throw new Error(`daemon ${method} ${endpoint} returned non-JSON: ${raw.slice(0, 200)}`)
  }
}

async function daemonHealthy(directory: string, sessionID: string | undefined): Promise<boolean> {
  try {
    await daemonRequest(directory, sessionID, "GET", "/health")
    return true
  } catch {
    return false
  }
}

async function startDaemon(
  directory: string,
  sessionID: string,
  connection?: { baseUrl?: string; password?: string; username?: string },
): Promise<void> {
  // Run the packaged office CLI and wait for it to
  // exit. The CLI returns once the socket is reachable.
  const args = [
    OPENCODE_OFFICE_BIN,
    "--directory",
    directory,
    "--session-id",
    sessionID,
    ...(connection?.baseUrl ? ["--base-url", connection.baseUrl] : []),
    ...(connection?.password ? ["--password", connection.password] : []),
    ...(connection?.username ? ["--username", connection.username] : []),
    "daemon-start",
  ]
  const proc = Bun.spawn(args, {
    env: process.env,
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
  })
  const stdout = await new Response(proc.stdout).text()
  const stderr = await new Response(proc.stderr).text()
  const code = await proc.exited
  if (code !== 0) {
    throw new Error((stderr || stdout || `daemon-start exited ${code}`).trim())
  }
}

async function ensureDaemon(
  directory: string,
  sessionID: string,
  connection?: { baseUrl?: string; password?: string; username?: string },
): Promise<void> {
  if (await daemonHealthy(directory, sessionID)) return
  await startDaemon(directory, sessionID, connection)
  // Tight retry loop to give the spawned daemon a moment to bind the socket.
  // ``daemon-start`` already waits for /health to come up, so this is
  // belt-and-braces.
  for (let i = 0; i < 20; i++) {
    if (await daemonHealthy(directory, sessionID)) return
    await new Promise((r) => setTimeout(r, 100))
  }
  throw new Error(`office daemon did not come up at ${projectSocketPath(directory, sessionID)}`)
}

async function stopDaemonHard(directory: string, sessionID: string): Promise<string> {
  // Hard kill, no negotiation. Use the CLI helper since it walks the pid
  // file and kills descendants even if the daemon is already half-dead.
  const proc = Bun.spawn([OPENCODE_OFFICE_BIN, "--directory", directory, "--session-id", sessionID, "daemon-stop"], {
    env: process.env,
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
  })
  const stdout = await new Response(proc.stdout).text()
  await proc.exited
  return stdout.trim()
}

// ----- formatting -------------------------------------------------------------

function formatTokens(slot: any): string {
  if (!slot) return "(no session)"
  const id = slot.session_id ?? "(unset)"
  const tokens = slot.tokens
  const ctx = slot.context_limit ?? 0
  if (!tokens) return `${id} | tokens=unknown`
  const ratio = ctx ? ` (${((tokens.total / ctx) * 100).toFixed(1)}%)` : ""
  return `${id} | tokens=${tokens.total.toLocaleString()} / ctx=${ctx.toLocaleString()}${ratio}`
}

function formatStatus(payload: any): string {
  let mode: string
  if (!payload?.enabled) mode = "OFF"
  else if (payload.paused) mode = `PAUSED (queued: ${payload.queued_events ?? 0})`
  else mode = "ON"
  return [
    `judge: ${mode}`,
    `worker:  ${formatTokens(payload?.worker)}`,
    `judge :  ${formatTokens(payload?.judge)}`,
    `last nudge: ${
      payload?.trigger?.last_judge_nudge_at
        ? new Date(payload.trigger.last_judge_nudge_at).toLocaleString()
        : "never"
    }`,
  ].join("\n")
}

function formatProcesses(payload: any): string {
  const lines = [
    `daemon pid: ${payload?.daemon_pid ?? "(none)"}`,
    `serve pid:  ${payload?.serve_pid ?? "(none)"}`,
    "",
  ]
  for (const proc of payload?.processes ?? []) {
    lines.push(`  ${String(proc.pid).padStart(7)}  ppid=${String(proc.ppid).padStart(7)}  ${proc.cmd}`)
  }
  if ((payload?.processes ?? []).length === 0) lines.push("  (no tracked processes)")
  return lines.join("\n")
}

function formatLogs(payload: any): string {
  const lines: string[] = []
  for (const entry of (payload?.daemon ?? []).slice(-30)) {
    try {
      const parsed = JSON.parse(entry)
      const ts = parsed.ts ?? ""
      const evt = parsed.event ?? "?"
      const rest = { ...parsed }
      delete rest.ts
      delete rest.event
      lines.push(`${ts}  ${evt}  ${JSON.stringify(rest)}`)
    } catch {
      lines.push(entry)
    }
  }
  if (!lines.length) lines.push("(no daemon events yet)")
  return lines.join("\n")
}

// ----- TUI plugin -------------------------------------------------------------

type ActionResult =
  | { kind: "alert"; title: string; message: string }
  | { kind: "toast"; variant: "info" | "success" | "warning" | "error"; message: string }
  | { kind: "noop" }

type Subaction = {
  title: string
  value: string
  description?: string
  destructive?: boolean
  run: (ctx: { directory: string; api: TuiPluginApi }) => Promise<ActionResult>
}

async function sessionDaemon(
  directory: string,
  api: TuiPluginApi,
  options: { start?: boolean } = {},
): Promise<ActionResult | { sessionID: string }> {
  const sessionID = currentSessionID(api)
  if (!sessionID) {
    return {
      kind: "toast",
      variant: "error",
      message: "open a worker session first",
    }
  }
  const daemonSessionID = await runtimeSessionIDFor(directory, sessionID)
  if (await daemonHealthy(directory, daemonSessionID)) return { sessionID: daemonSessionID }

  if (options.start === false) {
    return {
      kind: "toast",
      variant: "warning",
      message: "judge daemon is not running; run /judge on first",
    }
  }

  const connection = serverConnection(api)
  if (!connection.baseUrl || /opencode\.internal/i.test(connection.baseUrl)) {
    return externalServeRequired(sessionID, connection.baseUrl)
  }
  await ensureDaemon(directory, daemonSessionID, connection)
  return { sessionID: daemonSessionID }
}

const SUBACTIONS: Subaction[] = [
  {
    title: "On",
    value: "judge.on",
    description: "Fork the judge from the current session and start supervision",
    async run({ directory, api }) {
      const sessionID = currentSessionID(api)
      if (!sessionID) {
        return {
          kind: "toast",
          variant: "error",
          message: "open a worker session first, then run /judge on",
        }
      }
      const connection = serverConnection(api)

      // Detect "internal" TUI mode (the default `opencode` invocation) where
      // the SDK is wired to a fake URL (`http://opencode.internal`) routed
      // through an in-process worker fetch. The daemon is a separate process
      // and cannot resolve that URL, so judge mode is unusable in this mode.
      if (!connection.baseUrl || /opencode\.internal/i.test(connection.baseUrl)) {
        return externalServeRequired(sessionID, connection.baseUrl)
      }

      await ensureDaemon(directory, sessionID, connection)
      const payload = await daemonRequest<any>(directory, sessionID, "POST", "/judge/on", {
        worker_session_id: sessionID,
        base_url: connection.baseUrl,
        password: connection.password,
        username: connection.username,
      })
      const summary = await daemonRequest<any>(directory, sessionID, "GET", "/summary")
      const message = [
        "judge enabled",
        `worker: ${payload?.state?.worker_session_id ?? sessionID}`,
        `judge:  ${payload?.state?.judge_session_id ?? "(unknown)"}`,
        "",
        formatStatus(summary),
      ].join("\n")
      return { kind: "alert", title: "Judge", message }
    },
  },
  {
    title: "Off",
    value: "judge.off",
    description: "Disable supervision (sessions stay alive)",
    async run({ directory, api }) {
      const daemon = await sessionDaemon(directory, api, { start: false })
      if ("kind" in daemon) return daemon
      await daemonRequest(directory, daemon.sessionID, "POST", "/judge/off", {})
      return { kind: "toast", variant: "success", message: "judge supervision disabled" }
    },
  },
  {
    title: "Status",
    value: "judge.status",
    description: "Show current judge mode + token usage for both sessions",
    async run({ directory, api }) {
      const daemon = await sessionDaemon(directory, api, { start: false })
      if ("kind" in daemon) return daemon
      const summary = await daemonRequest<any>(directory, daemon.sessionID, "GET", "/summary")
      return { kind: "alert", title: "Judge status", message: formatStatus(summary) }
    },
  },
  {
    title: "Pause",
    value: "judge.pause",
    description: "Stop nudging the judge; queue events for later replay",
    async run({ directory, api }) {
      const daemon = await sessionDaemon(directory, api, { start: false })
      if ("kind" in daemon) return daemon
      const payload = await daemonRequest<any>(directory, daemon.sessionID, "POST", "/judge/pause", {})
      const queued = payload?.state?.pending_events?.length ?? 0
      return { kind: "toast", variant: "info", message: `judge paused (queued: ${queued})` }
    },
  },
  {
    title: "Resume",
    value: "judge.resume",
    description: "Replay any queued events and resume nudging",
    async run({ directory, api }) {
      const daemon = await sessionDaemon(directory, api, { start: false })
      if ("kind" in daemon) return daemon
      const payload = await daemonRequest<any>(directory, daemon.sessionID, "POST", "/judge/resume", {})
      const replayed = (payload?.replayed ?? []) as Array<{ reasons?: string[] }>
      if (!replayed.length) {
        return { kind: "toast", variant: "info", message: "judge resumed (nothing queued)" }
      }
      const lines = replayed.map((entry, idx) => {
        const reasons = (entry.reasons ?? []).filter(Boolean).join(", ") || "queued review"
        return `  [${idx + 1}] ${reasons}`
      })
      return {
        kind: "alert",
        title: "Judge resumed",
        message: [`replayed ${replayed.length} event(s)`, ...lines].join("\n"),
      }
    },
  },
  {
    title: "Poke",
    value: "judge.poke",
    description: "Force a judge review now (bypasses cooldowns)",
    async run({ directory, api }) {
      const daemon = await sessionDaemon(directory, api, { start: false })
      if ("kind" in daemon) return daemon
      await daemonRequest(directory, daemon.sessionID, "POST", "/judge/poke", {})
      return { kind: "toast", variant: "success", message: "judge nudged" }
    },
  },
  {
    title: "Logs",
    value: "judge.logs",
    description: "Tail recent structured daemon events",
    async run({ directory, api }) {
      const daemon = await sessionDaemon(directory, api, { start: false })
      if ("kind" in daemon) return daemon
      const payload = await daemonRequest<any>(directory, daemon.sessionID, "GET", "/logs?lines=200")
      return { kind: "alert", title: "Judge logs", message: formatLogs(payload) }
    },
  },
  {
    title: "PS",
    value: "judge.ps",
    description: "Show daemon + opencode-serve + descendants",
    async run({ directory, api }) {
      const daemon = await sessionDaemon(directory, api, { start: false })
      if ("kind" in daemon) return daemon
      const payload = await daemonRequest<any>(directory, daemon.sessionID, "GET", "/processes")
      return { kind: "alert", title: "Judge processes", message: formatProcesses(payload) }
    },
  },
  {
    title: "Switch to worker",
    value: "judge.worker",
    description: "Switch the TUI view to the worker session",
    async run({ directory, api }) {
      const daemon = await sessionDaemon(directory, api, { start: false })
      if ("kind" in daemon) return daemon
      const status = await daemonRequest<any>(directory, daemon.sessionID, "GET", "/status")
      const target = status?.state?.worker_session_id
      if (!target) {
        return {
          kind: "toast",
          variant: "error",
          message: "no worker session registered; run /judge on first",
        }
      }
      // The v2 SDK expects flat parameters, not { body: ... }. Passing the
      // wrong shape silently sends an empty body and the route 400s.
      await api.client.tui.selectSession({ sessionID: target })
      return { kind: "noop" }
    },
  },
  {
    title: "Switch to judge",
    value: "judge.judge",
    description: "Switch the TUI view to the judge session",
    async run({ directory, api }) {
      const daemon = await sessionDaemon(directory, api, { start: false })
      if ("kind" in daemon) return daemon
      const status = await daemonRequest<any>(directory, daemon.sessionID, "GET", "/status")
      const target = status?.state?.judge_session_id
      if (!target) {
        return {
          kind: "toast",
          variant: "error",
          message: "no judge session registered; run /judge on first",
        }
      }
      await api.client.tui.selectSession({ sessionID: target })
      return { kind: "noop" }
    },
  },
  {
    title: "Kill",
    value: "judge.kill",
    description: "Abort judge, hard-kill daemon + owned serve. No negotiation.",
    destructive: true,
    async run({ directory, api }) {
      const sessionID = currentSessionID(api)
      if (!sessionID) {
        return { kind: "toast", variant: "error", message: "open a worker session first" }
      }
      const daemonSessionID = await runtimeSessionIDFor(directory, sessionID)
      const out = await stopDaemonHard(directory, daemonSessionID)
      return { kind: "alert", title: "Judge killed", message: out || "(daemon already stopped)" }
    },
  },
]

function dispatchResult(api: TuiPluginApi, result: ActionResult): void {
  if (result.kind === "noop") {
    api.ui.dialog.clear()
    return
  }
  if (result.kind === "toast") {
    api.ui.dialog.clear()
    api.ui.toast({ variant: result.variant, message: result.message })
    return
  }
  const { DialogAlert } = api.ui
  api.ui.dialog.replace(() => DialogAlert({ title: result.title, message: result.message }))
}

function showSubactionDialog(api: TuiPluginApi, directory: string): void {
  const options: TuiDialogSelectOption<string>[] = SUBACTIONS.map((sub) => ({
    title: sub.title,
    value: sub.value,
    description: sub.description,
    category: sub.destructive ? "Danger" : "Judge",
  }))

  const { DialogSelect } = api.ui
  api.ui.dialog.replace(() =>
    DialogSelect<string>({
      title: "Judge",
      placeholder: "select an action",
      options,
      onSelect: (option) => {
        const sub = SUBACTIONS.find((s) => s.value === option.value)
        if (!sub) {
          api.ui.dialog.clear()
          return
        }
        sub
          .run({ directory, api })
          .then((result) => dispatchResult(api, result))
          .catch((err) => {
            const message = err instanceof Error ? err.message : String(err)
            api.ui.dialog.clear()
            api.ui.toast({ variant: "error", message: `judge ${sub.value} failed: ${message}` })
          })
      },
    }),
  )
}

const tui: TuiPlugin = async (api: TuiPluginApi) => {
  // Register a single `/judge` slash. Selecting it opens a sub-action
  // dialog; nothing here ever round-trips the LLM.
  api.command.register((): TuiCommand[] => [
    {
      title: "Judge controls",
      value: "office.judge",
      description: "Open the judge daemon control menu",
      category: "Judge",
      slash: { name: "judge" },
      onSelect: () => {
        const directory = api.state.path.directory
        if (!directory) {
          api.ui.toast({ variant: "error", message: "no project directory available" })
          return
        }
        showSubactionDialog(api, directory)
      },
    },
  ])
}

// TUI facet: default export carries `tui` only. The matching `server` facet
// lives in ./server.ts and is exposed via this package's `exports` map. The
// per-kind entrypoint resolver in opencode/src/plugin/shared.ts:103 will
// import this file when kind=="tui" and ./server.ts when kind=="server", so
// we never trip the "must default export either server() or tui(), not both"
// guard at shared.ts:294.
export default {
  id: "office",
  tui,
}

import path from "node:path"
import { createHash } from "node:crypto"
import { homedir } from "node:os"
import type { TuiPlugin, TuiPluginApi, TuiCommand, TuiDialogSelectOption } from "@opencode-ai/plugin/tui"

const OPENCODE_OFFICE_BIN = process.env.OPENCODE_OFFICE_BIN ?? "__OPENCODE_OFFICE_BIN__"

// ----- daemon socket helpers --------------------------------------------------

function projectSocketPath(directory: string): string {
  const hash = createHash("md5").update(directory).digest("hex")
  return path.join(homedir(), ".cache", "opencode_office_py", hash, "daemon.sock")
}

async function daemonRequest<T = any>(
  directory: string,
  method: "GET" | "POST",
  endpoint: string,
  body?: unknown,
): Promise<T> {
  const sock = projectSocketPath(directory)
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

async function daemonHealthy(directory: string): Promise<boolean> {
  try {
    await daemonRequest(directory, "GET", "/health")
    return true
  } catch {
    return false
  }
}

async function startDaemon(directory: string): Promise<void> {
  // Run the packaged office CLI and wait for it to
  // exit. The CLI returns once the socket is reachable.
  const proc = Bun.spawn([OPENCODE_OFFICE_BIN, "--directory", directory, "daemon-start"], {
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

async function ensureDaemon(directory: string): Promise<void> {
  if (await daemonHealthy(directory)) return
  await startDaemon(directory)
  // Tight retry loop to give the spawned daemon a moment to bind the socket.
  // ``daemon-start`` already waits for /health to come up, so this is
  // belt-and-braces.
  for (let i = 0; i < 20; i++) {
    if (await daemonHealthy(directory)) return
    await new Promise((r) => setTimeout(r, 100))
  }
  throw new Error(`office daemon did not come up at ${projectSocketPath(directory)}`)
}

async function stopDaemonHard(directory: string): Promise<string> {
  // Hard kill, no negotiation. Use the CLI helper since it walks the pid
  // file and kills descendants even if the daemon is already half-dead.
  const proc = Bun.spawn([OPENCODE_OFFICE_BIN, "--directory", directory, "daemon-stop"], {
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

const SUBACTIONS: Subaction[] = [
  {
    title: "On",
    value: "judge.on",
    description: "Fork the judge from the current session and start supervision",
    async run({ directory, api }) {
      const route = api.route.current
      if (route.name !== "session") {
        return {
          kind: "toast",
          variant: "error",
          message: "open a worker session first, then run /judge on",
        }
      }
      const sessionID = (route.params as any)?.sessionID as string | undefined
      if (!sessionID) {
        return { kind: "toast", variant: "error", message: "could not resolve current session id" }
      }
      await ensureDaemon(directory)
      // The session id only exists in the TUI's own opencode serve. Hand the
      // daemon that serve's URL + auth so it can fork against the right db.
      // OpencodeClient (from @opencode-ai/sdk/v2) wraps the underlying
      // hey-api client on its `.client` field; that one exposes getConfig().
      const inner: any = (api.client as any)?.client
      const cfg = typeof inner?.getConfig === "function" ? inner.getConfig() : {}
      const baseUrl = typeof cfg?.baseUrl === "string" ? cfg.baseUrl : undefined
      const password = process.env["OPENCODE_SERVER_PASSWORD"]
      const username = process.env["OPENCODE_SERVER_USERNAME"] ?? "opencode"

      // Detect "internal" TUI mode (the default `opencode` invocation) where
      // the SDK is wired to a fake URL (`http://opencode.internal`) routed
      // through an in-process worker fetch. The daemon is a separate process
      // and cannot resolve that URL, so judge mode is unusable in this mode.
      // Tell the user how to switch to a real-port serve.
      //
      // OPENCODE_SERVER_PASSWORD must be unset, otherwise the TUI cannot
      // authenticate against its own serve (the default opencode invocation
      // does not wire ServerAuth.headers into the SDK client; only `opencode
      // tui attach` does). Easier to just run unauthenticated on localhost.
      if (!baseUrl || /opencode\.internal/i.test(baseUrl)) {
        return {
          kind: "alert",
          title: "Judge requires external serve",
          message: [
            "opencode is running in internal-serve mode; the office daemon",
            "cannot reach its session API. Restart with a real port and no",
            "password (the default opencode invocation does not send auth",
            "headers, so password-protected serves break self-fetch):",
            "",
            "    unset OPENCODE_SERVER_PASSWORD",
            "    opencode --port 4096",
            "",
            "(any free port works). Then /judge On will fork against the",
            "session you're viewing.",
            "",
            `Detected baseUrl: ${baseUrl ?? "(none)"}`,
          ].join("\n"),
        }
      }
      // Password is optional: if OPENCODE_SERVER_PASSWORD is unset the serve
      // runs unauthenticated (it logs a warning but accepts requests). The
      // daemon's OpenCodeClient will simply skip the auth header.
      const payload = await daemonRequest<any>(directory, "POST", "/judge/on", {
        worker_session_id: sessionID,
        base_url: baseUrl,
        password,
        username,
      })
      const summary = await daemonRequest<any>(directory, "GET", "/summary")
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
    async run({ directory }) {
      await ensureDaemon(directory)
      await daemonRequest(directory, "POST", "/judge/off", {})
      return { kind: "toast", variant: "success", message: "judge supervision disabled" }
    },
  },
  {
    title: "Status",
    value: "judge.status",
    description: "Show current judge mode + token usage for both sessions",
    async run({ directory }) {
      await ensureDaemon(directory)
      const summary = await daemonRequest<any>(directory, "GET", "/summary")
      return { kind: "alert", title: "Judge status", message: formatStatus(summary) }
    },
  },
  {
    title: "Pause",
    value: "judge.pause",
    description: "Stop nudging the judge; queue events for later replay",
    async run({ directory }) {
      await ensureDaemon(directory)
      const payload = await daemonRequest<any>(directory, "POST", "/judge/pause", {})
      const queued = payload?.state?.pending_events?.length ?? 0
      return { kind: "toast", variant: "info", message: `judge paused (queued: ${queued})` }
    },
  },
  {
    title: "Resume",
    value: "judge.resume",
    description: "Replay any queued events and resume nudging",
    async run({ directory }) {
      await ensureDaemon(directory)
      const payload = await daemonRequest<any>(directory, "POST", "/judge/resume", {})
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
    async run({ directory }) {
      await ensureDaemon(directory)
      await daemonRequest(directory, "POST", "/judge/poke", {})
      return { kind: "toast", variant: "success", message: "judge nudged" }
    },
  },
  {
    title: "Logs",
    value: "judge.logs",
    description: "Tail recent structured daemon events",
    async run({ directory }) {
      await ensureDaemon(directory)
      const payload = await daemonRequest<any>(directory, "GET", "/logs?lines=200")
      return { kind: "alert", title: "Judge logs", message: formatLogs(payload) }
    },
  },
  {
    title: "PS",
    value: "judge.ps",
    description: "Show daemon + opencode-serve + descendants",
    async run({ directory }) {
      await ensureDaemon(directory)
      const payload = await daemonRequest<any>(directory, "GET", "/processes")
      return { kind: "alert", title: "Judge processes", message: formatProcesses(payload) }
    },
  },
  {
    title: "Switch to worker",
    value: "judge.worker",
    description: "Switch the TUI view to the worker session",
    async run({ directory, api }) {
      await ensureDaemon(directory)
      const status = await daemonRequest<any>(directory, "GET", "/status")
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
      await ensureDaemon(directory)
      const status = await daemonRequest<any>(directory, "GET", "/status")
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
    description: "Hard-kill daemon + opencode-serve + descendants. No negotiation.",
    destructive: true,
    async run({ directory }) {
      try {
        await daemonRequest(directory, "POST", "/stop", {})
      } catch {
        // Daemon may already be down; force-cleanup handles it.
      }
      const out = await stopDaemonHard(directory)
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

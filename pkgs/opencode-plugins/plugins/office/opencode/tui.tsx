import path from "node:path";
import { homedir } from "node:os";
import { createMemo, createSignal, onCleanup, onMount, Show } from "solid-js";
import type {
  TuiPlugin,
  TuiPluginApi,
  TuiCommand,
  TuiDialogSelectOption,
} from "@opencode-ai/plugin/tui";

const OPENCODE_OFFICE_BIN =
  process.env.OPENCODE_OFFICE_BIN ?? "__OPENCODE_OFFICE_BIN__";
const GLOBAL_SOCKET = path.join(
  homedir(),
  ".cache",
  "opencode_office_py",
  "daemon.sock",
);

function currentSessionID(api: TuiPluginApi): string | undefined {
  const route = api.route.current;
  if (route.name !== "session") return;
  const sessionID = (route.params as any)?.sessionID;
  return typeof sessionID === "string" && sessionID ? sessionID : undefined;
}

async function daemonRequest<T = any>(
  method: "GET" | "POST",
  endpoint: string,
  body?: unknown,
): Promise<T> {
  const init: {
    method: string;
    unix: string;
    headers?: Record<string, string>;
    body?: string;
  } = {
    method,
    unix: GLOBAL_SOCKET,
  };
  if (body !== undefined) {
    init.headers = { "Content-Type": "application/json" };
    init.body = JSON.stringify(body);
  }
  const response = await fetch(`http://localhost${endpoint}`, init as any);
  const raw = await response.text();
  if (!response.ok) {
    throw new Error(
      `daemon ${method} ${endpoint} -> ${response.status}: ${raw || "<empty>"}`,
    );
  }
  if (!raw.length) return {} as T;
  try {
    return JSON.parse(raw) as T;
  } catch {
    throw new Error(
      `daemon ${method} ${endpoint} returned non-JSON: ${raw.slice(0, 200)}`,
    );
  }
}

async function daemonHealthy(): Promise<boolean> {
  try {
    await daemonRequest("GET", "/health");
    return true;
  } catch {
    return false;
  }
}

async function startDaemon(): Promise<void> {
  const proc = Bun.spawn([OPENCODE_OFFICE_BIN, "daemon-start"], {
    env: process.env,
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
  });
  const stdout = await new Response(proc.stdout).text();
  const stderr = await new Response(proc.stderr).text();
  const code = await proc.exited;
  if (code !== 0) {
    throw new Error((stderr || stdout || `daemon-start exited ${code}`).trim());
  }
}

async function ensureDaemon(): Promise<void> {
  if (await daemonHealthy()) return;
  await startDaemon();
  for (let i = 0; i < 20; i++) {
    if (await daemonHealthy()) return;
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error(`office daemon did not come up at ${GLOBAL_SOCKET}`);
}

function formatTokens(slot: any): string {
  if (!slot) return "(no session)";
  const id = slot.session_id ?? "(unset)";
  const tokens = slot.tokens;
  const ctx = slot.context_limit ?? 0;
  if (!tokens) return `${id} | tokens=unknown`;
  const ratio = ctx ? ` (${((tokens.total / ctx) * 100).toFixed(1)}%)` : "";
  return `${id} | tokens=${tokens.total.toLocaleString()} / ctx=${ctx.toLocaleString()}${ratio}`;
}

function formatSlot(slot: any): string {
  if (!slot) return "(no slot)";
  let mode: string;
  if (!slot.enabled) mode = "OFF";
  else if (slot.paused) mode = `PAUSED (queued: ${slot.queued_events ?? 0})`;
  else mode = "ON";
  return [
    `directory: ${slot.directory ?? "(unknown)"}`,
    `judge:  ${mode}`,
    `worker: ${formatTokens(slot.worker)}`,
    `judge : ${formatTokens(slot.judge)}`,
    `last nudge: ${
      slot.trigger?.last_judge_nudge_at
        ? new Date(slot.trigger.last_judge_nudge_at).toLocaleString()
        : "never"
    }`,
  ].join("\n");
}

function formatSummary(payload: any): string {
  const slots = (payload?.slots ?? []) as any[];
  if (!slots.length) return "(no slots)";
  return slots.map(formatSlot).join("\n\n");
}

function formatLogs(payload: any): string {
  const lines: string[] = [];
  for (const entry of (payload?.daemon ?? []).slice(-30)) {
    try {
      const parsed = JSON.parse(entry);
      const ts = parsed.ts ?? "";
      const evt = parsed.event ?? "?";
      const rest = { ...parsed };
      delete rest.ts;
      delete rest.event;
      lines.push(`${ts}  ${evt}  ${JSON.stringify(rest)}`);
    } catch {
      lines.push(entry);
    }
  }
  if (!lines.length) lines.push("(no daemon events yet)");
  return lines.join("\n");
}

type ActionResult =
  | { kind: "alert"; title: string; message: string }
  | {
      kind: "toast";
      variant: "info" | "success" | "warning" | "error";
      message: string;
    }
  | { kind: "noop" };

type SubactionCtx = {
  directory: string;
  workerSessionID: string;
  judgeSessionID: string;
  currentSessionID: string;
  isOnJudge: boolean;
  api: TuiPluginApi;
};

type Subaction = {
  title: string;
  value: string;
  description?: string;
  destructive?: boolean;
  requireSlot?: boolean;
  run: (ctx: SubactionCtx) => Promise<ActionResult>;
};

async function withSlot(
  api: TuiPluginApi,
  directory: string,
  options: { requireSlot: boolean },
): Promise<ActionResult | SubactionCtx> {
  const current = currentSessionID(api);
  if (!current) {
    return {
      kind: "toast",
      variant: "error",
      message: "open a session first",
    };
  }
  if (!(await daemonHealthy())) {
    if (options.requireSlot) {
      return {
        kind: "toast",
        variant: "warning",
        message: "office daemon is not running; run /judge on first",
      };
    }
    await ensureDaemon();
  }

  const resolved = await daemonRequest<any>(
    "GET",
    `/slot/by-session?session_id=${encodeURIComponent(current)}`,
  );
  const slot = resolved?.slot;
  if (!slot) {
    if (options.requireSlot) {
      return {
        kind: "toast",
        variant: "warning",
        message: "no judge slot for this session; run /judge on first",
      };
    }
    return {
      directory,
      workerSessionID: current,
      judgeSessionID: "",
      currentSessionID: current,
      isOnJudge: false,
      api,
    };
  }
  return {
    directory: slot.directory ?? directory,
    workerSessionID: slot.worker_session_id,
    judgeSessionID: slot.judge_session_id ?? "",
    currentSessionID: current,
    isOnJudge: resolved.side === "judge",
    api,
  };
}

const SUBACTIONS: Subaction[] = [
  {
    title: "On",
    value: "judge.on",
    description: "Start supervision",
    async run({ directory, workerSessionID }) {
      const payload = await daemonRequest<any>("POST", "/judge/on", {
        directory,
        worker_session_id: workerSessionID,
      });
      const judge = payload?.state?.judge_session_id ?? "?";
      return {
        kind: "toast",
        variant: "success",
        message: `judge on (${judge})`,
      };
    },
  },
  {
    title: "Off",
    value: "judge.off",
    description: "Stop supervision",
    requireSlot: true,
    async run({ directory, workerSessionID }) {
      await daemonRequest("POST", "/judge/off", {
        directory,
        worker_session_id: workerSessionID,
      });
      return { kind: "toast", variant: "success", message: "judge off" };
    },
  },
  {
    title: "Status",
    value: "judge.status",
    description: "Slot status",
    requireSlot: true,
    async run({ directory, workerSessionID }) {
      const summary = await daemonRequest<any>(
        "GET",
        `/summary?directory=${encodeURIComponent(directory)}&worker_session_id=${encodeURIComponent(workerSessionID)}`,
      );
      return {
        kind: "alert",
        title: "Judge status",
        message: formatSummary(summary),
      };
    },
  },
  {
    title: "Status (all)",
    value: "judge.status.all",
    description: "All slots",
    async run() {
      const summary = await daemonRequest<any>("GET", "/summary");
      return {
        kind: "alert",
        title: "Judge status (all)",
        message: formatSummary(summary),
      };
    },
  },
  {
    title: "Pause",
    value: "judge.pause",
    description: "Queue events; no nudge",
    requireSlot: true,
    async run({ directory, workerSessionID }) {
      const payload = await daemonRequest<any>("POST", "/judge/pause", {
        directory,
        worker_session_id: workerSessionID,
      });
      const slot = (payload?.slots ?? [])[0];
      const queued = slot?.pending_events?.length ?? 0;
      return {
        kind: "toast",
        variant: "info",
        message: `paused (queued ${queued})`,
      };
    },
  },
  {
    title: "Resume",
    value: "judge.resume",
    description: "Replay queued + resume",
    requireSlot: true,
    async run({ directory, workerSessionID }) {
      const payload = await daemonRequest<any>("POST", "/judge/resume", {
        directory,
        worker_session_id: workerSessionID,
      });
      const n = (payload?.replayed ?? []).length;
      return {
        kind: "toast",
        variant: "info",
        message: n ? `resumed (${n} replayed)` : "resumed",
      };
    },
  },
  {
    title: "Poke",
    value: "judge.poke",
    description: "Force review now",
    requireSlot: true,
    async run({ directory, workerSessionID }) {
      await daemonRequest("POST", "/judge/poke", {
        directory,
        worker_session_id: workerSessionID,
      });
      return { kind: "toast", variant: "success", message: "judge nudged" };
    },
  },
  {
    title: "Forget",
    value: "judge.forget",
    description: "Drop slot (sessions stay)",
    requireSlot: true,
    destructive: true,
    async run({ directory, workerSessionID }) {
      const payload = await daemonRequest<any>("POST", "/judge/forget", {
        directory,
        worker_session_id: workerSessionID,
      });
      return {
        kind: "toast",
        variant: payload?.removed ? "success" : "info",
        message: payload?.removed ? "slot forgotten" : "no slot to forget",
      };
    },
  },
  {
    title: "Logs",
    value: "judge.logs",
    description: "Tail daemon events",
    async run() {
      const payload = await daemonRequest<any>("GET", "/logs?lines=200");
      return {
        kind: "alert",
        title: "Judge logs",
        message: formatLogs(payload),
      };
    },
  },
  {
    title: "PS",
    value: "judge.ps",
    description: "Daemon processes",
    async run() {
      const payload = await daemonRequest<any>("GET", "/processes");
      const lines = [`daemon pid: ${payload?.daemon_pid ?? "(none)"}`, ""];
      for (const proc of payload?.processes ?? []) {
        lines.push(
          `  ${String(proc.pid).padStart(7)}  ppid=${String(proc.ppid).padStart(7)}  ${proc.cmd}`,
        );
      }
      return {
        kind: "alert",
        title: "Judge processes",
        message: lines.join("\n"),
      };
    },
  },
  {
    title: "Switch to worker",
    value: "judge.worker",
    description: "View worker session",
    requireSlot: true,
    async run({ workerSessionID, currentSessionID, api }) {
      if (currentSessionID === workerSessionID) {
        return { kind: "toast", variant: "info", message: "already on worker" };
      }
      api.route.navigate("session", { sessionID: workerSessionID });
      return { kind: "noop" };
    },
  },
  {
    title: "Switch to judge",
    value: "judge.judge",
    description: "View judge session",
    requireSlot: true,
    async run({ judgeSessionID, currentSessionID, api }) {
      if (!judgeSessionID) {
        return {
          kind: "toast",
          variant: "error",
          message: "no judge; run /judge on",
        };
      }
      if (currentSessionID === judgeSessionID) {
        return { kind: "toast", variant: "info", message: "already on judge" };
      }
      api.route.navigate("session", { sessionID: judgeSessionID });
      return { kind: "noop" };
    },
  },
];

function dispatchResult(api: TuiPluginApi, result: ActionResult): void {
  if (result.kind === "noop") {
    api.ui.dialog.clear();
    return;
  }
  if (result.kind === "toast") {
    api.ui.dialog.clear();
    api.ui.toast({ variant: result.variant, message: result.message });
    return;
  }
  const { DialogAlert } = api.ui;
  api.ui.dialog.replace(() =>
    DialogAlert({ title: result.title, message: result.message }),
  );
}

function showSubactionDialog(api: TuiPluginApi, directory: string): void {
  const options: TuiDialogSelectOption<string>[] = SUBACTIONS.map((sub) => ({
    title: sub.title,
    value: sub.value,
    description: sub.description,
    category: sub.destructive ? "Danger" : "Judge",
  }));

  const { DialogSelect } = api.ui;
  api.ui.dialog.replace(() =>
    DialogSelect<string>({
      title: "Judge",
      placeholder: "select an action",
      options,
      onSelect: (option) => {
        const sub = SUBACTIONS.find((s) => s.value === option.value);
        if (!sub) {
          api.ui.dialog.clear();
          return;
        }
        (async () => {
          const slotScoped = new Set([
            "judge.on",
            "judge.off",
            "judge.status",
            "judge.pause",
            "judge.resume",
            "judge.poke",
            "judge.forget",
            "judge.worker",
            "judge.judge",
          ]);
          const needsSlot =
            sub.requireSlot === true || slotScoped.has(sub.value);
          let ctx: SubactionCtx;
          if (needsSlot) {
            const resolved = await withSlot(api, directory, {
              requireSlot: sub.requireSlot ?? false,
            });
            if ("kind" in resolved) return dispatchResult(api, resolved);
            ctx = resolved;
          } else {
            if (!(await daemonHealthy())) {
              try {
                await ensureDaemon();
              } catch {}
            }
            ctx = {
              directory,
              workerSessionID: "",
              judgeSessionID: "",
              currentSessionID: currentSessionID(api) ?? "",
              isOnJudge: false,
              api,
            };
          }
          const result = await sub.run(ctx);
          dispatchResult(api, result);
        })().catch((err) => {
          const message = err instanceof Error ? err.message : String(err);
          api.ui.dialog.clear();
          api.ui.toast({
            variant: "error",
            message: `judge ${sub.value} failed: ${message}`,
          });
        });
      },
    }),
  );
}

type SlotResolution = {
  side: "worker" | "judge" | null;
  workerSessionID?: string;
  judgeSessionID?: string;
  enabled?: boolean;
  paused?: boolean;
  health?: string;
  lastVerdict?: string | null;
};

const POLL_MS = 5000;

function useSlotForSession(sessionID: () => string | undefined) {
  const [resolution, setResolution] = createSignal<SlotResolution>({
    side: null,
  });
  let timer: ReturnType<typeof setTimeout> | undefined;
  let cancelled = false;

  async function fetchOnce() {
    const sid = sessionID();
    if (!sid) {
      setResolution({ side: null });
      return;
    }
    if (!(await daemonHealthy())) {
      setResolution({ side: null });
      return;
    }
    try {
      const payload = await daemonRequest<any>(
        "GET",
        `/slot/by-session?session_id=${encodeURIComponent(sid)}`,
      );
      const slot = payload?.slot;
      if (!slot) {
        setResolution({ side: null });
        return;
      }
      setResolution({
        side: payload.side ?? null,
        workerSessionID: slot.worker_session_id ?? undefined,
        judgeSessionID: slot.judge_session_id ?? undefined,
        enabled: !!slot.enabled,
        paused: !!slot.paused,
        health: slot.health ?? "unknown",
        lastVerdict: slot.last_judge_verdict ?? null,
      });
    } catch {}
  }

  function schedule() {
    if (cancelled) return;
    timer = setTimeout(async () => {
      await fetchOnce();
      schedule();
    }, POLL_MS);
  }

  onMount(() => {
    void fetchOnce();
    schedule();
  });
  onCleanup(() => {
    cancelled = true;
    if (timer) clearTimeout(timer);
  });

  return resolution;
}

function shortID(id: string | undefined): string {
  if (!id) return "";
  return id.length > 12 ? id.slice(0, 12) + "…" : id;
}

function healthLabel(health: string | undefined): string {
  switch (health) {
    case "ok":
      return "ok";
    case "worker_missing":
      return "worker gone";
    case "judge_missing":
      return "judge gone";
    case "orphaned":
      return "orphaned";
    case "degraded":
      return "degraded";
    default:
      return "unknown";
  }
}

function PromptBadge(props: { api: TuiPluginApi; session_id: string }) {
  const resolution = useSlotForSession(() => props.session_id);
  const theme = () => props.api.theme.current;
  const label = createMemo(() => {
    const r = resolution();
    if (r.side === "judge") return "JUDGE";
    if (r.side === "worker") return "WORKER";
    return null;
  });
  const color = createMemo(() => {
    const r = resolution();
    if (r.side === "judge") return theme().warning;
    if (r.side === "worker") return theme().info;
    return theme().textMuted;
  });
  return (
    <Show when={label() !== null}>
      <text fg={color()}>
        <b>[{label()}]</b>
      </text>
    </Show>
  );
}

function SidebarOfficePanel(props: { api: TuiPluginApi; session_id: string }) {
  const resolution = useSlotForSession(() => props.session_id);
  const theme = () => props.api.theme.current;
  const inSlot = createMemo(() => resolution().side !== null);

  return (
    <Show when={inSlot()}>
      <box>
        <box flexDirection="row" gap={1}>
          <text fg={theme().text}>
            <b>Office</b>
          </text>
          <text
            fg={resolution().side === "judge" ? theme().warning : theme().info}
          >
            <b>{resolution().side === "judge" ? "[JUDGE]" : "[WORKER]"}</b>
          </text>
        </box>
        <text fg={theme().textMuted}>
          peer:{" "}
          {shortID(
            resolution().side === "judge"
              ? resolution().workerSessionID
              : resolution().judgeSessionID,
          )}
        </text>
        <text fg={theme().textMuted}>
          state:{" "}
          {resolution().enabled
            ? resolution().paused
              ? "paused"
              : "on"
            : "off"}
        </text>
        <text
          fg={resolution().health === "ok" ? theme().success : theme().warning}
        >
          health: {healthLabel(resolution().health)}
        </text>
        <Show when={resolution().lastVerdict}>
          <text fg={theme().textMuted}>
            verdict: {resolution().lastVerdict}
          </text>
        </Show>
      </box>
    </Show>
  );
}

const tui: TuiPlugin = async (api: TuiPluginApi) => {
  api.command.register((): TuiCommand[] => [
    {
      title: "Judge controls",
      value: "office.judge",
      description: "Open the judge daemon control menu",
      category: "Judge",
      slash: { name: "judge" },
      onSelect: () => {
        const directory = api.state.path.directory;
        if (!directory) {
          api.ui.toast({
            variant: "error",
            message: "no project directory available",
          });
          return;
        }
        showSubactionDialog(api, directory);
      },
    },
  ]);

  api.slots.register({
    order: 100,
    slots: {
      session_prompt_right(_ctx: unknown, props: { session_id: string }) {
        return <PromptBadge api={api} session_id={props.session_id} />;
      },
    },
  });

  api.slots.register({
    order: 350,
    slots: {
      sidebar_content(_ctx: unknown, props: { session_id: string }) {
        return <SidebarOfficePanel api={api} session_id={props.session_id} />;
      },
    },
  });
};

export default {
  id: "office",
  tui,
};

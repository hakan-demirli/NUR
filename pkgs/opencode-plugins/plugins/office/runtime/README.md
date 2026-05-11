# Office Judge Plugin

Tracked home for the OpenCode `/judge` plugin and the per-session office daemon that backs it.

## Layout

- `office.ts` - thin `/judge ...` plugin that talks to the session daemon
- `office_orchestrator/` - Python implementation of the per-session daemon and a small CLI client
- `pyproject.toml` - Python package metadata

## Live wiring

```bash
~/.config/opencode/plugins/office.ts -> /home/emre/Desktop/test/office/office.ts
```

Edit `office.ts` here and OpenCode picks it up via the symlink.

## Architecture

- The plugin owns only the `/judge` UX inside OpenCode. It does not orchestrate.
- The office daemon is keyed by worker session id. It speaks plain HTTP/JSON over a Unix domain socket at `~/.cache/opencode_office_py/<directory-and-session-hash>/daemon.sock`.
- The Python CLI is a thin convenience wrapper around the same daemon API.
- The TUI plugin passes the current OpenCode serve URL to the daemon, so normal plugin use does not launch an extra `opencode serve` child. CLI-only use can still auto-launch one.
- Worker and judge sessions are normal OpenCode sessions. You can attach manually with `opencode -s <id>` if you want, but the plugin does this for you via TUI session-select events.

## `/judge` commands

- `/judge on` - enable supervision: launch the daemon if not already running, fork the current session into a judge session, persist the worker/judge mapping
- `/judge off` - disable supervision; sessions stay alive
- `/judge pause` - daemon keeps running and watching, but suppresses judge nudges. Worker turn / idle events that would have nudged the judge are queued.
- `/judge resume` - flip back to active. Coalesces queued events by worker message id and replays them as a single nudge per group, with the original reasons preserved.
- `/judge kill` - abort the judge session's active run, hard kill the daemon, its owned `opencode serve`, and any descendants. No negotiation.
- `/judge status` - one-line summary of worker and judge: ids, model, token usage, last nudge, and pause state with queued event count
- `/judge worker` - switch the OpenCode TUI to the worker session
- `/judge judge` - switch the OpenCode TUI to the judge session
- `/judge logs` - tail recent structured daemon events
- `/judge ps` - show the daemon, its `opencode serve`, and tracked descendants
- `/judge poke` - force an immediate judge review (bypasses pause)

## Daemon HTTP API (Unix socket)

Anyone can hit it directly with `curl --unix-socket`. The plugin and the convenience CLI are just clients.

```bash
SOCK=~/.cache/opencode_office_py/<directory-and-session-hash>/daemon.sock

curl -s --unix-socket "$SOCK" http://localhost/health
curl -s --unix-socket "$SOCK" http://localhost/status     # full state
curl -s --unix-socket "$SOCK" http://localhost/summary    # human summary
curl -s --unix-socket "$SOCK" http://localhost/paths      # log/socket/state file paths
curl -s --unix-socket "$SOCK" http://localhost/processes  # daemon + serve + descendants
curl -s --unix-socket "$SOCK" 'http://localhost/logs?lines=200'

curl -s --unix-socket "$SOCK" -X POST http://localhost/judge/on \
  -H 'Content-Type: application/json' -d '{"worker_session_id":"ses_..."}'
curl -s --unix-socket "$SOCK" -X POST http://localhost/judge/off    -H 'Content-Type: application/json' -d '{}'
curl -s --unix-socket "$SOCK" -X POST http://localhost/judge/pause  -H 'Content-Type: application/json' -d '{}'
curl -s --unix-socket "$SOCK" -X POST http://localhost/judge/resume -H 'Content-Type: application/json' -d '{}'
curl -s --unix-socket "$SOCK"        http://localhost/judge/queue
curl -s --unix-socket "$SOCK" -X POST http://localhost/judge/poke   -H 'Content-Type: application/json' -d '{}'
curl -s --unix-socket "$SOCK" -X POST http://localhost/stop         -H 'Content-Type: application/json' -d '{}'
```

## Compaction policy

The daemon flags compaction as recommended in two cases:

- usage `>= 300,000` tokens on a model whose context window is `>= 400,000`
- usage `>= 60%` of the context window on any model

The daemon will automatically compact the judge before a review nudge if the judge has crossed those thresholds. Worker compaction is a judge decision; the daemon surfaces the recommendation in nudge prompts.

## Convenience Python CLI

```bash
python3 -m office_orchestrator.cli --directory /path/to/repo --session-id <worker-ses> doctor
python3 -m office_orchestrator.cli --directory /path/to/repo --session-id <worker-ses> daemon-start
python3 -m office_orchestrator.cli --directory /path/to/repo --session-id <worker-ses> daemon-stop      # hard, no negotiation
python3 -m office_orchestrator.cli --directory /path/to/repo --session-id <worker-ses> judge-on <worker-ses>
python3 -m office_orchestrator.cli --directory /path/to/repo --session-id <worker-ses> summary
python3 -m office_orchestrator.cli --directory /path/to/repo --session-id <worker-ses> logs --lines 200
python3 -m office_orchestrator.cli --directory /path/to/repo --session-id <worker-ses> ps
python3 -m office_orchestrator.cli --directory /path/to/repo --session-id <worker-ses> worker-id
python3 -m office_orchestrator.cli --directory /path/to/repo --session-id <worker-ses> judge-id
```

The CLI talks to the same Unix socket described above, so it is fully optional.

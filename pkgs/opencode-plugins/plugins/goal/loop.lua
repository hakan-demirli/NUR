
local function emptyToNil(s)
  if s == nil or s == "" then return nil end
  return s
end

local HOME = os.getenv("HOME") or ""
local CACHE_ROOT = emptyToNil(os.getenv("GOAL_CACHE_DIR")) or (HOME .. "/.cache/opencode_goal")
local GLOBAL_SOCKET = CACHE_ROOT .. "/daemon.sock"
local PID_FILE = CACHE_ROOT .. "/daemon.pid"

local DEFAULT_GOAL_BIN = "__OPENCODE_GOAL_BIN__"

local function tryRun(argv)
  local ok, result = pcall(api.process.run, argv)
  if not ok then return nil, tostring(result) end
  return result, nil
end

local _cached_bin = nil
local function invalidateBinCache()
  _cached_bin = nil
end

local function resolveGoalBin()
  if _cached_bin then return _cached_bin end
  local tried = {}
  local fromEnv = emptyToNil(os.getenv("OPENCODE_GOAL_BIN"))
  if fromEnv then
    table.insert(tried, fromEnv)
    _cached_bin = fromEnv
    return fromEnv
  end
  local viaPath = tryRun({ "sh", "-c", "command -v opencode-goal" })
  if viaPath and viaPath.code == 0 then
    local found = (viaPath.stdout or ""):gsub("%s+$", "")
    if found ~= "" then
      table.insert(tried, found)
      _cached_bin = found
      return found
    end
  end
  if DEFAULT_GOAL_BIN ~= "" and not DEFAULT_GOAL_BIN:match("^__") then
    table.insert(tried, DEFAULT_GOAL_BIN)
    _cached_bin = DEFAULT_GOAL_BIN
    return DEFAULT_GOAL_BIN
  end
  local listing = #tried > 0 and table.concat(tried, ", ") or "(none)"
  error("opencode-goal binary not found. Set OPENCODE_GOAL_BIN, install opencode-goal on PATH, or rebuild opencode-plugins. Tried: " .. listing)
end

local function daemonRequest(method, endpoint, body)
  local init = { method = method, unix = GLOBAL_SOCKET }
  if body ~= nil then
    init.headers = { ["Content-Type"] = "application/json" }
    init.body = JSON.stringify(body)
  end
  local response = fetch("http://localhost" .. endpoint, init)
  local raw = response:text()
  if not response.ok then
    error(string.format("daemon %s %s -> %s: %s", method, endpoint, response.status, raw ~= "" and raw or "<empty>"))
  end
  if raw == "" then return {} end
  local ok, parsed = pcall(JSON.parse, raw)
  if not ok then error("non-JSON: " .. raw:sub(1, 200)) end
  return parsed
end

local function daemonHealthy()
  local ok = pcall(function() daemonRequest("GET", "/health") end)
  return ok
end

local function readPid()
  local f = io.open(PID_FILE, "r")
  if not f then return nil end
  local raw = f:read("*a") or ""
  f:close()
  return tonumber((raw:gsub("%s+", "")))
end

local function pidAlive(pid)
  if not pid then return false end
  local result = tryRun({ "kill", "-0", tostring(pid) })
  return result ~= nil and result.code == 0
end

local function startDaemon()
  local bin = resolveGoalBin()
  local result, spawnErr = tryRun({ bin, "daemon-start" })
  if spawnErr then
    invalidateBinCache()
    error("could not spawn '" .. bin .. "': " .. spawnErr)
  end
  if result.code ~= 0 then
    local msg = result.stderr ~= "" and result.stderr or result.stdout
    error((msg ~= "" and msg or ("daemon-start exited " .. tostring(result.code))):gsub("%s+$", ""))
  end
end

local function ensureDaemon()
  if daemonHealthy() then return end
  local pid = readPid()
  if pid and pidAlive(pid) then
    error("goal daemon wedged (pid " .. tostring(pid) .. "); run /goal restart")
  end
  startDaemon()
  for _ = 1, 20 do
    if daemonHealthy() then return end
    api.sleep(150)
  end
  error("goal daemon did not come up at " .. GLOBAL_SOCKET)
end

local function toast(variant, message)
  api.ui.toast({ variant = variant, message = message })
end

local function currentDirectory()
  return api.state.path.directory or ""
end


local function parseInterval(s)
  s = s:gsub("%s+", ""):lower()
  local n, unit = s:match("^(%d+)([smh]?)$")
  if not n then return nil end
  n = tonumber(n)
  if unit == "m" then return n * 60 end
  if unit == "h" then return n * 3600 end
  return n  
end


api.command.register(function()
  return {
    {
      title = "Loop",
      value = "goal.loop",
      description = "Re-prompt the worker on a timer (e.g. /loop 5m)",
      category = "Goal",
      slash = { name = "loop", aliases = {} },
      onSelect = function(ctx)
        local input = (ctx and ctx.args or ctx and ctx.input or ""):gsub("^%s+", ""):gsub("%s+$", "")
        local directory = currentDirectory()

        if input == "stop" or input == "off" or input == "clear" then
          ensureDaemon()
          local _, err = pcall(daemonRequest, "POST", "/loop/stop", { directory = directory })
          if err then toast("error", tostring(err)) else toast("info", "Loop stopped") end
          return
        end

        if input == "" then
          if not daemonHealthy() then
            toast("info", "Goal daemon not running")
            return
          end
          local ok, result = pcall(daemonRequest, "GET",
            "/status?directory=" .. directory)
          local goal = ok and result.goal or nil
          if not goal then
            toast("info", "No active goal for this directory")
          elseif goal.loop_interval_seconds then
            toast("info", string.format("Loop active: every %ds", goal.loop_interval_seconds))
          else
            toast("info", "Loop not set. Use /loop <interval> (e.g. 5m, 30s, 1h)")
          end
          return
        end

        local interval = parseInterval(input)
        if not interval or interval <= 0 then
          toast("error", "Invalid interval: " .. input .. "\nExamples: 5m  30s  1h  300")
          return
        end

        ensureDaemon()
        local _, err = pcall(daemonRequest, "POST", "/loop/start", {
          directory = directory,
          interval_seconds = interval,
        })
        if err then
          toast("error", "Loop start failed: " .. tostring(err))
        else
          local label = input
          toast("success", "Loop set: every " .. label)
        end
      end,
    },
  }
end)

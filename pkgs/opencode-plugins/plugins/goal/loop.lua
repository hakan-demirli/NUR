
local GOAL_BIN = os.getenv("OPENCODE_GOAL_BIN")
  or "__OPENCODE_GOAL_BIN__"
local HOME = os.getenv("HOME") or ""
local GLOBAL_SOCKET = HOME .. "/.cache/opencode_goal/daemon.sock"

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

local function startDaemon()
  local result = api.process.run({ GOAL_BIN, "daemon-start" })
  if result.code ~= 0 then
    local msg = result.stderr ~= "" and result.stderr or result.stdout
    error((msg ~= "" and msg or ("daemon-start exited " .. tostring(result.code))):gsub("%s+$", ""))
  end
end

local function ensureDaemon()
  if daemonHealthy() then return end
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

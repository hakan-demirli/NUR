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
  if not ok then error("daemon returned non-JSON: " .. raw:sub(1, 200)) end
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

local function spawnBin(argv0Suffix)
  local bin = resolveGoalBin()
  local result, spawnErr = tryRun({ bin, argv0Suffix })
  if spawnErr then
    invalidateBinCache()
    error("could not spawn '" .. bin .. "': " .. spawnErr)
  end
  if result.code ~= 0 then
    local msg = result.stderr ~= "" and result.stderr or result.stdout
    error((msg ~= "" and msg or (argv0Suffix .. " exited " .. tostring(result.code))):gsub("%s+$", ""))
  end
  return result
end

local function startDaemon()
  spawnBin("daemon-start")
end

local function stopDaemon()
  spawnBin("daemon-stop")
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

local function currentSessionID()
  local route = api.route.current
  if not route or route.name ~= "session" then return nil end
  local params = route.params or {}
  local sid = params.sessionID
  if type(sid) == "string" and sid ~= "" then return sid end
  return nil
end


local BUILTIN_PERSONALITIES = {
  { title = "Devil's advocate", value = "devils_advocate",
    description = "Assume the worker is wrong; demand concrete proof" },
  { title = "Compiler (objective only)", value = "compiler",
    description = "Evaluate only verifiable criteria, ignore prose" },
  { title = "Peer reviewer", value = "peer_reviewer",
    description = "Code quality, naming, edge cases, conventions" },
  { title = "Custom…", value = "__custom__",
    description = "Type your own persona instruction" },
}

local function showPersonalityPicker(onPick)
  api.ui.dialog.replace(function()
    return api.ui.DialogSelect({
      title = "Personality",
      options = BUILTIN_PERSONALITIES,
      onSelect = function(opt)
        if opt.value == "__custom__" then
          api.ui.dialog.replace(function()
            return api.ui.DialogSelect({
              title = "Custom personality (type and press enter)",
              renderFilter = true,
              options = {},
              placeholder = "Describe the evaluation persona…",
              onFilter = function(text)
              end,
              onSelect = function()
              end,
            })
          end)
          onPick({ key = nil, custom = "" })
        else
          onPick({ key = opt.value, custom = nil })
        end
      end,
    })
  end)
end

local function modelOptions()
  return {
    { title = "(use session default)", value = "__default__", description = "Inherit model from current session" },
  }
end

local function showJudgeConfig(slot, onDone)
  local function refresh()
    local items = {
      {
        title = "Model",
        value = "model",
        description = slot.provider_id and (slot.provider_id .. "/" .. (slot.model_id or "")) or "(session default)",
      },
      {
        title = "Variant",
        value = "variant",
        description = slot.variant or "(none)",
      },
      {
        title = "Personality",
        value = "personality",
        description = slot.personality_key or slot.personality_custom or "default",
      },
      {
        title = "✓ Done",
        value = "done",
        description = "",
      },
    }
    api.ui.dialog.replace(function()
      return api.ui.DialogSelect({
        title = slot.label or "Configure",
        options = items,
        onSelect = function(opt)
          if opt.value == "done" then
            onDone(slot)
          elseif opt.value == "model" then
            slot.provider_id = nil
            slot.model_id = nil
            toast("info", "Model set to session default (model picker integration: TODO)")
            refresh()
          elseif opt.value == "variant" then
            slot.variant = nil
            toast("info", "Variant cleared (variant picker integration: TODO)")
            refresh()
          elseif opt.value == "personality" then
            showPersonalityPicker(function(result)
              slot.personality_key = result.key
              slot.personality_custom = result.custom
              refresh()
            end)
          end
        end,
      })
    end)
  end
  refresh()
end


local function showHRPanel(directory, onSave)
  ensureDaemon()

  local ok, status = pcall(daemonRequest, "GET",
    "/status?directory=" .. directory)
  local goal = ok and status.goal or nil
  local hr = goal and goal.hr or { president = {}, judges = {} }

  if #hr.judges == 0 then
    hr.judges = {{ provider_id = nil, model_id = nil, variant = nil,
                   personality_key = nil, personality_custom = nil, session_id = nil }}
  end

  local function buildRows()
    local rows = {}

    local p = hr.president or {}
    table.insert(rows, {
      title = "President",
      value = "president",
      description = (p.provider_id and (p.provider_id .. "/" .. (p.model_id or "")) or "(session default)")
        .. (p.personality_key and ("  [" .. p.personality_key .. "]") or ""),
      category = "Chair",
    })

    for i, j in ipairs(hr.judges) do
      table.insert(rows, {
        title = "Judge " .. tostring(i),
        value = "judge_" .. tostring(i),
        description = (j.provider_id and (j.provider_id .. "/" .. (j.model_id or "")) or "(session default)")
          .. (j.personality_key and ("  [" .. j.personality_key .. "]") or ""),
        category = "Panel",
      })
    end

    table.insert(rows, {
      title = "+ add judge",
      value = "__add__",
      description = "",
      category = "Panel",
    })

    return rows
  end

  local function refresh()
    api.ui.dialog.replace(function()
      return api.ui.DialogSelect({
        title = "HR panel",
        options = buildRows(),
        onSelect = function(opt)
          if opt.value == "__add__" then
            table.insert(hr.judges, {
              provider_id = nil, model_id = nil, variant = nil,
              personality_key = nil, personality_custom = nil, session_id = nil,
            })
            local idx = #hr.judges
            local slot = hr.judges[idx]
            slot.label = "Judge " .. tostring(idx)
            showJudgeConfig(slot, function(updated)
              hr.judges[idx] = updated
              refresh()
            end)
          elseif opt.value == "president" then
            local slot = hr.president
            slot.label = "President"
            showJudgeConfig(slot, function(updated)
              hr.president = updated
              refresh()
            end)
          else
            local idx = tonumber(opt.value:match("judge_(%d+)"))
            if idx then
              local slot = hr.judges[idx]
              slot.label = "Judge " .. tostring(idx)
              showJudgeConfig(slot, function(updated)
                hr.judges[idx] = updated
                refresh()
              end)
            end
          end
        end,
      })
    end)
  end

  refresh()
end


local function showPinPicker(directory, sessionID)
  ensureDaemon()
  local ok, result = pcall(daemonRequest, "GET",
    "/session/messages?session_id=" .. sessionID .. "&limit=30")
  if not ok then
    toast("error", "Could not load messages: " .. tostring(result))
    return
  end
  local messages = result.messages or {}
  if #messages == 0 then
    toast("info", "No user messages to pin")
    return
  end
  local options = {}
  for _, m in ipairs(messages) do
    table.insert(options, {
      title = m.preview or "(no text)",
      value = m.id or "",
      description = "",
    })
  end
  api.ui.dialog.replace(function()
    return api.ui.DialogSelect({
      title = "Pin message",
      options = options,
      onSelect = function(opt)
        if opt.value == "" then return end
        local _, err = pcall(daemonRequest, "POST", "/goal/pin", {
          directory = directory,
          session_id = sessionID,
          message_id = opt.value,
          preview = opt.title,
        })
        if err then
          toast("error", "Pin failed: " .. tostring(err))
        else
          api.ui.dialog.clear()
          toast("success", "Message pinned")
        end
      end,
    })
  end)
end

local function showUnpinPicker(directory)
  ensureDaemon()
  local ok, status = pcall(daemonRequest, "GET",
    "/status?directory=" .. directory)
  if not ok then
    toast("error", "Could not load goal: " .. tostring(status))
    return
  end
  local goal = status.goal
  local pins = goal and goal.pins or {}
  if #pins == 0 then
    toast("info", "No pinned messages")
    return
  end
  local options = {}
  for _, p in ipairs(pins) do
    table.insert(options, {
      title = p.preview or p.message_id,
      value = p.message_id,
      description = "session: " .. (p.session_id or ""):sub(1, 8),
    })
  end
  api.ui.dialog.replace(function()
    return api.ui.DialogSelect({
      title = "Unpin message",
      options = options,
      onSelect = function(opt)
        local _, err = pcall(daemonRequest, "POST", "/goal/unpin", {
          directory = directory,
          message_id = opt.value,
        })
        if err then
          toast("error", "Unpin failed: " .. tostring(err))
        else
          api.ui.dialog.clear()
          toast("success", "Message unpinned")
        end
      end,
    })
  end)
end


local function goalStatusText(goal)
  if not goal then return "No active goal" end
  local status = goal.status or "?"
  local objective = goal.objective or ""
  if #objective > 60 then objective = objective:sub(1, 57) .. "…" end
  local phase = (goal.voting and goal.voting.phase) or "idle"
  local judges = goal.hr and goal.hr.judges and #goal.hr.judges or 0
  local blocks = (goal.voting and goal.voting.consecutive_blocks) or 0
  return string.format(
    "Status: %s\nPhase:  %s\nJudges: %d\nBlocks: %d\nObjective: %s",
    status, phase, judges, blocks, objective
  )
end


local SUBCOMMANDS = {
  start = function(args)
    local objective = args ~= "" and args or nil
    if not objective then
      toast("error", "Usage: /goal start <objective>")
      return
    end
    local sid = currentSessionID()
    if not sid then
      toast("error", "Open a session first")
      return
    end
    local directory = currentDirectory()
    if directory == "" then
      toast("error", "No project directory")
      return
    end
    ensureDaemon()
    local ok, status = pcall(daemonRequest, "GET",
      "/status?directory=" .. directory)
    local existing = ok and status.goal or nil
    local hr = existing and existing.hr or {
      president = {},
      judges = {{ provider_id = nil, model_id = nil }},
    }
    local _, err = pcall(daemonRequest, "POST", "/goal/start", {
      directory = directory,
      worker_session_id = sid,
      objective = objective,
      hr = hr,
    })
    if err then
      toast("error", "goal start failed: " .. tostring(err))
    else
      toast("success", "Goal started ◎")
    end
  end,

  pause = function(_)
    local directory = currentDirectory()
    ensureDaemon()
    local _, err = pcall(daemonRequest, "POST", "/goal/pause", { directory = directory })
    if err then toast("error", tostring(err)) else toast("info", "Goal paused") end
  end,

  resume = function(_)
    local directory = currentDirectory()
    ensureDaemon()
    local _, err = pcall(daemonRequest, "POST", "/goal/resume", { directory = directory })
    if err then toast("error", tostring(err)) else toast("info", "Goal resumed ◎") end
  end,

  clear = function(_)
    local directory = currentDirectory()
    ensureDaemon()
    local _, err = pcall(daemonRequest, "POST", "/goal/clear", { directory = directory })
    if err then toast("error", tostring(err)) else toast("info", "Goal cleared") end
  end,

  append = function(args)
    if args == "" then
      toast("error", "Usage: /goal append <text>")
      return
    end
    local directory = currentDirectory()
    ensureDaemon()
    local _, err = pcall(daemonRequest, "POST", "/goal/append", {
      directory = directory,
      text = args,
    })
    if err then toast("error", tostring(err)) else toast("info", "Goal updated") end
  end,

  checkpoint = function(_)
    local directory = currentDirectory()
    ensureDaemon()
    local _, err = pcall(daemonRequest, "POST", "/goal/checkpoint", { directory = directory })
    if err then
      toast("error", "Checkpoint failed: " .. tostring(err))
    else
      toast("success", "Checkpoint set (lazy fork-point)")
    end
  end,

  pin = function(_)
    local directory = currentDirectory()
    local sid = currentSessionID()
    if not sid then toast("error", "Open a session first") return end
    showPinPicker(directory, sid)
  end,

  unpin = function(_)
    local directory = currentDirectory()
    showUnpinPicker(directory)
  end,

  hr = function(_)
    local directory = currentDirectory()
    showHRPanel(directory, function(hr)
      local _, err = pcall(daemonRequest, "POST", "/hr/update", {
        directory = directory,
        hr = hr,
      })
      if err then
        toast("error", "HR update failed: " .. tostring(err))
      else
        api.ui.dialog.clear()
        toast("success", "HR panel saved")
      end
    end)
  end,

  restart = function(_)
    local stopOk, stopErr = pcall(stopDaemon)
    if not stopOk then
      toast("info", "stop: " .. tostring(stopErr))
    end
    api.sleep(500)
    local ok, err = pcall(ensureDaemon)
    if not ok then
      toast("error", "Restart failed: " .. tostring(err))
    else
      toast("success", "Goal daemon restarted ◎")
    end
  end,
}

local function showStatus()
  local directory = currentDirectory()
  if not daemonHealthy() then
    api.ui.dialog.replace(function()
      return api.ui.DialogAlert({ title = "Goal", message = "Daemon not running. Use /goal start <objective>." })
    end)
    return
  end
  local ok, result = pcall(daemonRequest, "GET", "/status?directory=" .. directory)
  local goal = ok and result.goal or nil
  api.ui.dialog.replace(function()
    return api.ui.DialogAlert({ title = "Goal status", message = goalStatusText(goal) })
  end)
end


api.command.register(function()
  return {
    {
      title = "Goal",
      value = "goal.main",
      description = "Manage the autonomous goal loop",
      category = "Goal",
      slash = { name = "goal", aliases = {} },
      onSelect = function(ctx)
        local input = (ctx and ctx.args or ctx and ctx.input or ""):gsub("^%s+", ""):gsub("%s+$", "")
        local sub, rest = input:match("^(%S+)%s*(.*)")
        if not sub or sub == "" then
          showStatus()
          return
        end
        local handler = SUBCOMMANDS[sub]
        if handler then
          handler(rest or "")
        else
          toast("error", "Unknown /goal subcommand: " .. sub ..
            "\nAvailable: start pause resume clear append checkpoint pin unpin hr restart")
        end
      end,
    },
  }
end)

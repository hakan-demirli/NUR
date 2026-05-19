api.command.register({
  name = "example.hello",
  title = "Hello from Lua",
  description = "Show a Lua plugin toast",
  category = "example",
  slash = {
    name = "hello",
    aliases = { "hi" },
  },
  run = function(ctx)
    api.ui.toast("Hello from Lua", ctx.args or "")
  end,
})

api.command.register({
  name = "example.pick",
  title = "Pick from Lua",
  description = "Open a Lua select dialog",
  category = "example",
  slash = {
    name = "pick",
  },
  run = function()
    api.ui.select({
      title = "Pick one",
      options = {
        { title = "Alpha", value = "alpha", description = "First option" },
        { title = "Beta", value = "beta", description = "Second option" },
      },
      on_select = function(value)
        if value then
          api.ui.toast("Picked " .. value)
        end
      end,
    })
  end,
})

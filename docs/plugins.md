# Lua plugins

Sterna plugins are ordinary Lua files which keep their state for the life of a
terminal tab. They can add menu items, install window-wide shortcuts, and react
when that tab connects or disconnects. A callback receives the same `tt`
terminal API as a standalone `.lua` macro.

This is Sterna's portable replacement for the useful part of Tera Term's TTX
extension surface. It is not compatible with TTX DLLs.

## Install a plugin

Create a directory named `plugins` beside Sterna's default `sterna.ini` and put
the file there. On Linux the default is:

```text
$XDG_CONFIG_HOME/sterna/plugins
```

When `XDG_CONFIG_HOME` is unset, that is `~/.config/sterna/plugins`. On Windows,
use the `sterna\plugins` directory under the roaming application-data folder.

Sterna loads direct children whose extension is `.lua`, case-insensitively, in
filename order. It does not search subdirectories. Prefixing names with numbers
is a simple way to make dependencies explicit:

```text
plugins/
  10-common.lua
  20-router-tools.lua
```

Plugin declarations are read when a terminal tab is created. Restart Sterna
after adding a plugin or changing its menus and keys. If any file fails to load,
the whole set is rejected and the status bar names the error; a partial menu is
never installed.

## A complete example

```lua
local connections = 0

sterna.menu {
  menu = 'Control/Router',
  label = 'Show interfaces',
  shortcut = 'Ctrl+Alt+I',
  action = function()
    tt.sendln('show interfaces')
  end,
}

sterna.key('Ctrl+Alt+R', function()
  tt.sendln('reload')
end)

sterna.on('connect', function(event)
  connections = connections + 1
  print(event .. ' #' .. connections)
end)

sterna.on('disconnect', function(event)
  print(event)
end)
```

The `connections` value demonstrates the important difference from a macro:
the file's top level runs once and its globals and closures remain alive. Each
tab has a separate Lua VM, so the value is not shared between sessions.

## Registration API

Registration is available only while the file's top level is loading. The
terminal API is available only inside callbacks.

### `sterna.menu(spec)`

Adds a visible action. `spec` has these fields:

- `menu` — required slash-separated path. The built-in roots are `File`,
  `Edit`, `Terminal`, `Control`, and `Setup`; other names create a new top-level
  menu. These API names stay English when the visible menus are translated.
- `label` — required action text.
- `shortcut` — optional key sequence.
- `action` — required callback function.

Nested paths such as `Control/Router/Diagnostics` create the missing submenus.

### `sterna.key(sequence, action)`

Adds a window-wide key binding without a visible menu item. The sequence is
required.

Menu and key sequences use Qt's portable spelling, including examples such as
`Ctrl+Alt+R`, `Ctrl+Shift+F12`, and `Alt+Left`. An invalid sequence is reported
and is not installed.

### `sterna.on(event, action)`

Registers a lifecycle callback. The supported events are `connect` and
`disconnect`; the event name is also passed as the callback's first argument.
Several callbacks may observe the same event, and they run in plugin filename
and declaration order.

## Callback behavior

Callbacks run on a worker, so waiting for terminal input or opening a dialog
does not block the window's event loop. Calls into `tt` are applied on the
frontend thread through the same boundary used by Lua and TTL macros.

One tab serializes its plugin callbacks. A second menu or key action is refused
while one is active. Connection events are different: they queue in arrival
order, because losing a disconnect while a dialog is open would leave the
plugin's state wrong.

`print(...)` writes to the terminal. Errors use the ordinary script error
dialog. Closing the tab cancels an active callback, including a Lua loop that
does not call `tt`, then stops its worker before destroying the session.

Inside a callback, `tt.name` is the plugin's source path and `tt.args` is an
empty table. The rest of `tt` is the existing Lua macro command surface:
connection I/O and waits, terminal and logging commands, dialogs, transfers,
serial controls, clipboard access, settings, and environment helpers.

## Current limits

This first plugin surface does not yet provide byte-stream filters or custom
settings pages. Both remain on the Stage 4 roadmap. It also deliberately does
not load native TTX or WASM plugins.

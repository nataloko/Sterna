# Lua plugins

Sterna plugins are ordinary Lua files which keep their state for the life of a
terminal tab. They can add menu items, install window-wide shortcuts, react
when that tab connects or disconnects, add typed settings pages, and transform
bytes in either direction. An ordinary callback receives the same `tt`
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
local preferences = sterna.settings {
  title = 'Router tools', section = 'Lua Router Tools', fields = {
    {name = 'view_only', label = 'View-only mode', kind = 'bool', default = false},
  },
}

sterna.filter('output', function(bytes)
  return preferences.view_only and nil or bytes
end)

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

sterna.menu {
  menu = 'Control/Router',
  label = 'Toggle view-only mode',
  action = function()
    preferences.view_only = not preferences.view_only
  end,
}

sterna.on('connect', function(event)
  connections = connections + 1
  print(event .. ' #' .. connections)
end)

sterna.on('disconnect', function(event)
  print(event)
end)
```

The `connections` value demonstrates the important difference from a macro:
the callback VM's globals and closures remain alive. Each tab has separate
state, so the value is not shared between sessions. A file which declares a
settings page or stream filter can share those typed controls with the
isolated fast-path VM; see [Settings pages](#settings-pages) and
[Stream filters](#stream-filters).

## Registration API

Registration is available only while the file's top level is loading. The
terminal API is available only inside callbacks.

### `sterna.menu(spec)`

Adds a visible action. `spec` has these fields:

- `menu` — required slash-separated path. The built-in roots are `File`,
  `Edit`, `Setup`, and `Control`; other names create a new top-level menu.
  These API names stay English when the visible menus are translated.
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

### `sterna.filter(direction, action)`

Registers a binary stream filter. `direction` is `input` or `output`. The
callback receives one Lua string and returns its replacement; returning `nil`
drops that chunk. Lua strings are binary-safe, including embedded NUL bytes.

The return value is a control proxy. `enabled` starts as `true`; setting it to
`false` bypasses the callback without removing it. Plugins may store their own
scalar controls on the proxy and read them from either an ordinary action or
the filter:

```lua
local incoming
incoming = sterna.filter('input', function(bytes)
  return incoming.prefix .. bytes
end)
incoming.prefix = '[remote] '

sterna.menu {
  menu = 'Control', label = 'Toggle input prefix', action = function()
    incoming.enabled = not incoming.enabled
  end,
}
```

Control values may be `nil`, booleans, numbers, or strings. `direction` is a
read-only property. Tables and functions cannot cross between Lua states.

### `sterna.settings(spec)`

Adds a searchable tab to Setup > Preferences and returns a typed control proxy.
The declaration has a title, an INI section, and one or more fields:

```lua
local preferences = sterna.settings {
  title = 'Router tools',
  section = 'Lua Router Tools',
  fields = {
    {
      name = 'enabled', label = 'Enable router tools',
      kind = 'bool', default = true,
    },
    {
      name = 'retries', label = 'Login attempts',
      description = 'How many times to retry authentication',
      kind = 'int', min = 1, max = 10, default = 3,
    },
    {
      name = 'prompt', key = 'PromptPrefix', label = 'Prompt prefix',
      kind = 'string', default = '[router] ',
    },
    {
      name = 'mode', label = 'Safety mode', kind = 'enum',
      choices = {'fast', 'safe'}, default = 'safe',
    },
  },
}

sterna.menu {
  menu = 'Control/Router', label = 'Send probe', action = function()
    if preferences.enabled then
      tt.sendln(preferences.prompt .. preferences.mode)
    end
  end,
}
```

The page fields are:

- `name` — required property name on the returned proxy. Lua property names
  are case-sensitive.
- `key` — optional key in the page's INI section; defaults to `name`.
- `label` — required text beside the editor.
- `description` — optional help text shown in the tooltip and included in
  settings search.
- `kind` — `bool`, `int`, `string`, or `enum`.
- `default` — required, with the Lua type implied by `kind`.
- `min` and `max` — required inclusive bounds for an `int`.
- `choices` — required ordered string list for an `enum`; `default` must be
  one of them.

The section and key are ordinary `sterna.ini` names. Their comparison follows
the file's case-insensitive rules, and two plugins cannot claim the same
section/key pair. Use a plugin-specific section such as `Lua Router Tools` to
avoid collisions. Strings and enum values must be UTF-8 and cannot contain NUL
or a line break.

Saved values are installed while `sterna.settings` runs. Code immediately
after the declaration therefore sees the saved value, not a temporary default.
A missing or invalid saved value takes the declared default. Reading and
assigning proxy properties is typed: a boolean property accepts a boolean, an
integer observes its bounds, and enum assignments must be one of the declared
strings.

The dialog applies changes when the user chooses OK; Cancel leaves the live
values alone. Setup > Save setup writes them into the active `sterna.ini`
without replacing comments or unrelated keys. A new tab starts from that file,
while Duplicate tab copies the source tab's live values even when they have not
been saved yet. Like every plugin state, values remain separate between
ordinary tabs.

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

## Stream filters

Input filters run after transport decoding and before VT parsing. The
transformed bytes are therefore what the screen, session log and macro receive.
Output filters run after terminal encoding—including keyboard input, macros and
terminal replies—and before transport framing and the pending write queue.
File-transfer protocol traffic bypasses both directions so a display plugin
cannot corrupt a transfer.

Filters run in plugin filename and declaration order. A callback receives an
arbitrary transport chunk, not a line or character. Boundaries may split an
escape sequence, UTF-8 character, or search pattern, so a filter which needs a
complete unit must retain its own partial input between calls. Returning `nil`
and returning an empty string both emit no bytes for that chunk.

### Why filters use an isolated VM

An ordinary callback may wait for terminal input or hold a dialog open. A
filter has to keep moving bytes while that happens, so filter callbacks run in
a separate fast-path VM with no `tt` API. Scalar properties on filter and
settings control proxies are the explicit bridges between the two VMs; this is
how a menu action or settings page can reconfigure a live filter safely.

Lua functions cannot be moved between VMs. For a file which declares a filter,
Sterna evaluates the top level once for the ordinary callback VM and once for
the filter VM. Keep top-level work deterministic and limited to declarations
and initial values. Initial control writes are applied only by the ordinary
load, so `counter = counter + 1` on a control is not accidentally doubled.
Ordinary files with no filters are still evaluated once.

The fast path is bounded to 100,000 Lua instructions per callback and 1 MiB of
output per source chunk. An error, invalid return, exceeded bound, or runaway
loop disables that filter, reports the problem in the status bar, and passes
the current bytes through. Another action may set `enabled = true` to try it
again. These failures do not disconnect the terminal.

## Current limits

Settings pages intentionally use the four portable controls above; a plugin
cannot inject a native widget or arbitrary code into the dialog. Sterna
deliberately does not load native TTX or WASM plugins.

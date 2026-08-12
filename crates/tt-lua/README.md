# tt-lua

Lua, over the same host the macro language uses.

`tt-ttl` is a port and this is not. Every quirk in TTL is upstream's, because
macros written against those quirks exist and are most of the reason anybody
wants this program; nobody has written a Sterna Lua script yet, so there is
nothing here to be faithful to and the shape is chosen rather than inherited.

```sh
cargo test -p tt-lua
```

## What it is

`tt_ttl::ScriptHost` is the whole of what a script can do to the world — send
bytes, wait for them, open a connection, move a file, put a dialog up, drive
the log. A frontend implements it once (`tt-macro`'s `SessionHost` is the one
that is a real terminal) and both languages run against it. This crate is the
glue between that trait and `mlua`, and it is glue rather than a second port
because the trait was built wide and shallow for exactly this.

```lua
tt.timeout = 30
tt.sendln('who')
local line = tt.recvln()
print(line)
```

## Decisions

**Lua is not a second TTL.** No `result`, no `inputstr`, no 1-based string
indexing, no `goto`. A function returns its answer and a failure raises, which
`pcall` catches. `PLAN.md` refused to transpile TTL *into* Lua for the mirror
image of this reason: the two languages are worth having precisely because they
are not each other.

**Only the terminal is exposed.** Roughly half of TTL's 231 reserved words
exist because the language had no standard library — `strlen`, `sprintf`,
`fileopen`, `getenv`, `int2str`, the checksums. Lua has those, or has `string`
and `io` to build them out of, and shadowing them with worse versions would be
the wrong half of the trade.

**Success is one value.** Lua expands a call's last argument to all of its
results, so a function answering `line, nil` puts a `nil` into the argument
list of whatever it is nested in. The convention is `io.open`'s — one value
when it worked, `nil` plus the detail when it did not — which is what makes
`tt.send(tt.recvln())` mean what it looks like. `tt.waitln` is the one
exception and returns `line, index`, the line first because the line is the
payload.

**Strings are bytes.** Lua strings are 8-bit clean and `ScriptHost` takes
`&[u8]`, so nothing is decoded on the way through. TTL's 511-byte ceiling is
`ttmdde.c`'s buffer size rather than anything about terminals and is not
reproduced.

**The matcher is `tt-ttl`'s.** `WaitSet` is upstream's incremental match with
upstream's back-off, and two languages disagreeing about when `wait 'ogin:'`
fires would be a bug nobody could find. The ten-pattern ceiling comes with it,
and a script that passes eleven is told so rather than quietly matching on the
first ten. What is *not* reused is the line buffer, for the reason above.

**`print` writes on the terminal**, through `disp_str`, with `\n` expanded to
`CR LF` because that is what a terminal needs to start a line. Lua's own
`print` goes to stdout, which for a window launched from a desktop menu is
nowhere — the same silent-diagnostic trap `AGENTS.md` records for `qWarning`
under journald. A host with no screen falls back to stderr.

**`os.exit` is removed.** The script is a thread inside the terminal, so
`os.exit` would take the window with it.

**A misspelled command says so.** `tt` has an `__index` that raises, because
otherwise `tt.sendlnn('x')` is "attempt to call a nil value", which names
nothing. TTL has the same problem one level down, where an unknown command is
read as a variable and reported as a bad assignment.

## The trap in the cancellation hook

The interpreter checks `ScriptHost::cancelled` once per line. Lua has no such
seam, so a debug hook does it every few thousand instructions — which is what
makes `while true do end` answer the End button.

The hook has to be `'static`, because `mlua` stores it in the `Lua`, so it
cannot capture the borrowed host the way every other callback here does. It
calls a **scoped** function out of the registry instead, which can; Lua clears
its own `allowhook` while a hook runs, so that call cannot re-enter it.

And `pcall` catches an error raised from a hook as readily as any other, so a
script that wraps its own loop in one could otherwise report a clean finish
after being told to stop. `Script::run` asks the host again at the boundary,
which makes the *answer* honest — it does not make the script stop sooner, and
nothing can: Lua has no uncatchable error.

## Plugins

The user-facing installation and API guide is [docs/plugins.md](../../docs/plugins.md).

`Plugin` is the long-lived form of the same interpreter. Its top-level chunk
runs once, without a terminal attached, and declares window integration on the
`sterna` table. Each callback later receives the ordinary `tt` command surface
and keeps the plugin's globals and closure state from one event to the next.

```lua
local connects = 0

sterna.menu {
  menu = 'Control/Examples',
  label = 'Send uptime',
  shortcut = 'Ctrl+Alt+U',
  action = function()
    tt.sendln('uptime')
  end,
}

sterna.key('Ctrl+Alt+L', function()
  tt.sendln('logout')
end)

sterna.on('connect', function(event)
  connects = connects + 1
  print(event .. ' #' .. connects)
end)

sterna.on('disconnect', function(event)
  print(event)
end)
```

Menu paths are slash-separated. Shortcuts use Qt's portable spelling so the
same file says the same keys on Linux and Windows. Registration closes when
the top-level chunk ends: plugins cannot silently replace the window's menu or
keys from an event callback. The VM is `Send` so a frontend can keep it on the
same worker-thread boundary macros use; callbacks which wait or open a dialog
therefore block the plugin, not the window.

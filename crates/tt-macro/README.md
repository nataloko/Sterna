# tt-macro

A TTL macro, running against a terminal.

`tt-ttl` is the language with nothing attached; `tt-session` is a terminal with
nothing scripting it. This is the join, and it is a crate rather than a module
in either half for the same reason upstream makes it a process boundary: a
terminal should not drag in Oniguruma, and a language should not know what a
serial port is.

```sh
cargo test -p tt-macro          # needs no hardware and no server
```

## What a macro actually reads

**Not the wire.** This is the single most surprising thing in the macro
language and everything else here follows from it.

Upstream's `wait`, `waitln`, `waitregex` and `recvln` match against the bytes
`DDEPut1` collected, and `DDEPut1` is fed from `OutputLogUTF32`
(`vtterm.c:448`) — the same function that feeds the text session log. What
reaches it is:

- every character the parser decided to **print**, re-encoded as UTF-8;
- `CR`, `LF`, `BS` and `HT` at the moment those controls executed;
- a `CR LF` where a line **wrapped**;
- and nothing else.

So an escape sequence never reaches a macro, because the parser consumed it —
`wait 'ESC['` cannot match, ever. A character that was printed and then erased
*is* in the stream, because it was printed once. A lone `CR` is dropped and a
`CR LF` survives intact, which is why a `waitregex` pattern ending in `$` never
matches a line from an ordinary host. And the space upstream parks in the last
column before a wide glyph wraps is **not** in the stream, because it is
written with `BuffPutUnicode` rather than `PutU32`, so a macro's copy of that
line is one column narrower than the screen's.

The tap is in `tt-vt` (`Vt::set_macro_tap_enabled`) and the ring behind it is
in `tt-session` (`MacroLink`): 64 KiB, and **full drops the oldest byte**
(`ttdde.c:107`). That is the right way round — a macro that has fallen behind
wants the prompt that just arrived, and the alternative is a stalled script
freezing the window.

## Which thread things happen on

```text
  frontend thread                        macro thread
  ───────────────                        ────────────
  Session  ── pump ──► Vt ── tap ──►  MacroLink ──► read_byte
     ▲                                                  │
     │                                                  ▼
  MacroReceiver::service ◄──── job ────────────── SessionHost
     │                                                  ▲
     └────────────── answer ────────────────────────────┘
```

A macro blocks; the window does not. Upstream *cannot* block — its macro shares
a thread with the window, so `wait` parks itself in `TTLStatus` and the message
loop drives it back to life — and deleting that state machine is most of what
the port buys.

The macro thread never sees a `Session`. It sends a **job**, a closure taking
the session and the frontend, and waits for the answer. Two consequences worth
knowing:

- **Not a mutex.** An `Arc<Mutex<Session>>` would work until a macro held the
  lock through a modal dialog and the window stopped repainting — a frame rate
  decided by a script.
- **Nothing is borrowed across the boundary.** Every job owns its arguments and
  its answer. The SSH host-key prompt is the worked example of what breaking
  that costs: a nested event loop invalidating strings an open dialog is
  showing.

Bytes bypass the channel entirely. A `wait` asks for a byte thousands of times
a second and none of those should queue behind a repaint.

## The frontend's side

Two calls, from two native wakeups, with no timer:

```rust
rx.service(&mut session, &mut ui);   // whatever the macro asked for
session.pump(Duration::ZERO)?;       // whatever the line brought
```

On Unix, `MacroReceiver::poll_fd` wakes on the first and `Session::poll_fd` on
the second. Windows uses each receiver's `wait_handle` instead. `service` may
show a dialog, because a macro's `messagebox` is a job like any other — that is
exactly why the macro is somewhere else.

`MacroUi` is what a frontend implements: the eleven dialogs, the clipboard, the
menu, the window. Every method refuses by default, so a frontend with three
dialogs is useful and the rest report "Unknown command" instead of pretending.
`NullUi` is that state made explicit, and it is what the tests here run
against — so everything they prove is about the session half.

**One method is not a dialog and does not refuse**: `connect_ssh`. A macro's
`connect` opens serial, telnet and a local shell for itself, and SSH is the one
target it cannot — a host key or a password is a prompt, and a prompt belongs to
whoever owns a window. Its default answers "the connection did not come up",
which is `result` 1 and an outcome the documentation already promises; a
`connect` reporting "Unknown command" would be the larger lie.

## What a macro's `connect` does

The argument is a command line, and *which* command line depends on the
spelling. `connect` takes Tera Term's, through both parsers — TTSSH's hook and
then `_ParseParam`, which is the order upstream's own DDE arm gets by calling
through a re-hooked function pointer — and ends at the same `OnCommStart` a
startup line does. `cygconnect` takes **CygTerm's**, a different program's, and
that maps onto a local shell on a pty: `-s` is the command, `-ls` the login
dash, `-v` the environment, `-d`/`-cd` the directory.

What the line sets it keeps: the parse writes into the session's settings, the
way `ParseParam(commandline, &ts, NULL)` writes into `ts`, so a `/BAUD=` given
once outlives the connection it was given for.

## What is not answered yet

Listed at the bottom of `src/host.rs` with a reason each, because a macro that
is quietly lied to is worse than one that is refused. In short: the serial
control lines are on `SerialConn` rather than on `Transport`, so a `Session`
cannot reach them through the box it holds; `sendfile` is the File menu's path
rather than a protocol; `scp` wants an SSH channel `tt-conn` does not open; the
unmapped fallback and frontend actions make `sendkcode` more than a session
send; and the broadcasts and `wait4all` are about the *other* sessions and
belong to whatever owns the tab bar. `loadkeymap` is answered now: it replaces
the live session map and resolves a relative name through `changedir`.

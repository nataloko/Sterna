# tt-ctl

The control socket, and the two programs that talk to it. This is what DDE
was for.

Upstream's macro is a second process and every command it runs is a DDE
transaction — `teraterm/ttdde.c` and `ttpmacro/ttmdde.c`, 2,600 lines between
them. This port runs the macro on a thread inside the window, which deleted
all of it, and with it the only way anything outside the process could ask the
terminal for anything: a person clicking Control > Run macro, or a `/M=` on the
command line at startup, and that was the list.

So the socket is **not** a replacement for DDE's command set. That set is
`ttddecmnd.h`, it is the macro language, and it is implemented once already in
[`tt-ttl`](../tt-ttl) and answered by [`tt-macro`](../tt-macro). It is a
replacement for DDE's *reachability*: a running window has a name, and
something else on the machine can find it and ask.

```sh
ttctl status                       # the one window that is open
ttctl --to A1B2C3D4 sendln 'show version'
ttctl screen | grep -c 'error'
ttctl macro ./login.ttl router-3   # ...and wait for it, as a script would
ttpmacro /D=A1B2C3D4 login.ttl     # the shortcut somebody already wrote
```

No client library is needed on Unix and that is the test the wire was written
against:

```sh
printf '{"jsonrpc":"2.0","id":1,"method":"status"}\n' \
  | nc -U "$STERNA_CTL" | jq .result.title
```

## The wire

JSON-RPC 2.0, one compact object per line, over a Unix stream socket or a
Windows byte-mode named pipe. `serde_json`'s writer never emits a bare newline,
so a value cannot break the framing. Batches (§6) are refused with a reason
rather than silently: every method here changes the terminal or waits on it,
nothing wants one, and a client with two things to do sends two lines.

## The methods

Nine, against `ttddecmnd.h`'s ninety-odd, and the difference is the design:
everything complicated is a macro, because the macro language is the thing that
has been ported and held against upstream's own scripts.

| | |
|---|---|
| `ping` | is it there, and what is it called |
| `status` | connected, transport, size, log, macro |
| `send` | `text`, `bytes` or `base64` — and the choice is load-bearing |
| `sendln` | ...and a CR, which the text path expands by `ts.CRSend` |
| `connect` | a Tera Term command line, the same string a macro's `connect` takes |
| `disconnect` | hang up |
| `screen` | the grid as text, with optional scrollback |
| `macro.run` | a `.ttl` file, optionally waiting for it to end |
| `macro.stop` | the End button |
| `close` | close the window |

## Where the endpoint lives

`$XDG_RUNTIME_DIR/sterna/<name>.sock`, or `/tmp/sterna-<uid>/` for a session
with no runtime directory. The name is a `/D=` topic or the pid — the same
command line upstream uses to tell a launched `ttpmacro` which window it
belongs to, doing the same job through a different mechanism.

On Windows it is `\\.\pipe\sterna-<session>-<name>`. The session id prevents
two signed-in desktops from colliding; `FILE_FLAG_FIRST_PIPE_INSTANCE` makes a
duplicate `/D=` a bind error rather than another server instance. The pipe is
byte mode, local-machine only, and each accepted client is impersonated only
long enough to compare its token user SID with the window's. The accept thread
reverts before it parses a byte.

**The Unix directory is the access control**: `0700`, the socket `0600`, and
`SO_PEERCRED` behind both. Windows uses the session-scoped name, rejects remote
pipe clients and checks the token SID. Anything that reaches either endpoint
can type at whatever the window is connected to.

`$STERNA_CTL` holds the path, and the window puts it in the environment of
what it launches — so a script running *inside* the terminal can drive the
window it is running in. That is the one thing DDE could not do at all.

## Two divergences, both deliberate

**A client given no name refuses to guess between two windows.** `DdeConnect`
with a wildcard takes whichever conversation answers first, so upstream's
`ttpmacro login.ttl` with two Tera Terms open logs into an arbitrary one of
them. That macro usually types a password. `--to`, `/D=` and `$STERNA_CTL` are
the three ways to say which.

**`connect` with nothing openable is refused rather than answered with the New
Connection dialog.** Upstream opens it, which is right when a person clicked.
Off a socket a modal dialog blocks the window — and the client with it — until
somebody finds the window and closes it. The same rule puts the *successful*
open on the next turn of the frontend's event loop: every failure path in the
shell's `openTarget` is a modal box, and one raised inside a request holds that
request open.

## `ttpmacro`, which is a client now

`PLAN.md` has asked since Stage 0 that existing shortcuts and `.bat` wrappers
keep working. The command line is upstream's, parsed by
[`tt_ttl::cmdline`](../tt-ttl/src/cmdline.rs) — `ParseParam` and the four
`.bat` lines in `macroparam.bat` that are its specification — so a `/V` before
the file name is a switch and a `/V` after it is a parameter, here as there.
`/D=` names the window and the exit status is the macro's.

What does not survive, stated rather than hidden: `/V`, `/I` and `/S` do
nothing. All three describe the control window of a second process that no
longer exists. And `params[0]` is the file and its parameters rather than the
command line as typed, because what the window can see is what was sent.

## Layout

| File | What it is |
|---|---|
| `proto.rs` | JSON-RPC 2.0 and the line framing. Knows nothing about terminals. |
| `addr.rs` | Where a socket lives, what it is called, and how a client finds one — the job DDE's topic names did. |
| `ipc.rs` | Unix stream socket / Windows named-pipe byte stream and peer identity. |
| `channel.rs` | A request's way to the thread that owns the terminal. Deliberately parallel to `tt-macro`'s. |
| `host.rs` | The four things the *window* owns rather than the terminal. |
| `server.rs` | The accept loop and one thread per client. |
| `dispatch.rs` | The method table. |
| `client.rs` | The other end, for the two binaries. |
| `bin/ttctl.rs` | The shell's way in. |
| `bin/ttpmacro.rs` | The compatibility entry point. |

## Tests

```sh
cargo test -p tt-ctl              # the wire, the address, the dispatch
cargo test -p tt-ctl --test cli   # ...and both binaries, as subprocesses
tt-ffi/run_abi.sh                 # ...and the ABI, from C with a raw socket
cd ../shell && ./build/control_test   # ...and a real window's event loop
```

`tests/cli.rs` runs the binaries against a socket and a real `Session`, with a
thread standing in for the window — so argv to a job on the frontend's thread
is checked whole. The C ABI test drives the other end with `sprintf` and a
`sockaddr_un` and no JSON library, which is the closest this suite gets to the
shell script the design is for.

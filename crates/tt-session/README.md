# tt-session

A terminal attached to a connection. `tt-vt` turns bytes into a grid and keys
into bytes; `tt-conn` moves bytes; neither knows about the other, and something
has to own the loop between them.

**This is what the C ABI exports**, more or less directly — see
[`tt-ffi`](../tt-ffi/README.md). A frontend deals
with a `Session` and never with a `Vt` — which is the point, because the pieces
that have to move together (a resize is a grid change *and* an ioctl; a mouse
click is an encoding *and* a write) are exactly the ones a frontend gets wrong
when handed the parts.

```sh
cargo test -p tt-session                        # memory transport, and a real pty
TT_SERIAL_A=/dev/ttyUSB0 TT_SERIAL_B=/dev/ttyUSB1 \
  cargo test -p tt-session -- --test-threads=1  # and over real wire
```

One package at a time when the rig is involved: `--test-threads=1` is per test
binary, so asking for `tt-conn` and `tt-session` together puts both hardware
suites on the same two ports at once.

## Nothing here spawns a thread

`Session::pump(budget)` blocks for as long as the caller allows and no longer,
so *where* the loop runs is the frontend's decision. That survived contact with
SSH: `russh` needs a tokio runtime, and it lives inside `tt-conn`'s SSH module
rather than here, so this layer, the C ABI and the Qt shell all stayed
synchronous. What crosses the seam is a descriptor and a pump.

The corollary is that events are **drained, not delivered**. A callback would
have to be `Send` and would fire on whichever thread the pump happens to be on,
which is precisely what a UI toolkit cannot take.

## The transport seam

`tt_conn::Transport` is four methods, a `describe()` and a `closing_note()`.
Serial, SSH, telnet and a pty have almost nothing in common internally, and a
terminal needs almost nothing from them: bytes both ways, plus the things that
are *not* bytes — a line break, a byte that arrived corrupted, the far end going
away.

Keeping it short is deliberate. Anything transport-specific — baud rate, host
key, the pty's child — is reached through the concrete type before it is boxed,
so the trait does not grow a method per protocol.

`TransportEvent` is not `SerialEvent` for the same reason: a break is a serial
concept, but telnet has `BRK` and SSH has break requests, and all three mean
the same thing to the host.

## Things that are easy to get wrong, and are tested

- **A quiet line must stay cheap.** Silence is the *normal* state of a serial
  console. A pump that spins to its deadline burns a core for no bytes, and one
  that manufactures a `Damage` event makes the frontend repaint forever.
- **A reply goes out on the same pump that provoked it.** A host that sent DSR
  is usually blocked waiting; holding the answer until the next pump stalls the
  session by however long the frontend's timer is.
- **A short write keeps the rest.** Flow control is entitled to hold the line.
  Dropping what it held back loses keystrokes, which is the kind of bug people
  abandon a tool over.
- **Resize is two halves.** A grid that resized without telling the far end
  leaves `vi` drawing to the old size, and the symptom looks like a redraw bug
  rather than a missing ioctl. `connect()` announces the size too, or a pty
  starts at 80x24 and emits a screenful of wrongly-wrapped output first.
- **A disconnect is reported once and leaves the screen alone.** The text
  explaining *why* it dropped is the whole reason anyone looks afterwards. The
  transport gets asked for its own account of it — `close_note()` — **before**
  it is dropped, because a pty's exit status dies with the child handle, and
  "bash exited with status 1" is a different message from "disconnected". It
  survives a `disconnect()` and is cleared by the next `connect()`, so a status
  line can keep showing it while the window sits there.
- **Typing at a dead session must not queue forever.** Otherwise pulling a
  cable turns into a slow leak.

## `Event::Damage` is deliberately coarse

It says "the screen changed", not which rows. The measured baseline is a full
80x24 repaint in 3.9 ms on the target Qt (`PLAN.md`) — roughly 40x what a
115200 baud link can dirty — so per-row damage is an optimisation to add when
something says it is needed, not a thing to design around now. The event exists
so the interface does not have to change when it is.

## Session logging is a tap, not a second stream

`start_log` writes what arrives to a file. The half worth explaining is text
mode: it records what the **parser** decided to print, through a tap in `tt-vt`
at upstream's `FLogPutUTF32` seam. Stripping escape sequences with a scanner
beside the log would be a second parser to keep in agreement with the one that
is verified against Tera Term — the same argument as everywhere else here.

Raw mode is every byte, verbatim, and is silently untimestamped
(`filesys_log.cpp:243` clears the flag with the mode): a `[time] ` inside a
byte capture makes it no longer replayable.

Rotation renames generations rather than dating them, and walks **backwards**
from the oldest — forwards overwrites `.2` with `.1` before `.2` has moved to
`.3`, and the history quietly collapses to two files.

One deliberate divergence: upstream writes CR LF for each logged line
(`vtterm.c:361` sets `log_cr_type = 0`) and this writes LF, because the
artefact is a text file read in a pager on Linux. `LogOptions::crlf` gets a
byte-identical Tera Term log back.

## The viewport, and why a line has a number

Scrolling back lives here rather than in the frontend because it has to be
**anchored to content**: `follow_scroll` moves the offset by however many lines
left the page, so a scrolled-back view stays on the same lines while the host
keeps printing. Counting from the bottom instead means a stack trace walks off
the screen while it is being read, which is exactly the situation anyone scrolls
back in.

`row(y)` is viewport-relative — there is no second row function a painter could
pick wrongly between — and the cursor gets its own accessor, because it belongs
to the live screen and scrolling back moves it *down* and off the bottom.

`line_at`, `top_line` and `line` are the other half, and the distinction is the
point: **a row says where a line is and a number says which line it is.** Only
the second survives output. It is `Grid::scrolled_off`, so the top of the live
page is always `top_line()` — true by construction, since every scroll pushes
one line off and increments it — and a frontend holding a selection holds two of
these. `line()` reports a line that has been evicted, or one not printed yet, as
absent rather than making the caller range-check first.

## The one end-to-end test that never skips

`tests/pty.rs`. Every other composition test here needs something the machine
may not have — two serial ports wired back-to-back, an `sshd`, a `telnetd` — and
skips loudly without it. A pty needs nothing, so this is the suite that actually
proves the loop on a fresh checkout and in CI: a real child process, a real
descriptor, a real disconnect with a real exit status.

## Still to come

- **Per-row damage**, if a measurement ever asks for it. See above.
- **The settings surface.** `TtConfig` carries six fields where `tt_vt::Config`
  has thirty, because the rest are `TERATERM.INI` keys and belong to Stage 2's
  generated schema — transcribing them by hand now is work done twice, the
  second time as a deletion.

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
cargo test -p tt-session                                 # memory transport
TT_SERIAL_A=/dev/ttyUSB0 TT_SERIAL_B=/dev/ttyUSB1 \
  cargo test -p tt-session -- --test-threads=1           # and over real wire
```

One package at a time when the rig is involved: `--test-threads=1` is per test
binary, so asking for `tt-conn` and `tt-session` together puts both hardware
suites on the same two ports at once.

## Nothing here spawns a thread

`Session::pump(budget)` blocks for as long as the caller allows and no longer,
so *where* the loop runs is the frontend's decision — a Qt worker thread, a
tokio task once SSH arrives, a test's main thread. Baking a runtime in before
the second transport exists would be guessing at the shape of a problem we have
not met, and `PLAN.md` puts `tokio` under `tt-conn` for reasons that only bite
when `russh` lands.

The corollary is that events are **drained, not delivered**. A callback would
have to be `Send` and would fire on whichever thread the pump happens to be on,
which is precisely what a UI toolkit cannot take.

## The transport seam

`tt_conn::Transport` is four methods and a `describe()`. Serial, SSH, telnet
and a pty have almost nothing in common internally, and a terminal needs almost
nothing from them: bytes both ways, plus the things that are *not* bytes — a
line break, a byte that arrived corrupted, the far end going away.

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
  explaining *why* it dropped is the whole reason anyone looks afterwards.
- **Typing at a dead session must not queue forever.** Otherwise pulling a
  cable turns into a slow leak.

## `Event::Damage` is deliberately coarse

It says "the screen changed", not which rows. The measured baseline is a full
80x24 repaint in 3.9 ms on the target Qt (`PLAN.md`) — roughly 40x what a
115200 baud link can dirty — so per-row damage is an optimisation to add when
something says it is needed, not a thing to design around now. The event exists
so the interface does not have to change when it is.

## Still to come

- **Selection**, which is a frontend concept the core only has to support.
- **Session logging**, which is a Stage 1 deliverable and belongs here — it is
  a tap on the same byte stream.
- **The prompt lifecycle** `PLAN.md` describes for SSH (password,
  keyboard-interactive, host-key verification). It needs a transport that can
  ask a question, which serial cannot.

# shell

The Qt 6 Widgets frontend. It links `libtermitta.so` and includes exactly one
header — `crates/tt-ffi/include/termitta.h` — and that is the whole of what it
knows about the core.

```sh
# Qt work goes in the termitta-fedora container: Qt 6.11.1, matching the
# desktop. See CLAUDE.md for why the Ubuntu container's 6.4.2 does not count.
distrobox-host-exec distrobox enter termitta-fedora --no-tty -- bash -lc '
  export PATH="$HOME/.cargo/bin:$PATH"
  cd ~/Projects/termitta/shell
  cmake -S . -B build -G Ninja && cmake --build build
  ./build/render_test            # the painter, against grabbed pixels
  ./build/ssh_test               # the window's event loop, against a real server
  ./build/ssh_test --write /tmp  # ...and the four SSH dialogs, as PNGs
  ./build/telnet_test            # the same, over telnet
  ./build/termitta --port /dev/ttyUSB0 --baud 115200
  ./build/termitta myrouter      # an alias out of ~/.ssh/config
  ./build/termitta --telnet console-server:2001
'
```

`ssh_test` needs a server and skips loudly without one — start `ssh-audit`'s
(`cd ssh-audit && ./servers.sh start`) and set `TT_SSH_HOST`, `TT_SSH_PORT`,
`TT_SSH_USER` and `TT_SSH_KEY`. It is the only thing that drives the *window's*
event loop against a real connection: the core's own SSH tests poll in a busy
loop, so this is what would notice a `QSocketNotifier` on the wrong descriptor
or the handover from connection to session losing a wakeup — which looks like a
window that connects and then shows nothing.

CMake drives cargo, so one build command produces a runnable binary. Its
`CARGO_TARGET_DIR` is inside the CMake build tree rather than `crates/target`
— the two containers share a home directory, and a shared target directory
would hand a library linked against one distribution's glibc to a binary built
on the other.

`systemd-devel` is needed here for `libudev.pc`, which `serialport-rs` will not
build without. It is the Fedora spelling of the `libudev-dev` that the Ubuntu
container already needed.

## The event loop has no timer in it

This is the design decision everything else hangs off.

`tt_session_pump` blocks for the transport's read timeout, so calling it from
the UI thread freezes the window for as long as the line is quiet — which on a
serial console is nearly all the time. Calling it on a timer instead trades the
freeze for a wakeup every frame, forever, to discover that nothing arrived, on
a terminal whose whole claim is being light.

So the core hands out a descriptor (`tt_session_poll_fd`), a `QSocketNotifier`
waits on it, and the pump runs only when there is something to pump — with a
budget of **zero**, which reads exactly once and returns. A burst arrives over
several turns of the event loop and the window keeps painting through it.

Measured: **zero CPU ticks over five seconds** with a port open and idle, at
65 MB RSS — in line with `PLAN.md`'s ~60 MB Qt floor.

The one thing a descriptor cannot cover is output the far end refused. Flow
control holds the line, the write comes up short, and the remainder waits for a
pump that never comes, because a device asserting backpressure is not sending
anything to wake us with. `tt_session_pending_out` makes that visible, and
`Session` runs a 20 ms retry timer **only while it is non-zero**.

## The colour model is ported, not invented

`Theme::resolve` is `vtdisp.c:GetDrawAttr`. It is more elaborate than a
foreground and a background because Tera Term's bold, blink and underline
attributes each carry their *own* colour pair, and which one applies is a
priority chain — blink beats bold beats underline — not a blend. An explicit
SGR colour then overrides whichever won.

The defaults are upstream's, which is why **the terminal is black on white,
bold text is blue and underlined text is magenta**. That is what Tera Term
looks like out of the box. Every one of those values is a `TERATERM.INI` key,
so they belong to Stage 2's generated settings schema; they are constants here
so that the schema ends up being the only thing that ever parses them.

Three things that look like painter bugs and are not:

- **Truecolor pure red comes out dark red.** `SGR 38;2;255;0;0` resolves
  through upstream's nearest-colour search, which flips bright and dim when a
  full-colour mode is on — and 256-colour ships on. The cell stores index 1, so
  index 1 is what gets painted. "Correcting" it here would put the renderer at
  odds with the grid the differential suite verifies.
- **`SGR 101` does nothing.** aixterm's bright backgrounds are gated on
  `Aixterm16Color`, which ships off (`ttset.c:770`), so 90-97 and 100-107 are
  ignored and the previous pen stands.
- **A palette index is only a palette index when the attribute bit says so.**
  Without `TT_ATTR2_FORE` / `TT_ATTR2_BACK` the cell wants the *configured*
  default. Painting index 0 there gives a black-on-black screen, which reads as
  a parser bug.

## Text is drawn in runs, and that needs the font's help

One `drawText` per run of cells that look alike, because real console output is
mostly long stretches of one colour.

That is only safe because the font is given absolute letter spacing that makes
its advance exactly one cell (`Theme::recomputeMetrics`). Even a monospace face
rarely advances by a whole number of device pixels, and without the correction
a run of 80 cells drifts off the grid — the visible symptom being a cursor that
no longer lines up with the character under it. Wide characters are drawn alone
in their two-cell box, since they advance by their own metrics.

DEC special graphics are mapped here, not in the core: the grid stores the raw
byte plus `TT_ATTR_SPECIAL`, because upstream's `DecSpMappingDir` defaults to
"do not map". Turning `q` into a horizontal line is the renderer's job, and a
frontend that skipped it would draw a literal `q`.

## `render_test` is the only thing that can check the painter

The differential suite proves the *grid* matches Tera Term and stops where
cells become pixels. `QWidget::grab()` re-renders offscreen, which is both the
thing worth testing and the only screenshot available: GNOME's screenshot D-Bus
API has been locked down since 45, and `QScreen::grabWindow(0)` is blank under
xcb and null under Wayland.

The assertions are on background fills, which are solid rectangles whose colour
is the entire output of `Theme::resolve`. Glyph coverage depends on the font,
hinting and antialiasing, so text is only checked for "there is ink here and
none there" — which is font-independent and catches the failure that actually
happens: a screen holding all the right codepoints and rendering blank.

`./build/render_test --write <dir>` dumps a sample screen as a PNG, for looking
at a failure rather than guessing at it.

## Keyboard

The core owns the keymap, so `Session::sendKey` takes a `TtKey` and the core
decides what goes on the wire. Two keys are not `TtKey`s, because upstream
handles them in `KeyDown` rather than in the table `GetKeyStr` walks:

- **Return** sends `"\r"` as *text*, and `tt_session_send_text` applies LNM —
  upstream marks it `IdText` for exactly that reason.
- **Backspace** reads `tt_session_backspace_sends_bs` (DECBKM) and sends BS or
  DEL. The wrong one erases nothing and the host beeps, which reads as a broken
  keyboard rather than as a mode.

  **It defaults to BS (0x08), which is Tera Term's default and is probably not
  what you want on Linux.** `ttset.c:877` reads `BSKey` with an empty fallback
  and only the literal string `DEL` takes the other arm, so an absent key means
  BS. A Linux `getty` usually has `stty erase` set to `^?`, so backspace at a
  login prompt will echo rather than erase until the host sets DECBKM.
  Deliberately left faithful rather than quietly changed — it is the `BSKey`
  INI key, so the fix belongs in Stage 2's settings schema, and it is the first
  thing to make configurable there.

F1-F5 map to xterm's `XF1`-`XF5`, not to DEC's PF1-PF4. DEC put PF1-PF4 where a
PC keyboard has F1-F4, which is why two numbering schemes exist; every host
this will meet on Linux expects xterm's.

Two deliberate divergences from upstream, both frontend policy that becomes a
setting when the schema exists:

- **Alt is Meta**, prefixing an ESC. Tera Term's `ts.MetaKey` ships off, but
  every Linux line editor and Emacs expects it. This is also why the menu bar
  has **no `&` mnemonics**: Qt opens a menu on Alt+letter when one matches, and
  a menu that stole Alt+B from readline would be a menu people disable.
- **Ctrl+Shift+C / Ctrl+Shift+V** for the clipboard, because Ctrl+C has to stay
  an interrupt. Middle-click pastes the primary selection, and releasing a drag
  fills it.

## Scrollback follows the session, not the other way round

The wheel, `Shift+PageUp`/`PageDown` and the scrollbar all move the same thing:
`tt_session_set_view_offset`, in lines back from the live screen.

What is easy to get wrong is the direction of the dependency. **The core moves
the offset itself**, on every pump, so that a scrolled-back view stays on the
same *lines* while the host keeps printing — anchoring to the bottom instead
would slide what you are reading up by one for every line the device emits,
which is precisely the situation anyone scrolls back in. So the scrollbar
re-reads the offset after every pump rather than assuming its own last write is
still current, and its own updates are signal-blocked, or each pump would write
back into the session and the rounding would fight the offset the core chose.

The cursor gets the opposite treatment: it belongs to the live screen, so
scrolling back moves it *down* and off the bottom. `tt_session_cursor_view_row`
says where — or that it is not in view, at which point it is simply not painted.
Using `TtCursor::y` there would stamp a block onto a line of history and look
like a prompt that is not there.

Two consequences worth knowing:

- **Typing snaps to the live screen**, because typing blind into a screen you
  cannot see is worse than losing your place. `Shift+PageUp` is checked before
  the key table, which would otherwise send PageUp to the host.
- **Scrolling clears the selection.** It is held in viewport coordinates, so
  scrolling would leave the highlight sitting on whatever text moved under it.
  Anchoring a selection to the history wants the same work as selecting
  *across* a scroll, so both wait.

## Session logging

`Terminal > Start logging` writes what arrives to a file, with a `REC <size>`
in the status bar. Timestamps default to **elapsed** rather than wall clock,
because the question on a console is nearly always "how long after reset did it
stop", not what time it was.

The indicator is driven by `damaged`, not by a timer: the byte count changes
exactly when bytes arrive, and that is what `damaged` means. A one-second
ticker was the first version, and it was both redundant and against the point
of this event loop.

Text mode strips escape sequences because the *parser* does — the tap is inside
`tt-vt` at upstream's `FLogPutUTF32` seam. Raw mode keeps every byte and is
silently untimestamped, which is upstream's rule and the right one: a `[time] `
in the middle of a byte capture makes it no longer replayable.

## Connecting over SSH is a conversation, not a call

Everything else the shell does to the core is a function that returns. SSH is
not: the far end asks whether its host key is acceptable, then what the
password is, and the answers come from a person. So `Session::startSsh` returns
as soon as the attempt is under way, the questions arrive as signals, and
`answerHostKey` / `answerAuth` reply whenever the dialog closes.

Three things about that are worth knowing before changing it.

**A dialog spins a nested event loop.** The notifier fires again while one is
open, so `Session::pollSsh` guards with `m_sshWaiting`: without it the poll
re-enters, invalidates the borrowed strings the open dialog is showing, and
asks the same question twice.

**The prompts are copied out of the ABI, not held.** Everything
`tt_ssh_connect_host_key` hands back dies at the next poll, and the dialog
outlives that. `HostKeyRequest` and `AuthRequest` are the copies.

**One descriptor spans the handover.** `tt_ssh_connect_poll_fd` and
`tt_session_poll_fd` return the same fd, so `Session::rearm` asks whichever
owns it now and the `QSocketNotifier` never has to be replaced at the moment
output starts.

Cancelling an authentication dialog ends the attempt rather than sending empty
strings: a device that counts failures should not be walked toward a lockout by
someone who changed their mind.

### The host-key dialog says different things for different reasons

A first connection, a key of a new type, and a key that *changed* are three
different events, and presenting them the same way is how users learn to click
through the one that matters. A changed key gets the critical icon, defaults to
Disconnect, shows both fingerprints one above the other for comparison, and has
"Accept and remember" **disabled** — remembering it would overwrite the only
evidence that it changed, and Return should not be able to do that.

"Accept once" is the third button, and it is not padding. "Yes, but do not
write it down" is what someone on a network they do not trust means.

### Break is not offered over SSH

`tt_session_supports_break` decides whether the menu item is enabled, rather
than the window guessing from the transport. RFC 4335 defines a break request
and `russh` does not implement it, and a break is what someone reaches for when
a console has stopped answering — the worst possible moment to find out the
menu item was decorative.

## Telnet is one call, and one setting worth understanding

`Session::connectTelnet` is synchronous where the SSH path is a state machine,
and that is not an inconsistency: telnet asks no questions. A login prompt is
terminal output, typed into like any other.

The one field that matters in the dialog is the protocol mode, and it **follows
the port** — negotiated on 23, auto-detected elsewhere — until the user changes
it, at which point it stops following. That rule is upstream's and it is asked
of the core rather than reimplemented here, because getting it wrong is silent:
a terminal server's per-line port is not a telnet server, and opening at one
with a negotiation puts protocol bytes into somebody's serial console.

### A resize can arrive from the far end

Telnet's NAWS is defined client-to-server, and a console server sends it the
other way to say what the equipment behind it actually is. Upstream honours it,
so this does too — by resizing the **window**, not the grid. Setting the grid
directly would leave the painter drawing 132 columns into an 80-column widget
until the next resize event undid it. A window manager that refuses the resize
leaves the size where it was, and the status-bar notice is then the only record
that anything was asked.

The request is bounded before it is honoured. It comes off the wire, and an
800x600 terminal from a confused server is a window nobody wants and a grid
allocation nobody asked for.

## Not here yet

- **Word and line selection on double and triple click**, which wants the same
  word-boundary rules the scrollback selection will need — one thing to write
  rather than two.
- **Blinking cursor and blinking text.** Tera Term colours blink rather than
  animating it (`VTBlinkColor`, on by default), which is reproduced; an
  animated form would need a timer, and the point of this event loop is not
  having one.
- **Session profiles.** The dialog remembers the last port and settings for the
  lifetime of the window. Saving them is Stage 2's, with the INI reader.
- **A local pty**, which is the last transport Stage 1 names.
- **A host-key manager.** Removing a changed key still means editing
  `~/.ssh/known_hosts` by hand — the dialog says which file and which line, and
  that is as far as it goes.
- **Saved SSH sessions.** The dialog remembers the last host for the lifetime
  of the window, and `~/.ssh/config` covers the rest. Profiles on disk are
  Stage 2's, with the INI reader.

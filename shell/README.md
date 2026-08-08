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
  ./build/termitta --port /dev/ttyUSB0 --baud 115200
'
```

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

## Not here yet

- **Scrollback.** The grid has it; nothing exposes a viewport onto it, so the
  wheel has nowhere to scroll and selection is limited to the visible screen.
- **Word and line selection on double and triple click**, which wants the same
  word-boundary rules the scrollback selection will need — one thing to write
  rather than two.
- **Blinking cursor and blinking text.** Tera Term colours blink rather than
  animating it (`VTBlinkColor`, on by default), which is reproduced; an
  animated form would need a timer, and the point of this event loop is not
  having one.
- **Session profiles.** The dialog remembers the last port and settings for the
  lifetime of the window. Saving them is Stage 2's, with the INI reader.
- **SSH, telnet and pty**, each of which adds one connect path and, for SSH, the
  prompt lifecycle `PLAN.md` describes.

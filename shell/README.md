# shell

The Qt 6 Widgets frontend. It links `libsterna.so` and includes exactly one
header — `crates/tt-ffi/include/sterna.h` — and that is the whole of what it
knows about the core.

```sh
# Qt work goes in the sterna-fedora container: Qt 6.11.1, matching the
# desktop. See AGENTS.md for why the Ubuntu container's 6.4.2 does not count.
distrobox-host-exec distrobox enter sterna-fedora --no-tty -- bash -lc '
  export PATH="$HOME/.cargo/bin:$PATH"
  cd ~/Projects/Sterna/shell
  cmake -S . -B build -G Ninja && cmake --build build
  ./build/render_test            # the painter, against grabbed pixels
  ./build/ssh_test               # the window's event loop, against a real server
  ./build/ssh_test --write /tmp  # ...and the four SSH dialogs, as PNGs
  ./build/telnet_test            # the same, over telnet
  ./build/pty_test               # ...and over a local shell, which never skips
  ./build/xfer_test              # a ZMODEM send, driven by the event loop
  ./build/xfer_test --write /tmp # ...and the transfer dialogs, as PNGs
  ./build/macro_test             # a TTL macro, driven by the event loop
  ./build/macro_test --write /tmp # ...and the dialogs it raises, as PNGs
  ./build/print_test             # a printer, which is a file — needs no printer
  ./build/sterna --port /dev/ttyUSB0 --baud 115200
  ./build/sterna myrouter      # an alias out of ~/.ssh/config
  ./build/sterna --telnet console-server:2001
  ./build/sterna --shell       # a local login shell
  ./build/sterna --shell -- journalctl -f
'
```

The performance gate is `bench_shell`, and it is deliberately not built by
default — it is not a test, it takes tens of seconds, and its numbers depend on
the machine. Build it in a **Release** tree and run it from `bench/`:

```sh
cmake -S . -B build-release -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build build-release --target bench_shell
../bench/bench.py                # measures both halves, compares to the baseline
./build-release/bench_shell      # ...or just this half
```

`pty_test` is the one that needs nothing at all — no server, no hardware, no
environment variables — so it is the end-to-end check of this event loop that
actually runs everywhere, including CI. A pty also exercises the case the other
two cannot: a connection that ends *by itself*, with a reason.

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

## Connections: tabs, or tiles

`TerminalPage` is the lifetime boundary for one connection: its `Session`,
`TerminalView`, scrollbar, printer, macro runner, plugin state, modeless
transfer dialog and status strip stay together even while it is off screen.
`PanelContainer` above those pages owns their order and their placement, and
offers two arrangements which are **exclusive**.

**Single** is the ordinary one: the current connection fills the client area,
with a tab bar over it when there is more than one. **Tiled** hides the tab bar
and gives every connection a cell. The grid is `ceil(sqrt(n))` columns and as
many rows as that needs — 1, 1x2, 2x2, 2x2, 2x3, 2x3, 3x3 — and it keeps
growing rather than capping; the way back is one View-menu click. When the
rectangle does not come out even, the cell after the last connection carries
Serial, SSH, Telnet and Local shell buttons and is widened to fill the rest of
its row, so there is never a blank hole. At 1, 2, 4, 6 and 9 connections the
rectangle is exactly full and there is no such cell.

Tiles are tab order, exactly, so dragging a tab in Single decides which tile a
connection gets in Tiled. **There is no hidden page in Tiled** — that was the
0.2.x arrangement, where tabs and panels ran alongside each other and a
connection past the fourth could only be reached by evicting a visible one.
Switching either way closes nothing.

The marked terminal is the active one. Clicking anywhere in a tile — its
terminal, its scrollbar, its status strip — changes the aliases `MainWindow`
uses for menus, title, macros, transfers, plugins and the control socket.
Empty cells are widgets with connect buttons, not preallocated sessions;
accepting a dialog creates a page which lands in that cell because a new
connection appends to tab order, while cancelling leaves it empty.

`[Sterna] PanelLayout=single|tiled` is window-wide, and View > Tiled is what
writes it. `two` and `four` are the 0.2.x spellings and are read as `tiled`.
A change through the generated settings surface, a macro or a plugin is copied
into every open page and writes only that key immediately, preserving the rest
of the INI. Every visible `TerminalView` refits its own grid to the cell it
receives, and the window pushes a separate client-origin, client-size and
cell-size snapshot to each visible session after the layout settles. Re-tiling
never resizes the top-level window: the client area is divided, never
multiplied.

## The connect bar takes a destination, not a port

`ConnectBar` is one editable `QComboBox` and a Connect/Disconnect action. The
dropdown is rebuilt as it opens — that is when `/dev` and `~/.ssh/config` are
worth re-reading, and a terminal that enumerates either on a timer is a
terminal that never lets the machine idle — and it holds four groups:
`RecentConnection`s newest first, the ports plugged in now, the SSH aliases,
and a local shell, then New connection and Forget.

**A port something else has open is greyed out** and says whose it is. The
answer is re-asked only as the popup opens — `setRecents` reaches
`rebuildList` after every successful connect, so anything expensive in
`composeList` lands between a connect and its first prompt — and it is
advisory: `Entry::busy` decides how a row is drawn and never whether Connect
may run, because a holder that took no exclusive lock does not actually stop
the open. The busy state is deliberately kept out of `Entry::text` so that
`operator==` has to carry it; fold it into the words and a row that goes busy
repaints by luck rather than by comparison. See `docs/deviations.md` 16.

**Choosing a row fills the field, and `commit()` is what connects.** A combo
popup opens under the pointer, so the release that opened it lands on a row and
`activated` arrives without anybody having chosen anything; a bar where that
dials a host is a bar nobody can safely open. `m_chosen` holds the picked
record's index until the text is *edited*, so Connect opens the record and not
its label — the label has spaces in it and would otherwise be read as a
command line.

**The bar has no parser and no session.** Committing emits either
`recentChosen`, which carries a whole record, or `destinationEntered`, which
carries a string; `MainWindow::parseDestination` is the only place that decides
what a string means, and it is a pure function so the vocabulary can be
asserted without opening anything. Its rule: **whitespace switches to Tera
Term's parser entire**. A destination is one word — an alias, `ssh://user@host`,
`telnet://host:port`, a device path, `COM3`, `shell` — and anything with a space
in it goes to `tt_cmdline_parse_line` and then through `openTarget`, the same
seam the command line uses. The two cannot be merged because a bare host name
means SSH in one vocabulary and telnet in the other (`docs/deviations.md` 14).

**A record is not a parameter set.** `RecentConnection` holds the destination
and exactly the fields the connect dialog asks for — five for a serial line,
four for SSH, two for telnet, none for a shell — and `appliedTo` lays them over
the settings' `TtSerialParams`. Everything it does not hold stays a setting, so
`DtrControl` keeps following Setup instead of being frozen the day a connection
was first opened. The list is `[Sterna] Recent`, ten records separated by `;`,
each written the way its destination is spoken; a record that does not parse is
dropped rather than repaired, because that file is hand-edited.

Two rules keep the bar still, and both are pinned by `connect_test` because
both failures read as a bug in the wrong widget. The model is rebuilt only when
the composed list differs from the one already in it — the port dropdown this
replaced had that guard in one line, and losing it means invalidating a
combo's geometry at the moment somebody is reaching for its arrow. And the
Connect action reserves the width of the longer of its two words at
construction: without that, the button narrows when a session opens, the
toolbar reflows, and the expanding field beside it absorbs the difference, so
*connecting* resizes the destination box.

**One bar, one field, and several terminals behind it.** The field belongs to
whichever page is in front, the way every other control on the bar does:
`TerminalPage` keeps the `RecentConnection` its session was opened with and
`MainWindow::refreshConnectionSelector` puts it back when that page is
activated, so a window with three sessions in it can be asked what each one is
connected to by clicking on it. The record is kept beside its label because a
label cannot be parsed back — an SSH row's identity file and compatibility mode
are not in the words — and the label is kept beside the record so that
selecting a serial tab does not re-enumerate `/dev` to render a friendly name
it already had.

Two things it must not do. Only a *genuine* page change refreshes the field: a
click inside the page that is already in front comes through `activatePage`
too, and would erase a destination half typed. And a page connected to nothing
leaves the field alone rather than emptying it — the connect path itself runs
through here, because `ensureIdlePage` makes the new page before the connection
exists, so clearing on a blank page greys out New tab's own Connect button and
throws away the host somebody mistyped when a second connection fails.

Two things the port list this replaced did not have to answer. Enumeration is
not a shortlist — an ordinary desktop returns thirty-two motherboard `ttyS`
ports with nothing attached — so the group sorts real adapters first, shows six
and says how many it did not. And the field stays live under a live session:
`ensureIdlePage` gives the second destination its own page, so going somewhere
else opens a tab and never closes what is there.

## One status line per terminal

`PageStatusBar` is a strip along the bottom of each `TerminalPage`: the
connection's name, its link state (with the red chip when it is down), its
`REC` counter while it is logging, and its own transient messages — a
transfer's result, a notice from its session, printer, macro or plugin. The
window has **no** `QStatusBar`; every one of those facts belongs to a session,
and a window can be showing nine.

It doubles as the active-tile marker, so a tile has one row of chrome rather
than a header above and a status below. The marker appears only when more than
one tile is on screen. With a single terminal the strip sits where a status bar
would, so nothing looks different.

Two rules are load-bearing. Its labels are `Ignored` horizontally and elide
their own text: a status label that quoted its text as its width would push
`TerminalPage::sizeHint()` out, and the window would grow the moment a long
host name connected. And the `REC` counter is driven by `Session::damaged`,
which fires on every read on **every** open session — so a page that is not
recording costs one predicate, and a page that is costs a relayout only when
the formatted size actually moves.

## The Windows build

The same container cross-compiles it, with `mingw64-qt6-qtbase` — Fedora ships
that at the same 6.11.1 as the native package, so this is the desktop's Qt for
another target rather than an older one standing in.

```sh
mingw64-cmake -S . -B build-win -G Ninja      # the toolchain file comes with
cmake --build build-win                       # mingw64-filesystem
```

It also needs `mingw64-gcc-c++` and **`nasm`** — the latter is `aws-lc-sys`'s
assembler for the Windows target, so its absence stops the *core* several
minutes into what looks like a Qt build.

`TT_CARGO_TARGET` is what the two builds disagree about. Cross-compiling needs
`--target`, which moves cargo's output under a directory named after the
triple; a native build must not be given one. It defaults to
`x86_64-pc-windows-gnu` when CMake is cross-compiling to Windows and to empty
otherwise, and everything downstream — the library, the import library, the
bench emitter — is derived from it rather than assumed.

The DLL is copied beside the executables after the core builds, because
Windows has no rpath and the loader will not go looking in the cargo tree.
`sterna.exe` is a GUI-subsystem binary like `ttermpro.exe`; the console
subsystem would open a console window behind every session started from the
desktop, and closing that window would kill the terminal.

`pty_test` and `xfer_test` are the two left out, for their content rather than
their transport: every `pty_test` case is a shell script built out of `stty`
and `od`, and `xfer_test` needs `rz`. `control_test` builds, because its client
half is written twice instead of shimmed — a `sockaddr_un` and a `poll(2)` on
one side, `CreateFileW` and `PeekNamedPipe` on the other. Only those four calls
differ. The waiting above them is shared on purpose: what the test is really
about is that a client on the window's own thread has to wait *in the event
loop*, and that is true of the window rather than of the address.

Running them needs `wine-core` and this environment, which is two corrections
away from the obvious one — see AGENTS.md for what each looks like when it is
wrong:

```sh
export WINEPREFIX=$HOME/.wine-sterna WINEDLLOVERRIDES="mscoree,mshtml="
export WINEPATH='Z:\usr\x86_64-w64-mingw32\sys-root\mingw\bin;C:\windows\system32;C:\windows'
export QT_QPA_PLATFORM=offscreen
export QT_PLUGIN_PATH='Z:\usr\x86_64-w64-mingw32\sys-root\mingw\lib\qt6\plugins'
export STERNA_TEST_NO_SERIAL_PORTS=1   # avoid Wine's faulty SetupAPI enumeration
wine build-win/cmdline_test.exe
```

What that proves and what it does not: `control_test` and `cmdline_test` pass
outright — the first is the one of the four where Wine is a fair witness, since
a named pipe, a `QWinEventNotifier` and a queued close are all things Wine
implements properly. `macro_test` fails only the two cases that need a macro to
read what `cmd.exe` printed — Wine's console host opens a ConPTY and then
delivers no output. `render_test` runs its whole suite and fails six font
metrics, which is Wine's font stack rather than an answer about Windows.
Nothing left failing there is ours; native Windows is still the authority for
the other three.

`STERNA_TEST_BUSY_PORTS` is the other hook of that family, and the one that
makes the greyed-out rows testable anywhere: `path=program`, comma-separated,
`STERNA_TEST_BUSY_PORTS=/dev/ttyUSB0=minicom,/dev/ttyUSB1=`. It **replaces**
the answer the platform would give rather than adding to it, which is what
keeps `connect_test` deterministic on a machine with a rig plugged in as well
as on a CI runner with no serial hardware at all; an empty program after the
`=` is a holder nobody could name, which is what a root-owned process looks
like in production. Never set outside a test.

## The updater is loaded only when there is a check to make

Two triggers, and nothing else: Help > Check for Updates, and — while
`[Sterna] CheckUpdatesOnStartup` is on, which is how it ships — a check three
seconds after startup, at most once every 24 hours. Either one loads
`sterna_updater` from the installed tree, creates its `QObject` through one C
symbol and invokes `check` or `checkQuietly` by name. The main executable
therefore does not link Qt Network or map a TLS backend during an ordinary
terminal session; the direct-link prototype cost about 5 MB of idle PSS before
it had made a request, and a session that is not due a check still maps neither.

The startup one is silent until it has something to say. No progress dialog, no
"Sterna is current", and no complaint about a release server that cannot be
reached or a manifest that does not verify — a box on every launch is how people
learn to turn a security feature off. An available update is offered, and from
there it is the same path the button takes. It also steps aside for a modal
dialog: a check that would land on top of an SSH password prompt is skipped
until the next start.

`[Sterna] LastUpdateCheck` is the schedule, ISO-8601 UTC, written when a request
goes out rather than when one succeeds — so an unreachable server costs one
attempt a day, not one per launch. Clearing it means "check at the next start";
a stamp that does not parse, or that is in the future because a clock moved
back, means the same thing. `MainWindow::checkForUpdatesOnStartup` makes that
decision before the updater is loaded, which is why `updateCheckDue` is a free
function in the terminal rather than a method on the updater; `main` calls it,
never the constructor, so the tests that build a `MainWindow` reach no network.

The updater verifies the detached manifest signature before it reads a version,
URL or size from that file. A confirmed download is bounded by the signed size
as it arrives, then checked for exact size, SHA-256 and an artifact Ed25519
signature — both from one read-only handle over one mapping, so there is no
window between hashing the bytes and verifying them. Linux atomically replaces
the current AppImage for the next start; Windows launches the verified NSIS
installer elevated and lets that installer wait for this process before it
touches the installed tree. A loose build opens the release page rather than
replacing an arbitrary file.

That read-only handle stays open until the platform has been handed the file,
and on Windows the download's own `QTemporaryFile` has to be destroyed first.
`close()` on one closes nothing — the object keeps the file open so its unique
name stays reserved — and Windows will not create an image section for a file
another handle holds open for writing, so the installer cannot start at all
while the object lives. The reader is not a writer and Qt opens it without
`FILE_SHARE_DELETE`, so keeping it is what pins the verified bytes to the path
being executed. See `AGENTS.md` for what the failure looks like, which is a
message from the shell rather than from Sterna.

`update_test` covers the committed signer fixture, strict manifest parsing,
atomic executable replacement, and that a detached download outlives its
temporary file — on Windows, that the loader's own open fails before the detach
and succeeds after it with the reader still held. `update_load_test` links no Qt Network and
proves the exact dynamic-library seam the terminal uses, including destruction
before unload. Package-specific signing, TLS and installer details live under
`packaging/`.

## The event loop has no timer in it

This is the design decision everything else hangs off.

`tt_session_pump` blocks for the transport's read timeout, so calling it from
the UI thread freezes the window for as long as the line is quiet — which on a
serial console is nearly all the time. Calling it on a timer instead trades the
freeze for a wakeup every frame, forever, to discover that nothing arrived, on
a terminal whose whole claim is being light.

So the core hands out the platform's native wakeup: a descriptor watched by
`QSocketNotifier` on Unix, or an event watched by `QWinEventNotifier` on
Windows. The pump runs only when there is something to pump — with a budget of
**zero**, which reads exactly once and returns. A burst arrives over several
turns of the event loop and the window keeps painting through it.

Measured: **zero CPU ticks over five seconds** with a port open and idle, at
65 MB RSS — in line with `docs/history.md`'s ~60 MB Qt floor. Re-measured with a local
shell attached, which is the harder case because there is a live child process
on the other end: 80 ms to start and paint `bash`'s prompt, then **zero ticks
over the next six seconds**, at 72 MB.

The one thing a descriptor cannot cover is output the far end refused. Flow
control holds the line, the write comes up short, and the remainder waits for a
pump that never comes, because a device asserting backpressure is not sending
anything to wake us with. `tt_session_pending_out` makes that visible, and
`Session` runs a 20 ms retry timer **only while it is non-zero**.

### One frame per read is one frame too many

"A burst arrives over several turns of the event loop" has a cost the design
did not account for: each of those turns ends in a repaint, so ten megabytes of
`cat` was painted three thousand times — and a frame costs about what parsing
8 KB does. Wayland's frame callbacks were coalescing about eight reads into a
frame and hiding it; X11 has no such brake, and measured **4 MB/s against
Wayland's 39 on the same machine**.

`TerminalView::requestRepaint` puts a floor of 8 ms under the frame interval —
125 a second, above any display refresh this will meet. X11 went to 36 MB/s and
the keystroke latency did not move (1.03 ms to 1.05), because a floor is not a
timer in the idle path: an idle window has not painted for a long time, so a
keystroke still repaints on the spot, and the timer exists only while output
outruns the floor. Same shape as the pending-out retry above.

The alternative — a time budget on `tt_session_pump`, so one wake consumes
several reads — is the wrong one. The pump reads until the line is quiet, and
serial and telnet both read with a 50 ms timeout, so the second read of a burst
would block the UI thread for 50 ms. Coalescing the frames costs nothing and
does not care what the transport is. See `bench/README.md` for the table.

## The colour model is ported, not invented

`Theme::resolve` is `vtdisp.c:GetDrawAttr`. It is more elaborate than a
foreground and a background because Tera Term's bold, blink and underline
attributes each carry their *own* colour pair, and which one applies is a
priority chain — blink beats bold beats underline — not a blend. An explicit
SGR colour then overrides whichever won.

The defaults are upstream's, which is why **the terminal is black on white,
bold text is blue and underlined text is magenta**. That is what Tera Term
looks like out of the box. They are compiled in as a starting point and then
replaced by `Theme::applySettings`, which asks the core for `color.normal` and
its siblings **by name** — so this file holds no list of settings and no
parser, and adding a colour setting to the schema is all it takes to honour
one.

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

`ANSIColor` is the exception that cannot be read as one more Qt colour. It
changes the palette used by the core's nearest-colour search *while SGR is
parsed*, as well as the RGB the painter uses later. `Theme::applySettings`
therefore asks `Session::paletteRgb` for the live 256-entry table instead of
parsing the setting itself. The first sixteen entries arrive already converted
from the file's legacy order; keeping that conversion on the core side gives
the search and the painter one answer.

Four settings act only at this last step. Bold and underline each have an
independent font gate and colour gate, so a cell may be blue in a regular face
or bold in the normal colour. `UseNormalBGColor` replaces an attribute pair's
background with the normal one. `UseTextColor` is not a general contrast fix:
it replaces only an explicit black-on-black, white-on-white or bright-white
pair, after reversal has been decided. In the reversed arm it uses the
configured reverse pair even when that pair is otherwise disabled. The grabbed
pixel tests pin these combinations because no core or differential dump can.

**And one shade that is not upstream's at all.** While the session has nothing
on the other end, `Theme::resolve` moves the background it arrived at
`color.disconnected_shade` percent of the way towards `color.normal`'s
foreground — but only when the host did not choose that background itself, which
is what the `hostBackground` flag through the function tracks. So a bold run,
whose configured pair carries its own background, shades with everything around
it, while `SGR 41` does not. The shade is why `Theme` holds one piece of
session state (`setConnected`), and why `TerminalView` repaints the whole view
on a connection edge rather than a damaged region. `docs/deviations.md` entry
13 has the reasoning, including why the blend runs towards the foreground and
what that means for a reversed cell.

## Highlight rules are the one colour the host did not choose

Everything in the section above is upstream's, and every colour in it is
decided by the far end. A highlight rule is the exception: the user's own
regular expressions, recolouring what is on the screen.

**Nothing is written into a cell.** The core matches the visible rows as the
painter asks for them — `Session::rowHighlights(y)`, beside the selection range
`paintEvent` already computes — and hands back column spans. So the grid still
holds exactly what the host sent, the log and the clipboard and the printer see
an unhighlighted terminal, the receive path is untouched, and a rule written now
colours text that arrived an hour ago, scrollback included. Matching is over the
*logical* line, so a wrapped command is one line to a pattern.

The span reaches `Theme::resolve` as a `CellOverride` and applies **last**,
after the priority chain and after the `UseTextColor` repair, so nothing can
take back a colour the user asked for. It goes through the same reverse flag an
SGR colour does, which is why a selection dragged across highlighted text still
inverts. A rule's bold and underline reach the font and the stroke and
deliberately not the configured bold and underline *colour pairs*: "underline
this" must not also mean "and repaint it magenta".

The run-batching below needed no change for any of it. Its key is already
`(fg, bg, bold, underline)`, so a highlighted span becomes its own run.

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

## The mouse pointer has a setting of its own

`MouseCursor` chooses an arrow, I-beam, cross or hand, independently of the
terminal's painted text cursor. The file's names are interpreted
case-insensitively, and an unknown raw name is a no-op rather than an implicit
default. That is why the shell keeps the spelling and interprets it when the
pointer is applied instead of asking the schema to normalise an enum.

A clickable URL temporarily uses the hand. Moving away reapplies
`MouseCursor`, so the pointer returns to the configured shape — not always to
the shipped I-beam. URL recognition, colour and underline remain independent
of both pointer settings.

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
- **Scrolling does not clear the selection**, because the selection is held in
  absolute line numbers rather than in rows. See below.

## The selection is anchored to lines, not to rows

A row number means "wherever this line has slid to by now", which is the wrong
thing to hold: the case people copy in is a line off a device that is *still
printing*, and a highlight held in rows stays where it is while the text walks
up the screen underneath it. So both ends are a `Session::line` number — the
core's `line_at`/`top_line`/`line` — and scrolling, new output and even the
history evicting the selected line all leave the selection meaning what it
meant. A line that has aged out is skipped from the copy rather than coming
back as a blank.

The rest is upstream's, and each piece looks arbitrary until you find the
original:

- **Endpoints round to the nearest boundary between characters**
  (`buffer.c:GetCharCell`), so dragging across `abc` selects `abc` rather than
  `ab`. A wide character is taken or left whole from either half.
- **Double click selects a word, on `ts.DelimList`'s default set** — a space
  and every ASCII punctuation mark *except* underscore, so `some_name` is one
  word and `some-name` is three. `CheckDelimiterChar` has two arms rather than
  one: starting on a delimiter takes the run of *that same character*, which is
  what makes double-clicking the gap between two columns of output select the
  gap. Starting anywhere else takes the run of non-delimiters, and stops where
  the character width changes while `ts.DelimDBCS` is on. That setting ships
  on; turning it off makes mixed-width non-delimiter text one selectable word,
  without ever allowing half of a wide character into the selection.
- **Triple click selects the line.** Qt has no triple-click event — the second
  press arrives as `mouseDoubleClickEvent` *instead of* a press — so the run is
  counted in the widget or the third click never reaches three.
- **The anchor is the whole unit the drag started on**, not the point it
  started at, which is what lets a double-clicked word be dragged *leftwards*
  and keep its right-hand edge. Upstream keeps the same pair
  (`DblClkStart`/`DblClkEnd`).
- **Edit > Select screen and Edit > Select all** are `BuffScreenSelect` and
  `BuffAllSelect` (`buffer.c:716`, `:704`) — the only selections nobody
  dragged. Screen means the lines the window is *showing*, so scrolled back it
  is the history in front of you rather than the live page; all means the
  scrollback and the page together. Both end at the last column of the last
  line rather than at column 0 of the line after it. Upstream ships the second
  form and left the first commented out one line below; the two mark the same
  cells, and the difference is only whether the copy ends with a line break —
  but here they would disagree with each other, because a line past the end of
  the buffer is not one the core can answer for, so upstream's spelling gives
  Select screen a trailing break while scrolled back and nowhere else. Neither
  command takes a keyboard shortcut: upstream gives them none, and a `QAction`
  shortcut is a key the host stops receiving.

A drag held outside the window scrolls it, on a timer that runs only while that
is true — same shape as the repaint floor and the pending-out retry. Without it
a selection can never be longer than one screen.

Two things still clear a selection, both because a resize re-flows every line
so the numbers stop meaning what they meant: the widget's own resize, and one
that arrives in the byte stream (DECCOLM, or a telnet NAWS from the far end),
which is noticed on the next pump rather than through a resize event.

## Session logging

`File > Log...` writes what arrives to a file, with a `REC <size>` on that
terminal's own status line. Timestamps default to **elapsed** rather than wall
clock, because the question on a console is nearly always "how long after reset
did it stop", not what time it was.

The indicator is driven by `damaged`, not by a timer: the byte count changes
exactly when bytes arrive, and that is what `damaged` means. A one-second
ticker was the first version, and it was both redundant and against the point
of this event loop.

Text mode strips escape sequences because the *parser* does — the tap is inside
`tt-vt` at upstream's `FLogPutUTF32` seam. Raw mode keeps every byte and is
silently untimestamped, which is upstream's rule and the right one: a `[time] `
in the middle of a byte capture makes it no longer replayable.

### The dialog is upstream's shape, minus a window it does not have

`LogDialog.{h,cpp}`. Tera Term 4 customised the Win32 common save dialog with a
strip of options; Tera Term 5 replaced that with `IDD_LOGDLG`
(`logdlg.cpp:267`) — a filename field, a `...` button onto the real picker, and
the options underneath. That is this dialog, which is also the only shape Qt
can offer: the desktop's file dialog is a portal on the other side of D-Bus and
nothing can be bolted to it.

Upstream's enabling rules are the dialog's only real logic and they are
reproduced (`ArrangeControls`, `logdlg.cpp:167`): Append is greyed until the
name names something that exists, binary greys plain text and the whole
timestamp row, and the byte-order mark is only offered for a new text file.
Two upstream defects are deliberately not: the `GetCurSel() - 1` the timestamp
type is written with against the plain index it is read back with
(`logdlg.cpp:106` versus `:322`), and "New / Overwrite" implemented as a
`DeleteFileW` before the open (`vtwin.cpp:4142`), where a file that cannot be
deleted quietly becomes an append.

**Hide dialog is not here**, because there is nothing to hide: upstream's
logging status window — file name, byte count, Pause, Close — is the thing the
per-terminal `REC` counter replaced. **The UTF-8/UTF-16 combo is not here**
either; a log is written from a Rust `String`, so it is UTF-8 and only the mark
is a question. **Rotation is here** where upstream keeps it on its Setup page,
because whether a capture should roll over is a fact about the capture. Its
unit is a multiplier and nothing else — `LogRotateSize` is bytes whatever
`LogRotateSizeType` says — and a "keep" of zero is upstream's ten thousand
generations rather than none, so the spin box says so.

On OK the dialog writes every control that has a `TERATERM.INI` key back to the
live settings, which is what `SetLogFlags` does to `ts`; only Setup > Save puts
any of it in the file. The two questions no key answers — the mark, and whether
to write the screen in first — ride the `TtLogOptions` struct instead, which is
the one call in the shell that passes one rather than a null.

### Pausing, and where the button went

`File > Pause logging` is checkable and so is the `REC` indicator: clicking the
counter pauses the log it is counting. Tera Term's Pause is a button on the
logging window this program does not have, so the thing showing the count is
the thing that stops it. Paused, the label says `PAUSED`, turns amber and stops
blinking — a steady number is the honest shape for a count that has stopped,
and the blink is what says a recording is running.

**What arrives while a log is paused is discarded, not held.** That is upstream
in two places at once — a binary log drops the byte at the input
(`filesys_log.cpp:1038`) and a text one drops it on the way out of the ring
(`:647`) — and it is the point of the feature: a pause that buffered would
write the gap into the file the moment it ended.

`PageStatusBar::setLogging` compares its whole state before returning early. It
is reached from `Session::damaged`, so it runs on every read on every open
session and the early return is load-bearing; a paused flag left out of that
comparison would never repaint. Same shape as `ConnectBar::Entry::operator==`.

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

**One native wakeup spans the handover.** The SSH-connect and session calls
return the same fd on Unix or event on Windows, so `Session::rearm` asks
whichever owns it now and the Qt notifier never has to be replaced at the
moment output starts.

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

## A local shell has no dialog, and a disconnect now has a reason

"Local shell" is a menu item that connects, because there is nothing to ask:
the shell, the size and the environment are all already known, and a dialog
whose only button is OK is one nobody wants twice. From the command line it is
`--shell`, with the positional arguments taken as the command to run instead —
`sterna --shell -- journalctl -f`, the spelling `xterm -e` and
`gnome-terminal --` already use.

The half worth recording is what it did to the *disconnect* path. Every other
transport ends the way it looks: an adapter is unplugged, a socket closes, and
"Disconnected" is the whole story. A local shell is different — it exits, with a
status — so `tt_session_close_note` is asked before the generic wording is used,
and the status bar says "bash exited with status 1". That routes through the
core rather than through a `if (transport == pty)` here, because the frontend
should not be the thing that knows which transports have something to say.

A network disconnect has one more outcome. `AutoWinClose` arrives as a core
close request because only this layer owns a window; serial and local shells
never ask. The window accepts it only while enabled, matching upstream's guard
against a socket disappearing inside a modal dialog's nested event loop.
`ClearScreenOnCloseConnection` is handled before that boundary: its blank page
is a page whose old contents moved into scrollback, not an erase.

## The title names the connection

`TitleFormat` is the same six-bit word Tera Term reads. Its shipped value is
13, so a connected window says `<endpoint> - <title> VT`; before and after the
line is ready it says `<title> - [connecting...] VT` or
`<title> - [disconnected] VT`. TCP port and serial speed are separate bits,
and a local pty uses its command description as the endpoint Tera Term has no
native word for.

The speed is asked from the live transport rather than copied from
`serial.baud`. A command-line `--baud` may never have touched that setting, and
a macro's `setbaud` changes the port again; the successful reset raises a title
event so the new value appears immediately. The file value is a 16-bit word,
not a range-clamped integer: values wrap modulo 65536 and unknown bits 6–15 are
kept.

## Alt is Meta only when the file says it is

`MetaKey` ships off, so Alt belongs to the desktop and the menu until the user
enables all, left or right Alt. The side-specific forms are tracked from the
native Alt press because Qt's later character event says only that *an* Alt key
is down. Once Meta is active, `Meta8Bit=off` prefixes ESC, `raw` sets the high
bit on the byte through the binary ABI, and `text` sets U+0080 on the character
before the ordinary UTF-8 text path encodes it. Those are different byte
streams and the real-pty test asserts each one.

`StrictKeyMapping` means a special key with no `KEYBOARD.CNF` entry has no
built-in fallback; it does not mean stricter validation. The file is loaded
beside the active settings file, can be replaced with `/K=` or Setup > Load key
map, and maps physical PC scan codes rather than layout-dependent characters.
Wayland, X11 and Windows native codes are normalised before the Shift, Ctrl and
Alt bits are added. `DeleteKey=on` is upstream's explicit exception and sends
DEL anyway.

Font rasterisation is applied at the painter boundary. `FontQuality` maps to
Qt's default, antialiased or non-antialiased request; ClearType uses the
antialias request away from Windows and leaves subpixel policy to the platform.
`DrawingResizedFont` gates horizontal fitting for a fallback glyph whose
natural advance does not match its cell. It does not remove the small letter
spacing correction that keeps an ordinary 80-column run on the cell grid.

## The setup dialog has no list of settings in it

`SettingsDialog` walks `tt_settings_field`, the core's metadata table: a tab per
page, a widget per kind, the bounds of a spin box from the schema's own range,
a combo box built from the INI's own spellings, and the citation for the
default in the tooltip. Adding a setting is a line in `schema/settings.txt`;
nothing here changes.

**That is the whole argument for the schema**, and it is `PLAN.md`'s risk 2 —
76 dialog templates and ~13.8k lines of dialog code are where the motivation
goes to die. The original sketch was to *generate* the dialog as C++, which is
worse: a second copy of the list, in the other build system, that every schema
change has to be pushed through. Reading the table at runtime leaves nothing to
keep in step.

Three things it does deliberately:

- **Only what changed is written.** A dialog that applied every field would pin
  all 296 settings into the user's file the first time it was opened, and a
  pinned setting stops following upstream's default for ever.
- **The combo box shows the file's spellings**, not prettified ones. Upstream
  compares `TerminalID` with `strcmp`, so `Vt320` would read back as a VT100.
- **A unique `.lng` label is translated; a shared one is not.** Several schema
  rows point at one upstream group caption — the foreground/background colour
  pairs, for example. Giving that translation to every generated row would
  make different settings display the same name, so those keep the clear name
  derived from their dotted setting.

Applying reaches the running terminal, and **it overwrites modes the host set** —
`ts.BSKey` is the same variable DECBKM writes, upstream and here. The size is
the window's business rather than the painter's: the *window* is resized and
the view fits the terminal to it, the same path a remote NAWS resize takes.
`AlphaBlendActive` and `AlphaBlend` likewise reach the top-level window: their
0..255 values become Qt opacity and switch as focus enters and leaves. An
active value omitted from the file inherits the loaded inactive one.

## Quick buttons are user keys with a face on them

The bar down the right holds commands the user defined, and almost none
of it is new code. A quick button *is* a `KEYBOARD.CNF` user key: the four
kinds are `UserKeyType`, the value carries the same `$HH` escape, and pressing
one calls `Session::run_user_key` — the arm `send_key_code` calls after it has
looked a scan code up. So text goes out through `send_text` with `CRSend` and
LNM applied, bytes go out raw, and the two that are not sends come back to the
window as `TT_KEY_CODE_MACRO` and `TT_KEY_CODE_COMMAND`, which is exactly what
a mapped key already returned. `MainWindow::runKeyAction` is the one dispatcher
for both.

The list is not settings. `[Sterna Buttons]` is parsed by
`tt-config/src/buttons.rs`, because a list of records is what the schema cannot
describe, and it reaches C++ through `tt_quick_buttons_*` — which hands out
each value twice, stored and decoded, so the escape has one implementation and
it is not this one. `QuickButtonsDialog` therefore edits plain text and the
core does the escaping when the window saves.

**`window.quick_buttons` alone decides whether the bar is there.** An empty
list still shows it, because the panel with nothing on it is the `+` that
defines the first command, and that is the shortest route into a feature
nothing else advertises.

**Its width comes out of the window, never out of the terminal.** The bar and
the terminals share one central widget — a `QHBoxLayout` of `PanelContainer`,
the grip, and the bar at a fixed width — rather than the `QDockWidget` this was
until 0.5.4. A dock separator divides the client area, so a drag took its
pixels off a terminal fitted to whatever was left in whole cells; a few pixels
is a column, a column is a real `Grid::resize`, and that truncates every line
it shortens in the page and in the scrollback. Dragging the panel destroyed
text and did not give it back on the way out.

`MainWindow::resizeQuickPanel` is the whole rule. It clamps the width so the
window never has to steal — shrinking always works, growing only while
`windowGrowthRoom()` says there is space between the frame and the edge of the
screen's work area — grows the window before it takes the pixels and shrinks it
after it gives them up, and holds every page's grid across the change.

**The hold is not an optimisation.** `setFixedWidth` marks the layout dirty and
Qt lays out on the next turn, while a top-level `resize()` is a request the
compositor answers when it likes; between them is a pass in which the view is
narrower and the window has not caught up. `TerminalView::setGridHeld` stops
that pass reaching `Session::resize`, and remembers whether it swallowed a
refit — a geometry change delivered during the hold does not come again, so
that one is answered on release, while a hold that swallowed nothing is left
for the resize event still on its way.

**Showing the panel is a resize too**, and it was the second route to the same
lost text: `onSettingsChanged`'s own window-grow arm runs while the panel is
still hidden, so it finds nothing to absorb, and it is gated on a single page
in the untiled layout anyway. Both routes go through `resizeQuickPanel` now,
which carries no such gate — a panel beside four tiles is the same question.

`window.quick_buttons_width` is `0` for "as wide as the buttons need", which is
what ships and what the panel did before it had a width at all. Only the end of
a drag writes a number there. Nothing writes one at close: a backstop comparing
the live pixels against the setting would find `0` against whatever the
captions measured and pin every window that had never been dragged.

**A button is as wide as the panel, not as wide as its caption.** The bar is a
plain `QWidget` with a `QBoxLayout` of `QToolButton`s, which it was not until
that mattered: `QToolBarLayout` sizes every item to its own text and centres it
across the bar's thickness, whatever size policy the button carries, so room
dragged out went into the margins beside a ragged column of captions rather
than into the buttons. The one lever that does move it — a minimum width on
each button — raises the bar's own minimum with it, and the panel can then grow
but never shrink. Both measured. The buttons keep the top of the panel rather
than being centred along it, for the reason a repeat count lives in the tooltip
and not in the caption: a bar of things to click must not move when the window
is resized or a button is added.

Two traps live here:

- **Deleting the buttons does not delete the actions.** They are children of
  the bar rather than of the buttons, so that `findChild` can install a
  shortcut on one; a rebuild that leaves them alive keeps every previous action
  answering its shortcut and hands `findChild` a button that is no longer on
  screen — the symptom is a button that stops following the session.
  `buttons_test` found it under the `QToolBar` this replaced, where the same
  fact wore `QToolBar::clear()`'s name.
- **A shortcut is a key the terminal stops receiving.** A `QAction` outranks
  `TerminalView::keyPressEvent`, silently, which is why no button ships with a
  shortcut and why the editor checks a sequence against the window's actions,
  Lua plugins, the loaded `KEYBOARD.CNF` and the keys a host plainly wants.
  `TerminalView::scanForSequence` is the inverse of what `keyPressEvent` does
  on the way in, and it lives beside that table rather than copying it. The
  shortcuts sit on the bar's own actions, so hiding the bar hands the keys
  back.

## Language catalogs stay Tera Term language catalogs

All 14 UTF-8 `.lng` files are vendored byte-for-byte under `vendor/lang/` and
installed under `share/sterna/lang`; the build-tree path is only a development
fallback. `settings.language_file` is upstream's `UILanguageFile`, including
its `lang\Default.lng` fallback, so a shared `TERATERM.INI` selects the same
file spelling in either program. The setup dialog offers every shipped file by
its own `[Info] language` name.

`I18n` owns the opaque catalog from the flat ABI. Lookups cross as a pointer
plus a length rather than as a C string because upstream file-dialog filters
contain embedded NULs. Main menus and their actions retranslate in place when
the setting changes; the menu structure remains the only list of actions.

The `.lng` menu text needs one deliberate adaptation. Upstream includes Win32
mnemonics and printable `Alt+…` captions, while Sterna reserves Alt for the
terminal and puts its actual shortcuts on `QAction`. Those markers are removed
from the displayed translation, including Japanese-style `設定(&S)`, so loading
a language cannot silently make Meta keystrokes open a menu.

Every shell family with a matching upstream key is now wired: the generated
settings UI, menus, serial/SSH/telnet connection forms, SSH prompts, transfer
dialogs, macro dialogs, paste and disconnect confirmation, and common
file-picker captions. The tests load the real Japanese catalog for each family;
the SSH and macro dialogs are rendered as well as inspected.

This is not a claim that a Tera Term catalog contains words for Sterna's new
UI. The ssh-agent, legacy-algorithm and telnet-mode controls, the safer host-key
explanation, Lua file filters and other Sterna-only copy keep their source text.
Attaching a nearby key with a different meaning would produce a more translated
but less truthful interface; translating those strings needs a future Sterna
catalog extension.

Native Wayland is the platform exception. Qt 6.11 retains the opacity property
but has no backend operation to give it to the compositor; Fedora's xcb backend
does. The render test therefore pins the setting and the focus policy, not
visible transparency on Wayland. Run with `QT_QPA_PLATFORM=xcb` when that
effect is wanted on this desktop.

`Setup > Save setup` writes the file, which is upstream's bargain — a change
applies now and outlives the session only if it is saved. With the default-on
`IniAutoBackup`, an existing file is first copied byte-for-byte to a timestamped
sibling. That copy belongs specifically to the full menu save: creating the
file for the first time and the close-time geometry-only write make no backup.

The file is `$XDG_CONFIG_HOME/sterna/sterna.ini`: Tera Term's *format*, in
the place a Linux configuration file belongs, since the executable may be
inside a read-only AppImage. Pointing it at a real `TERATERM.INI` is a
supported thing to do and `--ini` is how it will be spelled.

## A tab is a whole terminal, not another view

`TerminalPage` is the ownership boundary: one `Session`, `TerminalView`,
scrollbar, `Printer`, `Macro` and transfer dialog. Putting only the view in a
tab would leave modeless transfers and interpreters attached to whichever
session happened to be active when their next signal arrived. `Macro` is
destroyed before `Session`, deliberately, so a running worker cannot outlive
the terminal it drives.

The tab bar hides when there is one page. With more, it is movable and
closable; menus, status, window title and the window-wide control endpoint
follow the selected page. A background page keeps pumping and can update its
own label, but an ordinary notice does not steal focus. Starting a new
connection while the current page is live opens another tab rather than
replacing it.

`File > Duplicate session` is Tera Term's rule rather than a general clone:
it is available for a connected SSH or telnet page, never serial or a local
shell. It copies the live settings and current grid size, then reconnects the
same target. SSH may ask for authentication again; prompt answers are not
credentials Sterna keeps. Proxy settings travel with the copied settings, and
the SSH ABI takes the destination session explicitly so an ordinary or
duplicated connection cannot silently bypass them.

The control endpoint remains window-wide and follows the selected tab, like
the menu bar. `$STERNA_CTL` inherited by a local shell therefore addresses the
active tab, not permanently the tab that launched that shell; a fixed per-tab
address would be a second endpoint and a child-specific environment entry,
neither of which this window currently promises.

## A transfer is the second thing with a timer, and the only other one

`File > Send file...` and `File > Receive file...`. The protocols are Tera
Term's own C, vendored and driven by `tt-xfer`; what the window supplies is a
protocol chooser, a progress dialog, and the wakeups.

The pickers begin at `FileDir`, after `%NAME%` expansion, or at Downloads when
that path is absent or unusable. `FileSendFilter` uses Tera Term's semicolon
mask spelling and applies to every protocol send picker. `FileReceiveFilter`
belongs only to raw receive; it is retained in the settings but is not applied
to the protocol receive dialogs, matching upstream even for XMODEM's prompted
name.

**The wakeups are the part worth reading.** Everything else in this window
runs off the descriptor, and a transfer cannot: the protocols retry by
*timeout* — an XMODEM receiver that hears nothing re-sends its `NAK` after ten
seconds, ZMODEM finishes a cancel 500 ms after sending it — and a line that has
gone quiet produces no descriptor wakeup at all. So `Session` has a second
single-shot timer, armed after every pump from
`tt_session_transfer_deadline_ms` and stopped the moment no transfer is
running. It is the same shape as the flow-control retry timer already there,
and for the same reason: a case a descriptor genuinely cannot carry.
`xfer_test` covers it directly, by cancelling a transfer whose peer is a
`sleep`.

Three smaller decisions:

- **The progress dialog is modeless**, where upstream's is modal. Not a style
  preference: the transfer is driven by *this* window's event loop, so a dialog
  that blocked it would block the transfer it is showing. Modality has nothing
  left to protect either — the core refuses keystrokes and keeps the protocol's
  traffic out of the parser for the duration, which is what upstream's modal
  dialog was achieving by other means.
- **It stays open when the transfer fails.** The protocol's own words —
  "Cannot create file" — are often the only account of the failure there is,
  and a dialog that vanished at the moment of failure would say it to nobody.
- **A bar that cannot mean anything becomes a busy indicator.** XMODEM never
  learns the size and ZMODEM only learns it if the sender said, so a percentage
  is sometimes unavailable; a bar frozen at zero reads as "stuck", which is the
  one thing it must not say. Relatedly, `bytes == 0` does not mean "not
  started": the protocols throttle their own reporting to ten updates a second.

B-Plus and Quick-VAN are in the list, last, labelled *untested*. They compile
and are wired, and there has been no counterparty for either since CompuServe
and NIFTY-Serve shut down — saying so in the menu is better than letting
someone find out.

## A macro is the only thing that calls *into* this window

`Control > Run macro...`, `/M=` on a Tera Term command line, and `Macro.cpp`.
The interpreter runs on a thread inside the core and blocks whenever it wants
something out here; this window waits on its fd or event with the platform's
Qt notifier and, when it fires, runs whatever the macro asked for **on this
thread**. So a `messagebox` is an ordinary modal dialog: it spins a nested
event loop, the terminal goes on painting, and the script is parked until the
user answers.

That is the one place the ABI hands out function pointers, and it is not a
contradiction of the SSH design next to it. `tt_ssh_connect_poll` refuses a
callback because it would fire on a worker thread — the one place a Qt frontend
*cannot* raise a dialog. These fire from inside a call this window itself made.

**The notifier is disabled across that call**, because it is level-triggered
and the dialog's own nested loop would otherwise re-enter it and open a second
dialog inside the first. It is the same re-entrancy `Session::m_sshWaiting`
guards against, and it is why `macro_test` answers its dialogs from a repeating
timer rather than a `singleShot`: the timer has to fire *inside* the modal
loop, which is precisely the situation being defended.

Two things here differ from upstream and both are stated in the source:

- **A closed `yesnobox` reads as No.** Upstream ends the macro when the dialog
  is closed and carries on when No is clicked; Qt gives Escape and the title
  bar's close to the reject-role button, so the two cannot be told apart.
- **`enablekeyb 0` is released when the macro ends.** Upstream only puts
  `KeybEnabled` back from `Control > Reset terminal`, which this port does not
  have — so a macro that died between the two calls would leave a terminal
  nobody can type into. `enablekeyb.html` describes the lock as lasting "while
  the macro is sending the data", so this follows the manual.

`callmenu` is refused: its ids are `teraterm.rc`'s and there are about ninety
of them, which wants a table from Windows command ids onto `QAction`s and is
worth writing when there is a menu to map rather than ahead of one. `show` —
the macro's own control window — has nowhere to go, since the macro is a thread
in this process; its End button is `Control > Stop macro`.

## And a control socket is the only thing that calls in from *outside*

`Control.cpp`, and a fourth `QSocketNotifier`. The window binds
`$XDG_RUNTIME_DIR/sterna/<pid>.sock` — or `<topic>.sock` when a command line
gave `/D=` — and `tt_ctl_service` runs whatever a client asked for on this
thread, which is the same arrangement as the macro one level out. See
[`crates/tt-ctl`](../crates/tt-ctl/README.md) for the protocol and the nine
methods; what belongs here is the four callbacks that are about the *window*
rather than the terminal, and the two things a request is not allowed to do.

**A request may not raise a modal dialog**, and both places it would are easy
to miss. A `connect` naming nothing openable reaches `showConnectDialog`, and
a `connect` that fails to open reaches a `QMessageBox::critical` inside
`openTarget` — so a request from another process could park this window on a
box nobody is looking for, with the requester blocked behind it. The first is
refused with a reason; the second is queued to the next turn of the event
loop, where a dialog is an ordinary dialog and the client already has its
answer. Upstream's `connect` opens the dialog, which is right when a person
clicked and is exactly the difference.

**A `close` request may not close the window from where it is standing.** It
arrives inside `tt_ctl_service`, which is inside a call this window's own child
object is making, so it is invoked queued — and it is `close()` rather than a
delete, because the window's own close handler is what stops the macro, writes
the settings back and tears the socket down.

The path goes into `$STERNA_CTL`, so a shell started *inside* the terminal can
drive its window. That is the one thing DDE could not do at all. It is the
process's environment rather than the child's because `TtPtyParams` has no
environment array; with tabs, the endpoint follows the active page as
described above.

## Not here yet

- **Blinking cursor and blinking text.** Tera Term colours blink rather than
  animating it (`VTBlinkColor`, on by default), which is reproduced; an
  animated form would need a timer, and the point of this event loop is not
  having one.
- **Session profiles.** The connect dialogs remember the last port and host for
  the lifetime of the window; the *terminal's* settings now persist through
  `Setup > Save setup`, but which port was last opened is not one of the 39
  settings in the schema yet.
- **A host-key manager.** Removing a changed key still means editing
  `~/.ssh/known_hosts` by hand — the dialog says which file and which line, and
  that is as far as it goes.
- **Saved SSH sessions.** The dialog remembers the last host for the lifetime
  of the window, and `~/.ssh/config` covers the rest. Profiles on disk are
  Stage 2's, with the INI reader.

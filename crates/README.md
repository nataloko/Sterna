# Sterna core

The Rust side. Nine of the crates `PLAN.md` describes, plus three CLIs that
exist so the engine can be measured against something.

| Crate | What it is |
|---|---|
| `tt-grid` | Cells, lines, cursor, scroll region, scrollback, alternate screen. No I/O, no escape sequences. |
| `tt-charset` | ISO-2022 designation and invocation, and whether a byte is DEC special graphics. |
| `tt-vt` | The escape-sequence state machine. Byte-level parsing is the `vte` crate; the semantics are ported from Tera Term. |
| `tt-conn` | The connection layer — serial, SSH, telnet and a local pty, all four built. The serial half is written against `commlib.c`'s requirement; see [its README](tt-conn/README.md). |
| `tt-config` | `TERATERM.INI`, the settings schema everything else reads its list of settings from, and the Tera Term command line — `_ParseParam` and TTSSH's hook over it, which is what `connect` and a `ttermpro`-compatible entry point both need. Held against a real Win32 by `ini-audit/`. |
| `tt-xfer` | X/Y/ZMODEM, Kermit, B-Plus and Quick-VAN — Tera Term's own protocol C, vendored under `vendor/ttpfile/` and driven from Rust. See [its README](tt-xfer/README.md). |
| `tt-ttl` | Tera Term's macro language, ported from `ttpmacro/`, with the terminal behind a trait instead of behind DDE. See [its README](tt-ttl/README.md). |
| `tt-session` | A terminal attached to a connection: the loop between `tt-vt` and `tt-conn`, and what the C ABI exports. See [its README](tt-session/README.md). |
| `tt-macro` | The join: a `tt-ttl` script on its own thread, driving a `tt-session` on the frontend's. See [its README](tt-macro/README.md). |
| `tt-ffi` | The flat C ABI over `tt-session` — the whole core/frontend seam, and what the Qt shell links. See [its README](tt-ffi/README.md). |
| `tt-dump` | A CLI that drives `tt-vt` over a byte stream and prints the oracle's dump format. Exists for the differential harness. |
| `tt-host` | A terminal with no window: runs a program on a pty and is the terminal on the other end of it. Exists for `esctest/`, which cannot be a recording. |
| `tt-fuzz` | The engine's properties — no panic, the grid stays consistent, and the chunk boundaries do not matter — shared by the stable test suite and the libFuzzer targets in `fuzz/`. See [its README](tt-fuzz/README.md). |
| `tt-bench` | Ten megabytes through the engine, in the chunk sizes a pty gives. The half of the perf gate with no window in it, and the corpus generator the other half feeds through a pty. See [bench/README.md](../bench/README.md). |

```sh
cargo build && cargo test
cargo clippy --all-targets -- -D warnings
tt-ffi/run_abi.sh              # the C ABI, compiled and driven from C
../run_diff.sh                 # the gate that actually matters
../esctest/run_tests.sh        # ...and conformance, from inside our own terminal
../bench/bench.py --core       # ...and that it has not got slower
```

The pty suites in both crates need nothing and always run, so a bare
`cargo test` does exercise one transport end to end. Everything else in
`tt-conn` and `tt-session` that touches the outside world needs a rig or a
server and skips loudly without it.

`tt-conn`'s and `tt-session`'s hardware tests need two serial ports wired
back-to-back and skip without them. **Run them one package at a time:**

```sh
export TT_SERIAL_A=/dev/ttyUSB0 TT_SERIAL_B=/dev/ttyUSB1
cargo test -p tt-conn --  --test-threads=1
cargo test -p tt-session -- --test-threads=1
```

`--test-threads=1` is per test *binary*, and cargo still runs the binaries
concurrently — so asking for both packages in one command puts two hardware
suites on the same two ports at once, and one of them loses. It looks like a
flaky `tt-conn` rather than like a harness that overbooked the rig.

`cargo` is on `PATH` only for login shells in the dev container — export
`$HOME/.cargo/bin` first.

## The differential harness

`../run_diff.sh` feeds every case in `../oracle/cases/` to **both** engines —
`tt-dump` and `oracle/build/oracle`, the latter being Tera Term's real
`vtterm.c` and `buffer.c` compiled headless — and diffs the two dumps against
each other.

There are no golden files in that loop. The oracle *is* the expected output, so
a new case is an `input` file and an optional `cmd` line, with nothing to bless
and no opportunity to enshrine a wrong answer. (`oracle/run_tests.sh` still
keeps goldens, for a different job: catching the *oracle* drifting when upstream
is bumped or the stub layer changes.)

```sh
../run_diff.sh              # all cases
../run_diff.sh 09 27        # just the ones whose names contain 09 or 27
../run_diff.sh -v 27        # and print both dumps
```

Adding a case:

```sh
mkdir -p ../oracle/cases/39-my-case
printf -- '--cols 20 --rows 4' > ../oracle/cases/39-my-case/cmd
printf 'whatever\033[2Jbytes' > ../oracle/cases/39-my-case/input
../run_diff.sh 39
```

If it fails, the diff tells you what Tera Term does and what we do. Tera Term
wins, every time — that is the whole point. To also add it to the oracle's own
golden suite, run `../oracle/run_tests.sh --bless` and **read the golden it
produces** before committing it.

A case directory holding an `xfail` file is a *known* divergence. The file says
why; the diff is shown but not counted as a failure; and if the two engines ever
agree the case reports `XPASS` and fails, so a stale marker cannot survive.

## Where this is faithful

Being a *port* rather than a fresh VT implementation means some upstream
behaviour looks like a bug until you check. Reproduced deliberately:

- **Erase keeps the current colours but drops bold/underline/reverse.**
  `buffer.c` passes `CurCharAttr.Fore/Back` with `AttrDefault` to `memsetW`.
- **DECSTBM homes the cursor to the screen origin**, not to the top of the
  region it just set, unless origin mode is on. `vtterm.c:2473`.
- **A line feed at the bottom of the scroll region does not clear the pending
  wrap**, because it scrolls instead of calling `MoveCursor`.
- **A combining mark with no base character** gets a U+00A0 base and advances
  the cursor one column.
- **The padding half of a wide character carries no attributes at all.**
  `buffer.c:3400` writes `attr`, `attr2`, `fg` and `bg` as zero, so a
  background-coloured wide character reports its colour on the lead cell and
  nothing on the pad. (The *insert-mode* branch at `:3325` copies the pen onto
  the pad instead. That inconsistency is upstream's; we reproduce both.)
- **Breaking a wide character is not the same as erasing it, and which one
  happens depends on who broke it.** Overwriting, inserting, deleting and
  scrolling all go through `BuffSetChar(b, ' ', 'H')`, which blanks the text
  and the colour indices but leaves the SGR attribute bits untouched and never
  consults the pen. The erase paths go through `EraseKanji`, which paints the
  whole pen — bold included, unlike the `memsetW` that erases the cells around
  it. `Cell::crush` and `Grid::erase_kanji` are the two halves of that split.
- **G1 starts as DEC special graphics**, so a bare SO switches to line drawing
  with no `ESC ( 0` in the stream at all. `charset.cpp:CharSetInit2`.
- **A single shift never ends.** `ParseFirst` clears `SSflag` after one
  character, but the UTF-8 path returns before reaching that code, so one
  `ESC N` redirects every later character to G2 for the rest of the session.
- **DEC special characters keep their raw byte and carry `AttrSpecial`** rather
  than becoming U+25xx, because `DecSpMappingDir` defaults to "do not map".
  Turning `q` into a horizontal line is the renderer's job.
- **On a VT100, 8-bit C1 controls are masked to C0** — `U+008D` is a carriage
  return, not RI, and `U+009B` is an ESC, not a CSI introducer. Above VT100 the
  mask does not apply. `vtterm.c:1053`.
- **`DispFindClosestColor` flips bright and dim.** Truecolor red resolves to
  palette index 1, "dark red", not 9; the drawing path applies the inverse, so
  the round trip is consistent and index 1 is what the cell stores.
- **DECSCA's protect bit survives `SGR 0`.** `vtterm.c:2178` ORs it back in
  explicitly, so only another DECSCA clears it. And a *selective* erase is not
  an erase: `BuffSelectedEraseCharsInLine` masks the cell's own attributes to
  `AttrSgrMask` rather than painting the pen, so bold and underline outlive
  DECSEL where they would not outlive EL.
- **DECFRA fills with the whole pen; DECERA erases with a subset of it; and the
  wide-character halves they cut at the edges of a rectangle get a *third*
  treatment again.** `BuffFillBox` passes `CurCharAttr.Attr`, `BuffEraseBox`
  passes `AttrDefault` with only the colour bits of `Attr2` — and the straddling
  halves outside the range are written with the full pen either way. So a
  DECERA under a bold pen leaves a bold cell on each edge and unbold cells
  between them. Case 56 pins it.
- **Left and right margins gate far more than horizontal scrolling.** With
  DECSLRM set, the wrap point moves to the right margin, CR goes to the left
  margin (but to column 0 when the cursor is *left* of it), backspace stops
  dead on the left margin, CUB/CUF clamp to the margins only when the cursor
  started inside them, tabs stop at the right margin, ICH/DCH are **refused**
  outside the margins rather than clipped, and every vertical scroll — LF at
  the bottom of the region, SU, SD, IL, DL — moves only the margin columns and
  leaves the rest of each row alone. Cases 63 to 67.
- **A plain HT takes the pending wrap before it tabs** (`vtterm.c:Tab()`), so a
  tab arriving on a full line starts the next one. `CSI Ps I` (CHT) does not —
  it calls `CursorForwardTab` directly. And a tab that runs out of stops parks
  on the right margin and *arms* the wrap, because `ts.VTCompatTab` is off.
- **A scroll region that starts at row 0 fills the scrollback even when its
  bottom margin does not reach the last row.** `BuffScroll` slides the page and
  copies the rows below the region down to keep them in place, so the top rows
  leave the page rather than being discarded. Only a later resize can see the
  difference, which is how case 69 reads it back.
- **A soft reset reloads DECSC's slot with the origin.** `SoftReset` saves the
  cursor at 0,0 rather than where it is, so a DECRC straight after `CSI ! p`
  homes the cursor. It also leaves the screen, the cursor position and autowrap
  alone while resetting both margin pairs, insert mode, origin mode and the
  pen. Case 72.
- **DECRQSS reports colours through whichever `ColorFlag` happens to be on.**
  The same pen answers `;33`, `;93`, `;38;5;n` or nothing at all depending on
  `CF_ANSICOLOR`, `CF_PCBOLD16`, `CF_AIXTERM16` and `CF_XTERM256` — and bold
  brightens the *foreground* while blink brightens the *background*, which is
  upstream's pairing and not a typo. Case 71.
- **Resizing truncates; it never reflows.** `ChangeBuffer` copies each line's
  first `cols` cells and drops the rest, crushing a wide character cut by the
  new right edge. Height is expressed by sliding the page over the scrollback
  rather than by moving text: shrinking keeps the *top* rows unless the cursor
  would fall off the bottom, in which case the page ends at the cursor and the
  rows above it become scrollback; growing pulls lines back *out* of the
  scrollback before it extends downward. Cases 59 to 62 pin all four paths.
- **`ED 3` is not an erase either.** It is `ClearBuffer`, which drops the
  scrollback, homes the cursor and resets the scroll region — and only runs at
  all because `TF_REMOTECLEARSBUFF` ships on (`ttset.c:1950`).
- **`SGR 38`/`SGR 48` do not consume their arguments** when the matching colour
  mode is off. 256-colour is on by default so this is not normally visible, but
  turn it off and `ESC [ 38;5;196 m` parses as "38 ignored, 5 = **blink on**,
  196 ignored". `vtterm.c:2239`.

### Modes

Every private mode `vtterm.c` tracks is tracked here, and `DECRQM`
(`CSI [?]Ps $ p`) is the differential probe for all of them at once — one case
sweeps 43 modes, another flips them and asks again, and a third checks what a
soft reset and a RIS each clear. The frontend reads the ones it needs through
accessors (`cursor_visible`, `bracketed_paste`, `application_cursor_keys`,
`reverse_video`, …) rather than through the escape sequences.

The corners worth knowing, all upstream's:

- **An unknown ANSI mode answers 4, an unknown DEC private mode answers 0.**
  The two halves of `CSDolRequestMode` have different fallbacks.
- **`SM 2` and `SM 12` invert.** KAM *locks* the keyboard, and SRM being set
  means local echo is off.
- **DECPEX starts set** (`vtterm.c:176` initialises `PrintEX` TRUE) and no reset
  clears it, so a fresh terminal answers 1.
- **RIS clears bracketed paste, but not DECPEX, the keyboard lock, local echo
  or DECBKM.** Bracketed paste is cleared a hundred lines below the block that
  clears everything else.
- **DECSCUSR and `DECSET 12` are gated on `CursorCtrlSequence`, which ships
  off**, so by default they do nothing and DECRQM adds two to its answer for
  modes 12, 33 and 34 — "set" becomes "permanently set".
- **Highlight tracking (1001) can be set but always reports 4**, because the
  reporting arm is `#if 0`'d out.
- **`DECSET 8200` makes `ED 2` home the cursor**, to the region origin, or the
  screen origin under origin mode.

### The key table

`Vt::key(Key) -> Option<Vec<u8>>`, and the core owns it for the same reason it
owns the mouse encoding: which form a key takes is terminal state the frontend
never sees. What the frontend supplies is a [`Key`], not a keysym — mapping a
physical key onto one is platform work, and on Windows it is what
`KEYBOARD.CNF` does.

Verified rather than transcribed: the oracle compiles `keyboard.c` itself and
one case sweeps **55 keys across 10 mode combinations** — application cursor,
application keypad, 8-bit controls, every pairing, and LNM. See
`oracle/README.md` for how a `.c` file with a `static` table became testable.

Worth knowing:

- **PF1-PF4 are `SS3` in every mode.** They have no printed character to fall
  back to, so application-keypad mode changes nothing about them — an easy
  place to over-generalise from the other keypad keys.
- **Keypad Enter is the only key newline mode reaches.** Upstream marks its
  numeric form `IdText` rather than `IdBinary` precisely so the CR goes through
  `OutControl`'s conversion, which is why `SM 20` turns it into CR LF.
- **`ts.DisableAppKeypad`/`DisableAppCursor` veto at encode time, not at
  DECSET time.** A host can set DECCKM, have DECRQM confirm it is set, and
  still get the normal cursor keys.
- Hold, Print and Break have key ids so `KEYBOARD.CNF` can bind them, and put
  nothing on the wire.

### Mouse and focus reporting

Six wire formats and eight tracking modes, all of them upstream's, and all of
them driven from `Vt::mouse_event` / `Vt::focus_event` rather than from the byte
stream. The core owns the encoding for the same reason it owns the keymap: which
format is live is terminal state the frontend never sees.

Positions cross the boundary as **window pixels**, not cells — that is what
`MouseReport` takes, and SGR-pixel mode (`DECSET 1016`) reports them back
unconverted, so a cell-only API could not express it. `Vt::set_cell_pixels`
tells the core how to convert; it learns nothing else about pixels.

Testing it needed the oracle to grow an input side. `oracle/README.md` has the
directive syntax; a case reads

```
ESC [ ? 1000 h ESC _ tt.mouse down 0 24 80 ESC \
```

and the two engines are diffed on the bytes they send back. Fourteen cases cover
X10 through any-event tracking, all five encodings, the modifier bits, wheel,
focus, NetTerm, the DEC locator including one-shot and filter rectangles, and
what RIS and DECSTR each reset.

Things that look wrong in isolation and are upstream's:

- **A motion event before any press reports button 3**, because `LastButton` is
  a function static initialised to `IdButtonRelease` (`vtterm.c:5614`).
- **`DECRESET 9` turns off any-event tracking**, and every other mouse mode:
  the reset arm assigns `IdMouseTrackNone` regardless of which mode is live.
- **Ctrl suppresses every report** (`ts.DisableMouseTrackingByCtrl`, default
  on), so ctrl-click stays available for text selection.
- **X10 and NetTerm consume the button release and send nothing**, returning
  "handled" rather than falling through to selection.
- **RIS clears the mouse mode; DECSTR does not.** `SoftReset` deliberately
  leaves tracking alone, so a program that soft-resets keeps its mouse.

## Known divergences

- **DEL (0x7F) occupies a cell in Tera Term; `vte` discards it.** Only in the
  ground state — inside an escape sequence Tera Term strips it too — so
  reproducing it needs parser state `vte` does not expose. ECMA-48, xterm, VTE
  and alacritty all ignore DEL. Tracked by `oracle/cases/51-del-byte`, which
  carries an `xfail`.
- **Character width comes from the `unicode-width` crate, not Tera Term's
  tables.** Both derive from `EastAsianWidth.txt` and agree on unambiguous
  characters; ambiguous-width policy and emoji presentation will drift.
  Deferred with CJK.
- **Spacing combining marks** (Devanagari and friends), which join the base cell
  *and* advance the cursor, are not modelled — only nonspacing marks are.
- **Kanji and Katakana designations** (`ESC $ ...`, `ESC ( I`) are parsed and
  dropped. Deferred with CJK, and inert on a UTF-8 terminal anyway.
- **Tera Term's CSI parser takes intermediates and parameters in any order; the
  `vte` crate does not.** `ControlSequence()` dispatches each byte on its range
  alone, so `ESC [ * 2 x` is a perfectly good DECSACE upstream. `vte` follows
  ECMA-48, where an intermediate ends the parameter string, and drops the
  sequence. Upstream's own `tests/#38168-deccara-range.sh` opens with exactly
  that spelling, which is how this surfaced; it carries the XFAIL in
  `oracle/upstream.cases`. Fixing it means normalising the byte stream before
  `vte` sees it, which has not been worth the scanner so far.
- Of XTWINOPS, `CSI 8;h;w t` (set terminal size), `CSI 18 t` (report it),
  `CSI 20/21 t` (the title reports) and `CSI 22/23 t` (the title stack) are
  implemented. The rest of that switch asks the display layer where the window
  is or moves it, so in a headless diff the answers would come from the
  oracle's *stubs* rather than from Tera Term, and matching them would be
  matching a stub. Note the title reports answer with an **empty** OSC string:
  that is `TitleReportSequence`'s shipped default and an answerback mitigation,
  not a hole.
- **The colour palette is not in the engine.** Upstream's OSC 4/5/10-19/104/105
  go through `vtdisp.c`'s `DispGetColor`/`DispSetColor`, so the palette is the
  display layer's there and the frontend's here. Nothing answers a colour
  query; `esctest/expected` records the 47 tests that want one.
- **DECSCNM, DECPEX and `DECSET 12` are tracked but do nothing**, because what
  they change belongs to the renderer or the printer. They are here so DECRQM
  answers honestly and so the shell can read `reverse_video` when it paints.
- Of DCS, only DECRQSS (`DCS $ q … ST`) is implemented. `DCS + q` — xterm's
  termcap query — and `DCS ! {` (DECSTUI) are collected and dropped rather than
  answered wrongly.
- **The UTF-8 mouse encoding (`DECSET 1005`) still emits its button byte raw.**
  Upstream formats it with `%c` while the coordinates go through the two-byte
  encoder, so with enough modifiers held the report stops being valid UTF-8.
  Reproduced deliberately — it is the wire format hosts see — unlike the row
  coordinate beside it, which was an outright typo and is patched (bug 5 in
  `docs/upstream-bugs.md`).
- Highlight tracking (`DECSET 1001`) reports nothing, because upstream never
  implemented it. It still *displaces* whatever mode was active, which is
  observable, so the mode is tracked rather than ignored.

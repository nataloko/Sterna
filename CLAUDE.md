# Working notes for termitta

Read `PLAN.md` for the roadmap and current stage. This file is the working
agreements and the traps.

## What this is

A cross-platform Tera Term successor: Rust core + flat C ABI + Qt 6 Widgets
shell, Linux and Windows. **Not** a fork of Tera Term and **not** aiming at
parity — see `PLAN.md` for scope.

`termitta` is a working name; the real one is still undecided (the current one is
already taken in the wild).

## Ground rules

1. **`../teraterm` is read-only reference.** Never edit it. It is a sibling
   checkout of upstream Tera Term, used three ways: compiled unmodified as the
   test oracle, vendored for specific subsystems, and read as the behavioural
   specification. If a build needs a change to it, that change goes in
   `oracle/patches/` and is applied to a copy under `oracle/build/patched/`.
2. **Prefer compiling real Tera Term code over reimplementing it.** In the
   oracle, adding a source to `TT_CXX` in the Makefile always beats writing a
   stub. Every stub is a place the oracle can lie about ground truth.
3. **Never bless a golden you have not read.** `./run_tests.sh --bless`
   regenerates `cases/*/expected`. A wrong golden is worse than no test.
   The differential suite (`./run_diff.sh`) sidesteps this entirely — it diffs
   the two engines against each other, so a new case needs an `input` and
   nothing to bless. **Prefer adding cases there.** Bless a golden only when you
   also want the oracle's own suite to guard that case against upstream drift.
4. **The oracle's settings are load-bearing, and `ttset.c` lies about them.**
   `main.c:settings_defaults()` mirrors `ttpset/ttset.c`'s per-key fallbacks —
   the ones applied *after* the initialiser at the top of the function, not the
   initialiser itself. Every flag word (`ColorFlag`, `TermFlag`, `ISO2022Flag`,
   `WindowFlag`) is set to 0 near the top and then built up key by key hundreds
   of lines later; reading the 0 as the default is wrong and silently disables
   256-colour, ISO-2022 shifts, 8-bit controls and the alternate screen. If a
   dump looks subtly wrong, suspect a setting before suspecting the parser.
5. **Attribution and licensing are not paperwork.** Before vendoring anything
   from Tera Term, check `ATTRIBUTION.md` — the `.lng` and `.map`/`.tbl` assets
   have no per-file licence headers, unlike `ttpfile`.
6. **Git identity is set per-repo** to the GitHub noreply address, already
   configured locally. Don't change it.
7. **Commit often.** Small, self-contained commits as work lands, not one
   omnibus commit at the end of a session. A spike that compiles is a commit; a
   finding recorded in `PLAN.md` is a commit. This keeps the history bisectable
   and means an interrupted session leaves something behind.

## Build and test

```sh
./run_diff.sh                    # THE gate: Rust engine vs Tera Term, 92 cases
./run_diff.sh 27                 # just the cases matching "27"
./run_upstream.sh                # the same diff over Tera Term's OWN exercisers

cd crates                        # the Rust core
cargo test && cargo clippy --all-targets -- -D warnings

cd oracle
make            # build build/oracle
make test       # 72 regression cases
make stubs      # regenerate the stub layer after upstream headers change

cd xfer                          # Stage 0 spike 2
make && ./run_tests.sh           # 10 interop cases vs lrzsz and gkermit

cd ssh-audit                     # Stage 0 spike 5
./servers.sh start && cargo run && ./servers.sh stop

cd serial-audit                  # Stage 0 spike 4, needs the FTDI loopback rig
cargo run --bin serial-audit     # capability audit vs commlib.c
cargo run --bin rawpatch         # are the gaps patchable through the raw fd?
cargo run --bin hotplug          # needs a human to pull the cable
```

`xfer` needs `lrzsz` and `gkermit` installed for its interop suite.

The oracle needs `gcc` and Python 3.11+ and nothing else.

Rust, cmake, Qt 6, lrzsz, C-Kermit and G-Kermit are installed in the dev
container. For protocol interop use **G-Kermit** — C-Kermit sees a pty as a tty
and drops into interactive mode instead of speaking the protocol.
**`cargo` is on `PATH` only for login shells** — export
`$HOME/.cargo/bin` first or `cargo: command not found` will look like a missing
toolchain. It isn't; don't reinstall it.

Two packages were added on 2026-08-07 and a rebuilt container will need them
again: **`libudev-dev`** (`serialport-rs` enumeration — without it the crate
does not build), **`libxcb-cursor0`** (Qt's `xcb` platform plugin refuses to
start without it) and **`gkermit`** (xfer's kermit interop case).

## The dev container is not headless

Verified 2026-08-07 by opening real Qt windows and driving real serial hardware.
This is a rootless podman container (`agents`, `ubuntu:24.04`) run by distrobox
on a **Bluefin / Fedora Silverblue 44** host, and distrobox passes the desktop
session straight through. **Do not assume anything GUI- or hardware-shaped is
untestable here — check first.**

| | |
|---|---|
| Wayland | `WAYLAND_DISPLAY=wayland-0`; Qt runs under `-platform wayland` |
| X11 | Xwayland on `:0`; Qt runs under `-platform xcb` (needs `libxcb-cursor0`) |
| Screens | DP-1 1920x1080, HDMI-1 2560x1440 |
| Session bus | GNOME Shell, Mutter and the **ibus portal** are all reachable |
| Fonts | Noto CJK present (~108 ja / 107 zh / 105 ko families) |
| GPU / input | `/dev/dri/{card1,renderD128}`, `/dev/input/event*` |
| Serial | FTDI Quad RS232-HS — **`ttyUSB0` and `ttyUSB1` are wired back-to-back**, data *and* control lines |
| Hotplug | kernel uevents reach the container; `/run/udev/data` is mounted |
| Host | root at `/run/host`, home at `/var/home/nata`, and `distrobox-host-exec` runs commands on the host |

The serial pair is a complete loopback rig, so the `serialport-rs` audit needs
no new hardware. Confirmed working: data both directions, DTR→DSR, RTS→CTS,
break visible in-stream as a NUL, 9600 through 3000000 baud clean, and RTS/CTS
hardware flow control. Only a physical unplug/replug still needs the user.

### Qt work goes in the `termitta-fedora` container, not this one

This container has Qt **6.4.2**; the desktop runs **6.11.1**. That gap has
already manufactured one false finding — see the traps. So there is a second
distrobox, created 2026-08-07:

```sh
distrobox-host-exec distrobox enter termitta-fedora --no-tty -- <command>
```

Fedora 44, Qt **6.11.1** — an exact match for the host — plus `gcc-c++`,
`cmake`, `ninja`, `qt6-qttools-devel` (Designer/`uic`, which the generated
dialogs will need) and `xcb-util-cursor`. It inherits the same desktop
passthrough: Wayland, Xwayland, session bus.

**Its `$HOME` is `/var/home/nata/agents-home`** — deliberately the same home
this container uses, *not* the user's real `/var/home/nata`. Keep it that way:
**the user does not want their host home polluted.** A bare `distrobox create`
takes the host home, so always pass `--home /var/home/nata/agents-home`.

Two things `--home` does *not* cover, both checked on 2026-08-07:

- `distrobox create` writes an app-grid launcher to the **host** home at
  `~/.local/share/applications/<name>.desktop` regardless of `--home`, because
  its purpose is to be visible to GNOME. Deleted here; the container works fine
  from the CLI without it. Recreating puts it back — delete it again.
- `distrobox rm` cleans that launcher up on its own, so removing a container
  leaves nothing behind.

Add Rust to it when the shell work starts; it is not installed there yet.

## Traps

These cost real debugging time. Each is a place where the failure looks like
something other than what it is.

- **`UTF32ToUTF16` is not optional.** `buffer.c:234` uses it to fill
  `buff_char_t::wc2`, and `expand_wchar()` reads back from `wc2`, not `u32`.
  Stub it and you get a screen holding all the right codepoints that renders
  **entirely blank** — which looks exactly like a broken parser.
- **`_WideCharToMultiByte` is dereferenced with no NULL check**
  (`buffer.c:3076`), so a stub returning NULL segfaults on the first combining
  character.
- **`CRReceive`'s real default is `IdCR`** — the `else` branch at
  `ttset.c:643`, not the `IdCRLF` the surrounding code suggests. It shifts every
  row in the dump. With `IdCR` a bare CR is a carriage *return*, so
  `"Hello, world!\rSecond line"` correctly yields `Second lined!`.
- **`AcceptTitleChangeRequest` defaults to `overwrite`**, not off
  (`ttset.c:1568`). Zero means OSC title changes are silently ignored.
- **`buffer.c:134` hardcodes `CodePage = 932`** (Shift-JIS). Call
  `BuffSetDispCodePage()`.
- **`WinWidth`/`WinHeight` ≠ `NumOfColumns`/`NumOfLines`.** The first pair is
  the visible window in cells, the second is the terminal size, and only
  `BuffChangeTerminalSize` owns the latter. `DispChangeWinSize` must **not**
  call `BuffChangeWinSize` — that recurses infinitely against `buffer.c:4956`.
- **`BuffGetAnyLineDataW` takes an absolute buffer index**;
  `BuffGetCursorCharAttr` is screen-relative. `PageStart` maps between them.
- **`vtterm.c` owns `CharSetInit`** — it holds `charset_data`. The runner must
  not call it.
- **Make's VPATH beats pattern rules.** Patched sources need *explicit* rules or
  the generic `%.o: %.c` finds the unpatched original via VPATH and silently
  wins.
- **A `ts->X = 0` at the top of `ttset.c` is an initialiser, not a default.**
  This cost the most time of anything here. `ISO2022Flag`, `ColorFlag`,
  `TermFlag` and `WindowFlag` are each zeroed around `ttset.c:559-564` and then
  ORed together from per-key `GetOnOff(..., TRUE)` calls a thousand lines
  further down. The oracle took the zeros, so for a while it reported a Tera
  Term with **256-colour off, every ISO-2022 shift off, 8-bit controls off and
  the alternate screen off** — none of which is how it ships. Corrected
  2026-08. If you add a setting, find the key, not the initialiser.
- **`SGR 38`/`48` do not consume their arguments when their colour mode is
  off.** Normally invisible because 256-colour defaults on, but flip it off and
  `ESC [ 38;5;196 m` reads as 38 (ignored), 5 (**blink on**), 196 (ignored).
  `vtterm.c:2239`.
- **`TermIDGetID()` never fails.** It is a case-sensitive `strcmp` against an
  UPPERCASE table that returns `IdVT100` for anything unrecognised, so
  `--term vt220` silently ran as a VT100 and the `== 0` guard in `main.c` could
  never fire. Fixed in `oracle/src/main.c:resolve_term_id()`; the same shape of
  trap is anywhere else upstream "defaults" instead of erroring.
- **Breaking a wide character and erasing one are different operations, and
  the oracle knows which is which even when it looks arbitrary.** The
  overwrite/insert/delete/scroll paths call `BuffSetChar(b, ' ', 'H')`, which
  blanks the text and the colour indices and leaves the SGR bits; the erase
  paths call `EraseKanji`, which paints the whole pen. Using one where upstream
  uses the other is invisible until a case sets a background colour and then
  cuts a wide character with a different pen.
- **The padding half of a wide character is written with zeroed attributes**
  (`buffer.c:3400`) — except in the insert-mode branch (`:3325`), which copies
  the pen. Both are reproduced; neither is a typo on our side.
- **`disp_width()` in the oracle's own `main.c` is a stub of the same dangerous
  kind.** It used to test only `'W'` for full width, when
  `BuffIsHalfWidthFromPropery` treats `'W'` **and** `'F'` that way — so every
  fullwidth form (U+FF01 onward) counted one column in the dump while the
  buffer had given it two, and the row was padded past its own width. Also: the
  dump takes its size from `NumOfColumns`/`NumOfLines`, **not** from argv, or a
  mid-stream `CSI 8;h;w t` measures against the startup size.
- **Three more stubs lied, and all three were invisible until a mouse event
  arrived.** `ShiftKey`/`ControlKey`/`AltKey` are declared as *functions* in
  `keyboard.h` and were defined in `stubs_manual.c` as `BOOL` **variables** —
  which links, and jumps into the data section the first time anything calls
  one. `DispConvWinToScreen`/`DispConvScreenToWin` were empty generated stubs
  that never stored through their out-parameters, so `MouseReport` read an
  uninitialised position off the stack. And `IsCaretEnabled` returned 0
  unconditionally, which would have made DECRQM report DECTCEM permanently
  reset and taught the port to agree. All three now carry real behaviour.
- **`WinOrgY` drifts negative in a headless build and must not be used.**
  `buffer.c:3865` subtracts the scroll amount from it on every scroll so the
  visible rows stay put, and `vtdisp.c` — not compiled here — puts it back.
  The oracle's coordinate conversion therefore uses a fixed `(0,0)` origin,
  which is the state a real Tera Term is in when not scrolled back. Adding it
  back put a click six rows above the screen.
- **`ts.MouseEventTracking` and `ts.TranslateWheelToCursor` are the flag-word
  trap in a plain `WORD`.** Both are `GetOnOff(..., TRUE)` (`ttset.c:1523`,
  `:1515`) and both were left zeroed by `memset`, which silently disabled every
  mouse mode and made DECRQM answer "permanently reset" for all of them. When
  adding a setting, the rule is the same as for the flag words: find the key,
  not the initialiser.
- **Stubs lie, and `DispFindClosestColor` did.** It lived in `stubs_manual.c`
  with *xterm's* palette rather than Tera Term's, and without the bright/dim
  flip the real one applies, so every truecolor SGR resolved to the wrong
  index. When a manual stub reimplements upstream logic, diff it against the
  original — `vtdisp.c` is not compiled into the oracle, so nothing else will.

And for the desktop side:

- **The container's Qt is 6.4.2; the desktop's is 6.11.1.** Seven releases apart.
  Qt windows opening here proves the *plumbing*, not the *behaviour* — anything
  version-sensitive (Wayland protocol support, HiDPI, `text-input`) measured on
  6.4.2 does not transfer to the target. Build a Fedora 44 container before
  drawing conclusions about how the shell behaves on the real desktop. The host
  ships no Qt devel files, so `/run/host` is not a shortcut.
- **Never measure Qt behaviour in the Ubuntu container.** Worked example, and
  the reason this trap exists: on Ubuntu's Qt 6.4.2 a plain `QWidget` app under
  Wayland loads Mesa's gallium driver and costs 62 MB of *private* memory
  (95 MB RSS vs 32 on X11, confirmed by PSS), and
  `QT_WAYLAND_CLIENT_BUFFER_INTEGRATION` appears to fix it. **On Qt 6.11.1 none
  of that is true** — Mesa is never mapped, Wayland costs 3 MB more than X11,
  and the variable does nothing. Startup and RSS were also flattering by ~2x.
  A whole false optimisation, from one version gap. Use `termitta-fedora`.
- **You can screenshot your own widgets, not the desktop.**
  `org.gnome.Shell.Screenshot` returns `AccessDenied` (locked down since
  GNOME 45), `QScreen::grabWindow(0)` is uniform-blank under xcb — host windows
  are Wayland-native and invisible to Xwayland — and returns NULL under
  wayland. **`QWidget::grab()` works on both** and is the one to use; it
  re-renders offscreen, which is exactly right for checking our own painting.
  Full-desktop capture needs the xdg-desktop-portal Screenshot API, which
  prompts the user every time.
- **CJK is deferred indefinitely** (decision 2026-08-07, see `PLAN.md`). Don't
  start IME work, and don't read "IME untested" as an open risk. If it is ever
  revived: the plumbing is there — Qt's `libibusplatforminputcontextplugin.so`
  is installed and the ibus portal is on the session bus — but GNOME input
  sources are `[('xkb','gb'), ('xkb','es')]`, so nothing is configured to talk
  to. An empty result would mean "no input source", not "Qt is broken".
  This is **input only**. Wide and combining character handling in the grid
  stays in scope: it comes free with the oracle-driven port, and box drawing,
  emoji and combining accents need it regardless of CJK.

## Bugs found upstream, not yet reported

Five — four in `buffer.c` and one in `vtterm.c` — all found by diffing the two
engines. Patches in `oracle/patches/`, reports drafted in
`docs/upstream-bugs.md`. Filing needs a GitHub account and is an open item in
`PLAN.md`.

1. **`BuffGetAnyLineDataW` does not advance past padding cells** (`:5832`), so
   it parks on the padding after a full-width character and drops the rest of
   the line. Sole caller is `filesys_log.cpp:443` — **session logging truncates
   any line at its first CJK character.**
2. **`BuffGetAnyLineDataW` budgets output units with a column count.** `left`
   is seeded from `copysize` (cells) but spent in `wchar_t` units, so any line
   with combining marks truncates at about half the width. Independent of 1.
3. **ECH writes past the end of the line.** `CSI Ps X` clamps `Ps` to the
   terminal *width* and then writes that many cells *from the cursor*, so it
   overshoots by the cursor's column into the next line — and off the end of
   the allocation on the last line. The parameter comes off the wire, so this
   is an attacker-controlled out-of-bounds write. **File this one first.**
4. **`BuffSelectedErase*` index a line-relative pointer with an absolute buffer
   offset** (`:5491`, `:5531`). `CodeLineW` is the cursor's line, `j` is an
   absolute offset, so DECSED tests the protect bit on the wrong cell, writes
   to it, and leaves the allocation once the page is in the second half of the
   ring — proven under ASan. A second, unrelated defect in the same loop
   subtracts the start column from the end bound. **File this one second.**
5. **`MakeMouseReportStr` builds the row's UTF-8 lead byte from the column**
   (`vtterm.c:5591`). In `DECSET 1005` mouse tracking a row past 96 needs two
   bytes, and that branch reads `x` where it means `y` — so the row is wrong by
   a multiple of 64, or the report contains `0xC0`, which is not valid UTF-8.
   The only one outside `buffer.c`.

## Layout

```
PLAN.md          roadmap + status — read first
ATTRIBUTION.md   licensing, and what still needs clearing before vendoring
oracle/          Tera Term's real VT engine, headless on Linux (see its README)
xfer/            Stage 0 spike 2 — ttpfile's protocols, running and interoperating
serial-audit/    Stage 0 spike 4 — serialport-rs vs commlib.c, on real hardware
ssh-audit/       Stage 0 spike 5 — russh vs legacy SSH algorithms and auth
crates/          Rust core — tt-grid, tt-vt, tt-dump (see its README)
run_diff.sh      the differential gate: Rust engine vs Tera Term, every case
shell/           Qt 6 shell — not started
vendor/          vendored Tera Term subsystems — empty, see ATTRIBUTION.md first
```

None of `xfer/`, `serial-audit/` or `ssh-audit/` is throwaway. They become the
regression suites for `tt-xfer` and `tt-conn`, and every claim in `PLAN.md`'s
spike sections is reproducible from them.

`ssh-audit/servers.sh` needs `sudo`: it runs sshd and dropbear on localhost
ports and creates a throwaway `termitta-test` account. **Run `./servers.sh stop`
when done** — that is what removes the account.

**`oracle/winshim/` is shared, not oracle-private.** `xfer/` builds against it
too. Adding to it is usually right — the Win32 surface the protocols needed
turned out to be a subset of the VT engine's — but **re-run `oracle/run_tests.sh`
after touching it**, because the oracle is the thing that must not regress.

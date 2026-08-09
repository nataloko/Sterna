# Working notes for Sterna

Read `PLAN.md` for the roadmap and current stage. This file is the working
agreements and the traps.

## What this is

A cross-platform Tera Term successor: Rust core + flat C ABI + Qt 6 Widgets
shell, Linux and Windows. **Not** a fork of Tera Term and **not** aiming at
parity — see `PLAN.md` for scope.

Sterna is the settled project name. The mark is a banked tern tracing an
S-shaped flight path.

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
cargo fmt --all                  # the whole workspace, generated.rs included
tt-ffi/run_abi.sh                # the C ABI, compiled and driven from C
cargo test -p tt-xfer            # the protocols vs lrzsz and gkermit, over a pty
cargo test -p tt-ttl             # the macro language, with no terminal attached
cargo test -p tt-ttl --test scripts          # ...and upstream's own 53 macros
cargo test -p tt-lua             # the other language, over the same host
cargo test -p tt-macro           # ...and one driving a real session, threaded
cargo test -p tt-macro --test lua   # ...in either language, same host
cargo test -p tt-ctl             # the control socket, the wire and the address
cargo test -p tt-ctl --test cli  # ...and `ttctl`/`ttpmacro`, as subprocesses
TTL_BLESS=1 cargo test -p tt-ttl --test scripts   # rewrite the transcripts,
                                             # then read every one you changed
../vendor/ttpfile/sync.sh --check   # ...and that the vendored C has not drifted
cargo run -p tt-config --bin gen-settings   # after editing the settings schema
TT_SERIAL_A=/dev/ttyUSB0 TT_SERIAL_B=/dev/ttyUSB1 \
  cargo test -p tt-conn -- --test-threads=1   # + the serial hardware tests
TT_SERIAL_A=/dev/ttyUSB0 TT_SERIAL_B=/dev/ttyUSB1 \
  cargo test -p tt-session -- --test-threads=1   # one package at a time
cd ../ssh-audit && ./servers.sh start            # + the SSH tests need a server
D=$XDG_RUNTIME_DIR/sterna-ssh-audit
TT_SSH_HOST=127.0.0.1 TT_SSH_PORT=2222 TT_SSH_USER=$USER \
  TT_SSH_KEY=$D/id_ed25519 TT_SSH_PW_USER=sterna-test \
  TT_SSH_PASS=spike5-not-a-secret \
  cargo test -p tt-conn --test ssh -- --test-threads=1   # ...and PORT=2223
cd ../telnet-audit && ./servers.sh start          # needs no sudo, no accounts
TT_TELNET_HOST=127.0.0.1 TT_TELNET_PORT=2323 TT_TELNET_RAW_PORT=2324 \
  cargo test -p tt-conn --test telnet

cd crates/fuzz                   # the fuzzers — nightly only
./seed.sh                        # corpus out of oracle/cases/
cargo +nightly fuzz run vt_stream -- -max_total_time=300
cargo +nightly fuzz run vt_chunks    # ...where the chunk boundaries fall
cargo +nightly fuzz run telnet       # ...and the decoder that reads a server
cargo +nightly fuzz tmin vt_stream artifacts/vt_stream/<file>

cd shell                         # the Qt 6 frontend — build it in
                                 # sterna-fedora, never here
cmake -S . -B build -G Ninja && cmake --build build
./build/render_test              # the painter, asserted against grabbed pixels
./build/render_test --write /tmp # ...and dumped as a PNG to look at
./build/ssh_test                 # the window's event loop, against a real server
./build/ssh_test --write /tmp    # ...and the four SSH dialogs, as PNGs
./build/telnet_test              # the same, over telnet
./build/pty_test                 # ...and over a local shell, which needs nothing
./build/xfer_test                # a ZMODEM send, driven by the event loop
./build/xfer_test --write /tmp   # ...and the transfer dialogs, as PNGs
./build/macro_test               # a TTL macro, driven by the event loop
./build/macro_test --write /tmp  # ...and the dialogs it raises, as PNGs
QT_QPA_PLATFORM=offscreen \
  ./build/cmdline_test           # a Tera Term command line, argv to connected
                                 # — NOT under Wayland; see the traps
./build/control_test             # the control socket, against the window's loop
./build/sterna --port /dev/ttyUSB0 --baud 115200
./build/sterna myrouter        # an alias out of ~/.ssh/config
./build/sterna --shell         # a local login shell
./build/sterna /ssh /auth=publickey myrouter   # ...and Tera Term's own line,
                               # which a `/OPTION` anywhere switches to

./bench/bench.py --core          # the perf gate's half that runs anywhere
./bench/bench.py                 # ...and the Qt half, in sterna-fedora only
./bench/bench.py --update        # re-record baseline.json, on a QUIET machine
cmake --build shell/build-release --target bench_shell   # it is EXCLUDE_FROM_ALL

cd packaging/appimage            # the only Linux artifact — build it in
                                 # sterna-fedora, never here
./build.sh                       # → build/sterna-x86_64.AppImage
./build.sh --clean               # ...from scratch
./build.sh --run                 # ...and start it

cd esctest                       # conformance, from inside our own terminal
./run_tests.sh                   # 568 cases; gates on drift from `expected`
./run_tests.sh CUPTests          # just the ones matching (a regex)
./run_tests.sh --bless           # rewrite `expected`, then write the reasons
./run_diff.sh                    # whose decision is a failure? ask the oracle
./run_diff.sh -v DECIC           # ...and print the diff

cd oracle
make            # build build/oracle
make test       # 72 regression cases
make stubs      # regenerate the stub layer after upstream headers change

cd xfer                          # Stage 0 spike 2
make && ./run_tests.sh           # 10 interop cases vs lrzsz and gkermit

cd ssh-audit                     # Stage 0 spike 5
./servers.sh start && cargo run && ./servers.sh stop

cd telnet-audit                  # a real telnetd for the transport's interop
./servers.sh start && ./servers.sh stop

cd ini-audit                     # what GetPrivateProfile* really does
./run.sh                         # needs wine64 + mingw-w64; 127 without them
./run.sh --record                # ...and rewrite win32.txt with the answers

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

`sterna-fedora` needs **`lrzsz`** too, for `shell/build/xfer_test` (added
2026-08-08).

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

### Qt work goes in the `sterna-fedora` container, not this one

This container has Qt **6.4.2**; the desktop runs **6.11.1**. That gap has
already manufactured one false finding — see the traps. So there is a second
distrobox, created 2026-08-07:

```sh
distrobox-host-exec distrobox enter sterna-fedora --no-tty -- <command>
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

**Rust needs no install there.** `~/.cargo` is the same directory both
containers see, and the Ubuntu-built toolchain runs fine on Fedora 44 — glibc
2.43 runs a binary linked against 2.39, just not the other way round. Export
`$HOME/.cargo/bin` and it works. **`systemd-devel` does** need installing (done
2026-08-08): it is Fedora's `libudev.pc`, without which `serialport-rs` will
not build, and it is the same dependency the Ubuntu container needed as
`libudev-dev`.

That glibc asymmetry is also why `shell/CMakeLists.txt` points
`CARGO_TARGET_DIR` at its own build tree rather than at `crates/target`. A
shared target directory would let a library linked in one container be reused
by a binary built in the other — fine in one direction, a confusing loader
error in the other.

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
- **`ts.BSKey` defaults to `IdBS`** — `ttset.c:877` reads the key with an empty
  fallback and only the literal `"DEL"` takes the DEL arm, so an absent key
  means BS. **This is the third setting whose real default is an `else`
  branch**, after `CRReceive` and the flag words. When a setting is read as a
  *string* and compared, look at what an empty string does; when it is read as
  a flag word, find the key rather than the initialiser. Reading either wrong
  gives a terminal that is subtly not Tera Term.
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
And for the AppImage, where two of the three failures are silent:

- **linuxdeploy's `patchelf` corrupts every library it bundles on Fedora 44.**
  It predates `.relr.dyn`, the compact relocation format the base uses
  everywhere. Its `strip` hits the same wall and says so out loud — "unknown
  type [0x13]" — so `NO_STRIP=1` is set; its `patchelf` says nothing, and the
  file comes out ~2 KB larger and **segfaults in its own `_init`**, before
  `main`, before Qt can log a word. Whichever bundled library the loader
  reaches first is the one in the backtrace, so the crash appears to move
  between libgomp, libicudata and whatever else and to be *about* that library.
  `packaging/appimage/build.sh` lets linuxdeploy do the discovery and then puts
  the originals back, resolving by `LD_LIBRARY_PATH` instead of by rpath.
- **A Wayland window that never appears is not an error.** Qt's Wayland
  platform plugin needs `wayland-shell-integration/libxdg-shell.so` to create
  an `xdg_toplevel`; without it the process binds the registry and sits there
  with no window, no warning and no non-zero exit — which is indistinguishable
  from a working headless run. `WAYLAND_DEBUG=1` and a grep for
  `get_xdg_surface` is the only check that tells them apart, and it is why
  "it stayed alive for 8 seconds" was wrong the first time.
- **The desktop has Qt 6.11.1 installed, so an AppImage that quietly used the
  host's Qt would pass every obvious test.** Check
  `/proc/<pid>/maps` for where `libQt6Core.so.6` actually came from; it must be
  the `/tmp/.mount_sterna*` path. That prefix is the AppImage's own filename,
  so it followed the rename — grepping for the old one finds nothing and reads
  exactly like the failure this check exists to catch.
- **A named constant can be a flag word too, and `IdTitleReportEmpty` is.** It
  is **24**, which is `WF_TITLEREPORT` entire (8|16) — so
  `TitleReportSequence`'s "Empty" default sets *both* bits and lands on the
  `default:` arm, where `CSI 20 t` and `CSI 21 t` answer with an empty OSC
  string. The oracle read the name as "no bits" and was a Tera Term with title
  reporting switched off. Same trap as `ISO2022Flag` and friends, one level
  further disguised: here the wrong value is not a zero initialiser but a
  constant whose *name* sounds like one.
- **`rewrite_c1` has to know where UTF-8 sequences begin and end.** It replaces
  a bare `80..=9F` byte with U+FFFD, because that is what Tera Term's decoder
  does with invalid UTF-8 while `vte` would execute it as a C1 control. Do that
  test byte by byte and the `80` inside an em dash's `E2 80 94` is eaten too —
  and no differential case caught it, because none of them had a multi-byte
  character whose continuation byte fell in that range. Case 97 does now.
- **`vte` must never be handed a partial UTF-8 sequence, and `Vt::held` is what
  stops it.** `vte` 0.15.0's `advance_partial_utf8` prints only the *first*
  character of what it decoded and then returns `valid_up_to()` as the bytes it
  consumed, so a complete character in between is dropped in silence. It takes
  a two-byte sequence cut by a read boundary, then exactly one ASCII byte, then
  another multi-byte lead — which on a UTF-8 console is an ordinary Tuesday.
  Every test in this repository except `crates/tt-fuzz/` feeds whole files, so
  nothing else can see it. **A chunking bug is invisible to the differential
  suite by construction**; if you touch `rewrite_c1`, the property to run is
  `vt_chunking`.
- **The differential dump cannot see width classes, so it will call a broken
  grid `ok`.** A wide character whose halves have come apart still prints as one
  glyph in two columns, and an orphaned padding cell prints as nothing, so a row
  that has lost half a character renders identically to one that has not. Three
  divergences hid behind this in one session and two were real bugs. Dumping
  upstream's `AttrKanji` does **not** rescue it: the bit is set on the
  non-insert write path and not the insert one (`Attr_Attr` is the pen's byte
  alone) and `BuffSetChar` never clears it, so upstream's own copy is
  incoherent and the two engines would be compared on a quantity neither
  renders from. `Grid::check_wide_pairs` is the only check that covers this,
  and it is deliberately *not* an invariant — Tera Term breaks the pairing in
  three places itself, all listed on that function.
- **A parked space goes through the whole write path, not straight into the
  cell.** With one column left for a double-width glyph, upstream parks a space
  and retries by calling `BuffPutUnicode(0x20, …)` recursively
  (`vtterm.c:896`), so the space triggers the two crushes at the top of that
  function — and the cursor is standing on a padding cell rather often here,
  since a wide glyph at the right margin leaves it there by design. Writing the
  cell directly leaves a wide character with its right half replaced by a
  space, which is the one thing the branch exists to prevent.
- **DECRQCRA is not upstream's**, which is why `Config::decrqcra` exists and
  defaults off. It is the only way to read a cell back over the wire and
  `esctest/` asserts on nothing else; a real connection stays byte-for-byte
  Tera Term because only the conformance harness turns it on.
- **esctest's `--test-case-dir` recordings do not include its reset preamble.**
  The side channel is attached *after* `reset()`, so a replayed stream starts
  from the engines' own defaults rather than from esctest's 80x25 soft-reset
  state. Fine for `esctest/run_diff.sh`, which only asks whether the two
  engines agree; not fine as a way to reproduce a specific test's verdict.
- **Stubs lie, and `DispFindClosestColor` did.** It lived in `stubs_manual.c`
  with *xterm's* palette rather than Tera Term's, and without the bright/dim
  flip the real one applies, so every truecolor SGR resolved to the wrong
  index. When a manual stub reimplements upstream logic, diff it against the
  original — `vtdisp.c` is not compiled into the oracle, so nothing else will.

And for the vendored protocol C, which two compilers disagree about:

- **The Ubuntu container's GCC 13 compiles `vendor/ttpfile/` and Fedora's does
  not**, and neither failure names the real cause. `raw.c` calls `malloc`
  having included no `<stdlib.h>` — a warning on GCC 13, an *error* from GCC 14
  — and `zmodem.c:1586` calls `SetTimer`/`KillTimer`, which nothing declared.
  Both are fixed in `winshim/windows.h`, which is where MSVC's `<windows.h>`
  would have supplied them. **The rule: a vendored source that needs a
  declaration gets it from the shim, never from an edit.**
- **`common/ttcstd.h:45` typedefs `char8_t` when `__cplusplus >= 202002L`** —
  the guard is inverted, since that is exactly when the language already has
  it. So `protolog.cpp` compiles at C++17 and not at C++20, and GCC 13 defaults
  to the first while GCC 16 defaults to the second. `tt-xfer/build.rs` pins
  `gnu++17` for the vendored C++ rather than inheriting whatever the container
  has.

And for telnet:

- **The framing and the negotiation are two files upstream, and the framing
  runs first.** `ttcmn.c` unescapes `IAC IAC`, swallows the `NUL` after a `CR`
  and only then hands bytes to `telnet.c`. Reading `telnet.c` alone gives a
  parser that doubles every `0xFF` and passes `CR NUL` through to the terminal.
- **`ttcmn.c` clears its CR flag whatever the next byte is** (`:572`), so only
  a `NUL` is lost after a `CR`. Clearing it only on the `NUL` path drops the
  `IAC` in `CR IAC …` and leaves the negotiation one byte out of step for the
  rest of the session.
- **The opening burst goes out only when the port is 23** (`vtwin.cpp:3666`,
  `ts.TCPPort == ts.TelPort`). This looks like an oversight and is not: a
  terminal server's per-line port is not a telnet server, and opening at one
  with `WILL TERMINAL-TYPE` puts five bytes of protocol into somebody's serial
  console.
- **`MaxTelOpt` is 34 and everything above it is refused flat.** Reproduced
  rather than widened — a real `telnetd` opens with `WILL AUTHENTICATION`,
  `WILL ENCRYPT`, `DO XDISPLOC` and `DO NEW-ENVIRON`, all above it, so the
  refusal path runs before anything else in every session.
- **NAWS arrives backwards and upstream acts on it anyway.** `telnet.c:299`
  has the "did we negotiate this" test commented out, so a server's NAWS
  resizes the terminal whether or not NAWS was agreed. That is a console
  server describing the equipment behind it, and it is reproduced including
  the laxity.
- **Telnet has a break and SSH does not**, which is why `supports_break` is on
  the transport rather than assumed.

And for the local pty:

- **Hold the slave end open and nothing ever ends.** We own one end of the pty
  and the child owns the other; keeping ours after `spawn_command` means the
  master never sees the hangup when the child exits. No error, no data, no EOF
  — the window waits forever on a shell that left. `pty/mod.rs` drops
  `pair.slave` immediately and says so.
- **`portable-pty`'s unix reader maps `EIO` to `Ok(0)`**, so `read_to_string`
  terminates on it. `EIO` is a pty master saying the child is gone, and `Ok(0)`
  is already `tt-conn`'s word for "the line is quiet". Take that mapping and a
  dead shell looks idle — and because a hung-up descriptor is *permanently*
  readable, the frontend's `QSocketNotifier` fires forever against a read that
  returns nothing. **A dead shell would present as a terminal at 100% CPU.** So
  the read and write are done on the master's raw fd instead.
- **The exit status dies with the child handle**, which is why
  `Transport::closing_note` is asked *before* the transport is dropped rather
  than afterwards. Every other transport returns `None` from it.
- **`std::process::Child` does not reap on drop.** A session that opens and
  closes local shells all day would leave one zombie per shell, so `Drop`
  closes the master (which is the `SIGHUP`), waits briefly, then `SIGKILL`s.
  Both waits are bounded: leaving a zombie beats hanging the window that is
  trying to close.
- **`portable-pty` sets no `TERM`**, so the child would inherit ours — *that*
  terminal's name when launched from a terminal, and nothing at all when
  launched from a desktop menu. It is set explicitly, along with `COLORTERM`,
  and `LINES`/`COLUMNS` are removed because the `winsize` is the truth and a
  stale pair survives every resize.
- **`portable-pty` drags in `serial2`**, a second serial-port crate, with no
  feature to disable it. Accepted; don't "fix" it by hand-rolling the pty —
  what it buys is the child-side `setsid`/`TIOCSCTTY` dance and ConPTY in
  Stage 3.

And for SSH:

- **The SSH tests need `--test-threads=1`, and not for the rig's reason.**
  Nothing is shared; the *server* declines. OpenSSH's `MaxStartups` defaults to
  `10:30:100` and starts randomly refusing above ten concurrent unauthenticated
  connections, and dropbear's ceiling is lower — so in parallel a handful fail
  with what looks like a connection bug, all of them pass in isolation, and on
  dropbear it is ten of fifteen.

- **`~/.ssh/config` takes the FIRST value for a keyword, not the last.** The
  opposite of nearly every other config format, and it does not fail loudly: a
  `Host *` block at the *top* of a file silently overrides every specific block
  below it, so the user gets the wrong account or the wrong key and their setup
  "just doesn't work". `IdentityFile` is the single exception and accumulates.
- **The algorithm name for `known_hosts` comes out of the key blob, not from
  `PublicKey::algorithm()`.** A host key verified with `rsa-sha2-512`
  signatures is written down as `ssh-rsa` — RFC 8332 leaves the blob's own type
  string alone — so taking the negotiated name reports every RSA host in the
  file as unknown.
- **`check` has to read every file to the end.** Returning at the first
  accepting line is faster and wrong: an `@revoked` entry further down, or in
  the second file, has to be able to overrule it.
- **`best_supported_rsa_hash()` returns `Result<Option<Option<HashAlg>>>`** and
  each layer means something different. All three collapse to the same
  fallback — plain `ssh-rsa` — but `.ok().flatten()` type-checks against the
  wrong one.
- **`tt_session_pump` returns the moment the line is quiet**, which is the
  point of it. So a C or Qt caller that "waits" by pumping in a loop spins
  through a thousand iterations in a millisecond and concludes the far end
  never answered. Wait on the descriptor. This cost a debugging round in
  `abi.c`.
- **A Qt dialog spins a nested event loop**, so the `QSocketNotifier` fires
  again while a host-key prompt is open. Without `Session::m_sshWaiting` the
  poll re-enters, invalidates the borrowed strings the open dialog is showing,
  and asks the same question twice.
- **A test that connects to `127.0.0.1:2222` reads the developer's own
  `~/.ssh/known_hosts`** unless told otherwise, and a hashed entry left over
  from some earlier session turns `Unknown` into `NewAlgorithm`. Point
  `TtSshParams::known_hosts` at a scratch file.
- **`QWidget::grab()` on a dialog that has never been shown renders it before
  layout**, so wrapped labels overlap in the image and nowhere else. One
  `adjustSize()` removes the discrepancy — otherwise a perfectly good dialog
  looks broken in its own screenshot.
- **Qt caps a window's *initial* size at two thirds of the screen**
  (`QWidgetPrivate::adjustedSize`), so a `sizeHint` of 100x30 cells opens at
  59x23 under the 800x800 offscreen platform and at 100x30 on a real desktop.
  A test that asserts a configured terminal size has to `resize(sizeHint())`
  first, or it is measuring the screen rather than the code.
- **Anything that constructs a `MainWindow` now reads the developer's own
  settings**, because the window loads `sterna.ini` — and the terminal's
  *size* is in it. `bench_shell` calls `QStandardPaths::setTestModeEnabled`
  before `QApplication` for that reason: otherwise a 132x50 in somebody's file
  silently benchmarks a different window from the baseline's, consistently, for
  a reason nobody would think to look for. `cmdline_test` needs it too, and for
  the *title* as well as the size.
- **`qWarning` does not reach stderr on Fedora, and the case where that matters
  is the case it was written for.** Fedora builds Qt with journald support, so
  every `qWarning`/`qDebug` goes to the systemd journal rather than to stderr
  whenever stderr is not a terminal — which is how a script, a `.desktop` entry
  or a cron job launches something, and precisely how a windowless `/V` session
  is launched. The message is findable with `journalctl` and nowhere a user
  would look, and nothing says it was diverted: a diagnostic just does not
  appear. It cost a debugging round here, where it read as "the option was
  never parsed" rather than "the message went somewhere else" — `/ssh-bogus`
  was, in fact, parsed correctly all along. `QT_FORCE_STDERR_LOGGING=1` proves
  which it is in one run. Anything the user has to see uses `fprintf(stderr)`,
  which is what `QCommandLineParser` does with its own errors for the same
  reason.

And for measuring anything:

- **`QFile` cannot read `/proc`, and does not say so.** `QFileDevice::atEnd()`
  answers from `size()`, and every file under `/proc` reports zero because its
  contents are generated on read — so a `while (!f.atEnd())` loop never runs
  once, the field is never found, and the measurement comes out as a confident
  `0.0 MB`. Which is exactly what a window using no memory would look like.
  `bench_shell` uses stdio, which has no opinion about size.
- **`TerminalView`'s 8 ms frame floor is load-bearing, and only Wayland hides
  its absence.** The session pumps once per wake of its notifier, so a burst is
  one damage per 8 KB read on its own turn of the event loop — one frame per
  read, and a frame costs about what parsing 8 KB does. Wayland's frame
  callbacks coalesce about eight reads into a frame; X11 and the offscreen
  platform do not. Removing the floor takes xcb from 36 MB/s back to 4, and a
  Wayland desktop would never show it. The related consequence: **a headless
  measurement understates the desktop by 4x** rather than flattering it, and a
  throughput figure has to name its platform — which is why
  `bench/baseline.json` records the platform *and* the Qt version, since 6.4.2
  and 6.11.1 both answer `"wayland"`.
- **Do not "fix" this by giving `tt_session_pump` a budget instead.** It reads
  until the line is quiet, and serial and telnet both read with a 50 ms
  timeout — so the second read of a burst blocks the UI thread for 50 ms.
  Coalescing the frames costs nothing and does not care what the transport is.
- **A Wayland client cannot place its own window, and `cmdline_test` is where
  that shows up.** There is no set-position request in `xdg_shell` — placement
  is the compositor's — so `QWidget::move()` is silently ignored and `/X=120`
  reports `pos().x() == 0`. It is one failing check out of the suite, in the
  test named after the option, which reads as a command-line parsing bug: the
  option *was* parsed, and the window manager declined. Run that one under
  `QT_QPA_PLATFORM=offscreen` or `xcb`, which is what CI does. The same limit
  applies to anything else asserting a window's position.
- **A Wayland compositor stops sending frame callbacks to a surface it thinks
  is hidden**, so a short-lived probe window gets ~5 of 24 keystrokes painted
  inside a two-second wait where xcb gets 24. Not a bug in the shell — but any
  Qt measurement that waits for a paint has to tolerate it.
- **The calibration loop corrects for a slower machine, not a busier one.** The
  first baseline here was recorded while a `cmake` build was finishing: the
  fixed CPU loop was 1.5% slow and the engine was 14% slow, so nothing flagged
  it and the gate would have been permanently weaker with no way to tell.
  Re-record on a quiet machine, and read the file before committing it.

And for the serial side:

- **`tcsetattr` returns success if it could apply *any* of what you asked.**
  The FTDI accepts `CS5` and then transmits eight bits anyway. Read settings
  back before believing them; `tt-conn`'s `set_data_bits` does.
- **`serialport-rs` calls a busy port `ErrorKind::NoDevice`**, so the naive
  mapping says "unplugged" when the truth is "`minicom` is still running".
  Both that and the `BrokenPipe`-means-disconnect mapping are wrapped in
  `tt-conn/src/error.rs` — one place to fix.
- **Never call `tcdrain` from a thread that must stay responsive.** Flow
  control can hold the output queue forever. `tt-conn::SerialConn::flush` takes
  a timeout and polls `TIOCOUTQ`.
- **A test byte with bit 7 set cannot tell 7 data bits from 8.** At seven bits
  the stop bit lands in bit 7, so `0xA5` round-trips as `0xA5` either way and
  the test passes whatever the port is doing. Use `0x25`.
- **Ports left in flight leak into the next test.** Closing a port does not
  stop bytes already handed to the adapter; they turn up in the next test's
  first read and look like a bug in whatever it measures. `loopback.rs`
  settles the rig between tests.
- **Applying a settings-derived parameter set changes what the caller did not
  ask about, and the settings need not describe the open port.** Upstream can
  rebuild the whole `DCB` from `ts` because the port was opened from `ts`;
  here `Session::connect` takes any transport and the shell's `--baud` opens
  one from a `SerialParams` the settings never saw. A `setbaud 19200` built
  that way moved a 115200 port to the schema's default 9600 and took the
  parity with it — and reported itself as a *flow control* failure, because
  the test that noticed was the one where XOFF stopped mattering. The live
  parameters are the truth about the port; edit one field of them
  (`Session::reset_serial`).
- **A test that changes the line speed must settle the rig, and the garbage it
  leaves is invisible.** Bytes at the wrong baud arrive as framing errors,
  which `detect_break` turns into `BadByte` *events* rather than characters —
  so a row that lost its first four letters looks like a truncation bug, and a
  stray `ESC ]` in the noise opens an OSC string that silently eats the rest of
  the test. Assert at the far end's bytes rather than on the screen when a
  speed changes.
- **`--test-threads=1` is per test *binary*, and cargo runs the binaries
  concurrently anyway.** So `cargo test -p tt-conn -p tt-session --
  --test-threads=1` puts two hardware suites on the same two ports at the same
  time, and one loses — `lock_uses_whatever_the_flow_control_implies` is
  usually the one that reports it, which makes it look like a flaky `tt-conn`
  rather than an overbooked rig. Run one package at a time. There is no cargo
  flag for this; `--jobs` is about compilation.

And for the settings, all of which came out of `ini-audit/`:

- **`GetOnOff` is default-biased, so `Key=1` means opposite things for two
  different settings.** `ttset.c:344`: with a default of on, anything that is
  not literally `off` is on; with a default of off, only literally `on` is on.
  So `Xterm256Color=1` is on and `Aixterm16Color=1` is off, from the same file,
  with the same value. It also reads into a **four-byte buffer**, so only the
  first three characters reach the comparison and `offline` is `off`. This is
  the fifth member of the family that already holds `CRReceive`, `BSKey` and
  the flag words: **when a setting looks boolean, find out what its default is
  before deciding what a value means.**
- **`GetPrivateProfileString` strips a matched pair of quotes**, single or
  double — which `PLAN.md` had backwards, and which MSDN does document. An
  unmatched or interior quote is kept, and `""x""` loses exactly one pair. A
  reader that keeps them puts literal `"` into every quoted setting; one that
  strips unconditionally mangles a value that legitimately starts with one.
- **`Key=` is an empty string, not the default**, and upstream leans on it —
  that is exactly how `ts.BSKey` reaches its `else` branch. Collapsing empty
  into absent changes the setting for everyone who has the key without a value.
- **The first duplicate wins, and a duplicate section is not merged.** A key
  that appears only in the second `[Tera Term]` block is invisible to Tera Term,
  so it must be invisible here too — reading it would apply a setting the user's
  own terminal ignores.
- **A comment is only a comment to *enumeration*.** `;A=1` is an entry whose key
  is `;A`, and a lookup for `;A` returns `1`; only a key listing skips it. A
  line with no `=` is not an entry at all, either way. Both were guesses until a
  probe case settled them, which is the reason `ini-audit/` exists.
- **Wine is not Windows, and two of the recorded answers are Wine's alone**: a
  write rewrites every line ending in the file, and normalises `[ s ]` to
  `[s]`. Both are in `ini-audit/divergences.txt` as *not* reproduced. Re-run
  the battery on Windows in Stage 3 before trusting either.
- **`gen-settings` pipes its output through `rustfmt`, and that is load-bearing
  rather than tidy.** `cargo fmt --check` covers every file in the workspace,
  a generated one included, so an emitter rustfmt disagrees with makes the lint
  gate permanently red — and reformatting the file by hand is not the fix, it
  just moves the failure to `the_generated_file_is_current`, which then says
  "src/generated.rs is stale", the opposite of what happened. That pair had
  main red for a while and cost a session to unpick. **Do not "simplify" the
  generator by emitting pre-formatted text instead:** the one-line `if` bodies
  are easy, but where a call or a match arm wraps depends on the *width of a
  setting's name*, so the emitter would start losing to the gate the first time
  somebody added a long one, with no warning. The consequence to know about is
  that `cargo test -p tt-config` now needs `rustfmt` on `PATH` — it says so if
  it is missing, rather than reporting a stale file.
- **`TerminalID` is `strcmp`; every other enumerated setting is `_stricmp`.**
  `tttypes_termid.cpp:60`. And `TermIDGetID` never fails, so `TerminalID=vt320`
  is not an error — it is a VT100, silently, for ever. That is why the schema
  has an `enum_exact` at all. The same table also has `VT220` and a
  **lower-case `dumb`**, both of which the first schema pass omitted, so both
  read as VT100 too.
- **`TermWidthMax` is 1000 and `TermHeightMax` is 500** (`tttypes.h:633`), one
  line apart. Taking the wrong one put a 500-column cap on `tt-grid` and a
  wrong range in the schema, and neither shows up until somebody asks for a
  640-column terminal and quietly gets half of it.
- **`ttset.c:615` bounds a size and does not clamp it.** At or below the floor
  takes the *default*, above the ceiling takes the ceiling — so
  `TerminalSize=0,0` is 80x24, not 1x1. Clamping to the floor gives a
  one-column window out of a file the user's own Tera Term opens fine.
- **`ScrollBuffSize` is the whole buffer, page included, and upstream grows it
  to hold the page rather than shrinking the page to it** (`buffer.c:641`,
  `:4983`). `Grid::scrollback_max` counts the lines *beyond* the page, so the
  conversion is `max(lines, rows) - rows`. The related trap the grid itself
  had: the row ceiling is `ts.ScrollBuffMax` (`MaxBuffSize`, 10000), a
  different setting, and using the history's depth for it makes
  `EnableScrollBuff=off` a terminal one row tall.
- **Applying settings overwrites what the host set, and that is upstream.**
  `vtterm.c` reads `ts` at the point of use, so DECBKM assigns `ts.BSKey`
  (`:2992`), SRM assigns `ts.LocalEcho` (`:2053`) and LNM assigns `ts.CRSend`
  (`:2059`) — the setting and the mode are one variable. `Vt::set_config`
  refreshes exactly those and deliberately leaves `LFMode` and
  `AcceptWheelToCursor`, which upstream *does* keep separately and
  `CVTWindow::SetupTerm` does not touch.
- **`TCPPort`'s default is another setting's *initialiser*, and this is the
  initialiser trap the other way up.** `ttset.c:966` reads
  `GetPrivateProfileInt(…, "TCPPort", ts->TelPort, …)`, which looks like "the
  file's `TelPort`" and is not: `TelPort=` is read at `:1311`, four hundred
  lines later, so the value in hand is the hardcoded `ts->TelPort = 23` from
  `:566` — sitting with the very flag-word initialisers that lie about
  everything else. So a file with `TelPort=2323` and no `TCPPort=` opens port
  **23**. When a default is another field, check the read *order*: for the flag
  words the initialiser at the top is a lie, and here it is the answer.
- **`Session::set_setting` takes the schema's *dotted* name and answers
  `false` for anything else**, which is a silent no-op wherever the `false` is
  dropped. `setecho` wrote `LocalEcho` — the INI key, which is the obvious
  thing to reach for and is what the whole rest of the port calls it — so the
  command parsed, reported nothing and changed nothing for four commits. The
  key belongs to the *file*; `terminal.local_echo` is what everything above
  the file says.

And for the command line, which is two parsers and one of them is a plugin:

- **A bare host name cancels `/C=`.** Its arm assigns `ParamPort = IdTCPIP`
  outright (`ttset.c:3954`), so `ttermpro /C=1 myhost` is a **TCP session with
  no COM port** and `ttermpro myhost /C=1` is a serial one whose host name still
  has the colon in it. Word order decides, nothing warns, and a launcher script
  written the wrong way round opens the wrong kind of session.
- **`/AUTOWINCLOSE=1` means off.** That arm is an `_wcsicmp` against `on` with
  an `else` (`ttset.c:3716`), *not* `GetOnOff` — so the same `1` that means on
  for `AutoWinClose=` in the file means off on the command line. Sixth member of
  the family that already holds `CRReceive`, `BSKey`, the flag words and
  `GetOnOff` itself.
- **`/C=` is bounded against a setting, and out of range is dropped rather than
  clamped.** `ts.MaxComPort` defaults to 256, so `/C=300` selects the serial
  transport with *no port* and puts the New Connection dialog up. The same line
  works on a machine whose `MaxComPort=1024`.
- **`_ParseParam` discards its first token**, so `connect 'myhost'` connects to
  nothing unless something is in front of it. `ttdde.c:617` prepends a literal
  `"a "` — "`a` = dummy exe name" — and passes **NULL** for the DDE topic, which
  is why a `/D=` inside a `connect` string neither sets a topic nor cancels the
  startup macro. Both facts are `CommandLine::parse_argument`.
- **A `/D=` topic frees `ts.MacroFNW` unconditionally** (`ttset.c:3963`), and
  `StartupMacro` is an INI setting — so a terminal launched by a macro does not
  run the startup macro. Reading `/D=` as "just a DDE name" gives a window that
  launches a second macro on every `connect`.
- **`/ssh` and friends are not in `ttset.c` at all.** TTSSH hooks the parser,
  runs first, and **blanks the options it consumed out of the line**
  (`ttxssh.c:1521`) — so the two halves compose through a string, and
  `ssh://user@host/` is rewritten *into* a bare `host:22` token, which is the
  only reason Tera Term's own parser can find a host in an SSH URL. A port that
  reads only `ttset.c` has a command line that cannot open an SSH session.
- **In TTSSH `-` leads a switch and `ssh` is case-sensitive.** `-ssh` works
  where `-nolog` reaches nobody, and `/SSH` matches nothing, is left in the
  line, and is then ignored by Tera Term too — it does nothing, in silence.
  `/t=2` is consumed as TTSSH's own and `/t=0` is deliberately left behind.
- **A forwarding letter is given once for the whole list.** `option2[0]` is
  written before the loop and the index resets to 1 rather than 0
  (`ttxssh.c:1556`), so `/ssh-L1:h:2,3:h:4` is two `L` specs and
  `/ssh-L1:h:2,L3:h:4` makes the second one `LL3:h:4`. The `;` separator the
  documentation offers only works **quoted**, because an unquoted `;` is where
  the tokeniser stops reading the line.
- **Nothing sets the SSH port, so upstream sends SSH to port 23.** TTSSH never
  assigns `ts.TCPPort` — only its half of the New Connection dialog does
  (`ttxssh.c:1347`) — so `ttermpro /ssh myhost` connects to whatever `TCPPort=`
  holds, which on a fresh install is 23. `Target::of` diverges and uses 22 when
  no port was asked for; the test for that is upstream's own `TCPPort ==
  TelPort`, the same comparison `vtwin.cpp:3666` uses to decide whether a port
  was chosen for a protocol. A user whose file already says 22 sees no change.
- **Two of `OnCommStart`'s three arms open nothing** (`vtwin.cpp:3708`), and
  which test goes with which transport is not the obvious pairing: a **host
  name** decides for anything that is not serial, and `ComAutoConnect` decides
  for serial. So `myhost /M=x` connects and `/C=1 /M=x` also connects — an
  in-range `/C=` re-enables auto-connect *after* the option loop, in either
  order — while `/M=x` alone opens the dialog, or nothing at all under `/DS`.
- **A macro's `connect` gets TTSSH's half too, and `ttdde.c` does not show
  it.** `(*ParseParam)(commandline, &ts, NULL)` (`:620`) is a call through a
  *function pointer*, and the `LoadTTSET()` two lines above it re-installs
  `_ParseParam` and then calls `TTXGetSetupHooks` (`ttsetup.c:47`) — which is
  where the plugin hooks it again. Read the DDE arm alone and `connect 'myhost
  /ssh'` cannot work, which is most of what the command is used for.
- **`cygconnect`'s argument is a third program's command line.** `ttl.cpp:73`
  spells the launcher `cyglaunch -o`, so the string is CygTerm's — ten options
  describing a shell to spawn (`cygterm.cpp:317`), not Tera Term's. And it is
  split by **two** rules, upstream as well as here: the line by cygwin's C
  runtime, where a backslash is ordinary, and `-s`'s shell string by `get_argv`
  in `cygterm.cpp` itself, where a backslash escapes. One splitter for both is
  tidier and turns the manual's own `-d C:\ -nocd -nols` into two options and a
  directory called `C: -nocd`.
- **CygTerm's default directory is the launcher's, not the user's home.**
  `home_chdir` is false with no `-cd`, the shipped `cygterm.cfg` has no key for
  it, and `exec_shell` calls `chdir` in neither case. `PtyParams::cwd`'s `None`
  means *home*, so passing the default straight through diverges in exactly the
  case nobody writes an option for.

And for the macro language:

- **Every TTL argument is a whole expression, so a space is not a separator.**
  `fileseek fh -3 2` is `fileseek (fh - 3) 2` and then runs out of parameters;
  `recvfile 'f' 0 -5` is `recvfile 'f' (0 - 5)` and does the same. The report
  is `Syntax error` on a line that looks obviously right, and it happens to
  every command that takes two integers in a row. Write the negative one in
  brackets. Upstream parses identically — this is the language, not the port.
  **The same rule makes `listbox`'s keywords quoted strings**: a bare
  `listboxsize=40x10` is the variable `listboxsize`, so the line reports
  `Variable not initialized` — a message about the *keyword* nobody would
  connect to a missing pair of quotes.
- **A macro path with no extension is not the file you wrote.**
  `FitTTLFileName` (`ttmmain.cpp:253`) fits `.TTL` onto the last component of
  a name that has no dot at all, and it does it to `FileName` itself rather
  than only to the name the macro is told — so a `mkstemp` template opens
  `…-XXXXXX.TTL`, which is not the file the test just created. Reproduced;
  give a test macro a `.ttl` on the end.
- **`SendCmnd` is where the link check lives**, so a command whose body never
  mentions `Linked` still fails with `ErrLinkFirst` — and *after* its arguments
  are parsed, so `sendbreak junk` is a syntax error where `send 'x'` with no
  terminal is a link error. Porting one of the thin `TTLCommCmd*` commands by
  reading only its own four lines gives a command that quietly works with no
  connection.
- **`DDE_FNOTPROCESSED` reads to a macro as success.** It is what the terminal
  answers when the port is not serial, so `setdtr`, `setrts`, `setbaud` and
  `setflowctrl` are silent no-ops over SSH and not errors. A host that refuses
  them loudly is not faithful.
- **`ttl.cpp` bounds-checks its handle arrays in about half the places it
  indexes them**, and the halves are not the obvious ones — `HandleGet` has a
  check that is off by one, `HandleFree` and `FPointer[fhi]` have none. Seven
  out-of-bounds accesses have been found in `ttpmacro` by reading; none is
  reproduced, all are listed in `PLAN.md`. Assume the next array is unchecked
  until you have looked.
- **A missing reserved word is not diagnosed as a missing command — it is
  diagnosed as a bad assignment.** An unrecognised name is read as a *variable*,
  so a command left out of the table falls into `ExecCmnd`'s `else` arm
  (`ttl.cpp:6480`), which reports `ErrNotSupported` when what follows is not an
  `=`. The message is "Unknown command.", which sounds like a dispatch that
  failed and is really a word that was never a command at all — and on a line
  that is perfectly good, which is what hid `filenamebox` for four commits.
  Same arm, same reason: `a[1 = 2` says "Unknown command" rather than
  "] expected". `rsv.rs`'s table is a transcription of
  `ttmparse.cpp:CheckReservedWord`, and the way to check a transcription is to
  extract both lists and diff them, not to read them. That goes for every
  upstream list this port copies.
- **The commands are not all named what their documentation page is called.**
  `logautoclosemode` is the reserved word; `logautoclose` is nothing at all,
  and a test written against it passes through the same variable-not-command
  path above. Take the spelling from `CheckReservedWord`, never from prose.
- **`waitregex` matches whole lines and the line still has its CR on it**, so
  a pattern ending in `$` never matches a CRLF line and the obvious first
  guess — that the regex dialect is wrong — is the wrong place to look. The
  match is attempted when the LF arrives and *before* it is added to the
  buffer, so the CR is the last byte of what the pattern sees. It also matches
  nothing at all on an empty line, whatever the pattern.
- **Oniguruma's `onig` crate needs `default-features = false` or the build
  needs `libclang`.** With the default `generate` feature on, `onig_sys` runs
  `bindgen`; with it off the pre-generated bindings are used and `cc` is the
  only requirement. The failure is a build-script panic naming `libclang`,
  which reads as a missing toolchain rather than as a feature flag.
- **A macro that shows a dialog must not be run on the UI thread**, which is
  the same rule `wait` already imposed and for a different reason: upstream's
  dialogs are modal on the macro's own thread, so the faithful shape is a host
  method that blocks, and a frontend answers it by spinning a nested event
  loop. See the `Session::m_sshWaiting` trap for what re-entering that loop
  costs if the notifier is left armed.
- **`getpassword` can report success and hand back nothing, and the bug is in
  the INI layer rather than in the cipher.** `Encrypt`'s output is printable
  ASCII including `'` and `"`, it goes into a `[Password]` section, and
  `GetPrivateProfileString` strips one matched pair of surrounding quotes — so
  about one record in four thousand comes back two characters short, fails
  `Decrypt`'s complement check, and yields `result` 1 with an empty password
  while `ispassword` still says the entry is there. Reproduced. Debugging it
  from the symptom leads straight into the cipher, which is fine.
- **Two things about the v2 password format are quirks, not choices, and both
  are invisible until a file written elsewhere refuses to open.** The HMAC key
  is derived from `EncSalt` **as stored** — its own ciphertext, because the
  field was overwritten before the derivation — and the three encrypted fields
  are one continuous keystream, the MAC starting at offset 219, because
  upstream pushes all three through the same cipher BIO. Deriving from the
  plaintext salt, or restarting the counter per field, produces a file that
  round-trips perfectly here and nowhere else.
- **A `/V` before the macro's name and a `/V` after it are different things**,
  and `ParseParam` is the only place that says so — the switch tests live inside
  `if (ParamCnt == 0)` (`ttmdlg.cpp:112`), so the first non-switch argument
  closes the door and everything after it is a parameter whatever it looks like.
  `macroparam.bat` is four command lines that differ only in where the filename
  sits, and it is the specification. There is no `--`.
- **`params[0]` is the whole command line and `params[1]` is not the path.**
  The array is indexed from zero and `ttl.cpp:243`'s loop skips only index 1, so
  `Params[0]` — set to `GetCommandLineW()` before anything was tokenised — is
  visible to the macro; and index 1 holds `ShortName`, the basename with `.TTL`
  appended if it has no dot at all, not the path the launcher was given. Reading
  `params[0]` as an unused hole and `param1` as the path is what this port did
  first, and only one golden line disagreed.
- **`GetParam` is not `CommandLineToArgvW` and guessing costs a path.** A
  backslash is ordinary, `""` inside a quoted run is one literal quote, and an
  unquoted **`;` ends the command line** — everything after it is a comment
  (`ttlib.c:888`). Quotes survive tokenising and come off afterwards in
  `DequoteParam`, which is what makes the doubled-quote rule work at all.
- **A macro does not read the wire, and building the obvious thing gets it
  wrong.** `wait`, `waitln`, `waitregex` and `recvln` match against
  `DDEPut1`'s buffer, which is fed from `OutputLogUTF32` (`vtterm.c:448`) —
  the *text session log's* tap. So a macro sees the characters the parser
  **printed**, re-encoded UTF-8, plus `CR`/`LF`/`BS`/`HT` where those controls
  executed and a `CR LF` where a line wrapped, and **no escape sequences at
  all**. Teeing the transport's bytes instead is what anyone would build and it
  is wrong on every host that emits a colour code. `Vt::set_macro_tap_enabled`
  is the seam.
- **`CheckEOLCheckLog` drops a lone CR** (`checkeol.cpp:105`), so `abc\rdef`
  reaches a macro as `abcdef` while the screen shows `def` — and a `CR LF`
  survives whole, which is the mechanism behind the `waitregex` `$` trap above.
  The parked space before a wrapped wide glyph is **not** in the stream:
  `vtterm.c:896` writes it with `BuffPutUnicode`, not `PutU32`.
- **The macro ring drops the OLDEST byte when it is full** (`ttdde.c:107`, 64
  KiB). Backwards from a queue and deliberate: a macro that has fallen behind
  wants the prompt that just arrived, and blocking the parser instead lets a
  stalled script freeze the window.
- **The macro thread must not hold a lock on the session, which is why
  `tt-macro` posts closures instead.** A `Mutex<Session>` works until a macro
  holds it through a modal dialog and the window stops repainting. Everything
  crossing the boundary is owned — see the `Session::m_sshWaiting` trap for
  what borrowing across a nested event loop costs.
- **`sendln` puts a bare CR on the wire by default**, because `ts.CRSend` is CR
  and the *text* send path expands the newline by it (`ttcmn.c:814`) while the
  binary path does not. So `send`'s mode is load-bearing: `Session::send_bytes`
  and `Session::send_text` are two paths and picking one for both breaks half
  the world. `looks_like_text` decides which, and its last-byte quirk is
  upstream counting a NUL terminator rather than an off-by-one.
- **The shell has already done half of `ParseParam` on Unix.** Joining `argv`
  back into a string and running the real tokeniser over it quote-processes
  everything twice, so `"param 7"` becomes two parameters. `CmdLine::from_args`
  takes the tokens as given and skips both `GetParam` and `DequoteParam`;
  `CmdLine::parse` is for a genuine command line, which is Stage 3 and the
  `.bat` transcriptions in `tests/scripts.rs`.
- **A `recvfile` that receives nothing waits for ever, whatever its auto-stop
  says.** `raw.c:168` arms the stop timer inside the *packet reader*, so the
  first byte starts the clock — the argument means "quiet for this long after
  something arrived", not "give up after this long". A capture that the host
  never answers is indistinguishable from a hung macro, and End is the only
  thing that ends it. `raw.c:184` also throws away whatever was already
  buffered when the transfer starts, so the prompt that triggered it is not in
  the file.
- **Three receives are told their own name and four hear it from the wire**,
  and getting the list wrong fails silently: `GetNextFname` answers NULL and
  the protocol opens a file called nothing. XMODEM carries no filename, `raw.c`
  writes into whatever it is handed, and a Kermit `GET`'s name is the
  **remote** one — `kermit.c:1160` takes its basename before it goes in the `R`
  packet, so `kmtget 'sub/x'` asks the peer for `x`. `Job::needs_name` is the
  list.
- **A transfer is the one blocking command a macro cannot notice a dead
  frontend from.** Everything else either polls the ring — which goes quiet
  when the terminal does — or is a job that comes back empty. A transfer's
  outcome is *posted* from the other thread, so a window that closed mid-ZMODEM
  would leave the macro thread parked on a condvar for ever. `PROBE` in
  `tt-macro/src/host.rs` is a quarter-second knock on the door, and it is there
  for that and nothing else.

And for the other language, where three of the four are about the fact that
Lua's escape hatches are not the interpreter's:

- **`pcall` catches an error raised from a debug hook**, so a script can
  swallow its own cancellation. The hook is how `while true do end` answers
  End at all — Lua has no per-line seam the way `ExecCmnd` does — and it stops
  the script by raising, which `pcall(function() while true do end end)` then
  absorbs. Nothing can fix that: Lua has no uncatchable error. `Script::run`
  asks the host again at the boundary so the *answer* is honest, and the
  distinction matters — a run reported as finished cleanly leaves the frontend
  thinking a script it stopped ran to the end.
- **The hook cannot see the host, and the way round it is not obvious.**
  `mlua` stores the callback in the `Lua`, so it must be `'static` and cannot
  capture the `&mut dyn ScriptHost` that every other callback here borrows
  through a `RefCell`. It calls a **scoped** function out of the registry
  instead, which can. Safe because Lua clears its own `allowhook` while a hook
  runs, so the nested call cannot re-enter it.
- **A success that returns two values breaks nesting, silently.** Lua expands
  a call's *last* argument to all of its results, so `tt.recvln()` answering
  `line, nil` makes `tt.send(tt.recvln())` a two-argument call whose second
  argument is `nil` — an error from `tt.send`, on a line that looks obviously
  right. Every function here returns one value on success and `nil` plus the
  detail on failure, which is `io.open`'s shape and the reason it composes.
  `tt.waitln` is the deliberate exception.
- **`set_name` without a leading `@` makes every error say
  `[string "login.lua"]:12:`.** That is Lua's rendering for a chunk compiled
  from a *string*, so an editor asked to jump to it does not find the file and
  the reader is told the wrong thing about where the code is. One character,
  and nothing warns.

And for the session log, whose settings are the same family of trap as all the
others plus one of their own:

- **There are two `strftime` expanders upstream and they are not the same
  one.** A log *file name* is checked against `IsValidStrftimeCode`'s Visual
  Studio 2005 table (`ttlib_static_cpp.cpp:1881`) and then handed to the C
  runtime; a log *timestamp* goes through `ttstrftime`
  (`ttlib_static.c:380`), which is Tera Term's own implementation of twelve
  conversions. They disagree **in both directions**: `%N` — upstream's own
  milliseconds, and the last thing in the shipped `LogTimestampFormat` —
  works in a timestamp and is silently *deleted* from a file name; `%e` is
  implemented in a timestamp and rejected from a name; and `%j`, `%p`, `%U`,
  `%W`, `%x`, `%X`, `%z`, `%Z`, `%A`, `%c` and `%I` all work in a name and
  come back as **literal text** in a timestamp, because `ttstrftime`'s
  `default` arm emits the `%` and does not consume the letter. Writing one
  expander for both is the obvious thing and is wrong twice over.
- **`LogRotateSize` is in bytes whatever `LogRotateSizeType` says.** The
  dialog multiplies by 1024 per unit *before* storing (`log_pp.cpp:471`), so
  the type is a display unit and nothing else. Scaling the stored value by it
  turns the 1 MB somebody asked for into a terabyte, and their log never
  rotates — which presents as rotation being broken rather than as a unit bug.
- **A `LogRotateStep` of zero is ten thousand generations, not none.**
  `filesys_log.cpp:507` leaves `loopmax` at a hardcoded 10000 when the step is
  unset. Reading the zero as "off" is the natural mistake and disables the
  feature for every file that does not mention it.
- **`LogRotate` is not a bool and must not be given a range.** 0 is none and 1
  is by-size (`tttypes.h:106`), and `filesys_log.cpp:513` treats anything else
  as "do not rotate" — so an `int(0..1)` in the schema would clamp a 2 to a 1
  and switch rotation *on* for a file that had it off.
- **`LogTimestampType`'s empty value is a value, because a second key answers
  for it.** `ttset.c:1007`: an absent or empty `LogTimestampType` consults
  `LogTimestampUTC`, Tera Term 4's key, while a present `Local` does not — and
  a Tera Term 5 that saves a Tera Term 4 file writes the new key and leaves
  the old one behind, so both are in real files. That is why the schema gives
  the empty spelling a variant of its own. The cost is the one divergence, in
  `settings.txt` and asserted in `tests/settings.rs`: a *misspelt* value falls
  to local time upstream and to the empty spelling here, since the schema has
  one fallback and it is the default.
- **`LogTypePlainText` is one byte, and it is not only the log's.**
  `vtterm.c:666` and `:671` gate the tapped BS on it — that is the whole of
  what the setting does — and the tap is shared with the macro language's
  received-line buffer, so a setting named after the log changes what every
  `wait` in every script matches against.
- **The file-*transfer* directory decides where a log goes.**
  `GetTermLogDir` (`ttlib_types.cpp:63`) falls back to `FileDir` when
  `LogDefaultPath` is empty *and* that directory exists, before reaching the
  per-user one. Nobody would guess the relationship, and it means a `/FD=` on
  the command line moves the log.
- **A relative `/L=` does not land in the working directory.** Only an
  absolute request escapes the log directory (`filesys_log.cpp:964`), so
  `ttermpro /L=out.log` writes into `LogDefaultPath` and the file is not next
  to the shortcut that asked for it.

And for a macro reached from outside the process:

- **A macro that ends without asking for anything never wakes its frontend.**
  The frontend only looks when the macro's descriptor fires, and the last
  thing a script does is usually a `sendln` whose job has already been
  serviced — so `dispstr 'done'` on the last line is noticed and a bare
  `pause 1` is not, and the window sits with a Stop button for a script that
  finished ten minutes ago. `tt_macro_start`'s thread knocks once on its way
  out. **And it sets its "done" flag before knocking**, because
  `JoinHandle::is_finished` is still false at that point: read that instead
  and the frontend services the knock, finds the macro still running, and goes
  back to waiting for a wakeup that has already happened.
- **The `QSocketNotifier` has to be disabled across `tt_macro_service`.** It
  is level-triggered and a `messagebox` spins a nested event loop, so the
  notifier fires again *inside* the open dialog — the same re-entrancy that
  made the SSH host-key prompt ask twice, except that here it would run a
  second dialog inside the first. The core drains its wakeup pipe before it
  runs a single job, so in practice nothing is pending; "in practice" is not
  a guard.
- **Qt cannot tell No from the close box**, and `yesnobox` distinguishes them:
  closing it ends the macro where No does not. `QMessageBox` gives Escape and
  the title bar's close to the reject-role button, so a closed `yesnobox`
  reads as No here and the script carries on. Stated in `Macro.cpp` rather
  than hidden — and the same limit applies to `listbox`, where Closed is -2.
- **`tt_macro_free` cannot detach the terminal.** It is not given a session,
  deliberately, so the tap `tt_macro_start` turned on outlives it and every
  character the terminal prints goes on being copied into a ring nobody
  reads. `tt_session_unlink_macro` is the other half, and the frontend calls
  both.

And for the control socket:

- **A socket file outlives the process that bound it, and `bind` cannot tell a
  leftover from a collision.** `bind(2)` on an existing path is `EADDRINUSE`
  whether or not anybody is behind it, so a window that crashed leaves a name
  the next window with the same `/D=` cannot take. The only way to tell the two
  apart is to *connect*: `ECONNREFUSED` means the file is rubbish and can be
  unlinked, and a success means somebody really is there. The same test is what
  prunes dead names out of the directory — and without the pruning, "there is
  exactly one window, so I know which you meant" starts refusing a session that
  has exactly one window.
- **A modal dialog raised from inside `tt_ctl_service` holds the client open**,
  and neither of the two places it happens looks like a dialog. A `connect` that
  names nothing reaches `showConnectDialog`, and one that fails to open reaches
  a `QMessageBox::critical` inside `openTarget` — so a request from another
  process can park the window on a box nobody is looking for, with the requester
  blocked behind it. The first is refused outright and the second is queued to
  the next turn of the event loop. **In a test this is not a failure, it is a
  hang**, which is what it looked like the first time.
- **`Vt::encode_text` translates a CR and not an LF**, so `sendln` appends
  `\r`. Appending `\n` instead puts a bare LF on the wire under *every*
  `CRSend` setting, including the default — and it reads as correct, because a
  newline is what a line ending is called everywhere else in the file. The macro
  language's own `sendln` appends the CR for the same reason.
- **A `spin(predicate, ms)` helper calls its predicate one extra time to produce
  its return value.** Harmless for "is it connected yet"; wrong for anything
  that consumes what it is testing for, and taking a pending connection off a
  listening socket is exactly that — the first call accepts and returns true,
  the second finds nothing and returns false, and the test reports that the
  connection never arrived. Latch the result.
- **`QLocalSocket` is in Qt6::Network, which the shell does not link.** A test
  that wants to be a client uses a `sockaddr_un` and `poll(2)` — which is also
  what the claim "this needs no library" is actually asserting.

And for the C ABI:

- **cbindgen parses files, not crates, so it cannot see `pub(crate)` or a
  private module.** `tt-vt`'s private `locator_flag` put `PIXEL`, `ONE_SHOT`
  and `FILTERED` into the public header, unprefixed, until they were excluded
  by name in `cbindgen.toml`. Anything new and `pub const` in a parsed file
  lands in the header; the committed-header diff is what catches it.
- **`Builder::with_crate` runs `cargo metadata` from inside a build script**,
  which can block on the package cache lock — and passing it *and*
  `with_src("src/lib.rs")` parses the crate twice, emitting every declaration
  twice. `tt-ffi/build.rs` lists source files and never calls `with_crate`.
- **The header is the only place an ABI break shows up.** `TtKey`, `TtParity`
  and `TtCell` come straight from the core crates rather than from a second
  list here, which is the right trade — one list of key names, no mapping
  table to get wrong — but it means reordering `tt_vt::Key` silently renumbers
  the ABI. CI regenerates the header and fails on a diff, so it becomes a
  review question instead of a runtime mystery.

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
  A whole false optimisation, from one version gap. Use `sterna-fedora`.
- **You can screenshot your own widgets, not the desktop.**
  `org.gnome.Shell.Screenshot` returns `AccessDenied` (locked down since
  GNOME 45), `QScreen::grabWindow(0)` is uniform-blank under xcb — host windows
  are Wayland-native and invisible to Xwayland — and returns NULL under
  wayland. **`QWidget::grab()` works on both** and is the one to use; it
  re-renders offscreen, which is exactly right for checking our own painting.
  Full-desktop capture needs the xdg-desktop-portal Screenshot API, which
  prompts the user every time.
- **Cargo does not give a cdylib a `DT_SONAME`.** So whatever links against
  `libsterna.so` records the path it was *handed*, and the shell built out of
  tree got a relative `DT_NEEDED` of `cargo/debug/libsterna.so` — it ran from
  the build directory and nowhere else, reporting a missing file that plainly
  exists. Fixed in `tt-ffi/build.rs` with `rustc-cdylib-link-arg`, which
  applies to the cdylib alone; the same flag through `RUSTFLAGS` would attach a
  soname to every test binary in the workspace.
- **A run of text drifts off the grid unless the font is told the cell size.**
  Batching a row into one `drawText` is the large win over a call per cell, but
  even a monospace face rarely advances by a whole number of device pixels, so
  80 cells accumulate the error. The symptom is not obviously a font problem —
  it is a cursor that stops lining up with the character under it.
  `Theme::recomputeMetrics` rounds the advance to a cell and hands the
  difference back as `QFont::AbsoluteSpacing`. Wide characters advance by their
  own metrics, so they are drawn alone in their two-cell box.
- **`QWidget::grab()` works under `-platform offscreen`, and so does focus** —
  but only after `show()` **and** `activateWindow()`. Without them `hasFocus()`
  is false, so a cursor test silently measures the hollow unfocused form and
  fails on a colour that was never going to be there.
- **Sample a cell's corner, not its middle, to read a background.** The middle
  of a cell with a glyph in it is ink. And a CJK glyph can overhang its own
  advance by a pixel, so "the next cell is untouched" has to be asserted as a
  fill *width* rather than by probing the neighbour.
- **`git add -A` from the repository root sweeps in-progress work from every
  other subtree.** It put six half-written `shell/` files into a commit whose
  message was about the key table. Stage the paths the commit is actually
  about, or check `git status` first — the two commits will otherwise have to
  be split apart afterwards, which only works while nothing is pushed.
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

Five in Tera Term — four in `buffer.c` and one in `vtterm.c` — all found by
diffing the two engines. Patches in `oracle/patches/`, reports drafted in
`docs/upstream-bugs.md`. Filing needs a GitHub account and is an open item in
`PLAN.md`.

**Twenty-three more are in `ttpmacro` and none of them is in that file**, because
they were found by reading the source rather than by two engines disagreeing,
and `docs/upstream-bugs.md` holds only what a differential run proved. They are
written up in `PLAN.md`'s TTL sections: `waitn`'s timeout arm leaving the
received-line buffer in the wrong mode, `getmodemstatus` never reporting
failure, `logopen` discarding the error from its own mandatory arguments,
`filenamebox`'s Open and Save flag sets being each other's, `inputbox` copying
an uninitialised stack buffer into `inputstr` when Escape dismisses it,
`getspecialfolder`'s always-1 result and the NULL it hands `strncpy_s`,
`gettime`'s timezone argument leaking into the environment, seven
out-of-bounds accesses (`strtrim`, `strsplit`, `GetFactor`, `HandleGet`,
`HandleFree`, `FPointer`, `logrotate`) and six in the password family — two
stack overflows in the v1 codec, two uninitialised reads and a wild `free()`
in the v2 one, and a v1 record that is silently unreadable when the INI layer
strips a matching pair of quotes off it — one in the regex matcher, which
indexes the target with a non-participating group's -1 and writes a NUL before
the buffer, and one in `ParseParam`, which passes `sizeof` where `GetParam`
counts `wchar_t` and so overflows a 512-element stack array by up to 1022 bytes
on an argument longer than 511 characters. **That last one is the only one an
attacker reaches without already running a macro** — it is the command line, so
a shortcut or a `.bat` file is enough. Demonstrate each against a real
`ttpmacro.exe` in Stage 3 before filing, and file that one first.

**And one in the terminal rather than in `ttpmacro`, which is the only one so
far where the code and the *documentation* disagree.** `logwrite.html` says the
string "can be written even while logging is paused", and `FLogWriteStr`
(`filesys_log.cpp:833`) cannot: it puts the characters in the ring the tap
fills and then drains it, and the drain loop discards everything it pulls while
paused (`:647`). So the note a script writes to explain a gap in the log falls
into the gap. **This is the first of the three places the port follows the
manual instead** — see `SessionLog::write_str` — because reproducing it would
mean implementing the sentence the manual does not say. It wants filing with
the rest.

**And a second of the same kind, in the command line: `/NOLOG` does not stop a
log that `/L=` named.** Its arm clears `ts.LogAutoStart` and the *ANSI* copy of
the filename, `ts.LogFN` (`ttset.c:3850`), but the wide `ts.LogFNW` is the one
that counts and `vtwin.cpp:3631` starts logging when
`ts.LogAutoStart || ts.LogFNW != NULL`. So `ttermpro /L=out.log /NOLOG` writes
`out.log` — the one thing the option exists to prevent — while
`teraterm.html` says only "start Tera Term without logging". The port lets
`/NOLOG` win, which is the second place it follows the manual; it is the
twenty-fifth defect on file and the second outside `ttpmacro`. Reachable from a
shortcut, and it *creates a file* the user asked not to have.

**And two in CygTerm, which is a Cygwin program this distribution does not
ship** — so they are here and in `PLAN.md` rather than in that file, and they
bring the count to twenty-seven. Both are in `env_add` (`cygterm_cfg.cpp:42`)
and both are reachable from a macro: `cygconnect '-v FOO'` — a variable with no
`=` — hands it a NULL value that goes straight into `strdup`, and replacing the
**first** variable drops every variable after it, because the same-name arm
assigns `pr_data->envp = e` without carrying `e->next` across. So
`-v A=1 -v B=2 -v A=3` loses `B`. Neither is reproduced;
`cmdline::cygterm::add_env` says so where it declines to.

**And a twenty-eighth, which is the third where the code and the documentation
disagree: `setflowctrl` changes the setting and not the port.** `ttdde.c:1002`
assigns `ts.Flow` and stops — no `CommResetSerial` under it, unlike `setbaud`
one case arm away — so upstream's flow control does not take effect until
something else resets the port, while `setflowctrl.html` says the command
changes flow control. **The third place the port follows the manual instead**,
and for the same reason as the other two: the harm is dropped bytes on a real
cable, and a `setdtr` whose "flow control is none" guard opens while the driver
still has `CRTSCTS`. `Session::set_flow_control` says so where it diverges.

**And one in `vte`**, which is a dependency rather than the specification, so it
is not in that file: `vte` 0.15.0's `advance_partial_utf8` (`lib.rs:687`) prints
only the first character of what it decoded across a chunk boundary and then
reports `valid_up_to()` as the bytes it consumed, dropping anything complete in
between. `[.. C3] [A9 'a' E4 B8 80]` prints `é一` and eats the `a`. Worked
around in `tt-vt` rather than waited on — see the trap below — but it wants
filing too, drafted in `docs/vte-bug.md`, and it needs the same GitHub account.

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
esctest/         the conformance suite, run inside our own terminal (see its README)
packaging/       the AppImage, which is the whole of Linux packaging (see its README)
xfer/            Stage 0 spike 2 — ttpfile's protocols, running and interoperating
serial-audit/    Stage 0 spike 4 — serialport-rs vs commlib.c, on real hardware
telnet-audit/    a real telnetd, so the telnet port has an independent check
ssh-audit/       Stage 0 spike 5 — russh vs legacy SSH algorithms and auth
ini-audit/       what GetPrivateProfile* really does, asked of Wine (see its README)
vendor/ttpfile/  Tera Term's file-transfer protocols, verbatim — the only
                 upstream code the distribution ships (see its README)
crates/          Rust core — tt-grid, tt-vt, tt-conn, tt-session, tt-config,
                 tt-xfer, tt-ttl, tt-lua, tt-macro, tt-ctl, tt-ffi (see its
                 README)
crates/tt-fuzz/  the properties, and what they found (see its README)
crates/fuzz/     the libFuzzer targets — nightly, weekly in CI
bench/           the perf gate: a floor in CI, a baseline locally (see README)
run_diff.sh      the differential gate: Rust engine vs Tera Term, every case
shell/           Qt 6 shell — one window on the C ABI (see its README)
winshim/         what Tera Term's C needs from Windows — shared by the three
                 things that compile it (see its README)
```

None of `xfer/`, `serial-audit/` or `ssh-audit/` is throwaway. They become the
regression suites for `tt-xfer` and `tt-conn`, and every claim in `PLAN.md`'s
spike sections is reproducible from them.

`ssh-audit/servers.sh` needs `sudo`: it runs sshd and dropbear on localhost
ports and creates a throwaway `sterna-test` account. **Run `./servers.sh stop`
when done** — that is what removes the account.

**`winshim/` is shared by three consumers** — `oracle/`, `xfer/` and
`crates/tt-xfer/`. It was `oracle/winshim/` until the third arrived; a shipped
crate must not reach into the test harness for its build. Adding to it is
usually right — the Win32 surface the protocols needed turned out to be a
subset of the VT engine's — but **re-run `oracle/run_tests.sh` after touching
it**, because the oracle is the thing that must not regress.

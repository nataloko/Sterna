# Working notes for Sterna

Read `PLAN.md` for the roadmap and current stage. This file is the working
agreements and the traps.

**This is the one instruction file, whichever agent you are.** `CLAUDE.md`
imports it and holds nothing of its own, so a rule belongs here or it is a rule
half the agents on this repository will never see. There may also be an
untracked `AGENTS.local.md` beside it — context about the user and the machine
that is deliberately not in the repository. Read it if it is there.

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
./run_diff.sh                    # THE gate: Rust engine vs Tera Term, 134 cases
./run_diff.sh 27                 # just the cases matching "27"
./run_upstream.sh                # the same diff over Tera Term's OWN exercisers

cd crates                        # the Rust core
cargo test && cargo clippy --all-targets -- -D warnings
cargo fmt --all                  # the whole workspace, generated.rs included
tt-ffi/run_abi.sh                # the C ABI, compiled and driven from C
tt-ffi/run_abi_windows.sh        # ...and its Win32 DLL/HANDLE/pipe seam
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
./build/print_test               # the printer, which is a file, so it needs none
QT_QPA_PLATFORM=offscreen \
  ./build/cmdline_test           # a Tera Term command line, argv to connected
                                 # — NOT under Wayland; see the traps
./build/control_test             # the control socket, against the window's loop
./build/sterna --port /dev/ttyUSB0 --baud 115200
./build/sterna myrouter        # an alias out of ~/.ssh/config
./build/sterna --shell         # a local login shell
./build/sterna /ssh /auth=publickey myrouter   # ...and Tera Term's own line,
                               # which a `/OPTION` anywhere switches to
mingw64-cmake -S . -B build-win -G Ninja      # ...and the same shell, for
cmake --build build-win                       # Windows — sterna-fedora only

./bench/bench.py --core          # the perf gate's half that runs anywhere
./bench/bench.py                 # ...and the Qt half, in sterna-fedora only
./bench/bench.py --update        # re-record baseline.json, on a QUIET machine
cmake --build shell/build-release --target bench_shell   # it is EXCLUDE_FROM_ALL

cd packaging/appimage            # the only Linux artifact — build it in
                                 # sterna-fedora, never here
./build.sh                       # → build/sterna-x86_64.AppImage
./build.sh --clean               # ...from scratch
./build.sh --run                 # ...and start it

cd packaging/windows             # the only Windows artifact — sterna-fedora
./build.sh                       # → build/sterna-0.0.0-x86_64-setup.exe
./build.sh --stage               # ...the file tree, without makensis

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

**The Windows cross build of the shell lives there too**, added 2026-08-11:
`mingw64-qt6-qtbase` (6.11.1, the same version as the native one — Fedora
ships the pair in step), `mingw64-gcc-c++`, and **`nasm`**, which is
`aws-lc-sys`'s assembler for the Windows target and whose absence stops the
*core* rather than the shell, several minutes into a build that looked like a
Qt one. `mingw64-cmake` is the wrapper that supplies the toolchain file; it
comes with `mingw64-filesystem` and needs no install. Fedora's `updates` repo
metalink failed here on the day, so all three went in with
`--setopt=updates.metalink= --setopt=updates.baseurl=https://dl.fedoraproject.org/pub/fedora/linux/updates/44/Everything/x86_64/`
— worth knowing before concluding the container has no network.

**And the Windows installer, added 2026-08-12**: `mingw32-nsis` and
`mingw64-nsis`, which between them are one native Linux `makensis` — from
`mingw-nsis-base`, which both depend on — plus the compiled stubs, x86 in the
first package and amd64 in the second. `packaging/windows/sterna.nsi` targets
amd64, so the second one is the one that matters and the first one carries the
compiler.

## Traps

These cost real debugging time. Each is a place where the failure looks like
something other than what it is.

- **`cargo test -p tt-ffi` runs zero ABI tests.** That crate deliberately has
  no Rust-side seam tests: Rust calling Rust cannot prove the generated header
  compiles or the shared library links from C. Run `tt-ffi/run_abi.sh` for the
  Unix ABI and `tt-ffi/run_abi_windows.sh` for the focused Win32 DLL, HANDLE
  and named-pipe smoke; native Windows uses `run_abi_windows.ps1` under MSVC.
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
- **`struct opts` in `oracle/src/main.c` is a positional initialiser, and
  adding a field in the middle silently reassigns every default after it.**
  Inserting two settings after `invalid_decrqss` moved `AutoInvoke`'s zero onto
  one of them, `LockTUID`'s **one** onto the other — switching a pass-through
  printer port on for every case — and `MaxOSCBufferSize`'s 4096 two places
  further along, leaving the OSC buffer at zero. What that looked like was an
  *unrelated* differential case changing its answer while the case being added
  passed. Add to the initialiser as well as to the struct, in the same place.
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
  **The dump's other blind spot was the history, and that one is now
  openable**: `--scrollback` on both engines prints the lines that have left
  the page, and a case whose `cmd` asks for it compares them. It found `ED 2`
  erasing a screen that upstream scrolls out — see the scrollback traps below.
  Ask what a case cannot see before trusting that it passed.
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
- **`ANSIColor` is parser state as well as paint.** Truecolor is reduced to a
  palette index while SGR is parsed, so parsing this key only in Qt paints a
  grid whose indices were chosen against a different table. Its first sixteen
  entries also have two orders: the file's legacy 1–7 are the bright colours,
  and `GetIndex256From16` moves them to drawing indices 9–15. The value itself
  wraps rather than validates: `colorid & 15`, channels narrowed to `BYTE`,
  duplicate IDs winning last, and incomplete groups ignored — after a
  259-byte whole-value limit and a fourteen-byte per-field limit. The live
  256-entry table belongs to `tt-vt::Config`; both nearest-colour search and
  `tt_session_palette_rgb` read it.

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
- **`cl.exe` cannot open a source file spelt `\\?\D:\...`, and it misnames the
  one it could not open.** `std::fs::canonicalize` answers with that verbatim
  prefix on Windows, so `tt-xfer/build.rs` handed every vendored source to MSVC
  that way and got `C1083: Cannot open source file: '\\raw.c'` — a path that
  exists nowhere, which reads as a missing checkout rather than as a prefix.
  MinGW accepts it, so the cross build was green throughout and only the native
  job saw it. `plain()` strips it, and only for the drive spelling: the prefix
  is load-bearing in `\\?\UNC\server\share`.

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
- **`Telnet=off` is not a raw socket, and `/T=0` is not either.**
  `TelAutoDetect` is a key of its own, it ships **on**, and `ttcmn.c:590` reads
  it without reference to `ts.Telnet` — so the framing still switches on at the
  first `0xFF` byte. TTSSH clears the flag by hand (`ttxssh.c:981`) under a
  comment saying the line "should not be needed"; it is, because an SSH stream
  is full of `0xFF`. Raw needs *both* keys off.
- **The framing and the opening burst are two questions, so there are four
  modes and not two.** `ts.Telnet` decides the first (`commlib.c:340`) and
  `ts.TCPPort == ts.TelPort` decides the second (`vtwin.cpp:3666`). The mode
  this port was missing is `Telnet=on` at a port that is not the telnet port —
  IAC framing with nothing offered, which is the *ordinary* state of a console
  server. It read as `Auto`, and a `CR NUL` arriving before any `IAC` reached
  the terminal as two characters. `TelnetMode::of` is the table; do not
  assemble the mode by hand.
- **`TelEcho` is not "echo locally", it is "let the `ECHO` option decide"** —
  and in both directions. Off, `WILL ECHO` changes nothing (`telnet.c:411`,
  `:497` both test it first). On, the negotiated state assigns `ts.LocalEcho`
  *and* the burst runs `TelChangeEcho` (`:845`), which asks the server to echo
  only when local echo is off and asks it to **stop** when it is on — so a
  `LocalEcho=on` file opens with the opposite request. Same shape as `ts.BSKey`
  and DECBKM: one variable with two names. It is a `TransportEvent` rather than
  a state to poll, because SRM assigns the same variable and a transport
  re-asserting its answer every read would undo a host's `ESC [ 12 h`.
- **The keepalive interval is a *quiet* period, and it only runs where the
  burst ran.** `telnet.c:913` compares against `cv.LastSendTime`, stamped by
  every telnet send including the NOP (`commlib.c:1062`), so a session being
  typed at sends none; and `TelStartKeepAliveThread` is called inside the
  `TCPPort == TelPort` arm, so a telnet-framed console port gets none at all.
  Second governor in this port to measure quiet rather than elapsed time, after
  the bell's. **And a pump cannot drive it** — an idle socket produces no
  descriptor wakeup, which is the whole case it exists for — so it needs
  `tt_session_tick` on a timer. That is the one wakeup in the window's idle
  path and `Session.h` says so.
- **`TELNET.LOG` holds one half of the conversation.** All eight `TelWriteLog`
  calls sit directly after a `CommRawOut` and nothing on the receive path logs,
  so the `>` leading each record has no inbound counterpart. Logging both
  directions is the obvious build and produces a file upstream never writes.
- **A cloned Windows socket stays alive, and its read timeout is shared state.**
  Telnet's Windows frontend wakeup owns a blocking reader clone, a bounded
  1 MiB queue and a manual-reset event; setting the Unix 50 ms read timeout
  there turns an idle connection into a 20 Hz worker and makes its timeout look
  like EOF. Dropping the original handle is not enough either — `Drop` must
  `shutdown` the underlying connection to wake the clone.
- **`TCPLocalEcho` and `TCPCRSend` do not sit beside the terminal's settings —
  they spend them and put them back.** `vtwin.cpp:3696` assigns `ts.LocalEcho`
  and `ts.CRSend` when a non-telnet TCP connection opens, `:3589` restores
  `ts.LocalEcho_ini`/`ts.CRSend_ini` at `FD_CLOSE`, and **off is not a value**:
  a key left unset borrows nothing, so a host's own SRM survives the
  disconnect. That is what `TCPLocalEchoUsed`/`TCPCRSendUsed` are for.
- **`TCPCRSend` moves the keyboard's line ending and not LNM.** `LFMode` is a
  separate variable seeded from `ts.CRSend` at reset and nowhere else
  (`vtterm.c:285`), so `SM 20` moves the pair and this does not — DECRQM goes
  on reporting mode 20 reset while Return sends CR LF. `Vt::set_cr_send` says
  so where it declines to touch it.
- **`ConfirmDisconnect` is TCP only.** Both tests are `cv.PortType==IdTCPIP`
  (`vtwin.cpp:1668`, `:4448`), so a serial session closes without a word
  however it is set — which is why `tt_session_link_kind` exists rather than a
  bool. A macro's `disconnect` *can* raise that dialog upstream
  (`ttdde.c:634` passes the argument through) and deliberately cannot here, for
  the reason the control socket already has: a modal dialog raised from inside
  a request holds the requester open.
- **`AutoWinClose` is TCP only, and Disconnect takes the same close branch as
  a dropped line.** `Disconnect` posts `FD_CLOSE` (`vtwin.cpp:4462`), which
  reaches the `IdComEndTimer` branch at `:3005`; with auto-close on a network
  window closes even when the user chose File > Disconnect. Serial and local
  pty windows stay. If the window cannot close — upstream tests
  `IsWindowEnabled` — an enabled `ClearScreenOnCloseConnection` runs instead,
  and its “clear” is `BuffClearScreen`: the page scrolls into history and the
  cursor homes rather than the rows being erased in place. A disconnect
  discovered while writing has to take this same branch too; duplicating only
  the read arm also forgets to restore TCP's borrowed echo/CR values and to end
  a file transfer.
- **Five keys are written in a case their own readers do not use** —
  `Historylist`, `Metakey`, `XmodemRcvCommand`, `YmodemRcvCommand`,
  `ZmodemRcvCommand`, against readers spelling them `HistoryList`, `MetaKey`
  and `X/Y/ZModemRcvCommand`. Harmless only because `GetPrivateProfile*`
  matches key names case-insensitively and `Ini` reproduces that, which
  `ini-audit/` measured rather than assumed. A hand-rolled lookup that compares
  bytes silently loses four settings already in the schema.

And for the local pty:

- **Wine's `Z:` drive can make Unix-only fixtures pass in a Windows binary.**
  `/bin/sh` and `/tmp` name the host's files there, even though neither exists
  on native Windows; a bare `sh` in the same suite then fails and makes the
  production path look inconsistent. Cross-platform shell tests use
  `cmd.exe` on Windows and `std::env::temp_dir()` for a real directory, and a
  serial test compares against the enumerator rather than guessing `/dev/`.
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
- **ConPTY's pipes are synchronous `CreatePipe` handles.** Reading or writing
  either one on the UI thread can wait forever, and the handle itself is not a
  readiness notification. Windows therefore owns a blocking worker per
  direction, a bounded 1 MiB read queue for backpressure, and a manual-reset
  event for the frontend. Do not replace that event with a timer or make the
  queue unbounded to simplify the workers.
- **`PSEUDOCONSOLE_INHERIT_CURSOR` begins with a terminal conversation.**
  ConPTY asks for `CSI 6 n` and waits for the reply before ordinary child
  output; a raw transport test must answer it, while a real `Session` does so
  through the VT engine. Wine 9 cannot exercise this path: its console host
  rejects the internal `--inheritcursor` switch and closes the output pipe
  empty. That is a Wine gap, not evidence about the reader.
- **ConPTY's output pipe belongs to the console host, not to the child, so the
  hangup trap has a second form here and it is the opposite way round.** On
  Unix the child owns the slave and dropping our end hangs the master up; on
  Windows the child can exit, its last output can arrive, and the reader goes
  on blocking in `ReadFile` for ever — no error, no EOF, a window waiting on a
  process that left. `ClosePseudoConsole`, which is what dropping the master
  does, is the only thing that ends it, and it is the right thing rather than
  simply declaring the connection dead: the console host flushes what it still
  holds and *then* closes the pipe, so the trailing bytes keep their place
  ahead of the disconnect. It is also on `tick`, because a child that exits
  without printing produces no wakeup to read on — and once the pipe closes the
  worker signals the frontend itself, which is the wakeup that was missing.
  Wine cannot see any of this; only the native job could, and it presented as
  both ConPTY tests spending their whole ten-second deadline.

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

- **A command line can select the serial port without looking as though it
  does, and then a test opens whatever is plugged into the machine.** `/BAUD=`
  and `/SPEED=` set `port_type` as well as the speed — as `/C=` and every
  `/CDATABIT=`-shaped option do — so word order decides, exactly as it does for
  a bare host name, and `connect '127.0.0.1:1 /T=0 /BAUD=115200'` is a *serial*
  connect to `COM1` with a host name it never uses. That is faithful; what is
  not is a unit test doing it. It cost a CI run of 74 minutes against a job
  that normally takes eleven. Put the options before the host name when the
  point is that they reached the settings.
- **A `connect` is serviced on the frontend's thread, so a transport that
  blocks while opening takes the window's event loop with it** — and in a test,
  the watchdog too: `tt-macro`'s harness has a ten-second limit and it could
  not fire, because the thread that checks it was the thread that was stuck. A
  hang here is therefore never "the macro is slow"; look at what the *loop* is
  inside. Whether the Windows serial open can block indefinitely is open and
  needs a real port — Wine faults instead, inside its own DLL, and its
  PTY-backed COM mapping is not evidence about Windows either way.
- **`tcsetattr` returns success if it could apply *any* of what you asked.**
  The FTDI accepts `CS5` and then transmits eight bits anyway. Read settings
  back before believing them; `tt-conn`'s `set_data_bits` does.
- **`serialport-rs` calls a busy port `ErrorKind::NoDevice`**, so the naive
  mapping says "unplugged" when the truth is "`minicom` is still running".
  Both that and the `BrokenPipe`-means-disconnect mapping are wrapped in
  `tt-conn/src/error.rs` — one place to fix.
- **Never call `tcdrain` or `FlushFileBuffers` from a thread that must stay
  responsive.** Flow control can hold the output queue forever.
  `tt-conn::SerialConn::flush` takes a timeout and polls `TIOCOUTQ` on Unix or
  `COMSTAT.cbOutQue` on Windows. The latter comes from `ClearCommError`, so a
  `CE_BREAK` found during the snapshot has to be retained for the receive path
  or checking whether output drained can silently eat an input event.
- **A Win32 COM handle does not become readable by waiting on the handle.**
  `serialport-rs` opens it synchronously, so the Windows wakeup duplicates the
  handle into a worker blocked in `WaitCommEvent`, publishes one notice, and
  waits for the read to acknowledge it — the same handshake as upstream's
  `CommThread`/`ReadEnd`. Cancel it with `SetCommMask(handle, 0)` before the
  original handle dies; a timer brings the idle polling back. Wine's
  PTY-backed COM mapping rejects ordinary port setup with
  `ERROR_NOT_SUPPORTED`, so `tests/serial_windows.rs` needs native Windows.
- **`ClearCommError` clears the error it reports.** `bytes_to_read()` calls it,
  so using that apparently harmless queue-length check between a
  `WaitCommEvent` notice and the read can eat a break which arrived meanwhile.
  Windows reads up to upstream's 64 KiB input-buffer size and synthesises one
  more notice only when that fills. And do not send its bytes through the Unix
  `PARMRK` decoder: an ordinary `0xFF` would be held as an incomplete escape.
- **The portable serial setters describe less than half of a Win32 DCB.** They
  have no MARK/SPACE parity, DSR flow, custom XON/XOFF bytes or independent pin
  modes, and applying them one by one can leave four new values behind when the
  fifth fails. Windows builds upstream's zeroed DCB, calls `SetCommState` once,
  and reads every controlled field back. DTR toggle is rejected before that
  call because Win32 has no such mode; do not turn the readback into a cached
  `SerialParams` comparison, which would merely prove what the caller asked.
- **`serialport-rs` throws away the Win32 COM open error code.** Missing ports,
  exclusive-use collisions and access denial all become `NoDevice` carrying
  only a localized message, and `Path::exists("COM3")` is false whatever the
  device's state. Windows opens with the same `CreateFileW` flags directly:
  `ERROR_ACCESS_DENIED`/`ERROR_SHARING_VIOLATION` is busy, while missing/path/
  invalid-name is disconnected. Do not route it back through
  `COMPort::open` and try to parse the translated text.
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
- **The two control lines default to a sentinel, and it is the `TCPPort` trap
  the right way up.** `FlowCtrlRTS` and `FlowCtrlDTR` are read with a default of
  `-1` (`ttset.c:2034`, `:2042`), which is not a `DCB` value — it means "derive
  from `ts.Flow`", Handshake for RTS under `hard` and for DTR under `dsrdtr`.
  Here the read *order* is the answer rather than the lie: `FlowCtrl` is read at
  `:943`, eleven hundred lines earlier, so the derivation sees the file. Taking
  `-1` as a value gives a port whose control lines are held low.
- **One out-of-range number in the file discards every serial setting in it.**
  `CommResetSerial` copies `ts->FlowCtrlRTS` into the `DCB` and never checks
  `SetCommState`'s return (`commlib.c:240`), so `FlowCtrlRTS=9` makes Windows
  refuse the whole structure and the port silently keeps the baud, parity and
  stop bits it already had. Not reproduced — the symptom points at everything
  except the cause.
- **Upstream's save *pins* a derived control line.** It resolves the sentinel at
  load and writes the concrete number back, so a file that derived RTS from
  `FlowCtrl=hard` comes back saying `FlowCtrlRTS=2` and changing the flow
  control no longer moves it. This port keeps the `-1`. Either file opens
  correctly in either program; only the second keeps the derivation alive.
- **`RTS_CONTROL_TOGGLE` is not a termios bit** — it is RS-485 keying, so on
  Linux it is `TIOCSRS485`, and whether it exists is the *driver's* answer.
  Measured: the FTDI Quad RS232-HS answers `ENOTTY` to even the get, so nothing
  on the rig can test an implementation. `PinControl::Toggle` therefore leaves
  the line where the kernel put it rather than pretending.
- **`ClearComBuffOnOpen` gates the purge on open only.** Control > Reset port
  purges whatever the setting says (`vtwin.cpp:4913` passes TRUE outright), so
  it is not the answer to "does resetting the port clear it". And it is only
  testable on real hardware: it acts on the driver's queue, which a memory
  transport does not have, and both answers look identical from the session.
- **`SendBreakTime` is the only break length there is**, and a parameter for it
  is a parameter every caller has to invent. Upstream's menu, accelerator and
  `sendbreak` all reach one value; this port had 300 ms in the window, 250 in
  the macro host under a comment claiming it was upstream's, and an `ms`
  argument on the ABI. Same defect as `RingBell`'s dead `type`, in our code.

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
- **A key the schema invents cannot fail loudly, and four of the first 77
  were.** `AltScreenBuffer`, `EnableUnderlineAttrColor`, `RemoteClearsBuffer`
  and `WindowChangeSequence` appear nowhere in upstream — the real spellings
  are `AlternateScreenBuffer`, `UnderlineAttrColor`,
  `ClearScrollBufferFromRemote` and `WindowCtrlSequence`. Reading a key
  upstream never writes gives the default from a file that *sets* the setting;
  writing it puts a line in the user's `TERATERM.INI` that their own Tera Term
  ignores. Both halves are silent, and the invented name also hides the
  `GetOnOff` call that would have shown `UnderlineAttrColor`'s default is on
  rather than off. `tt-config/tests/upstream.rs` diffs both lists, which is the
  same rule this file already states for `CheckReservedWord`: **the way to
  check a transcription is to extract both lists and diff them, not to read
  them.**
- **`int(lo..hi)` and `int_min(lo)` are not the same bound, and the difference
  shows up on exactly the values people type.** Everything else in `ttset.c`
  takes the *default* below its floor (`:615`); the three transfer timeout sets
  clamp to the floor (`:1822`), so `XmodemTimeouts=0,0,0,0,0` is five
  one-second timeouts rather than `10,3,10,20,60`. And `ZmodemTimeouts`' second
  field floors at **0**, not 1, because 0 there means "never time out" on a
  network link — floor it at 1 and a stalled ZMODEM over SSH gives up after a
  second.
- **The C field's *width* is part of the bound, and it runs first.** Many of
  these settings live in a `WORD` or a `BYTE`, so `GetPrivateProfileInt`'s
  `UINT` wraps in the assignment before any `if` can see it — which is why the
  schema spells the width as a `uint8`/`uint16` prefix on the ordinary integer
  spec rather than as another bound. Two places it decides the answer:
  `MaxComPort=-1` — what somebody writes meaning "no limit" — is 65535 and then
  **4096** (`ttset.c:1218`), while a bound that reads the -1 as a negative gives
  **4**; and `AlphaBlend=-1` is **255**, an opaque window, where the same
  mistake gives 0, a window nobody can see. `AlphaBlend`'s is the nastier of the
  two because upstream *does* write a clamp there — `max(0, …)`/`min(255, …)` at
  `ttset.c:1467` — and it is **dead code** behind the narrowing, so copying the
  two lines that look like the rule is how the wrong answer gets in. When a
  bound looks obvious, check the field's type in `tttypes.h` before believing
  the `if`.
- **Seventeen rows had this wrong, and reading them was never going to find
  it.** They were found by extracting every integer key's `ts` field from
  `ttset.c` and its type from `tttypes.h` and diffing that against the schema —
  the same rule this file already states for `CheckReservedWord` and for the
  invented INI keys. If you add an integer setting, do that rather than reading
  the read.
- **`ComPort`'s ceiling is another setting and its answer is a reset**, which
  is two departures at once. `ttset.c:1223` tests it against `ts.MaxComPort` —
  read three hundred lines *later*, at `:1218` — and an out-of-range port
  becomes **1**, the first port, rather than the nearest legal one. Neither
  half is a schema row here, so it is in `Settings::normalize`; clamping
  instead opens COM256 for a file that asked for COM300, which is a different
  device on somebody's desk.
- **`XmodemOpt`'s default is plain checksum**, the `else` branch of an
  `_stricmp` chain read with an empty default (`ttset.c:1039`) — eighth member
  of the family that holds `CRReceive`, `BSKey`, the flag words, `GetOnOff`,
  `/AUTOWINCLOSE=1` and `IdTitleReportEmpty`. Upstream's *writer* emits
  `checksum` (`:2594`), which its own reader has no arm for; the value
  round-trips only because anything unmatched takes the default. **And XMODEM's
  binary flag is not the one every other protocol uses**: `XmodemBin` ships on,
  `TransBin` ships off, and `filesys_proto.cpp:324` derives the *text* flag as
  `1 - XmodemBin`. Folding them into one setting ships XMODEM translating line
  endings or ZMODEM not translating them, in silence.
- **An absent key and a misspelt one can be two different settings, and the
  schema needs `*` to say so.** `AcceptTitleChangeRequest` is read with a
  default string of `overwrite` and then compared down an `_stricmp` chain
  whose `else` is **off** (`ttset.c:1568`), so `AcceptTitleChangeRequest=ovewrite`
  is a terminal that ignores every OSC title while an absent key is one that
  accepts them. Every other enumerated setting here has the two coinciding —
  which is why the schema had one fallback for four years of upstream and why
  this looked like a rule rather than a coincidence. `off/*=Off` is the else
  arm; a row without `*` still falls to its default.
- **`MaxBuffSize` caps the terminal's rows, not only the buffer's lines.**
  `buffer.c:511` and `:4977` apply `ts.ScrollBuffMax` to two different things
  one line apart, so `MaxBuffSize=30` is a thirty-row terminal in a window of
  any size. Rows are cut first and the total after; doing it the other way
  round gives negative history on a small ceiling. And it is `ttset.c:615`'s
  bound with no ceiling of its own — under 24 takes the *default* of 10000, so
  `MaxBuffSize=1` is not a one-line buffer.
- **`TerminalSpeed`'s second field defaults to its first, which no schema type
  can say.** `GetNthNum` answers 0 for a field that is not there
  (`ttlib_static_cpp.cpp:1182`) and `ttset.c:1946` assigns the input speed, so
  `TerminalSpeed=57600` is 57600 in *both* directions. Two `int` rows with any
  constant default make that line a terminal claiming two different speeds, so
  it is held as a string and parsed in `tt-session::open`.
- **`TermType`'s default is plain `xterm`** (`ttset.c:961`), and TTSSH has none
  of its own — `ssh.c:8593` puts `ts.TermType` into the `pty-req` — so one key
  decides what every curses program on the far end believes, over telnet *and*
  over SSH. This port had `xterm-256color` hardcoded in two crates; both now
  say which default is whose.
- **`ISO2022ShiftFunction`'s list starts from nothing, not from its default.**
  `ttset.c:1875` reads the key with a default string of `"on"` and then runs a
  loop from `ISO2022_SHIFT_NONE`, so the default applies only when the key is
  *absent*. `ISO2022ShiftFunction=-SS2` therefore disables **every** shift
  rather than all but one — which is precisely what somebody writing that line
  means by it, and the `-` prefix exists to make them think it works.
- **`EnableANSIColor` is a rendering gate, not a parse gate**, unlike the three
  colour flags read beside it. `SGR 30-37` still stores the colour in the cell
  and `vtdisp.c:2417` declines to draw with it, so the screen is the normal
  pair while the buffer disagrees — and `Grid`'s dump looks perfect. The only
  outward sign is that DECRQSS' SGR (`vtterm.c:4332`) and the termcap `Co`
  query (`:4451`) stop naming a colour.
- **`MaximizedBugTweak=on` is a numeric alias, not a boolean.** The reader maps
  that one spelling to 2 and sends every other value through `atoi` before
  assigning it to a `WORD` (`ttset.c:1527`). So `off` is 0, `65537` is 1 and
  `-1` is 65535; a bool row loses three distinct behaviours and writes a value
  upstream did not read.
- **`DebugModes` can turn `Debug` back off.** `all`/`on` admits all three
  modes, `none`/`off` admits none, and any other list starts empty and adds only
  `normal`, `hex` and `noout` (`ttset.c:1798`). A list with no recognised word
  therefore clears `Debug`, rather than leaving an enabled shortcut which can
  never find a mode. TTL's `setdebug` bypasses both the gate and the mask,
  matching the DDE path.
- **The settings generator can be blocked by the stale file it is meant to
  replace.** If handwritten code starts referring to a newly generated field
  before regeneration, `cargo run -p tt-config --bin gen-settings` compiles the
  old `generated.rs` first and fails. Generate before wiring consumers; if the
  order has already been crossed, run the previously built
  `crates/target/debug/gen-settings` once, then return to the normal command.

And for the scrollback and the wheel, where three settings are named after
something other than what they do:

- **`BuffClearScreen` is a scroll, not an erase, and the differential dump
  could not see the difference.** `buffer.c:4021` is
  `BuffScroll(NumOfLines, NumOfLines-1)`: `ED 2` moves the whole page into the
  history and comes back blank, which is why `clear` at a Tera Term keeps what
  was on the screen. Filling the rows in place gives a page that compares
  equal, so the port did that for months — the fix came with `--scrollback` on
  both engines, which is the section that can tell them apart. The scroll
  region has no say (the raw `BuffScroll` is handed the last row), and
  `DECSET 1049` scrolls out on the way **in and out** (`vtterm.c:3044`,
  `:3202`), so leaving `vim` leaves two pages in the history.
- **`ScrollWindowClearScreen` does not gate `ED 2`.** `case 2` calls
  `BuffClearScreen` whatever it says; the key decides only whether an `ED 0`
  with the cursor at the home position is *promoted* to one (`vtterm.c:1728`),
  which is what `ESC [ H ESC [ J` is and what a good many programs send in
  place of `ESC [ 2 J`. Reading the name as "does a clear screen scroll" gets
  the gate onto the wrong sequence.
- **`ClearOnResize` clears on a resize that changed no size.** The
  `BuffScroll` and the cursor-home sit *outside* the `if (size changed)` block
  (`buffer.c:5028`), so `CSI 8 ; 24 ; 80 t` clears an 80x24 terminal — and so
  does the `BuffChangeTerminalSize` upstream makes on its way to its first
  screen, which puts a blank page in the history before a byte arrives. It is
  also why DECCOLM skips its own clear when the flag is on (`vtterm.c:2925`):
  the resize has already done one.
- **`AutoScrollOnlyInBottomLine` ships off, so output drags a scrolled-back
  view back down.** `MoveCursor` and `MoveRight` call `DispScrollToCursor` on
  every step (`buffer.c:3794`, `:3805`) and `BuffScrollNLines` leaves
  `NewOrgY` alone (`:3866`). It is the *minimum* scroll rather than a jump —
  invisible while a host prints lines, since the cursor is on the last row
  then, and visible when a full-screen program draws at the top. This port had
  the `on` behaviour hardcoded before the key existed. **And the cursor
  following belongs to the feed, not to a settings change**: sharing one
  function between them made opening the settings dialog snap the reader back
  to live, which is `Session::reanchor_after_resize`'s whole reason to exist.
- **`MouseWheelScrollLine` applies only to a notch that arrived alone.**
  `vtwin.cpp:2536` computes `abs(zDelta)/WHEEL_DELTA` and multiplies under
  `line == 1`, so a flick fast enough to coalesce two notches into one message
  scrolls two lines rather than six. The guard is `> 0` rather than a clamp, so
  `MouseWheelScrollLine=0` is one line per notch and so is a negative value.
  It is also the step for something with no other name: over the title bar the
  wheel changes the window's opacity by this many units of 255
  (`vtwin.cpp:2500`).
- **`ScrollThreshold` is a repaint coalescer counted in lines**
  (`vtdisp.c:3132`), which is `TerminalView`'s 8 ms frame floor measuring the
  same thing in a different unit. Carried and acting on nothing, like
  `NotifySound`.

And for URLs, where one plausible master switch is really three independent
ones:

- **`EnableClickableUrl` does not enable URL recognition.** The write path
  always sets `AttrURL`; `EnableURLColor` and `URLUnderline` independently
  decide how it is painted, and `EnableClickableUrl` gates only the hand
  cursor and double-click launch. It ships off, while both paint switches ship
  on, so treating it as a master gate gives the wrong default screen.
- **`MouseCursor` looks like an enum and is not one.** `ttset.c:1460` keeps the
  file's raw spelling, while `SetMouseCursor` (`vtwin.cpp:159`) compares
  `ARROW`, `IBEAM`, `CROSS` and `HAND` case-insensitively and returns without
  changing anything for an unknown value. Normalising one to `IBEAM` changes a
  shared file and live behavior at once. The hand over a clickable URL is only
  temporary; moving away calls this same setting again, so hardcoding an
  I-beam there loses a configured arrow, cross or hand.
- **A URL beginning at buffer pointer zero loses its own marking when it
  grows.** `mark_url_line_w` stops its backward search at zero and then
  increments unconditionally (`buffer.c:2658`), so typing one character after
  `http://` in the first cell leaves only the `h` marked. `sftp://` and
  `tftp://` are the exceptions because the mistaken rescan from cell one finds
  their `ftp://` suffix. Differential case 130 pins this; do not clean it up
  by replacing the incremental detector with a regex.
- **A wrapped URL is copied with the clipboard setting, not either split-URL
  setting.** `invokeBrowserW` uses `BuffGetStringForCB`, so
  `EnableContinuedLineCopy=off` inserts its exact `CR CR LF` between the two
  marked rows. `JoinSplitURL` and `JoinSplitURLIgnoreEOLChar` are read and
  written but never consulted anywhere in current upstream. Their names are
  more convincing than their code.

And for the parser's own switches, where three of the eight are two settings
wearing one name:

- **Debug display saves one attribute and restores another.** `PutDebugChar`
  copies the current pen into `svCharAttr`, clears and edits `char_attr`, then
  mistakenly restores `char_attr` (`charset.cpp:757`). Attribute-2 colours
  survive because that copy is only changing the low attribute byte, but the
  low pen left behind is the last debug byte's normal or reverse state. The
  port reproduces that typo; restoring the obviously named saved value changes
  what the next ordinary character looks like.
- **A broken multi-byte sequence is one U+FFFD per *byte*, and `vte` says one
  per run.** Tera Term's decoder emits a replacement character for every byte
  it had already taken when the sequence breaks, so `E2 82 'b'` is two and
  `F0 9F 98 'b'` is three; `vte` follows the WHATWG rule and emits one for the
  whole maximal subpart. **Case 97 could not see it** — a bare C1 byte is one
  byte either way, which is the only broken sequence the suite had. The fix is
  in `rewrite_c1`, which already tracks where the sequence started because
  `Vt::held` needs it. The half that stays divergent is case 128: a sequence
  cut off by an *OSC terminator* is decoded here at stream level and upstream
  at string level, so upstream never sees the terminator break it and discards
  the tail in silence.
- **An OSC's string is everything after the FIRST semicolon.** `vte` splits on
  every one and hands over a slice per parameter, so `params[1]` is a window
  title of `a;b` truncated to `a`. `ParseString` (`vtterm.c:5297`) reads digits
  into `Param[1]` until one `;` sets `HasParamStr`, and every byte after that
  including the next semicolon is the string. `Vt::osc_string` is the join, and
  `ts.MaxOSCBufferSize` bounds it — one byte *short* of the setting, because
  the test is `StrLen + 1 < StrBuffSize`.
- **`=` is a private marker and not an intermediate, and `csi_plain` drops
  intermediates.** `vte` reports `?`, `>` and `=` in the same place as a real
  intermediate because it has nowhere else to put them, so a guard written as
  "anything with an intermediate is a sequence we have not ported" silently
  ate the tertiary DA — with the arm for it sitting there looking right. The
  primary DA answers whatever its parameter is; `CSI > Ps c` and `CSI = Ps c`
  both insist on zero (`vtterm.c:CSGT`, `CSEQ`).
- **DECSCUSR's space *is* a real intermediate, so it needs an explicit
  dispatch arm.** `CSI Ps SP q` cannot go through `csi_plain`, whose refusal of
  real intermediates is load-bearing, and for a while it was silently dropped
  while DECRQSS reported the configured style as if the control worked. With
  `CursorCtrlSequence=on` it changes `CursorShape` and `NonblinkingCursor`
  themselves (`vtterm.c:3966`), so a frontend must read the live terminal
  style rather than the two file settings. `KillFocusCursor` is separate
  again: on, `CaretKillFocus` draws a full-cell outline whatever that live
  shape is; off, an unfocused window has no cursor.
- **HTS is the one C1 that must not be folded into its 7-bit form.**
  `TABF_HTS7` and `TABF_HTS8` are separate bits (`vtterm.c:1512` and `:1160`),
  so a file can accept `ESC H` and refuse `0x88` — and `rewrite_c1`'s fold is
  exactly what makes the two indistinguishable. `0x88` therefore goes through
  raw and `Perform::execute` answers for it, which is the only channel `vte`
  has that an `ESC H` cannot arrive on; the refusal stays in `rewrite_c1`,
  where the byte is still eight-bit. Gating the folded `ESC H` on both bits
  instead reads as correct and lets each spelling through under the other's
  key.
- **`VTCompatTab` is two changes, and the second is not "leave the wrap
  alone".** Off — as shipped — a tab is like a printed character at the end of
  a line: `Tab` (`vtterm.c:713`) breaks the line before tabbing, and
  `CursorForwardTab` arms the pending wrap when it runs out of stops. On,
  `buffer.c:5211` stashes `Wrap` on the way in and *puts it back* after
  `MoveCursor` has cleared it, so a tab on a line that was already full comes
  out still full. CHT never sees the first half — it calls `CursorForwardTab`
  directly.
- **`BackWrap` lands on the right margin, not the last column**, and does not
  scroll: `MoveCursor(CursorRightM, CursorY-1)` under `CursorY > 0`
  (`vtterm.c:664`). With DECSLRM in force a BS at the left margin comes back
  *inside* the margins.
- **`TabStopModifySequence` is a flag list whose `on` never reaches the list.**
  `on`/`all` and `off`/`none` are tested against the whole value and assign the
  whole word; anything else starts from `TABF_NONE` and only adds, so a value
  with no recognised word in it — `HTS9`, or an empty string — is a terminal
  that refuses all four. Same shape as `ISO2022ShiftFunction` and one arm less
  surprising, since `on` is a value the list arm cannot see.
- **`LockTUID` defaults on, so DECSTUI does nothing as Tera Term ships**, and
  `TerminalUID` is validated in two places with the same rule — eight
  characters, all hex, upper-cased (`ttset.c:1691` for the file,
  `vtterm.c:4567` for the wire). Nine digits is not eight: the old value
  stands rather than being truncated to fit. The file keeps whatever was
  written and the terminal answers with the valid form, which is why the
  validation is at the boundary and not in the schema.
- **`AutoInvoke`'s invoke is outside the switch and outside the ISO-2022
  gate.** `ESCSBCSSelect` (`vtterm.c:1409`) performs the G0→GL locking shift
  after the `switch` that handled the final byte, so `ESC ( Z` — a designation
  of nothing — still invokes; and it is the one locking shift in the parser
  that `ts.ISO2022Flag` does not gate, so `ISO2022ShiftFunction=off` does not
  stop it.
- **`UseInvalidDECRQSSResponse` flips the digit and keeps the body**
  (`vtterm.c:4400`), so an "I did not understand" reply still carries the value
  it was about to send. Upstream's comment is "(for testing)": it exists to
  exercise the *host's* parser, and it is the only setting in the terminal
  whose purpose is to lie.

And for the painter, whose decisions the differential dump cannot see:

- **`VTFontSpace` is four signed margins, not extra letter spacing.** The left
  and top values move the glyph, left+right and top+bottom expand the cell, and
  the negative-value clamps in `ttset.c:1346` are commented out. With
  `DrawingResizedFont`, upstream stretches a fallback glyph to `FontWidth`, not
  the padded `CellWidth`; using the latter makes changing a margin distort the
  font as well as moving it.
- **Bold and underline each have a font switch and a colour switch.**
  `EnableBold`/`UnderlineAttrFont` select the face;
  `EnableBoldAttrColor`/`UnderlineAttrColor` select the pair, independently.
  All four ship on, which makes hardcoding the face look right until a file
  turns only one half off. The attribute stays in the cell whatever either
  switch says.
- **`UseTextColor` repairs only three exact same-colour pairs, after
  reversal.** Both explicit colour bits must be set, the indices must match,
  and the foreground must be 0, 7 or 15 (`vtdisp.c:2542`); red-on-red is left
  invisible. Under selection, SGR 7 or DECSCNM the repair uses the configured
  reverse pair even when `EnableReverseAttrColor=off`, because this arm runs
  after that gate. A broad "ensure contrast" implementation changes far more
  output than upstream does.
- **`UseNormalBGColor` substitutes only an attribute pair's background.**
  Bold, blink, underline and URL use the normal background; reverse puts that
  colour in the foreground, and a later explicit SGR background still wins.

And for the colour OSCs, where the whole family lives in the file the oracle
does not compile:

- **A host cannot read back a colour it just set, and that is upstream.**
  `DispSetColor` writes `vtdraw_t`'s live pair (`vtdisp.c:3376`) and
  `DispGetColor` reads `ts` (`:3561`), so `OSC 10;#ff0000` then `OSC 10;?`
  answers the *configured* foreground — the paint moves and the report does
  not. Only the palette round-trips, both halves of it being `vt->ANSIColor`,
  and Tek does, because its setter happens to write the same `ts` field the
  getter reads. Thirty-first defect on file, and the reason `esctest`'s
  `ChangeDynamicColor` cases cannot pass however the parser is written.
- **`vtdisp.c` is not in the oracle, so `stubs_manual.c` is the specification —
  and it was invented rather than transcribed.** One flat array indexed by the
  `CS_` number let a dynamic colour be read back; there was no eight-colour
  permutation; and `DispResetColor` ignored its argument and reset everything.
  Same trap `DispFindClosestColor` fell into in the same file. **When a manual
  stub reimplements upstream logic, diff it against the original**, and check
  `esctest/run_diff.sh` — with the three transcribed it went from 25
  disagreements to 5.
- **`XsParseColor` accepts `rgb:` case-insensitively and parses it
  case-sensitively.** The guard is `_strnicmp` and the parse is `sscanf`
  against a lower-case literal, so `RGB:0/0/0` passes the first and fails the
  second, in silence. It takes two forms and no others: `rgbi:` is a
  commented-out arm at `:4773` and no CIE or TekHVC spelling was ever written.
  `#RGB` is `<< 4`, so it is 0xF0 and not xterm's 0xFF, and a query shows it.
- **`OSC 10;a;b;c` walks its own number along the list** (`vtterm.c:5156`), so
  it sets the foreground, the background and then a cursor colour that has no
  arm at all. Reading the number as fixed gives a terminal that sets its
  foreground three times. `OSC 12`, `13`, `14` and `18` — and their resets —
  are the four xterm colours `XtColor2TTColor` has no case for, so they do
  nothing whatever.
- **`OSC 104;` is not `OSC 104`, and the difference is 255 colours.** An empty
  parameter string is still a parameter string, so the loop leaves `color_num`
  at 0 and resets palette entry 0 alone; only a wholly absent one resets the
  table. `OSC 105`'s "all" is three colours — bold and blink foregrounds and
  the reverse background — not the four `OSC 5` can set, so the underline
  foreground is a colour the matching reset cannot put back. And `OSC 110-119`
  reads its parameter string as a list of further *OSC numbers*, which xterm
  does not.
- **`CS_UNSPEC` is a sentinel and not a flag**, so `OSC 105;4294967295` is a
  bare `OSC 105`. Modelling "no number was given" as an `Option` loses that.
- **The termcap query answers out of the colour flags and `EnableANSIColor`
  silences it.** `Co`/`colors` is the only capability upstream has
  (`vtterm.c:4444`), it says 256/16/8, and with `EnableANSIColor` off it says
  nothing — the one place on the wire that setting is visible, since it gates
  painting rather than parsing and the grid looks identical either way.
- **Applying settings does not refresh the live colours upstream**, and this
  port diverges. `ResetSetup`'s `BGInitialize(FALSE)` is inside an `#if 0`
  (`vtwin.cpp:1348`) whose comment says it was removed to keep a startup-only
  theme alive; only Restore setup and Reset terminal still reach
  `DispResetColor(CS_ALL)`. There are no themes here, and copying it gives a
  settings dialog whose colour tab silently does nothing.

And for the window operations, where the reports and the actions are the same
switch and nothing else about them matches:

- **The reports have to be answered out of a snapshot, and there is no second
  option.** `CSI 14 t`'s reply is composed while `advance` is parsing, so there
  is nowhere to call into a toolkit and ask; the frontend pushes
  `WindowMetrics` on every move, resize and window-state change and the engine
  reads what it was last told. The *actions* are a queue, which is
  `take_bells`' split for `take_bells`' reason. Building either one the other
  way round is what looks obvious and does not work.
- **A frontend that pushes nothing gets a notional window, and the oracle's
  stubs answer with the same numbers on purpose.** No chrome, at the origin,
  8x16 cells, a 1920x1080 work area — so `esctest/run_diff.sh` compares the two
  engines on which flag gates which report, which sub-parameter means the frame
  and which axis is printed first, rather than on a desktop neither of them
  has. Changing one side's constants without the other turns an adjudicable
  suite into a disagreement about furniture.
- **`CSI 13 t` reports x then y and every size report is height then width.**
  It reads as a typo in `vtterm.c` and in xterm's own documentation and is
  neither. Worse, the sub-parameters go the other way from each other: on
  `CSI 13 t` the 2 is the *text area* and 0/1 the frame, and on `CSI 14 t` the
  2 is the frame and 0/1 the text area. Both are upstream's and xterm's.
- **An unknown sub-parameter answers nothing at all.** Cases 13 and 14 have a
  `default: return`, so `CSI 13;3 t` is silence rather than a fallback to the
  plain form — and a host waiting on it waits until its own timeout.
- **`CSI 10 t` is maximise, not full screen**, and its comment says so: a
  PuTTY-style full screen is what upstream meant to write and maximising is the
  shortcut it took. So cases 9 and 10 are one operation, except that 10 has a
  toggle and 9 does not — `CSI 9;2 t` falls off the end of its own switch.
- **`CSI 8 t` resizes the grid in the engine and the window has to be told.**
  Upstream's `ChangeTerminalSize` resizes too, and the differential dump is
  taken at `NumOfColumns`/`NumOfLines`, so the engine cannot simply ask; but a
  window that does not follow paints the new number of cells into the old
  widget until something else resizes it. `Vt::take_terminal_resized` is the
  flag, and it is deliberately not set by `Session::resize` — otherwise the
  frontend's own resize comes back as another request and the two chase each
  other.
- **`CSI 4 t`'s zero axis means "leave it alone" and `CSI 8 t`'s means "use the
  default".** `DispResizeWin` reads the current `GetWindowRect` for a missing
  pixel axis (`vtdisp.c:3652`); `CSI 8 t` replaces a cell axis of 0 *or 1* with
  24 or 80 (`vtterm.c:2545`), where xterm reads it as the maximum in that
  direction. One sequence apart, opposite rules, and the same-looking parameter.
- **`GetDesktopRect` is the work area of one monitor**, `MONITORINFO::rcWork`
  (`ttlib_static.c:135`) — not the virtual desktop and not the whole screen. Qt
  spells it `QScreen::availableGeometry()`. Reporting the full geometry
  over-reports `CSI 15 t` and `CSI 19 t` by whatever the panel takes.
- **Raise does not take focus, on purpose.** `WINDOW_RAISE` is
  `BringWindowToTop` plus a `FlashWindow` if that left the window behind
  another one; the `SetForegroundWindow` version is in the source behind a
  `#if` nobody turns on. `QApplication::alert` is the flash.
- **Wayland cannot honour `CSI 3 t` and must not pretend to.** There is no
  placement request in `xdg_shell`, so `QWidget::move()` is silently ignored —
  the same limit `/X=` and `VTPos` already have. The difference here is that
  `CSI 13 t` answers from the metrics the frontend pushed, so a move that was
  quietly dropped and then reported as done puts a lie on the wire rather than
  merely in a window position.

And for the clipboard, where the surprise is what happens to a line break:

- **OSC 52 has two permission bits and notification is neither of them.**
  `ClipboardAccessFromRemote=read` and `write` are independent, `on` sets
  both, and anything else sets neither (`ttset.c:1742`). Access ships off while
  `NotifyClipboardAccess` ships on, so a rejected attempt is visible. Turning
  notification off must make an allowed action quiet, not refuse it; turning
  access off must not hide the rejection while notification is on.
- **OSC 52 base64 is deliberately permissive.** `ttlib.c:b64decode` skips
  whitespace, stops at the first invalid byte — `=` included — and decodes a
  final group of two or three digits anyway. A malformed remote write can
  therefore replace the clipboard with a valid prefix or with an empty string;
  a strict decoder that rejects the sequence is observably different. `Pc`
  accepts only `cps01234567`, and only a payload equal to exactly `?` is a
  read; `?x` is a write of whatever its base64 prefix decodes to.
- **An OSC 52 read reply has a fourteen-byte-looking limit that is really
  thirteen.** `XsProcClipboard` starts `char hdr[20]` with five bytes of
  `ESC ] 52 ;`, then appends `Pc` **and its semicolon** through `strncat_s`, so
  at most thirteen selector bytes fit. A longer read is accepted and notified
  but never reaches the clipboard or sends a response. A response always ends
  in ST even when the request ended in BEL, and `IsTextW` permits an empty
  clipboard while refusing binary control characters. The terminal owns these
  rules; the Qt layer owns only the operating system clipboard.

- **A paste is a keyboard, so every line break goes on the wire as a single
  `CR`.** `NormalizeLineBreakCR` (`ttlib_static_cpp.cpp:535`, called at
  `clipboar.c:289`) maps `LF` and `CR LF` alike onto `CR` — the Return key's
  byte — *before* the brackets are added. Queueing the clipboard's own bytes is
  the obvious build and reads as correct, because a newline is what a line
  ending is called everywhere else; it puts a byte on the wire that no key
  produces, under every `CRSend` setting including the default. Same trap as
  `Vt::encode_text` in the control socket, one layer up.
- **`BracketedSupport` is a second gate on `DECSET 2004`.** `clipboar.c:265`
  tests the *setting* and then the mode, so a host that asked for bracketed
  paste gets an unbracketed one when the key is off. It ships on, so an engine
  that omits it looks right until somebody turns it off — and
  `BracketedControlOnly` narrows it further to a paste containing a control
  character, which means a pasted word goes bare and a pasted block does not.
- **`EnableContinuedLineCopy` is upstream's `logFlag`, and it changes what a
  macro's `wait` sees.** The argument threaded through `CarriageReturn` and
  `LineFeed` (`vtterm.c:675`, `:688`) is TRUE for a CR or LF off the wire and
  FALSE for the pair the terminal invents at a wrap; with the setting on, only
  the invented pair is kept out of the log and the macro tap. So the key named
  after *copying* decides whether a script matches a wrapped line as one line
  or as two — the same shape as `LogTypePlainText`, which is named after the
  log and does the same thing to the same tap.
- **The two mouse-paste keys ship the opposite way round from what a Linux user
  expects of either.** `DisablePasteMouseMButton` is **on** and
  `DisablePasteMouseRButton` is off (`ttset.c:1425`, `:1422`), so Tera Term
  pastes on the right button and not on the middle one. Both are the file's to
  change; neither default is a bug.
- **A paste happens on the button coming *up*** (`vtwin.cpp:2375`, `:2645`),
  and `AutoTextCopy`'s copy happens there too — with the extra condition that
  `SelectOnlyByLButton` **suppresses the copy** when the button that came up
  was the middle or the right one (`vtwin.cpp:819`). That second half is not in
  the setting's name and is the bug it was added for.
- **`PasteDelayPerLine` is the only setting in `ttset.c` clamped at both ends**
  (`:1633`), which is why `int_clamp(lo..hi)` exists beside `int(lo..hi)` and
  `int_min(lo)`. The three disagree on exactly the values a hand-edited file
  holds: below the floor, `int(0..5000)` would give the default and this gives
  0; above the ceiling, `int_min(0)` would leave `60000` alone and this gives
  5000.

And for the bell, where the surprise is that a beep is a state machine:

- **The bell that trips the over-used limit still sounds, and the suppression
  measures quiet rather than elapsed time.** `RingBell` (`vtterm.c:5791`) sets
  the suppression clock in the arm that decides the *next* bell is too many and
  then falls through to the noise, so `BeepOverUsedCount=5` is heard **six**
  times; and the arm that finds itself already suppressed assigns `now` to the
  clock it just tested (`:5796`), so a host beeping steadily is silenced until
  it stops and for `BeepSuppressTime` afterwards rather than for that long in
  total. `teraterm-term.html` says five and describes a fixed delay, and it is
  wrong about its own code both times. This is the fourth place the code and
  the manual disagree and the first where the port follows the **code** — the
  other three had a concrete harm on the other side and this has none.
- **`BEL` is gated by the setting and `ESC g` is not.** `vtterm.c:1077` tests
  `ts.Beep != IdBeepOff` before calling `RingBell` and `:1561` calls it
  outright, so with the bell switched off a stream of `ESC g` still spends the
  terminal's allowance — invisibly, since nothing is heard either way.
- **The bell governor needs a clock, so it is not in the engine.** `Vt` is a
  function of its bytes, which is the whole basis of the differential suite and
  the fuzzers; `Vt::take_bells` therefore hands `tt-session` a **count** rather
  than an event, and one step of the state machine per BEL is the point of it.
  Collapsing a burst into one bell in the engine leaves the terminal audible
  through the next one.
- **`BeepOnConnect` never fires on a serial port**, whatever its name suggests:
  both places it is read test `PortType==IdTCPIP` first (`vtwin.cpp:3018`,
  `:3658`). It also bypasses `RingBell`, so it is always audible, never the
  visual bell, and neither thinned by the governor nor counted against it.
- **The visual bell is DECSCNM's own flag, toggled twice.** `VisualBell`
  (`vtterm.c:5784`) XORs `CF_REVERSEVIDEO` either side of a `Sleep`, so a flash
  on a screen the host has already reversed shows it the *normal* way round,
  and painting it as "reverse the screen" instead is wrong in exactly that
  case. Upstream's `Sleep` is on the parsing thread, so its flash also stops
  the terminal; ours is a timer and does not.
- **`Answerback` and `DelimList` are stored as hex, and `Hex2Str` is
  default-biased like everything else here.** `ttlib.c:406` reads `$` as the
  lead of two hex digits; `ConvHexChar` answers **0** for anything that is not
  one, and a `$` with fewer than two characters behind it borrows `'0'` for
  each one it is missing — so `$ZZ` is a NUL, a trailing `$` is a NUL, and `$A`
  is `0xA0`. Reading the value literally puts three characters on the wire
  where one byte belongs, and a word-delimiter list that begins `$20` then has
  no space in it. **There are two decoders and the difference is not
  cosmetic**: the answerback goes on the wire and is bytes, the delimiter list
  is compared against the screen and is *characters* (`Hex2StrW`), so `$E9` is
  one byte in the first and U+00E9 in the second. `hex_decode` and
  `hex_decode_str`. And a setting stored this way must not be read through
  `tt_session_setting`, which gives the file's own spelling — that is why
  `tt_session_word_delimiters` exists.
- **`DelimDBCS` is not a DBCS decoder switch.** `CheckDelimiterChar`
  (`buffer.c:4479`) compares `b->cell == 1`, so with it on a double-clicked
  non-delimiter word stops between any one-cell and multi-cell glyphs, emoji
  included. It is consulted only in that arm: starting on a delimiter still
  takes consecutive copies of the same character, whatever their widths.
  Turning it off joins the width runs but does not make half a wide character
  selectable; padding still resolves to its lead. It ships on, through
  `GetOnOff(..., TRUE)` (`ttset.c:1176`).
- **`IniAutoBackup` does not cover every write to the INI.** It is consulted
  only when Setup > Save setup overwrites the active file
  (`vtwin.cpp:4738`): a first save has no old file, and close-time `SaveVTPos`
  writes no backup. `CreateBakupFile` prefixes the original name with local
  `YYYYMMDDTHHMMSS+zzzz_` and calls `CopyFileW(..., TRUE)`, then ignores the
  result — so the first copy in a second wins, and a failed backup does not
  stop the save. Putting the switch inside the generic INI writer would back
  up operations upstream does not and risks recursively backing up a backup.
- **`AlphaBlendActive` defaults to the loaded `AlphaBlend`, not to 255.** The
  inactive value is read and clamped first, then passed as
  `GetPrivateProfileInt`'s fallback at `ttset.c:1471`. An absent or empty
  active key therefore follows it; a non-numeric value is still zero, which is
  the Win32 integer parser's separate rule. This is what the schema's
  `default-from=` option exists for.
- **`windowOpacity()` succeeding does not mean Wayland changed a pixel.** Qt
  6.11's X11 library has `QXcbWindow::setOpacity`; its Wayland client has no
  backend override and sends no alpha-modifier request. The property still
  round-trips, the activation test passes and no warning is printed, while the
  native Wayland window remains opaque. Use xcb to inspect visible opacity;
  do not treat a property assertion as a compositor test.
- **`BPAuto=on` silently discards `Answerback=`.** `ttset.c:1132` overwrites
  `ts.Answerback` with B Plus's five-byte activation string, four hundred lines
  after reading the key. It is the only setting in the file that another
  setting takes over.

And for the printer, where the mode named after taking the stream away does not
take it away:

- **Printer controller mode is not a diverter.** `CSI 5 i` stops the terminal
  *executing* controls — they go to the printer uninterpreted, so a line feed
  does not feed a line and an `ESC [ 2 J` clears nothing — but printable
  characters go on reaching the screen, and their copy to the printer rides
  `OutputLogUTF32` (`vtterm.c:487`), the same tap the session log and the macro
  language read. Building it as "send the bytes to the printer instead" gives a
  terminal that goes blank for the length of a print job. The two halves also
  have to be **interleaved**: text is handed to the parser and controls to the
  printer, so a run of text has to go through before the next control byte is
  written or `A LF B` prints as `LF A B`, which only shows up on paper.
- **`CSI 5 i` turns the mode on from inside the parser, and `advance` cannot be
  asked to stop there.** So `Vt::feed` cuts the chunk after every `i` while
  `PrinterCtrlSequence` is on — the only final byte that can change the answer.
  Handing `vte` the whole chunk instead loses everything after the sequence on
  the ordinary case, which is a host that sends `CSI 5 i` and its data in one
  segment. Nothing is needed the other way: while the mode is on the only bytes
  `vte` is given are printable ones, so it cannot dispatch at all.
- **Four of the five media-copy sequences are gated and the fifth is not.**
  `TF_PRINTERCTRL` (`PrinterCtrlSequence`, **off** as Tera Term ships) gates
  `CSI 0 i`, `CSI 5 i`, `CSI ? 1 i` and `CSI ? 5 i`; `CSI ? 4 i` — auto print
  off — is deliberately reachable whatever the setting says, so a host can
  always stop a terminal printing every line.
- **`ts.PrnDev` is not a gate on printing, it is a gate on parsing.**
  `DirectPrn` is sampled from `ts.PrnDev[0] != 0` when the controller starts
  (`vtterm.c:2095`), and what it decides is whether the locking shifts and the
  ISO-2022 designations arriving during the job are the terminal's to interpret
  or bytes the printer should receive. With it off `ESC ( 0` still designates;
  with it on the same four bytes are printed. Differential cases 133 and 134
  are the same input under the two answers.
- **The spool holds code points, not bytes.** `WriteToPrnFile` takes a `BYTE`
  and stores it into a `char32_t` array (`teraprn.cpp:527`), and
  `PrnFileDirectProc` converts each one back with `UTF32ToMBCP(u32, CP_ACP)` on
  the way out — so a raw `0x1b` reaches the printer as U+001B and the encoding
  is decided at the device, not at the parser.
- **A control arriving inside a half-read sequence pushes that sequence out
  ahead of itself.** `WriteToPrnFile(b, TRUE)` flushes the buffer and *then*
  appends, so `ESC [ 12 BEL m` prints as `ESC [ 12 BEL m`. And the exit
  sequence works only because the buffer is discarded rather than flushed:
  `PrnParseCS`'s `CSI 4 i` arm calls `WriteToPrnFile(PrintFile_, 0, FALSE)`,
  which is the *clear* form of the same function's four meanings.
- **`RingBell`'s dead argument has a sibling here: `ResetTerminal` clears
  `PrinterMode` and a host cannot reach it.** While the controller has the
  stream an `ESC c` is four bytes of printer data, so the flag clearing at
  `vtterm.c:327` only ever runs for Reset terminal on the menu. It also does
  not close the job or clear `AutoPrintMode`, so a RIS mid-job leaves an open
  spool nothing will print.
- **Whether a wrapped line breaks in the printer's copy depends on whether a
  log or a macro is running.** The wrap's `CarriageReturn`/`LineFeed` pair is
  behind `NeedsOutputBufs()` (`vtterm.c:512`), which is the log **or** the
  macro and pointedly not the printer — while the character itself reaches
  `OutputLogUTF32` directly and is always copied. Reproduced; it is the only
  thing that decides whether a printed wrapped line is one line or two.
- **Auto print's byte argument is the whole of what it selects.** `LineFeed`
  dumps the line for LF, VT and FF and not for IND or NEL, which pass a zero
  (`vtterm.c:1153`, `:1505`) — so `ESC D` scrolls a line the printer never
  sees, and the wrap's `LineFeed(LF, FALSE)` prints one.
- **The dump is of the *grid*, not of the stream**, so `hello\rH` prints as
  `Hello`. And upstream's version of it is the thirty-second defect on the list
  below; this port prints what it meant to.
- **A `String` local that merely *might* be filled is not free in the caller
  that never fills it**, and `Perform::print` is where that bill arrives. Auto
  print has to snapshot the line before the character lands, and holding that
  snapshot in an `Option<String>` put a destructor in every character's stack
  frame — 4% of `core.plain`, in a session with no printer. It is a field on
  `State` now, assigned only under the flag, and the printer's copy of a
  character is behind an explicit test at the call site rather than inside the
  function. Anything else added to that function wants measuring the same way:
  `./bench/bench.py --core` against the commit before it, not against
  `baseline.json`, which has its own drift.

And for the title, which is two strings in three places:

- **OSC 1 sets the window title.** `vtterm.c:5109` is `case 0: case 1: case 2:`
  falling into one arm, so "change icon name" writes `cv.TitleRemoteW` and
  repaints the caption like the other two. Reading the documentation, or the
  `case` labels, gives an engine that ignores it — this one did, with a comment
  asserting the opposite, until case 107 was written.
- **`gettitle` cannot see the title the host set, and `settitle` does not set
  it.** `CmdGetTitle` answers with `ts.Title` (`ttdde.c:646`) and `CmdSetTitle`
  writes it (`:636`), while an OSC writes `cv.TitleRemoteW`; the window shows
  the two combined. Implementing `settitle` as an `\e]2;…\a` through the parser
  is the obvious build and puts the string in the other half — invisible under
  `overwrite`, where they render the same, and wrong under `ahead` and `last`.
- **The window title and the title *report* are two chains that disagree about
  an empty host title.** `ttwinman.c:101` falls back to `ts.Title` whenever the
  host's is empty, whatever the mode; `vtterm.c:2677` only does that under
  `overwrite`, so `ahead` answers `CSI 21 t` with a **leading space**. Sharing
  one function between them is the tidy thing and changes what goes on the
  wire.
- **`TitleFormat` is a wrapping word, not six booleans and not an enum.** The
  dialog exposes bits 0–5, but `ts.TitleFormat` is a `WORD`, so unknown bits
  through 15 survive, `-1` becomes 65535 and `65537` becomes 1. Its default 13
  is endpoint + VT + swapped order: `<endpoint> - <title> VT`. Connecting and
  disconnected captions do **not** take the swap arm; both remain
  `<title> - [state] VT`.
- **A displayed serial speed comes from the live port.** A `--baud` open need
  not match `serial.baud`, and `setbaud` changes it again. Upstream posts
  `WM_USER_CHANGETITLE` after the reset (`ttdde.c:988`), so the core emits the
  same caption edge and the shell re-reads the transport. Caching the opening
  parameters makes the title stale after the first macro speed change.

And for the menu, where three plausible names are three independent controls:

- **`PopupMenu` hides the menu bar; `EnablePopupMenu` gates its replacement.**
  Ctrl+left-click opens the full menu only when the ordinary bar is absent,
  and `HideTitle` makes it absent without touching `PopupMenu`
  (`vtwin.cpp:863`, `:3461`). The gesture runs before mouse reporting, so a
  host cannot capture the only route back to the menu by asking for
  Ctrl-modified clicks.
- **`EnableShowMenu` adds a recovery command and does not show anything by
  itself.** Upstream puts "Show menu bar" in the Win32 system menu
  (`vtwin.cpp:3509`). A Qt client cannot add an application action to the
  compositor's system menu, so this shell puts it in the Ctrl+left-click popup
  and clears only `PopupMenu`, the same assignment upstream makes. If
  `HideTitle` is still on, the bar correctly stays hidden.
- **The popup reuses the menu bar's `QAction`s.** Building a second tree would
  duplicate every enabled state and shortcut and let the two drift as soon as
  a command was added. A `QAction` may belong to both widgets; destroying the
  temporary `QMenu` only removes that association.

And for the keyboard, where two settings both say Meta but mean different
things:

- **`MetaKey` chooses whether an Alt key is Meta; `Meta8Bit` chooses what an
  enabled Meta does.** Both ship `off`, but those two `off`s are unrelated:
  `MetaKey=off` leaves Alt to the desktop, while `MetaKey=on` plus
  `Meta8Bit=off` sends an ESC prefix (`vtwin.cpp:2856`). `raw` ORs 0x80 into
  the byte and `text` ORs U+0080 into the character before text encoding, so
  they cannot share the UTF-8 send path. Left/right modes require remembering
  the native Alt press because a later Qt character event carries no side.
- **`StrictKeyMapping` removes defaults; it does not validate a map.** A key
  absent from `KEYBOARD.CNF` normally falls through to Tera Term's built-in VT
  sequence and strict mode suppresses that fallback (`keyboard.c:960` onward).
  Delete is an explicit exception: `DeleteKey=on` still sends 0x7f before the
  strict check. With no `KEYBOARD.CNF` reader yet, strict mode therefore makes
  the built-in special keys quiet, which is the faithful incomplete behavior.

And for the last batch of file-shaped settings:

- **The key Tera Term reads is `CygwinDirectory `, including the trailing
  space, and the key it writes is `CygwinDirectory`, without it.** The two
  literals are at `ttset.c:1476` and `:2250`. Trimming schema columns or using
  one spelling both ways makes a saved value invisible on the next load.
  Backtick-quoted schema keys preserve the space; `write-key=` records the
  writer's different spelling.
- **`FileSendFilter` reaches raw send and every protocol send picker;
  `FileReceiveFilter` is raw receive only.** `_GetXFname` deliberately passes
  no receive mask (`filesys_proto.cpp:727`), because protocol receive either
  carries a name or asks through its own flow. Applying the apparently paired
  setting symmetrically changes a dialog upstream does not filter.
- **`DrawingResizedFont` is glyph fitting, not cell measurement.** Upstream
  measures the selected glyph and stretches a mismatch into its assigned cell
  box (`vtdisp.c:2902`). Turning it off must not remove the separate spacing
  correction which keeps a batched monospace run aligned to the grid.

And for remembered window geometry, where one switch controls writes rather
than reads:

- **`SaveVTWinPos` does not gate loading `VTPos`.** The position is read first
  (`ttset.c:598`) and is applied even when the switch is off; off means both
  Save setup and close must leave the old `VTPos` line byte-for-byte alone,
  matched quotes included. The generated schema's `write-if=` expresses that
  gate. Moving the read behind the switch makes an existing file open in the
  wrong place.
- **`GetNthNum` and `GetNthNum2` disagree about an omitted comma field.** The
  first writes zero, so present `VTPos=12` is `(12,0)` and
  `PasteDialogSize=400` is `(400,0)`; the second takes its caller's fallback,
  which is why `XmodemTimeouts=5` keeps the other four defaults. An absent key
  still takes the whole key's fallback. The schema spellings are `int_zero`
  and `int`; sharing one helper silently changes whichever family did not
  supply it.
- **Close-time `SaveVTPos` is not Save setup.** It writes only `VTPos` and the
  live `TerminalSize`, and only when `SaveVTWinPos` is on (`ttset.c:3338`). A
  close path that calls the full writer pins every known default into a small
  shared file; a writer that takes `TerminalSize` from the settings snapshot
  saves the last loaded size rather than the grid the user resized.
- **Wayland's `(0,0)` is not a window position to remember.** Wayland has no
  client placement request, so both restoring `VTPos` and overwriting it from
  `QWidget::pos()` are skipped there; the live terminal size is still saved.
  On position-owning platforms upstream also rejects points beyond the virtual
  desktop and clamps the twenty-pixel fringe at its top/left edge
  (`vtdisp.c:1517`), which keeps a removed monitor from stranding the window.

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
- **`TT_MACRO_UNSET` means inherit `StartupMacro`, not run nothing.** The four
  launch states are inherit, cancel (`/D=`), prompt and file (`/M`), and
  collapsing the first two makes the setting inert. Upstream launches the
  macro first with `/S` and its DDE init starts the connection (`ttdde.c:657`);
  with an in-process link the shell starts the attempt and then the macro
  immediately, without waiting for the connection to finish. The raw setting's
  relative path works upstream because the process has changed to `HomeDirW`;
  here it resolves beside the active INI without a global `chdir`. And
  TTPMACRO tests only `FileName[0] == '*'` (`ttmmain.cpp:285`), so
  `StartupMacro=*anything` is a picker too.
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
- **Rust's whole-file lock is not TTL's Windows lock.** The standard library
  uses `LockFileEx`; upstream pairs `LockFile` and `UnlockFile` over the exact
  range `(0, 0, DWORD_MAX, DWORD_MAX)`. Wine accepts the standard lock and then
  refuses its unlock, leaving `filelock` reporting success and `fileunlock`
  failure on the same handle. `tt-ttl::files` uses the exact Win32 pair on
  Windows and keeps the portable advisory lock only on Unix.
- **Stable Rust's process builder cannot set `STARTUPINFO.wShowWindow`.** TTL's
  `exec` does, for all four show words and for the default `show`; dropping the
  argument is only the Unix compromise. The Windows launcher therefore calls
  `CreateProcessW` directly, which also keeps the original raw command line
  instead of splitting and reconstructing it.
- **The upstream TTL script suite contains a blocking GUI program.**
  `#35797.ttl` runs `notepad` with `wait=1`; it happened to fail immediately on
  Linux, but a correct Windows `exec` opens it and waits for a person forever.
  The transcript harness substitutes one guaranteed-missing program name on
  both targets. Do not remove that isolation just because the Linux gate stays
  green without it.
- **A path already passed through the transcript's `esc` is not the path
  anymore.** Every Windows `\` is doubled before `portable` sees it, while raw
  macro command lines still carry one. Normalise both spellings and the one
  separator after `<dir>`/`<home>`/`<exedir>`; matching only `Path::display()`
  leaves six Windows-only golden diffs that look like interpreter failures.
- **Do not bless the TTL script goldens on Windows.** Five scripts intentionally
  expose drive, separator or shell-folder answers, and Wine is not their
  authority. The Windows gate runs all 53 against the portable goldens and
  requires exactly those five names to differ; `TTL_BLESS` is refused before
  it can overwrite anything.
- **A BOM-less TTL file means the machine's ACP on Windows.** Wine commonly
  supplies CP1252 while the upstream corpus mixes CP932 and UTF-8, so letting
  the transcript harness decode those fixtures normally manufactures
  locale-shaped diffs no native implementation bug caused. Its private copies
  get a BOM on Windows; `source.rs` tests the real ACP conversion separately.
  Do not grow the five-name platform allowlist to absorb an encoding locale.
- **`ToU8W` is not `WideCharToMultiByte` for UTF-8.** `_WideCharToMultiByte`
  diverts CP65001 through Tera Term's `WideCharToMBCP`, whose invalid UTF-16
  answer is ASCII `?`, not U+FFFD. This affects a damaged UTF-16 macro file on
  every platform; using Rust's `from_utf16_lossy` agrees with the API name and
  disagrees with the code under it.
- **`expandenv` is a Win32 parser, not just `%NAME%` replacement.** Windows
  calls `ExpandEnvironmentStringsW`; an unknown name's closing percent is
  **not** consumed — it is emitted as the opener of the next name, so
  `%UNSET%KNOWN%` is `%UNSET` followed by `KNOWN`'s value rather than the
  whole string left alone. Unix mirrors that. Keep the Windows API call: a
  tidy shared parser puts the old Stage 2 guess back into the shipping
  platform.
  **This entry said the opposite until a native Windows run corrected it**,
  and every test agreed with the wrong rule, because the only input that can
  tell the two apart is two names in a row with the first one unset — which is
  now the last assertion in that test, with a note saying so. The lesson is
  the one this file already gives for transcriptions: a rule about a Win32
  parser is worth exactly the platform it was measured on, and Wine is not
  that platform.
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
- **A transfer's clock is not `Instant`'s, and on Windows they disagree by a
  system tick.** `tt_xfer.c`'s `now_sec` is `GetTickCount64` there — faithfully,
  since upstream's `FTSetTimeOut` is `SetTimer` — and its resolution is about
  15.6 ms on a counter QPC knows nothing about. So a one-second auto-stop can
  measure **993 ms** to an `Instant`, and a test asserting `elapsed >= 1s`
  against it is comparing two clocks rather than testing the transfer. Assert
  that it waited, with a tick of slack; the failure worth catching returns in
  milliseconds.
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
- **Windows will not say who a named-pipe client is until it has spoken**, so
  the peer check cannot sit where its Unix counterpart does.
  `ImpersonateNamedPipeClient` answers `ERROR_CANNOT_IMPERSONATE` — "unable to
  impersonate using a named pipe until data has been read from that pipe" — and
  the accept loop had it before the first read, so *every* connection was
  refused as an intruder and every client saw the window hang up on it. It runs
  on the first line now, before that line is parsed or answered; reading is not
  acting. `SO_PEERCRED` is a property of the connection rather than of the
  traffic, so Unix keeps the stricter order and the two are deliberately not
  the same code path.
- **A refusal and a broken check were the same `false`, which is what made the
  above take a round trip to find.** "This peer is not us" is the answer that
  closes the connection, and it was also what a Win32 call failing produced —
  so the symptom was five tests reporting a hang-up and nothing saying why.
  `peer_check` keeps the reason and the tests assert through it; the library
  itself still has nowhere to complain, which is why it is `cfg(test)`.
- **`FindFirstFile` on `\\.\pipe` says `ERROR_NO_MORE_FILES` where a directory
  says `ERROR_FILE_NOT_FOUND`.** So a namespace with no window in it — an
  ordinary machine with no terminal open — failed every client that had to
  look, rather than answering with an empty list. Wine gives the directory
  spelling, so the focused ABI smoke could not see it.
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

And for the shell's Windows build, where CMake's own answers are the wrong
ones:

- **`CMAKE_SHARED_LIBRARY_PREFIX` is `lib` for a MinGW target and cargo does
  not use it.** The core is `sterna.dll` there, so the composed name is
  `libsterna.dll` — a file nothing writes, in a build that configures cleanly
  and fails at the link naming a library that plainly exists a directory away.
  The names are cargo's on every platform now, including the import library,
  which MSVC spells `sterna.dll.lib` and MinGW spells `libsterna.dll.a`.
- **The import library has to be a `BYPRODUCTS` entry as well.** It is the
  half a Windows link consumes, and an imported target's `IMPORTED_IMPLIB`
  gives the generator a dependency it has no rule for: Ninja stops with
  "missing and no known rule to make it" against a path cargo does in fact
  write, one step after the custom target that writes it.
- **`--target` and no `--target` are different builds, not the same one spelt
  twice.** Passing the host's own triple moves cargo's output under a triple
  directory and re-does what a native invocation would reuse, so
  `TT_CARGO_TARGET` is empty unless cross-compiling and every path is derived
  from it rather than assumed.
- **A GUI-subsystem binary has no stderr, and that is the right subsystem.**
  `ttermpro.exe` is one; a console-subsystem terminal opens a console window
  behind every desktop-launched session, and closing that window kills the
  terminal. So `WIN32_EXECUTABLE` is on and the windowless `/V` diagnostics
  need somewhere else to go — `MainWindow::note` uses `QCommandLineParser`'s
  own test (an inherited console, or `STARTF_USESTDHANDLES`) and puts up a
  parentless box when the answer is no. Same shape as the `qWarning`-to-
  journald trap: the message is not lost, it is *never written*.
- **A Winsock `SOCKET` is unsigned**, so `fd < 0` — the POSIX way to spell "no
  socket" — is a comparison that can never be true. `cmdline_test`'s listener
  compares against `INVALID_SOCKET` on both sides instead; the failure the old
  spelling would have given is a listener reporting success with no descriptor.
  `write` is a *file* call there too, so it sends with `send`.

And for running those binaries under Wine, where the harness lies before the
code does:

- **`WINEPATH` is a list of *Windows* paths, and a Unix one replaces `PATH`
  rather than adding to it.** `WINEPATH=/usr/x86_64-w64-mingw32/sys-root/mingw/bin`
  — the obvious way to let a cross-built binary find its Qt DLLs — leaves the
  process with a `PATH` of exactly that string, so `C:\windows\system32` is no
  longer on it. Spell it
  `Z:\usr\x86_64-w64-mingw32\sys-root\mingw\bin;C:\windows\system32;C:\windows`.
- **`wineboot` does not finish in this container**, so a prefix made by hand
  never gets its registry `PATH` either and `%PATH%` comes back as the literal
  string. Running the executable creates the prefix on its own, and the
  explicit `WINEPATH` above is what makes up for the missing value. A killed
  `wineboot` also leaves a `wineserver` behind that the *next* run waits on
  for ever — kill that too, or every later run looks like a hang in whatever
  it was testing.
- **What those two look like from inside Sterna is a missing `cmd.exe`.**
  `CreateProcessW "cmd.exe /c pause" … File not found`, about a file sitting
  in `system32`: `portable-pty`'s Windows `search_path` gives up when there is
  no `PATH` and hands the *bare* name to `CreateProcessW`, which does not
  search `PATH` once it has been given an application name. It reads as a
  broken pty and is a broken environment.
- **Wine's fonts are not Windows' fonts, so `render_test` is not Wine's
  question to answer.** Six of its assertions fail there — ink in cells that
  should be blank, an underline no shorter than a letter, two cells that
  should differ and do not — and all six are metrics. It also faults on exit,
  after every assertion has run, and the crash handler then starts `winedbg`,
  which wedges any script running the tests in a loop.
- **Wine's ConPTY opens and then delivers nothing.** The connection succeeds
  and the caption names the child, so a test that only checks it connected
  passes; `macro_test`'s two shell-driving cases then sit in front of a blank
  screen. Same limit already recorded for `tt-conn`, one layer up.
  `ResizePseudoConsole` is `E_NOTIMPL` there, which is worth knowing because
  the session propagates that error out of an unrelated settings change.
- **The two containers have different Wines, and neither one is "the" Wine.**
  `sterna-fedora` has the full `wine`/`wine64` pair and wedges in `wineboot`
  on a fresh prefix, exactly as the entry above describes — the run sits there
  and a `wineserver` outlives it. The Ubuntu box has no `wine` on `PATH` at
  all, only `/usr/lib/wine/wine64`, and an already-booted `~/.wine` that
  `ini-audit` made; copying that prefix skips the boot rather than fighting it,
  which is how the installer was smoke-tested. Ask which one you are in before
  concluding anything about Wine.
- **And that `wine64` has no WOW64, so a 32-bit Windows binary cannot start at
  all.** It fails with `failed to open C:\windows\syswow64\rundll32.exe`,
  naming a file the prefix genuinely does not have — which reads as a broken
  program rather than as a Wine without the other half. It is the reason the
  installer's stub is amd64; see below.

And for the Windows installer, where the traps are about what an installer can
do to a machine that is not the one it was built on:

- **The finish page must not start the program.** The installer asks for
  administrator rights, so anything it launches inherits them — and Sterna's
  settings are under the *running user's* AppData. A first run as
  Administrator writes `sterna.ini` into the administrator's profile, and the
  user's own later runs start from defaults, permanently, with nothing
  anywhere to see. `StartSterna` goes through `explorer.exe`, which is already
  running as the user and hands the program back its proper token.
- **`RMDir /r "$INSTDIR"` is a recursive delete of a path the user typed into
  the directory page.** So `build.sh` generates the uninstall list out of the
  staging tree — every file by name, every directory with a plain `RMDir`,
  which refuses one that is not empty. Verified under Wine: a file left in the
  program folder survives the uninstall and so does the folder.
- **An upgrade in place leaves the old version's files behind, and a stale Qt
  DLL is not inert.** The loader finds it first and the program dies before
  `main` with a missing-entry-point box naming a symbol nobody has heard of.
  `.onInit` runs the previous uninstaller first, and `_?=` is what keeps it in
  place long enough to be waited on rather than having it copy itself to the
  temp directory and return immediately.
- **The licence page is a RichEdit control and renders LF-only text as one
  unreadable line.** Every text file a user reads gets CRLF on the way in; the
  `.lng` files do not, because they are read by us.
- **Qt's deployment tooling does not exist for this target.** `windeployqt` is
  a Windows program and the MinGW package ships no `qtpaths` — CMake says so
  during configuration, which is a warning worth not dismissing. The DLL set is
  therefore closed by walking `objdump -p` to a fixed point, and the rule for
  ours-versus-Windows' is whether the MinGW sysroot has the file: that tree
  holds only the 76 the cross toolchain provides and none of `kernel32`,
  `msvcrt`, `shell32`, `user32`, `advapi32` or `ole32`. Checked rather than
  assumed — shipping a private copy of a system DLL is worse than shipping
  none.
- **Fedora's MinGW packages are shipped unstripped**, and so is everything
  cargo and the CMake tree produce: 154 MB staged before `--strip-unneeded`
  and 106 after, `libstdc++-6.dll` alone accounting for 29.7 of the 48.
  Stripping a PE file is safe — the export table a DLL is loaded through is
  part of the image, not of the symbol table.

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
- **But CI's Qt is the Ubuntu container's, so that is where a CI paint failure
  reproduces.** The two rules do not conflict: a *measurement* — footprint,
  startup, protocol support — belongs in `sterna-fedora`, and a *verdict CI has
  already given* belongs where CI gave it. `cmake -S . -B build-ubuntu` in this
  container is that tree; it is gitignored and it is not for measuring
  anything. Without it, "render ok" here and one failed check there is a
  standoff with nothing to run.
- **A glyph can put ink outside its own advance, so measuring a margin from
  column 0 clamps.** `render_test`'s `VTFontSpace` case moved an `A` right by
  three pixels and measured two: DejaVu Sans Mono's `A` overhangs to the left
  by a pixel at the size Qt 6.4.2 picks, and the pixel before column 0 is off
  the image, so the search saturated at x=0 and the *first* measurement was the
  wrong one. It painted correctly throughout. Anything asserting where ink
  begins measures a column with a blank one beside it, and lets the answer be
  negative.
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

**And a twenty-ninth, in `vtterm.c` and reachable from the wire: `RingBell`
never reads the argument it is given.** `RingBell(int type)` (`:5791`) switches
on `ts.Beep` instead, so the parameter is dead — and the only caller that
passes anything else is `ESC g`, GNU screen's **visual** bell, which asks for
`IdBeepVisual` at `:1561` and gets an audible beep under the default or nothing
at all when the bell is off. The one sequence whose entire purpose is to flash
the screen is the one that cannot. Reproduced, because it is what a user of
Tera Term sees. Its documentation has a defect of its own that is not the same
one: `teraterm-term.html` says five bells are permitted where six sound, and
describes the suppression as a fixed delay when it is a *quiet* period that
every further bell extends.

**And a thirtieth, a two-byte heap overflow in `Hex2StrW`**
(`ttlib_static_cpp.cpp:837`), which is the decoder `DelimList` and the
user-defined key strings both go through. It grows its buffer in 512-`wchar_t`
steps under a `wp + 1 > str_len` test and then writes its NUL terminator at
`Str[wp]` *after* the loop, so a decoded length that is an exact multiple of
512 lands one `wchar_t` past the allocation. Reachable from a `TERATERM.INI`
and from `keyboard.c:856`.

**And a thirty-first, in `vtdisp.c`, which is the first defect on this list
that is visible on the wire rather than in memory: a host cannot read back a
colour it just set.** `DispSetColor` writes the live `vtdraw_t` pair (`:3376`)
and `DispGetColor` reads `ts` (`:3561`), so `OSC 10;#ff0000` followed by
`OSC 10;?` answers with the *configured* foreground — the window repaints and
the report does not move. Only the palette round-trips, because both halves of
it are `vt->ANSIColor`, and Tek does by accident, its setter writing the same
`ts` field the getter reads. A program that queries a colour, changes it and
restores what it read therefore restores the wrong thing. Reproduced, because
the alternative is a terminal reporting something Tera Term never reports; it
is also the reason `esctest`'s whole `ChangeDynamicColor` family cannot pass
here.

**And a thirty-second, in `buffer.c`, and it is the worst thing on this list:
`BuffDumpCurrentLine` (`:2400`) smashes the stack, and prints the wrong bytes
on its way there.** Twenty-eight lines with four faults in them, all in the
handling of a double-byte character, and all reachable from the wire whenever
`PrinterCtrlSequence` is on — auto print calls it at every line feed
(`vtterm.c:693`) and `CSI ? 1 i` calls it directly.

1. `char bufA[TermWidthMax+1]` is **1001 bytes** and the fill loop writes up to
   **two per column**. A thousand-column terminal holding five hundred
   full-width characters produces fifteen hundred bytes — five hundred past the
   end of a stack buffer, with content the host chose.
2. It writes the **low** byte of a double-byte code *twice* —
   `*p++ = (c & 0xff); if (c > 0x100) *p++ = (c & 0xff);` — where
   `buffer.c:3597`, the same file, correctly writes `(c >> 8)` and then
   `(c & 0xff)`. And the boundary is `> 0x100` there against `< 0x100` here.
3. The **write** loop is bounded by the column count rather than by the bytes
   the fill produced, so whatever those doubled bytes pushed past it is
   silently dropped.
4. A padding cell's `ansi_char` is zero, and `WriteToPrnFile(handle, 0, FALSE)`
   is the *clear the buffer* form of that function (`teraprn.cpp:504`) — so the
   zero belonging to the second half of every wide character **discards
   everything accumulated for the line so far**. `あab` prints as `a`.

Not reproduced, and that is the only entry on this list where the reason is
that reproducing it means reproducing a remote stack overflow.
`Vt::dump_current_line` prints what upstream meant to print; for a line with no
full-width character in it the two agree byte for byte, which is every line
this port has been asked about so far.

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
AGENTS.md        this file: the working agreements and the traps
CLAUDE.md        one `@AGENTS.md` import, so Claude Code reads the same text
ATTRIBUTION.md   licensing, and what still needs clearing before vendoring
oracle/          Tera Term's real VT engine, headless on Linux (see its README)
esctest/         the conformance suite, run inside our own terminal (see its README)
packaging/       the AppImage and the NSIS installer, which are the whole of
                 Linux and Windows packaging (see the README in each)
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

# Working notes for qtterm

Read `PLAN.md` for the roadmap and current stage. This file is the working
agreements and the traps.

## What this is

A cross-platform Tera Term successor: Rust core + flat C ABI + Qt 6 Widgets
shell, Linux and Windows. **Not** a fork of Tera Term and **not** aiming at
parity — see `PLAN.md` for scope.

`qtterm` is a working name; the real one is still undecided (the current one is
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
4. **The oracle's settings are load-bearing.** `main.c:settings_defaults()`
   mirrors `ttpset/ttset.c`'s per-key fallbacks. If a dump looks subtly wrong,
   suspect a setting before suspecting the parser. See the traps below.
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
cd oracle
make            # build build/oracle
make test       # 18 regression cases
make stubs      # regenerate the stub layer after upstream headers change

cd serial-audit                  # Stage 0 spike 4, needs the FTDI loopback rig
cargo run --bin serial-audit     # capability audit vs commlib.c
cargo run --bin rawpatch         # are the gaps patchable through the raw fd?
cargo run --bin hotplug          # needs a human to pull the cable
```

The oracle needs `gcc` and Python 3.11+ and nothing else.

Rust, cmake, Qt 6, lrzsz and ckermit are installed in the dev container.
**`cargo` is on `PATH` only for login shells** — export
`$HOME/.cargo/bin` first or `cargo: command not found` will look like a missing
toolchain. It isn't; don't reinstall it.

Two packages were added on 2026-08-07 and a rebuilt container will need them
again: **`libudev-dev`** (`serialport-rs` enumeration — without it the crate
does not build) and **`libxcb-cursor0`** (Qt's `xcb` platform plugin refuses to
start without it).

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

### Qt work goes in the `qtterm-fedora` container, not this one

This container has Qt **6.4.2**; the desktop runs **6.11.1**. That gap has
already manufactured one false finding — see the traps. So there is a second
distrobox, created 2026-08-07:

```sh
distrobox-host-exec distrobox enter qtterm-fedora --no-tty -- <command>
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
  A whole false optimisation, from one version gap. Use `qtterm-fedora`.
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

## Bug found upstream, not yet reported

`BuffGetAnyLineDataW()` (`buffer.c:5832`) does `continue` without advancing `b`
on padding cells, so it parks on the padding cell after a full-width character
and drops the rest of the line. Only caller is `filesys_log.cpp:443` — so
**Tera Term's session logging truncates any line at its first CJK character.**

One-line fix in `oracle/patches/0001-buffgetanylinedataw-padding.patch`.
Reporting it upstream is an open item in `PLAN.md`.

## Layout

```
PLAN.md          roadmap + status — read first
ATTRIBUTION.md   licensing, and what still needs clearing before vendoring
oracle/          Tera Term's real VT engine, headless on Linux (see its README)
serial-audit/    Stage 0 spike 4 — serialport-rs vs commlib.c, on real hardware
crates/          Rust core — not started
shell/           Qt 6 shell — not started
vendor/          vendored Tera Term subsystems — empty, see ATTRIBUTION.md first
```

`serial-audit/` is not throwaway: it becomes the regression test for the serial
patch layer once `tt-conn` exists, and every claim in `PLAN.md`'s spike 4
section is reproducible from it.

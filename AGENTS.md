# Working notes for Sterna

Read `PLAN.md` for the roadmap and current stage. This file is the working
agreements and the traps.

**This is the one instruction file, whichever agent you are.** `CLAUDE.md`
imports it and holds nothing of its own; a rule belongs here or half the
agents on this repository never see it. An untracked `AGENTS.local.md` beside
it may hold user/machine context — read it if it is there.

## What this is

A cross-platform Tera Term successor: Rust core + flat C ABI + Qt 6 Widgets
shell, Linux and Windows. **Not** a fork of Tera Term and **not** aiming at
parity — see `PLAN.md` for scope. Sterna is the settled name; the mark is a
banked tern tracing an S-shaped flight path.

## Ground rules

1. **`../teraterm` is read-only reference.** Never edit it. It is compiled
   unmodified as the test oracle, vendored for specific subsystems, and read
   as the behavioural spec. Build fixes go in `oracle/patches/`, applied to a
   copy under `oracle/build/patched/`.
2. **Prefer compiling real Tera Term code over reimplementing it.** Adding a
   source to `TT_CXX` beats writing a stub; every stub is a place the oracle
   can lie.
3. **Never bless a golden you have not read.** Prefer differential cases
   (`./run_diff.sh` needs only an `input`); bless an oracle golden only to
   also guard against upstream drift.
4. **The oracle's settings are load-bearing, and `ttset.c` lies about them.**
   `main.c:settings_defaults()` mirrors the per-key fallbacks applied *after*
   the zero initialisers at the top, not the initialisers. If a dump looks
   subtly wrong, suspect a setting before the parser.
5. **Attribution and licensing are not paperwork.** Check `ATTRIBUTION.md`
   before vendoring — the `.lng` and `.map`/`.tbl` assets have no per-file
   licence headers.
6. **Git identity is set per-repo** to the GitHub noreply address. Don't
   change it.
7. **Commit often.** Small self-contained commits as work lands; a spike that
   compiles is a commit, a finding recorded in `PLAN.md` is a commit.
8. **Some divergence is deliberate.** `docs/deviations.md` is the list and the
   rule for joining it — a difference recorded there is not a bug to fix, and
   a change that creates one gets its entry in the same commit.
9. **Use ASD-STE100 for user-facing English.** Before writing, rewriting, or
   reviewing user-facing text, load and follow the `ste100` skill from the
   adjacent `STE100` repository. Resolve it from the main checkout when working
   in a worktree. Use its Issue 9 rule index and dictionary workflow instead of
   memory, and preserve exact interface labels, commands, identifiers, and
   necessary technical terminology.
10. **Python scripts here are plain `python3`, not `uv`** — a deliberate
    exception to the general preference for PEP 723 headers, decided
    2026-08-15 and scoped to this repository only. All seven declared no
    dependencies, so uv was a launcher enforcing a `>=3.11` floor that nothing
    here needs: no script uses a 3.11-only feature, and every environment they
    run in ships 3.12 or newer. It bought one more thing to install on a
    release machine, three CI steps, and a cache with nothing in it. Don't add
    a `# /// script` block back; the version note at the top of each file is
    what replaced it.
11. **The `iced` experiment is over.** `worktree-iced` and its worktree were
    removed on 2026-08-16 at the user's request; the branch was never pushed,
    so its 57 commits live only in this checkout's reflog (tip `c27e542`,
    recoverable for the usual ninety days). The rule that used to stand here
    protected it from tidy-up sweeps — there is nothing left to protect, and
    a rule guarding a deleted branch only misleads. `PLAN.md`'s toolkit note
    is the surviving record of why Qt was chosen.

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
./docs/macro/generate.py --check              # ...and the generated TTL manual
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
(cd ../../sterna-rig && ./run.sh all)   # + the OTHER rig, in a subshell
                                # because the `cd`s below are relative: an
                                # ESP32-S3 that witnesses the USB side of a
                                # port and can unplug itself. See its README.
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
./build/ssh_test                 # the window's event loop, against a real server
./build/telnet_test              # the same, over telnet
./build/pty_test                 # ...and over a local shell, which needs nothing
./build/xfer_test                # a ZMODEM send, driven by the event loop
./build/macro_test               # a TTL macro, driven by the event loop
./build/print_test               # the printer, which is a file, so it needs none
./build/highlight_test           # the highlight rules, to the pixels — needs nothing
./build/gutter_test              # line numbers: painted, and never copied
./build/buttons_test             # the quick buttons, over a pty — needs nothing
./build/send_test                # ...and a file fed to one a piece at a time.
                                 # The send queue's *clock* is only testable
                                 # here: below the ABI it is a fake instant
QT_QPA_PLATFORM=offscreen \
  ./build/cmdline_test           # a Tera Term command line, argv to connected
                                 # — NOT under Wayland; see the traps
./build/control_test             # the control socket, against the window's loop
# every *_test also takes --write /tmp to dump its dialogs/frames as PNGs
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

cd packaging/appimage            # the only Linux artifact — manylinux_2_28 only
./build-qt.sh && ./build.sh       # → build/sterna-x86_64.AppImage; see README

cd packaging/windows             # the only Windows artifact — sterna-fedora
./build.sh                       # → build/sterna-0.1.0-x86_64-setup.exe
./build.sh --stage               # ...the file tree, without makensis

# Releases: the whole procedure is packaging/RELEASING.md — read it, do not
# assemble one from these three lines. The version is in six files and only
# this script knows all six; CI and the tag preflight both run its --check.
./packaging/bump-version.sh 0.1.0    # ...and --check, which is what they call
./packaging/release.sh v0.1.0        # sign the built draft and publish it

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

`xfer` needs `lrzsz` and `gkermit`. The oracle needs `gcc` and Python 3.11+
only. For protocol interop use **G-Kermit** — C-Kermit sees a pty as a tty and
goes interactive. **`cargo` is on `PATH` only for login shells** — export
`$HOME/.cargo/bin` first; it is not a missing toolchain.

Ubuntu container packages a rebuild needs again: `libudev-dev`
(serialport-rs), `libxcb-cursor0` (Qt xcb plugin), `gkermit`.
`sterna-fedora` also needs `lrzsz` for `shell/build/xfer_test`.

## The dev container is not headless

Verified 2026-08-07 with real Qt windows and real serial hardware. Rootless
podman (`agents`, `ubuntu:24.04`) via distrobox on a Bluefin / Fedora
Silverblue 44 host; the desktop session passes straight through. **Do not
assume anything GUI- or hardware-shaped is untestable here — check first.**

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

The serial pair is a complete loopback rig: data both ways, DTR→DSR, RTS→CTS,
break visible as a NUL, 9600–3000000 baud clean, RTS/CTS flow control. Only a
physical unplug/replug needs the user.

### Qt work goes in the `sterna-fedora` container, not this one

This container has Qt **6.4.2**; the desktop runs **6.11.1**, and that gap has
already manufactured one false finding (see the traps). The second distrobox:

```sh
distrobox-host-exec distrobox enter sterna-fedora --no-tty -- <command>
```

Fedora 44, Qt 6.11.1 (exact desktop match), plus `gcc-c++`, `cmake`, `ninja`,
`qt6-qttools-devel`, `xcb-util-cursor`, `systemd-devel` (Fedora's `libudev.pc`
— serialport-rs needs it), `lrzsz`; for the Windows cross build
`mingw64-qt6-qtbase` (also 6.11.1), `mingw64-gcc-c++`, `nasm` (`aws-lc-sys`'s
assembler — its absence stops the *core*, minutes in), `mingw64-cmake`; for
the installer `mingw32-nsis` + `mingw64-nsis` (one native `makensis` plus x86
and amd64 stubs; the `.nsi` targets amd64). If Fedora's `updates` metalink
fails, pass `--setopt=updates.metalink=` and an explicit
`--setopt=updates.baseurl=https://dl.fedoraproject.org/pub/fedora/linux/updates/44/Everything/x86_64/`.

- **Its `$HOME` is `/var/home/nata/agents-home`** — deliberately not the
  user's real home. Always pass `--home /var/home/nata/agents-home` on
  create; **the user does not want their host home polluted.**
- `distrobox create` still writes a launcher to the *host*
  `~/.local/share/applications/` — delete it; `distrobox rm` cleans it up.
- **Rust needs no install there**: `~/.cargo` is shared and Ubuntu-built
  binaries run on Fedora's newer glibc (not the reverse — which is why
  `shell/CMakeLists.txt` points `CARGO_TARGET_DIR` at its own build tree
  rather than sharing `crates/target` between containers).

## Traps

These cost real debugging time. Each is a place where the failure looks like
something other than what it is. Kept deliberately terse; the fuller story is
usually in a comment at the file:line cited, or in git history.

Oracle and core:

- **`cargo test -p tt-ffi` runs zero ABI tests** — deliberate. Run
  `tt-ffi/run_abi.sh` and `run_abi_windows.sh` (native: `run_abi_windows.ps1`).
- **`UTF32ToUTF16` is not optional**: `expand_wchar()` reads `wc2`
  (`buffer.c:234`). Stub it and the screen renders entirely blank — which
  looks exactly like a broken parser.
- **`_WideCharToMultiByte` has no NULL check** (`buffer.c:3076`); a stub
  returning NULL segfaults on the first combining character.
- **`CRReceive`'s real default is `IdCR`** — the `else` at `ttset.c:643`, not
  the `IdCRLF` the surrounding code suggests.
- **`AcceptTitleChangeRequest` defaults to `overwrite`**, not off
  (`ttset.c:1568`).
- **`ts.BSKey` defaults to `IdBS`** (`ttset.c:877`; only literal `"DEL"`
  takes the DEL arm). Rule for the whole family: a string-read setting —
  check what an *empty string* does; a flag word — find the key, not the
  initialiser.
- **`buffer.c:134` hardcodes `CodePage = 932`.** Call `BuffSetDispCodePage()`.
- **`WinWidth`/`WinHeight` ≠ `NumOfColumns`/`NumOfLines`** (visible window vs
  terminal size); only `BuffChangeTerminalSize` owns the latter, and
  `DispChangeWinSize` must not call `BuffChangeWinSize` (infinite recursion,
  `buffer.c:4956`).
- **`BuffGetAnyLineDataW` takes an absolute buffer index**;
  `BuffGetCursorCharAttr` is screen-relative; `PageStart` maps between them.
- **`vtterm.c` owns `CharSetInit`** — the runner must not call it.
- **`struct opts` in `oracle/src/main.c` is a positional initialiser**: add a
  field to the initialiser in the same place as the struct, or every default
  after it silently shifts — the symptom is an *unrelated* differential case
  changing its answer.
- **Make's VPATH beats pattern rules**: patched sources need explicit rules
  or the unpatched original silently wins.
- **A `ts->X = 0` at the top of `ttset.c` is an initialiser, not a default.**
  `ISO2022Flag`/`ColorFlag`/`TermFlag`/`WindowFlag` are ORed together from
  per-key `GetOnOff(..., TRUE)` calls a thousand lines later. Taking the
  zeros once reported a Tera Term with 256-colour, ISO-2022 shifts, 8-bit
  controls and the alternate screen all off.
- **SGR 38/48 do not consume their arguments when their colour mode is off**
  (`vtterm.c:2239`): `38;5;196` then reads 5 as blink on.
- **`TermIDGetID()` never fails** — case-sensitive strcmp, anything
  unrecognised is VT100. Fixed in `oracle/src/main.c:resolve_term_id()`; the
  same shape hides anywhere upstream "defaults" instead of erroring.
- **Breaking a wide character and erasing one are different operations**:
  overwrite/insert/delete/scroll use `BuffSetChar(b,' ','H')` (keeps SGR
  bits); erase paths use `EraseKanji` (paints the whole pen). Match
  upstream's choice per path.
- **The padding half of a wide character gets zeroed attributes**
  (`buffer.c:3400`) except the insert-mode branch (`:3325`), which copies the
  pen. Both reproduced; neither is a typo.
- **`disp_width()` in the oracle's `main.c`**: full width is `'W'` **and**
  `'F'`; and the dump sizes from `NumOfColumns`/`NumOfLines`, not argv.
- **Stubs lie.** `ShiftKey`/`ControlKey`/`AltKey` are *functions* in
  `keyboard.h`, not BOOL variables (defining them as data links and then
  jumps into the data section); `DispConvWinToScreen`/`DispConvScreenToWin`
  must store through their out-params; `IsCaretEnabled` must not return 0
  unconditionally. All now carry real behaviour.
- **`WinOrgY` drifts negative in a headless build** (`buffer.c:3865`;
  `vtdisp.c`, not compiled, restores it). The oracle's coordinate conversion
  uses a fixed `(0,0)` origin.
- **`ts.MouseEventTracking` and `ts.TranslateWheelToCursor`** are
  `GetOnOff(..., TRUE)` (`ttset.c:1523`, `:1515`) — the flag-word trap in a
  plain WORD; zeroed they disable every mouse mode.

The AppImage, where two of the three failures are silent:

- **`LD_LIBRARY_PATH` is exported, so every child gets our libraries** — the
  login shell, everything its rc files run, the browser a URL opens. They are
  host programs built against the host's libraries and ours are older
  (`manylinux_2_28`), so the first prompt fills with `no version information
  available` and a `version 'MOUNT_2_40' not found` that is fatal to whatever
  hit it. `environment::unshadowBundledLibraries()` takes the `$APPDIR`
  entries back out at startup and unsets `APPDIR` itself, which is our package
  root and not a child program's. Removing the library entries is safe because
  **glibc reads
  `LD_LIBRARY_PATH` once, at `exec`** — a later `dlopen` uses the captured
  list, so the bundled Qt plugins still resolve. One call at startup, not a
  scrub at each spawn site; the next spawn site would forget.
- **The update signature covers bytes, not an AppImage identity.** Verify the
  detached manifest before trusting even its URL or size; then the download's
  signed size, SHA-256 and Ed25519 signature. Linux: set executable perms on
  `QSaveFile` *before* `commit()`. Windows: the NSIS updater must wait for
  the running pid before invoking the old uninstaller.
- **`QTemporaryFile::close()` does not release a downloaded installer** — the
  object keeps its read/write handle open, and Windows refuses to execute it.
  `detachUpdateDownload` must destroy that object before `ShellExecuteExW`.
  Keep the verification `QFile` open across the call: Qt shares reads and
  writes but not deletes, pinning the verified bytes without blocking execute.
- **Qt Network can load while HTTPS is entirely absent** — TLS backends are
  plugins, invisible to the deployment closure. Linuxdeploy's Qt plugin
  carries the OpenSSL backend; the Windows stage copies
  `tls/qschannelbackend.dll` explicitly.
- **linuxdeploy's `patchelf` corrupted every `.relr.dyn` library in the old
  Fedora 44 build**, silently — segfault in `_init` before `main`, backtrace
  naming whichever library loaded first. The portable build keeps the same
  conservative repair: `NO_STRIP=1`, restore the originals, and resolve via
  `LD_LIBRARY_PATH`, not rpath.
- **A Wayland window that never appears is not an error.** Without
  `wayland-shell-integration/libxdg-shell.so` the process binds the registry
  and sits there, no warning, zero exit. `WAYLAND_DEBUG=1` + grep
  `get_xdg_surface` is the only check that tells it from a headless run.
- **An AppImage can quietly use the desktop's Qt 6.11.1 and pass every
  test.** Check `/proc/<pid>/maps`: `libQt6Core.so.6` must come from
  `/tmp/.mount_sterna*` (the prefix follows the image's current filename).
  Two things make that check harder than it reads. The pid you want is not
  the one you launched — argv[0] is the image, and the process doing the
  work is `/tmp/.mount_sternaXXXX/usr/bin/sterna`, so `pgrep -f "sterna
  --shell"` finds nothing and `pgrep -f "AppImage --shell"` finds the
  *launcher*, whose maps hold no libraries at all. And the child's maps
  spell its libraries `/usr/lib/...` with no mount prefix, which reads
  exactly like the host's — tell them apart by the device column (the mount
  has its own) or by the fact that this host keeps its Qt in `/usr/lib64`
  and has no `/usr/lib/libQt6Core.so.6` to load.
- **Building the AppImage from a git worktree needs two mounts and a
  relative symlink.** `packaging/appimage/toolchain/` is gitignored, so a
  worktree has no Qt; symlinking it to the main checkout's only works if the
  link is **relative** — an absolute one spells this container's `$HOME`
  (`/home/nata/agents-home`) and dangles inside a container that mounts the
  host's (`/var/home/nata/...`), where the failure is `qmake6 not found —
  run build-qt.sh first`. And a worktree's `.git` is a *file* holding an
  absolute gitdir in the container-side spelling, so `build.sh`'s
  `git rev-parse HEAD` fails and `BUILD-INFO.txt` says `commit: unknown`
  — mount the repository at *both* paths to fix it. `.gitignore`'s
  `/packaging/appimage/toolchain/` has a trailing slash and does not match a
  symlink, so the link shows up as untracked: remove it when done.
- **GNOME draws no title bars, so a bundled plugin is the title bar.** Mutter
  advertises no `zxdg_decoration_manager_v1` at all — check the registry, not
  the presence of a bar — so Qt draws its own, and Qt Base ships only
  `bradient`: a 3/30/3/3 frame with no clock in it, therefore no double click,
  therefore no maximise. `adwaita` is Qt's own and does both
  (`qwaylandadwaitadecoration.cpp:673`), lives in Qt Wayland, and its feature
  switch turns itself *off* unless Qt Svg is installed first — so the order of
  the three stages in `build-qt.sh` decides whether the plugin exists. Qt picks
  it only when the desktop says GNOME (`qwaylandwindow.cpp:1147`), which is
  right: every other compositor here draws its own. Verify by grepping
  `/proc/<pid>/maps` for the decoration, the way the Qt-from-the-mount check
  above works — a fallback to `bradient` opens a perfectly good window.
- **GLVND frontends are not display drivers.** linuxdeploy excludes the whole
  OpenGL-shaped family, but QtGui has direct NEEDED entries for `libOpenGL`,
  `libEGL`, `libGLX` and `libGLdispatch`; a minimal host then fails before Qt
  can select `offscreen`. Bundle those four ABI dispatch libraries, not the
  Mesa/NVIDIA implementation behind them.
- **The Actions cache is the wrong place for a build input a *tag* needs, and
  it fails silently in two ways.** A cache belongs to the ref that wrote it and
  is readable only from that ref or the default branch — the release job runs
  on `refs/tags/vX.Y.Z`, so four releases each spent 40 minutes on Qt and each
  saved it where no later run could open it. Warming it on main fixes the scope
  and leaves the second half: the 10 GB repository limit evicts
  least-recently-used, and 26 MiB of Qt behind seven 400 MiB `v0-rust-*` caches
  is the cheapest thing to drop. Both symptoms are one symptom — a release that
  takes 55 minutes instead of 6, with nothing in the log calling it unusual.
  Qt now ships as a **release asset** pinned by SHA-256 in
  `packaging/appimage/toolchain.env` (`fetch-qt.sh`, `publish-qt.sh`), which has
  neither property. Before blaming a cache key, ask
  `gh api repos/OWNER/REPO/actions/caches --jq '.actions_caches[].ref'`: a
  tag-scoped hit reads exactly like a plain miss.
- **Two workflow files with the same constant in them is one constant too
  many.** `ci.yml` and `release.yml` each held the image digest and cache key,
  with a step that grepped one for the other's strings. It worked once and then
  the job holding the check became the thing that broke — main red on every
  push, the release quietly uncached. One sourced file (`toolchain.env`) has
  nothing to disagree about; note that `$GITHUB_ENV` is not a shell, so a
  comment line in it fails the step and the workflow greps assignments out.
- **The hosted Qt build's fifty-minute disconnect is OOM, not a time limit.**
  Two workers consumed 14 of the runner's 15 GiB before it vanished with no
  retained log; one measured 6.3 GiB used / 8.9 GiB available. Keep one
  prepared container alive across the release job, pass the numeric job count
  to `cmake --build --parallel`, and let that one-worker build finish. A GNU
  `timeout` inside `docker exec` cancels the Docker context with 143 instead of
  making a resumable slice.
- **`IdTitleReportEmpty` is 24 — `WF_TITLEREPORT` entire.** The "Empty"
  default sets *both* bits and lands on the `default:` arm (empty OSC
  answer); it is not "no bits". The flag-word trap disguised as a name.
- **`rewrite_c1` must know where UTF-8 sequences begin and end.** Replacing
  bare `80..=9F` byte-by-byte eats continuation bytes (an em dash's `80`).
  Case 97 pins it.
- **`vte` must never see a partial UTF-8 sequence — `Vt::held` stops it.**
  vte 0.15.0's `advance_partial_utf8` silently drops complete characters. A
  chunking bug is invisible to the differential suite by construction; touch
  `rewrite_c1` and the property to run is `vt_chunking`.
- **The differential dump cannot see width classes**, so a broken wide pair
  renders identically to a whole one, and upstream's own `AttrKanji` is
  incoherent so dumping it cannot rescue this. `Grid::check_wide_pairs` is
  the only cover — deliberately not an invariant; Tera Term breaks the
  pairing in three places itself, listed on that function. The history blind
  spot is opened by `--scrollback` on both engines. Ask what a case cannot
  see before trusting that it passed.
- **A parked space goes through the whole write path** — upstream retries via
  `BuffPutUnicode(0x20,…)` recursively (`vtterm.c:896`) so the crushes run;
  writing the cell directly leaves half a wide character behind.
- **DECRQCRA is not upstream's** — `Config::decrqcra` defaults off; only
  esctest turns it on.
- **esctest's `--test-case-dir` recordings lack its reset preamble** — fine
  for `esctest/run_diff.sh`, wrong for reproducing one test's verdict.
- **When a manual stub reimplements upstream logic, diff it against the
  original** — `DispFindClosestColor` shipped with xterm's palette and no
  bright/dim flip; `vtdisp.c` is not compiled into the oracle, so nothing
  else will catch it.
- **`ANSIColor` is parser state as well as paint** — truecolor reduces to a
  palette index during SGR parse, so the table must live in `tt-vt::Config`.
  File entries 1–7 are the bright colours, moved to drawing indices 9–15 by
  `GetIndex256From16`; values wrap rather than validate (`colorid & 15`,
  BYTE channels, last duplicate wins, incomplete groups ignored, 259-byte
  value / 14-byte field limits).
- **Modern sixel gives DECSDM the surprising sense**: scrolling starts on,
  `DECSET ?80` turns it *off* (image fixed at page origin, cursor untouched),
  `DECRST ?80` restores cursor-relative. xterm and `DECRQM` are the
  contract, not the old DEC manual.
- **A sixel paints after the old text and before the cursor.** `SixelImage`
  snapshots covered cells and clears a pixel tile when one changes; any
  caller editing through `grid_mut` must call `reconcile_sixels`, and the
  frontend must supply the live cell size before decoding.

The vendored protocol C, which two compilers disagree about:

- **GCC 13 compiles `vendor/ttpfile/` and GCC 14+ does not** (`raw.c` misses
  `<stdlib.h>`; `zmodem.c:1586` calls undeclared `SetTimer`/`KillTimer`).
  Fixes live in `winshim/windows.h`: **a vendored source that needs a
  declaration gets it from the shim, never from an edit.**
- **`common/ttcstd.h:45` typedefs `char8_t` under an inverted C++20 guard**,
  so the vendored C++ compiles at C++17 only; `tt-xfer/build.rs` pins
  `gnu++17`.
- **`cl.exe` cannot open `\\?\D:\...` paths and misnames the file it could
  not open** (`C1083 ... '\\raw.c'`). `plain()` in `tt-xfer/build.rs` strips
  the prefix — drive spellings only; it is load-bearing in `\\?\UNC\`.

Telnet:

- **The framing (`ttcmn.c`) runs before the negotiation (`telnet.c`)** — it
  unescapes `IAC IAC` and swallows the `NUL` after a `CR`. Reading
  `telnet.c` alone gives a parser that doubles `0xFF` and passes `CR NUL`.
- **`ttcmn.c` clears its CR flag whatever the next byte is** (`:572`) — only
  a `NUL` is lost after a `CR`; clearing only on the NUL path drops the `IAC`
  in `CR IAC …`.
- **The opening burst goes out only when the port is 23** (`vtwin.cpp:3666`,
  `TCPPort == TelPort`) — deliberate: a console server's per-line port is not
  a telnet server.
- **`MaxTelOpt` is 34; everything above is refused flat.** Reproduced — a
  real telnetd opens with options above it, so the refusal path runs first in
  every session.
- **NAWS resizes whether or not it was negotiated** (`telnet.c:299`, the
  check is commented out). Reproduced including the laxity.
- **Telnet has a break and SSH does not** — hence `supports_break` on the
  transport.
- **`Telnet=off` is not a raw socket**: `TelAutoDetect` ships on and switches
  framing on at the first `0xFF` regardless (`ttcmn.c:590`; TTSSH clears it
  by hand, `ttxssh.c:981`). Raw needs *both* keys off.
- **Framing and opening burst are two questions — four modes, not two**
  (`ts.Telnet`, `commlib.c:340`; `TCPPort == TelPort`, `vtwin.cpp:3666`).
  The easy one to miss is framing-without-burst — the ordinary console
  server. `TelnetMode::of` is the table; do not assemble the mode by hand.
- **`TelEcho` means "let the ECHO option decide", both directions**: off,
  `WILL ECHO` changes nothing (`telnet.c:411`, `:497`); on, negotiation
  assigns `ts.LocalEcho` and the burst asks the server for the *opposite* of
  local echo (`:845`). A `TransportEvent`, not a state to poll — SRM assigns
  the same variable.
- **The keepalive interval is a quiet period, and only runs where the burst
  ran** (`telnet.c:913` vs `cv.LastSendTime`; thread started inside the
  `TCPPort == TelPort` arm). A pump cannot drive it — an idle socket wakes
  nothing — so it needs `tt_session_tick` on a timer (`Session.h` says so).
- **`TELNET.LOG` holds one half of the conversation** — every `TelWriteLog`
  follows a `CommRawOut`; nothing on the receive path logs. Logging both
  directions produces a file upstream never writes.
- **A cloned Windows socket stays alive, and its read timeout is shared
  state.** The Windows wakeup owns a blocking reader clone, a bounded 1 MiB
  queue and a manual-reset event; setting the Unix 50 ms timeout there makes
  a 20 Hz idle worker whose timeout reads as EOF. `Drop` must `shutdown` the
  connection to wake the clone.
- **`TCPLocalEcho`/`TCPCRSend` spend `ts.LocalEcho`/`ts.CRSend` and put them
  back at close** (`vtwin.cpp:3696`, `:3589`), and **off is not a value** —
  an unset key borrows nothing, so a host's SRM survives the disconnect.
  That is what `TCPLocalEchoUsed`/`TCPCRSendUsed` are for.
- **`TCPCRSend` moves the keyboard's line ending and not LNM** — `LFMode` is
  seeded from `ts.CRSend` at reset only (`vtterm.c:285`), so `SM 20` moves
  the pair and this does not (`Vt::set_cr_send` says so).
- **`ConfirmDisconnect` is TCP only** (`vtwin.cpp:1668`, `:4448`) — a serial
  session closes silently regardless; hence `tt_session_link_kind`. A
  macro's `disconnect` can raise the dialog upstream and deliberately cannot
  here (modal dialog inside a request holds the requester open).
- **`AutoWinClose` is TCP only, and upstream's Disconnect takes the
  dropped-line branch** (`FD_CLOSE` → `IdComEndTimer`, `vtwin.cpp:4462`,
  `:3005`) — so choosing Disconnect closes the window there. **This port
  deliberately does not** (deviation 15): the `asked` flag on
  `connection_closed` is the seam, and a window that quits when somebody hangs
  up cannot offer them the next connection. If the
  window cannot close, `ClearScreenOnCloseConnection` runs instead — and its
  "clear" is `BuffClearScreen`, a scroll into history. A disconnect found
  while *writing* must take the same branch, restore TCP's borrowed echo/CR
  values and end a file transfer.
- **Five keys are written in a case their own readers do not use**
  (`Historylist`, `Metakey`, `X/Y/ZmodemRcvCommand`) — harmless only because
  `GetPrivateProfile*` matches case-insensitively and `Ini` reproduces that.
  A byte-comparing lookup silently loses four settings.

The proxy — a Winsock hook upstream, so it has no seam at all:

- **TTProxy is 2,155 lines mostly about recovering a host name** from a
  `sockaddr` (it hooks `connect`, `gethostbyname` and eight more).
  `proxy.rs` is the four `begin_relay_*` functions and nothing else — the
  transports here already know where they are going.
- **The hook is also why TTSSH has no proxy** — hence one on `SshParams`
  here. The handshake runs on a *blocking* socket even for SSH
  (`spawn_blocking` + `client::connect_stream`): one implementation of the
  four wire formats, no async copy to drift.
- **`[TTProxy]` strings are C-escaped and quoted; `[Tera Term]`'s are not**
  (`YCL/IniFile.h:258`). Load-bearing: the quoting preserves trailing spaces
  that `GetPrivateProfileString` would trim. The schema's `string_esc` is
  that layer; the generator refuses a section mixing it with `string`.
- **The escaping is C's with two departures, both in the decoder**: `\x` is
  accepted, never produced (writer emits three-digit octal); an escape
  decoding to NUL is left as written (`StringUtil.h:255`). `\t` *is*
  recognised, so a hand-edited `C:\temp` comes back with a tab in it.
- **The four relay defects are documented in the schema and the module**
  (items 33–36 in `docs/upstream-bugs.md`) — they are where somebody
  diffing the implementations will think this port is wrong.
- **An unrecognised `ProxyType` is a direct connection** — reproduced (the
  schema's ordinary enum rule), but it is the one place that rule has a real
  cost; `none` is the spelling that says so on purpose.
- **Three parsers share the command line and TTProxy's runs first**
  (`TTXInternalGetSetupHooks` installs from the *end* of the plugin table,
  `ttplug.cpp:664`; TTProxy order 10 vs TTSSH 2500). They compose by
  blanking what they consumed out of the line — `cmdline::parse_all` is the
  only way to ask.
- **A bare `socks5://p:1080/realhost` token is a proxy *and* a host name**
  (`TTProxy.h:181`, copied into `ts.HostName` only if `_ParseParam` found
  none). A rule about two parsers — `parse_all` owns it.
- **`proxy` matches case-insensitively; `ssh` does not.** And the `=` is
  tested at a fixed offset (`option[6] == '='`), so exactly one five-letter
  word reaches the arm; `/noproxy` is only tried when it did not.
- **An unrecognised scheme leaves the configured proxy alone; a recognised
  scheme yielding no real host switches it off** (`TYPE_NONE` overwrite).
  `/proxy=socks5://p:1080/` disabling the proxy is the one parser behaviour
  this port does not reproduce — defect 37 in `docs/upstream-bugs.md`.
- **`/proxy=` replaces the whole record** — a file's `ProxyUser` does not
  survive a `/proxy=` that named no user. Right, and unlike everything else
  in this parser.
- **There are two log directories and their names do not distinguish them.**
  `ts.LogDirW`/`GetLogDirW()` is the *program's* (`%LOCALAPPDATA%\teraterm5`,
  no key moves it): `TELNET.LOG`, the six protocol logs, the proxy
  `DebugLog`. `GetTermLogDir` is the *terminal's* (`LogDefaultPath`, then
  `FileDir`, then the first). They coincide exactly when neither key is set.
  `logname::program_log_dir` / `logname::term_log_dir`.
- **`DebugLog` is the only thing that can see a proxy handshake fail** (it
  all happens before the terminal has a session). Record form follows the
  relay: binary for SOCKS (`sendToSocket`), quoted text for HTTP/telnet
  (`sendToSocketFormat`). Reproduced byte-exactly so traces can be compared
  against Tera Term's — "improving" the format costs that. Credentials are
  in it.

The local pty:

- **Wine's `Z:` drive makes Unix-only fixtures pass in a Windows binary**
  (`/bin/sh`, `/tmp` resolve). Cross-platform shell tests use `cmd.exe` and
  `std::env::temp_dir()`; serial tests compare against the enumerator.
- **Hold the slave end open and nothing ever ends** — the master never sees
  the hangup. `pty/mod.rs` drops `pair.slave` immediately.
- **`portable-pty`'s unix reader maps `EIO` to `Ok(0)`** — a dead shell
  looks idle, and the permanently-readable fd makes `QSocketNotifier` fire
  forever: a terminal at 100% CPU. Read/write the master's raw fd instead.
- **The exit status dies with the child handle** — `Transport::closing_note`
  is asked *before* the transport is dropped.
- **`std::process::Child` does not reap on drop** — `Drop` closes the master
  (the SIGHUP), waits briefly, then SIGKILLs; both waits bounded.
- **`portable-pty` sets no `TERM`** — set it and `COLORTERM` explicitly;
  remove `LINES`/`COLUMNS` (the winsize is the truth).
- **`portable-pty` drags in `serial2`.** Accepted — it buys the
  `setsid`/`TIOCSCTTY` dance and ConPTY. Don't hand-roll the pty.
- **ConPTY's pipes are synchronous `CreatePipe` handles** — never touch them
  on the UI thread. Windows owns a blocking worker per direction, a bounded
  1 MiB queue and a manual-reset event; don't replace the event with a timer
  or unbound the queue.
- **`PSEUDOCONSOLE_INHERIT_CURSOR` opens with a `CSI 6 n` conversation** a
  raw transport test must answer (a real `Session` does, via the engine).
  Wine 9 rejects `--inheritcursor` and closes the pipe empty — a Wine gap,
  not evidence.
- **ConPTY's output pipe belongs to the console host, not the child** — the
  child can exit and the reader blocks in `ReadFile` forever.
  `ClosePseudoConsole` (dropping the master) is what ends it, and it flushes
  first, so trailing bytes keep their place; it is on `tick` because a
  silent exit produces no wakeup. Wine cannot see any of this.

SSH:

- **The SSH tests need `--test-threads=1`** — not the rig: OpenSSH
  `MaxStartups` (and dropbear, lower) randomly refuse concurrent
  unauthenticated connections, which reads as a flaky connection bug.
- **`~/.ssh/config` takes the FIRST value for a keyword** — a `Host *` block
  at the top overrides everything below, silently. `IdentityFile` is the one
  exception (accumulates).
- **The alias list is drawn from `/etc/ssh/ssh_config` too, and a Linux
  desktop's already names two hosts that cannot be dialled** — systemd ships
  `Host .host machine/.host` with a `ProxyCommand` onto an `AF_UNIX` socket,
  and neither name has a wildcard in it to catch. `aliases()` therefore asks
  `resolve()` and drops anything whose effective first `ProxyCommand` or
  `ProxyJump` requires a relay; either directive's `none` value is a direct
  connection and stays. The question a dropdown entry has to answer is not
  "is it a name" but "can this program open it".
- **The `known_hosts` algorithm name comes from the key blob, not
  `PublicKey::algorithm()`** — an RSA host verified via `rsa-sha2-512` is
  recorded as `ssh-rsa` (RFC 8332); using the negotiated name reports every
  RSA host as unknown.
- **`check` must read every file to the end** — an `@revoked` entry further
  down (or in the second file) must overrule an accepting line.
- **`best_supported_rsa_hash()` returns `Result<Option<Option<HashAlg>>>`**
  and each layer means something different; `.ok().flatten()` type-checks
  against the wrong one.
- **`tt_session_pump` returns the moment the line is quiet** — "waiting" by
  pumping in a loop spins. Wait on the descriptor.
- **A Qt dialog spins a nested event loop** — the `QSocketNotifier` fires
  again while a host-key prompt is open. `Session::m_sshWaiting` stops the
  re-entry (which invalidates the borrowed strings the dialog shows).
- **A test against `127.0.0.1:2222` reads the developer's own
  `~/.ssh/known_hosts`** unless `TtSshParams::known_hosts` points at a
  scratch file.
- **`QWidget::grab()` on a never-shown dialog renders before layout** — one
  `adjustSize()` fixes the overlapping labels.
- **Qt caps a window's *initial* size at two thirds of the screen** — a size
  test must `resize(sizeHint())` first or it measures the screen.
- **`QTabWidget` caches its page's size hint** — `TerminalView`'s
  `applySettings`/`applyFont` call `updateGeometry()` so the invalidation
  reaches the window.
- **...and so does every `QWidgetItem` between the terminal and the window,
  which is why `TerminalPage::sizeHint` is composed by hand.** `updateGeometry`
  invalidates the item holding *that* widget; put the view inside a row widget
  and the page's layout is holding the row's item, which nothing invalidates.
  The page then quotes the 80x24 it was constructed with — settings are loaded
  after the pages exist — so a configured 100x30 window opens at 80x24 and
  `TerminalSize` follows it down at the next save. The view's own hint read
  900x630 while the layout above it read 720x525; ask the *item*
  (`layout()->itemAt(i)->sizeHint()`), not the widget, or the numbers agree and
  the window still comes out wrong.
- **...and that hint must never be the size `refit` measured.** When the window
  cannot hold the configured terminal — a wide font, a small screen, a capped
  window — the layout hands the view its share of the *shortfall*, so the view's
  width is a function of its own hint. Answering `sizeHint` with the live grid
  closes the loop: refit shrinks the grid, the hint follows, the layout
  redistributes, the next resize event measures a different column count. It
  wobbles by a cell rather than settling. `TerminalView::m_hintCells` moves only
  from the configured size, in `applySettings`.
- **Anything that relays out the window's chrome resizes the terminal, and
  `ClearOnResize` turns that into a clear.** The terminal is fitted to whatever
  width is left in whole cells, so a few pixels of chrome either way is a
  column, and a column is a real `Grid::resize` — which with the flag on scrolls
  the page into history (`buffer.c:5028` puts the clear outside the
  size-changed `if`). `reloadQuickButtons` runs on *every* settings change and
  rebuilt every button widget, so toggling line edit blanked the screen. Hence
  `QuickButtonBar::setButtons` returns early on an unchanged list — the same
  load-bearing early return as `PageStatusBar::setLogging` and
  `ConnectBar::Entry::operator==`, and the same rule about the comparison
  covering every field. It cannot be `m_buttons.isEmpty()`: the empty panel
  still builds its `+`.
- **...and a `QDockWidget` beside the terminal is that trap with a handle on
  it.** A dock separator divides the client area, so dragging the quick-button
  panel took its pixels off the terminal — and `Grid::resize` *truncates*,
  page and scrollback alike, so a drag out and back destroyed the text past
  the narrowest point for good. `ClearOnResize` only made it louder; the loss
  is there with the flag off. There is no dock feature that turns the
  separator off (the flags are Closable/Movable/Floatable/VerticalTitleBar),
  which is why the panel is a plain widget in a central `QHBoxLayout`.
  **The rule for any chrome that can be resized beside a terminal: move the
  window's outer edge, not the terminal's.**
  `MainWindow::resizeQuickPanel` clamps against `QScreen::availableGeometry`
  and `frameGeometry` so the window never has to steal, and holds every page's
  grid across the change — `setFixedWidth` defers its layout while a top-level
  `resize()` is a request a compositor answers later, so the two straddle a
  pass in which the view is narrower and the window has not caught up.
  `TerminalView::setGridHeld` also records whether it *swallowed* a refit: a
  geometry change delivered during the hold never repeats, so that one is
  answered on release and a hold that swallowed nothing is left to the resize
  event still coming.
- **...and that rule is also why the panel has no drag handle.** One was built
  and taken out. With the window's left edge pinned, growing the panel by N
  grows the window by N — so the handle's screen position never moves and the
  window's far edge shoots out instead, which is the honest rendering of the
  rule and nothing like what a splitter feels like. The two ways to make it
  feel right are a rubber band that follows the pointer with one apply on
  release, or growing leftward — and `QWidget::move()` is silently ignored on
  Wayland, so the second degrades to the first behaviour without saying so.
  `window.quick_buttons_width` in Setup is the width instead, and it reaches
  what a handle could not: a maximised window, the keyboard, a macro. Before
  adding a handle to anything else beside the terminal, work out which edge
  moves.
- **Showing that panel is a resize too, and the window-grow arm cannot see
  it.** `onSettingsChanged`'s arm runs while the panel is still hidden, so it
  finds nothing to absorb, and it is gated on `count() == 1 && Single` — so a
  panel beside a tiled grid was never covered at all. Anything that appears or
  disappears beside the terminal goes through the same absorb helper, not
  through that arm.
- **...and a column of widgets beside the terminal decides the window's
  *height* unless something stops it.** A `QBoxLayout`'s minimum is the sum of
  its children's, and `QToolButton::minimumSizeHint` is its natural height —
  so the quick panel's minimum grew with the list: twenty buttons made the
  window at least twenty buttons tall, changing to a page with more of them on
  it grew it again, and past the point where the screen ran out Qt shared the
  shortfall among the items and every button became a sliver. Both symptoms,
  one missing viewport. The fix is not the scrollbar but the **size hint**:
  `ButtonScroll` asks for its content's width and *no height at all*, so the
  terminal goes on deciding. A scroll area's own hint is its content's height
  and would put the panel straight back in charge, and its own
  `minimumSizeHint` is room for a scrollbar and a frame, which would silently
  become the narrowest the panel could be — both are overridden. The same
  answer is waiting for anything else that grows without bound beside a
  terminal.
- **A rebuilt `QObject` can land on the freed one's address**, so
  `CHECK(thing() != before)` after a rebuild is a comparison against freed
  memory that passes on the allocator's habits. `an_unrelated_setting_leaves_
  the_buttons_alone` had it and went green for a year; it broke on a change
  that only reordered the deletes in `clearContents`, which reads as a panel
  that stopped rebuilding. `QPointer` answers the question actually being
  asked — was it destroyed — and never dereferences the corpse.
- **Removing `QMainWindow::statusBar()` silently kills every `setStatusTip`** —
  the `QEvent::StatusTip` arm of `QMainWindow::event` needs a bar to show it in,
  and with none the event falls through and is dropped with no warning.
  `MainWindow::event` answers it itself. And `statusBar()` *creates* one on
  first call, so a single stray call — a test's included — reinstates the
  chrome and moves every size hint; ask `findChild<QStatusBar *>()` instead.
- **Anything constructing a `MainWindow` reads the developer's own
  `sterna.ini`** — terminal size and title included. `bench_shell` and
  `cmdline_test` call `QStandardPaths::setTestModeEnabled` before
  `QApplication`.
- **`qWarning` does not reach stderr on Fedora when stderr is not a
  terminal** (journald build) — the message goes to the journal, silently,
  precisely in the windowless-launch case it was written for.
  `QT_FORCE_STDERR_LOGGING=1` proves it in one run; anything the user must
  see uses `fprintf(stderr)`.
- **`QToolBar::clear()` removes its actions and does not delete them**, and
  `addAction(text)` parents them to the bar — so a rebuilt toolbar keeps every
  previous action alive as a child, holding its shortcut and answering
  `findChild` first. The symptom is a widget that stops following the session.
- **...and the other half: deleting a `QAction` does not delete the button
  showing it.** A `QToolButton` whose default action has been destroyed stays
  in the layout still painting the caption that action last gave it. A partial
  rebuild that deletes actions and only the widgets it tracks therefore *grows*
  one more of the untracked ones each time — `QuickButtonBar::rebuildPageColumn`
  deletes `m_widgets`, one slot per button, and the panel's **+** is not a
  button, so every page switch added another **+**. No assertion saw it and the
  screenshot did: **when a rebuild is partial, list what it leaves behind, not
  what it takes.**
- **`QComboBox::findData` compares `QVariant`s, so the type it was stored as is
  part of the key.** `addItem(label, int)` then `findData(quint32)` is a lookup
  that can answer -1 for a value that is there, and the fallback is a field
  quietly showing the wrong row — which the next edit then writes.
- **A `QToolBar` cannot have a button wider than its own caption.**
  `QToolBarLayout` sizes each item to its text and centres it across the bar's
  thickness, and it ignores the button's size policy in that direction — so a
  panel dragged wider puts every new pixel in the margin. The one lever that
  moves it, a minimum width per button, raises the bar's *own* minimum with it,
  and the panel beside it can then grow but never shrink: a ratchet, not a
  layout. Measured both ways. `QuickButtonBar` is a plain widget and a
  `QBoxLayout` for that reason, and the same answer is waiting for anything
  else that wants a toolbar to fill a panel.
- **A combo popup opens under the pointer, so the release that opened it is a
  choice** — `activated` arrives without anybody having chosen anything, and a
  row that connects makes one click on the arrow dial a host. Choosing fills
  the field; a separate commit acts. Its sibling: **a toolbar action whose text
  changes width reflows the whole bar**, and an expanding widget beside it
  absorbs the difference — Connect/Disconnect resized the destination box until
  the button reserved the longer word.
- **`ConnectBar::setRecents` is on the connect path** — `rememberRecent` calls
  it after every successful open — so anything added to `composeList` lands
  between the connect and the first prompt, which is the race `render_test`
  already carries scar tissue for. The busy scan therefore runs on
  `showPopup` only, and every other rebuild reuses the last answer.
- **A connect makes its page before it makes its connection** — `ensureIdlePage`
  runs at the top of every `connectX`, so `activatePage` and everything hanging
  off it fires on a page connected to nothing, in the middle of the connect
  path, and again for a connect that then fails. Hence
  `refreshConnectionSelector` leaves the destination field alone for a blank
  page: emptying it there is emptying the field New tab is about to connect
  *from*, and the one holding the host somebody mistyped.
- **`ConnectBar::Entry::operator==` must compare everything that reaches the
  widget.** `rebuildList` returns early on an unchanged list, so a row whose
  *state* changed but whose text did not never repaints. Busy state is
  deliberately a field rather than part of `text`: bake it into the words and
  the comparison catches it by accident, which leaves the property untested.
- **A `QAction` shortcut outranks `TerminalView::keyPressEvent`**, silently, so
  every shortcut installed on the window is a key the host stops receiving —
  and `Shift+F1`..`F12` are ordinary `KEYBOARD.CNF` bindings *and* F13-F24 to
  the far end. `TerminalView::scanForSequence` is what asks the core whether a
  sequence is already spoken for; quick buttons warn and do not refuse.

Measuring anything:

- **`QFile` cannot read `/proc` and does not say so** (`atEnd()` answers
  from a size of 0) — the measurement comes out a confident `0.0 MB`.
  `bench_shell` uses stdio.
- **`TerminalView`'s 8 ms frame floor is load-bearing and only Wayland hides
  its absence** — without it xcb drops from 36 MB/s to 4 (one frame per
  8 KB read). A headless throughput figure understates the desktop ~4x, so
  `bench/baseline.json` records platform *and* Qt version.
- **Do not "fix" that with a `tt_session_pump` budget** — serial and telnet
  read with 50 ms timeouts, so a budgeted pump blocks the UI thread.
- **A Wayland client cannot place *or size* its own window on demand** —
  `QWidget::move()` is silently ignored and `resize()` is acked by the
  compositor later, so a test that resizes and then measures can have the
  resize land inside the next thing it does and blame that instead.
  `QWidget::move()` "fails" for `/X=120` the same way. Run `cmdline_test`
  under offscreen or xcb (CI does). Applies to anything asserting position.
- **A Wayland compositor stops frame callbacks to a surface it thinks
  hidden** — short-lived probe windows paint a fraction of their frames; any
  paint-waiting measurement has to tolerate it.
- **A pty's prompt arrives when it arrives** — asserting the exact contents of
  row 0 right after `connectPty` is a race with the shell, and *anything*
  added to the connect path (a settings write, a `/dev` enumeration) changes
  who wins. `render_test`'s line-edit case read `still here` for a year and
  then read `nata@natux:~$ still here`. Assert `contains`, and let the
  scrollback length carry the "nothing scrolled" half.
- **`run_diff.sh` cannot run from a git worktree** — the oracle compiles
  `../teraterm` relative to the *checkout*, and under `.claude/worktrees/x`
  that is nothing; the failure is `No rule to make target
  build/patched/buffer.c`, which reads as a broken Makefile. Symlink the
  reference beside the worktree for the run, or run the gate from the main
  checkout.
- **The calibration loop corrects for a slower machine, not a busier one** —
  the first baseline was recorded during a build and nothing flagged it.
  Re-record on a quiet machine and read the file before committing.

Serial:

- **`/BAUD=` and `/SPEED=` select the serial port type** (as `/C=` and the
  `/CDATABIT=` family do), so word order against a bare host name decides —
  and a test doing it opens whatever is plugged in. Options before the host
  name when the point is that they reached the settings.
- **A `connect` is serviced on the frontend's thread** — a transport that
  blocks while opening takes the event loop (and a test's watchdog) with it.
  A hang is never "the macro is slow"; ask what the loop is inside.
- **`tcsetattr` returns success if it applied *any* of it** — the FTDI takes
  `CS5` and transmits eight bits. Read settings back (`set_data_bits` does).
- **`serialport-rs` calls a busy port `ErrorKind::NoDevice`** — both that and
  BrokenPipe-means-disconnect are wrapped in `tt-conn/src/error.rs`.
- **Never `tcdrain`/`FlushFileBuffers` on a responsive thread** — flow
  control can hold the queue forever. `SerialConn::flush` polls `TIOCOUTQ` /
  `COMSTAT.cbOutQue` with a timeout; the latter comes via `ClearCommError`,
  so a `CE_BREAK` seen there must be retained for the receive path.
- **A Win32 COM handle does not become readable by waiting on it** — the
  wakeup duplicates the handle into a `WaitCommEvent` worker (upstream's
  `CommThread`/`ReadEnd` handshake); cancel with `SetCommMask(handle, 0)`
  before the original dies. Wine rejects port setup (`ERROR_NOT_SUPPORTED`);
  `tests/serial_windows.rs` needs native Windows.
- **...and an adapter unplugged on Windows therefore tells nobody.** Three
  silences, all reported as one bug — a session that said it was connected
  until somebody typed, and then answered `os error 22`. (1) `ERROR_BAD_COMMAND`
  (22) and its six relatives have no Rust `ErrorKind`, so a failed write came
  back `Uncategorized` and never reached `is_disconnected` —
  `windows_device_gone` in `error.rs` is the list, and `ERROR_INVALID_HANDLE`
  is deliberately not on it. (2) The wait worker's `break`s ended the only
  thing that can wake the frontend, and removal arrives as an
  `ERROR_OPERATION_ABORTED` completion with an empty mask, which is
  indistinguishable from our own `SetCommMask(handle, 0)` — so the loop has
  one exit and it always knocks. (3) Nothing asks at all on a quiet line:
  `Transport::tick` probes with `GetCommModemStatus`, **not** `ClearCommError`,
  which clears the errors it reports and would eat a break. `Session::tick`
  routes the answer to `line_went_away` and `Session.cpp`'s tick drains, or the
  events sit in a queue whose notifier went with the port.
- **And that worker is why the handle must be opened `FILE_FLAG_OVERLAPPED`,
  which `serialport-rs` does not** — a synchronous file object serialises its
  I/O and a duplicate shares the file object, so the worker's pending wait
  holds every `WriteFile` behind it until a byte arrives, which on an idle
  line is for ever. The port opens, the DCB reads back, the window says
  connected, and it freezes on the first keystroke — a talkative device hides
  it completely, and `COMMTIMEOUTS` cannot bound it because the request never
  reaches the driver. The comm API wrappers are synchronous whatever the
  handle is, so open, apply and the modem lines work and only the data path
  hangs; `COMPort`'s `Read`/`Write` pass a null `OVERLAPPED` and are the one
  part of it unusable here. Every operation in `serial/windows.rs` is
  overlapped, and each is driven to completion or cancelled *and reaped*
  before its `OVERLAPPED` leaves the stack.
- **On Windows a zero-byte read is a timeout; on Unix it is the far end
  leaving** — a COM handle has no EOF, and `serialport-rs` hid that behind
  `ErrorKind::TimedOut`. Reading the handle directly and keeping the shared
  `n == 0` arm drops the session on any quiet line.
- **The driver's write timeout is the port's read timeout** — `set_timeout`
  writes one number into both halves of `COMMTIMEOUTS`, so a write stalled by
  flow control ends there, as a short count the pump retries, and not at the
  caller's deadline. The overlapped wait is the outer bound, not the tighter
  one.
- **`ClearCommError` clears the error it reports** — `bytes_to_read()`
  between a notice and the read can eat a break. Windows reads up to the
  64 KiB input-buffer size; and never feed its bytes through the Unix
  `PARMRK` decoder (an ordinary `0xFF` would hang as a partial escape).
- **The portable serial setters describe less than half a Win32 DCB** and
  apply piecemeal. Windows builds upstream's zeroed DCB, one `SetCommState`,
  reads every controlled field back; DTR toggle is rejected up front (no
  such Win32 mode). Don't reduce the readback to a cached comparison.
- **`serialport-rs` throws away the Win32 COM open error code** (everything
  is `NoDevice`; `Path::exists("COM3")` is false regardless). Windows opens
  with `CreateFileW` directly: access-denied/sharing-violation is busy,
  missing/invalid-name is disconnected.
- **Enumeration is not a shortlist** — an ordinary desktop answers
  `tt_serial_enumerate` with thirty-three ports, thirty-two of them
  motherboard `ttyS` UARTs with nothing on the far end. Any list a user picks
  from has to sort the real adapters first and bound the tail, or the one
  adapter they own is buried mid-alphabet. It is not free either: don't call
  it on the connect path to render a label.
- **"Who has this port open" and "will opening it fail" are different
  questions**, and only the first can be answered without opening it.
  `serialport-rs` takes `TIOCEXCL` *and* an exclusive `flock` (`posix/tty.rs:131`),
  but a plain `cat /dev/ttyUSB0` takes neither and does not stop a second open
  — so `serial::inuse` greys a row and must never gate Connect. The two Linux
  sources see different halves: `/proc/locks` is world-readable and names
  `flock` holders of any uid (every Sterna window included), `/proc/<pid>/fd`
  names this user's own processes lock or no lock. A root-owned holder that
  took no lock is invisible to both, so the modal error stays as the backstop.
- **Match a device by `st_rdev`, never by its name** — `/dev/ttyUSB0`,
  `by-path` and `by-id` are three names for one node and the picker stores the
  second; and `stat` the `/proc/<pid>/fd` entry rather than reading the link,
  because the text lies about a renamed or deleted node.
- **Probing a port by opening it raises DTR for the life of the probe** and
  drops it on close — measured on the rig, where `ttyUSB0`'s DTR reaches
  `ttyUSB1`'s DSR. That reboots an Arduino-style board and drops a modem's
  carrier, which is why nothing in the busy check opens anything.
- **A test byte with bit 7 set cannot tell 7 data bits from 8** — at seven
  bits the stop bit lands in bit 7. Use `0x25`.
- **Ports left in flight leak into the next test** — bytes already at the
  adapter arrive in the next test's first read. `loopback.rs` settles the
  rig between tests.
- **Apply one field, not a settings-derived parameter set** — the settings
  need not describe the open port (`--baud` opens from a `SerialParams` the
  settings never saw), so a `setbaud` rebuilt from settings once moved a
  115200 port to 9600 and reported as a flow-control failure.
  `Session::reset_serial` edits the live parameters.
- **A speed-changing test must settle the rig** — wrong-baud garbage arrives
  as framing errors, `detect_break` turns those into `BadByte` events, and a
  stray `ESC ]` opens an OSC that eats the rest of the test. Assert on the
  far end's bytes, not the screen.
- **`--test-threads=1` is per test binary and cargo runs binaries
  concurrently** — two hardware suites on one rig; run one package at a
  time. There is no cargo flag for this.
- **`FlowCtrlRTS`/`FlowCtrlDTR` default to sentinel `-1` = "derive from
  `ts.Flow`"** (`ttset.c:2034`, `:2042`; `FlowCtrl` is read earlier, so the
  derivation sees the file). Taking `-1` as a value holds the lines low.
- **One out-of-range number discards every serial setting**:
  `CommResetSerial` never checks `SetCommState` (`commlib.c:240`), so
  `FlowCtrlRTS=9` silently keeps the old baud/parity/stop. Not reproduced.
- **Upstream's save pins a derived control line** (writes the resolved
  number back). This port keeps the `-1` — only that keeps the derivation
  alive; either file opens correctly in either program.
- **`RTS_CONTROL_TOGGLE` is RS-485 keying** (`TIOCSRS485` on Linux); the
  FTDI answers `ENOTTY`, so nothing on the rig can test it.
  `PinControl::Toggle` leaves the line where the kernel put it.
- **`ClearComBuffOnOpen` gates the purge on open only** — Control > Reset
  port purges regardless (`vtwin.cpp:4913`). Only testable on real hardware.
- **`SendBreakTime` is the only break length there is** — no per-caller
  `ms` parameter; this port once had three different values.
- **"Is the node there" and "is there anything at this path" are two
  questions, and `tt-conn` answers them separately on purpose.**
  `serial::present` follows a symlink and insists on a character device, and on
  Windows asks `QueryDosDeviceW` rather than stat'ing (`Path::exists("COM3")`
  is false for a working port) or enumerating (upstream's `CheckComPort` runs
  SetupAPI over all thirty-three ports a desktop has). `Error::from_open` keeps
  its plain `Path::exists`: making it strict turns `cannot open
  /home/me/notaport: Is a directory` into "the device disconnected", which says
  nothing about what to fix. Do not unify them.
- **The reopen record has to be taken before `self.conn = None`**, beside
  `closing_note` and for the same reason — the live `SerialParams` die with the
  port, and a frontend remembering what it passed to `connect` brings a session
  back at the speed in the settings file rather than the speed a macro's
  `setbaud` left it at. `Transport::reopen_target` is that moment;
  `Session::line_went_away` is the one place both disconnect paths reach it.
- **`Session::tick` is not where the auto-reopen runs, and the reason is a Qt
  timer type.** The shell's tick is `Qt::VeryCoarseTimer` (`Session.cpp:76`),
  which rounds to whole seconds, and `AutoComPortReconnectDelayNormal` ships at
  500 ms — riding it would round a setting to twice its value and say nothing.
  The core owns the instant (`tt_session_reopen_deadline_ms`) and
  `Session::m_reopenTimer` owns the sleep, which is `m_xferTimer`'s
  arrangement. `tick` keeps its "no-op with nothing connected, raises no
  events" contract.
- **...and that arrangement makes every deadline the core hands out a *when*,
  never a *how long from now*.** `Session::rearm` re-reads the deadline and
  restarts one single-shot timer, and it is called from far more than the pump
  — `Session::mouse` calls it on **every mouse-move event**, and
  `TerminalView` has mouse tracking on for the URL cursor. So a state whose
  deadline is an interval measured from the moment it is asked gets a fresh
  full wait sixty times a second, and never fires. `Reopen`'s indefinite
  `Waiting` had this: the poll backs off to two seconds after half a minute,
  so moving the pointer over a terminal that was waiting for its adapter
  stopped it noticing the adapter until the pointer stopped. `State::Waiting`
  carries `next` as an `Instant` for that reason, and `Reopen::deadline` is
  idempotent in all four states. Anything else that grows a deadline owes the
  same property — the frontend cannot supply it, because it has no way to know
  which of the answers it just got was a countdown and which was a fresh wait.
- **A reopening session must not count as *connecting*.** `ensureIdlePage`
  (`MainWindow.cpp:860`) opens a **new tab** for a session that is connecting,
  so folding `isReopening` into `isConnecting` takes somebody who gave up
  waiting and clicked Connect to a fresh tab — throwing away the scrollback the
  feature exists to keep. It is a separate predicate, and File > Disconnect is
  the one command that calls a wait off.

Settings — all of it out of `ini-audit/`:

- **`GetOnOff` is default-biased** (`ttset.c:344`): default on → anything
  but literal `off` is on; default off → only literal `on` is on. So
  `Xterm256Color=1` is on and `Aixterm16Color=1` is off. It also reads into
  a four-byte buffer, so `offline` is `off`. **When a setting looks boolean,
  find its default before deciding what a value means.**
- **`GetPrivateProfileString` strips one matched pair of quotes**, single or
  double; unmatched or interior quotes are kept.
- **`Key=` is an empty string, not the default** — upstream leans on it
  (that is how `ts.BSKey` reaches its `else`). Don't collapse empty into
  absent.
- **The first duplicate key wins and a duplicate section is not merged.**
- **A comment is only a comment to enumeration** — `;A=1` is an entry whose
  key is `;A`; a line with no `=` is not an entry at all.
- **Two recorded answers are Wine's alone** (line-ending rewrite on write,
  `[ s ]` → `[s]`) — in `ini-audit/divergences.txt` as not reproduced;
  re-run on Windows in Stage 3.
- **`gen-settings` pipes through `rustfmt` and that is load-bearing** —
  `cargo fmt --check` covers `generated.rs`, and where a line wraps depends
  on the width of a setting's name, so do not emit pre-formatted text.
  `cargo test -p tt-config` therefore needs `rustfmt` on `PATH` (it says so).
- **`TerminalID` is `strcmp`; every other enumerated setting is `_stricmp`**
  (`tttypes_termid.cpp:60`), and it never fails — `vt320` is a VT100
  forever. Hence `enum_exact`; the table also holds `VT220` and lower-case
  `dumb`.
- **`TermWidthMax` is 1000 and `TermHeightMax` is 500** (`tttypes.h:633`).
- **`ttset.c:615` bounds a size and does not clamp it** — at/below the floor
  takes the *default* (so `TerminalSize=0,0` is 80x24), above the ceiling
  takes the ceiling.
- **`ScrollBuffSize` is the whole buffer, page included**, and upstream
  grows it to hold the page (`buffer.c:641`, `:4983`); `Grid::scrollback_max`
  is `max(lines, rows) - rows`. The row ceiling is `ts.ScrollBuffMax`
  (`MaxBuffSize`), a different setting.
- **Applying settings overwrites what the host set, and that is upstream** —
  DECBKM/SRM/LNM assign `ts.BSKey`/`ts.LocalEcho`/`ts.CRSend` directly.
  `Vt::set_config` refreshes exactly those and leaves `LFMode` and
  `AcceptWheelToCursor`, which upstream keeps separately.
- **`TCPPort`'s default is `ts->TelPort`'s hardcoded initialiser (23), not
  the file's `TelPort`** — `TelPort=` is read 400 lines later
  (`ttset.c:966` vs `:1311`). When a default is another field, check the
  read *order*.
- **`Session::set_setting` takes the schema's dotted name** and answers
  `false` for anything else — `setecho` wrote the INI key `LocalEcho` and
  silently changed nothing for four commits. The file owns the INI key;
  everything above it says `terminal.local_echo`.
- **A key the schema invents cannot fail loudly** — four of the first 77
  were misspellings upstream never reads or writes.
  `tt-config/tests/upstream.rs` diffs both lists: **check a transcription by
  extracting both lists and diffing, never by reading.**
- **`int(lo..hi)` takes the default below its floor; `int_min(lo)` clamps**
  (`ttset.c:615` vs `:1822`). `ZmodemTimeouts`' second field floors at 0 —
  0 means "never time out"; floor it at 1 and a stalled ZMODEM over SSH
  gives up in a second.
- **The C field's width is part of the bound and runs first** — `WORD`/
  `BYTE` narrowing wraps before any `if`. `MaxComPort=-1` is 65535 then 4096
  (`ttset.c:1218`); `AlphaBlend=-1` is 255 (its visible clamp at `:1467` is
  dead code behind the narrowing). Check the field type in `tttypes.h`
  before believing the `if`. Seventeen schema rows had this wrong; they were
  found by extraction and diff, not by reading.
- **`ComPort`'s ceiling is another setting and out-of-range resets to 1**
  (`ttset.c:1223` vs `ts.MaxComPort` at `:1218`) — in
  `Settings::normalize`, not the schema; clamping would open a different
  device.
- **`XmodemOpt`'s default is plain checksum** (the `else` of an `_stricmp`
  chain; upstream's writer emits a value its reader has no arm for). And
  XMODEM's binary flag is its own: `XmodemBin` ships on, `TransBin` off,
  text = `1 - XmodemBin` (`filesys_proto.cpp:324`). Don't fold them.
- **An absent key and a misspelt one can be two different settings** —
  `AcceptTitleChangeRequest` absent is `overwrite`, misspelt is **off**
  (`ttset.c:1568`). The schema's `*` arm (`off/*=Off`) exists for this.
- **`MaxBuffSize` caps the terminal's rows too** (`buffer.c:511`, `:4977`) —
  `MaxBuffSize=30` is a thirty-row terminal. Rows are cut first, total
  after. And under 24 it takes the default 10000 (`ttset.c:615`'s bound).
- **`TerminalSpeed`'s second field defaults to its first** (`GetNthNum`
  answers 0; `ttset.c:1946` assigns the input speed) — held as a string and
  parsed in `tt-session::open`; no schema type can say it.
- **`TermType`'s default is plain `xterm`** (`ttset.c:961`) and TTSSH reuses
  it for the `pty-req` — one key for telnet *and* SSH. Not
  `xterm-256color`.
- **`ISO2022ShiftFunction`'s list starts from nothing** — the default
  applies only when the key is absent, so `-SS2` disables every shift. Same
  shape: `TabStopModifySequence` (whose `on` never reaches the list arm).
- **`EnableANSIColor` is a rendering gate, not a parse gate** — the cell
  still stores the colour; only DECRQSS' SGR and the termcap `Co` query show
  it on the wire.
- **`MaximizedBugTweak=on` is a numeric alias for 2**; everything else goes
  through `atoi` into a WORD (`ttset.c:1527`). Not a bool.
- **`DebugModes` can turn `Debug` back off** — an unrecognised list clears
  it (`ttset.c:1798`). TTL's `setdebug` bypasses gate and mask.
- **The settings generator can be blocked by the stale file it replaces** —
  generate before wiring consumers; if crossed, run the previously built
  `crates/target/debug/gen-settings` once.

Scrollback and the wheel:

- **`BuffClearScreen` is a scroll, not an erase** (`buffer.c:4021`): `ED 2`
  moves the page into history and the differential dump could not see the
  difference — `--scrollback` can. The scroll region has no say, and
  `DECSET 1049` scrolls out on the way in **and** out (`vtterm.c:3044`,
  `:3202`), so leaving vim leaves two pages in history.
- **`ScrollWindowClearScreen` does not gate `ED 2`** — it decides only
  whether `ED 0` at home is *promoted* to a clear (`vtterm.c:1728`), which
  is what `ESC [ H ESC [ J` is.
- **`ClearOnResize` clears on a resize that changed no size**
  (`buffer.c:5028`, outside the size-changed `if`) — including upstream's
  own startup resize, which puts a blank page in history before a byte
  arrives; also why DECCOLM skips its own clear when the flag is on.
  **So every caller of `Grid::resize` needs upstream's own guard**, which
  lives one level up in `SetupTerm` (`vtwin.cpp:1396`): resize only when the
  configured size differs from the live one. `Grid::resize`'s early return
  cannot stand in for it — that return is conditional on the flag being
  *off*. `Vt::set_config` missed the guard and every settings change blanked
  the screen; the symptom was a *frontend* toggle (line edit) clearing the
  terminal, which points nowhere near the parser.
- **...and that guard only holds because a resize moves `TerminalSize` with
  it** — `BuffChangeTerminalSize` assigns `ts.TerminalWidth`/`Height` on its
  way out (`buffer.c:5022`), so the setting is a live variable upstream and
  not the file's snapshot. `Session::resize` and the `CSI 8 t` arm of
  `collect_window_requests` both write it back; without that, the settings go
  on saying 80x24 after a dragged window and *every* later settings change
  disagrees with the live size. The symptom is the same frontend toggle
  (line edit again) restoring the window to its default size.
- **The frontend cannot ask the grid whether a settings change moved the
  size** — the core applies the setting before `settingsChanged` reaches Qt,
  so the grid already matches and the answer is always no.
  `MainWindow::onSettingsChanged` compares the configured size against how
  many cells the *view* has room for; and when it does resize the window it
  must suppress the refit that follows, or the view's old geometry writes the
  old size straight back through `Session::resize`.
- **...and it must read the configured size *before* anything reacts to the
  settings.** `QLayout::activate()` sets its children's geometry there and
  then, and Qt delivers the resize event from that synchronously — so
  `TerminalView::refit` runs inside the call and `Session::resize` has written
  the new, smaller `terminal.cols` before the comparison below gets to read it.
  It then always finds the setting equal to what the view has room for and the
  window never moves. That is how the line-number gutter came to cost five
  columns permanently instead of widening the window: the setting had already
  followed it down. Anything else that takes room from the view on a settings
  change — a second gutter, a margin, a side panel — lands on this.
- **...and the same question asked about a *host's* resize has the same wrong
  answer.** `CSI 8 t` is applied by the core — `collect_window_requests` resizes
  the grid and then reports it — so `onRemoteResize` comparing the request
  against `Session::cols()` found them equal every time and returned before
  growing the window. The symptom is invisible in the obvious place: the
  terminal really is 132 columns, and 52 of them are off the right-hand edge
  until the next resize event refits them away. Compare against the view.
- **`TermIsWin` off means the window is a viewport, and the origin lives in the
  frontend** — the core has no idea any of this is happening.
  `TerminalView::m_originX` is upstream's `WinOrgX`, applied as one
  `QPainter::translate` in `paintEvent` and undone by one `gridPos` on the way
  in. Two places, not a dozen: anything new that turns a pixel into a column
  must go through `gridPos`, and anything that paints must do it inside the
  translate. The host's mouse reporting is the one that fails quietly if you
  forget — the selection is visibly wrong, a tracked click is merely in the
  wrong column.
- **Moving a `bool` default is a listed act, because `GetOnOff` is
  default-biased.** `tt-config/tests/upstream.rs`'s `DEFAULTS_MOVED_ON_PURPOSE`
  is the list and it is deliberately one key at a time: with a default of on
  anything but literal `off` is on, so flipping a default changes what
  `Key=1` means in a shared file. Two entries so far
  (`ConfirmPasteMouseRButton`, `TermIsWin`), each owing `docs/deviations.md` a
  paragraph about exactly that.
- **`AutoScrollOnlyInBottomLine` ships off** — output drags a scrolled-back
  view down by the *minimum* scroll (`buffer.c:3794`, `:3805`, `:3866`).
  And the cursor-following belongs to the feed, not to a settings change —
  hence `Session::reanchor_after_resize`.
- **`MouseWheelScrollLine` applies only to a notch that arrived alone**
  (`vtwin.cpp:2536`; coalesced notches scroll notches, not multiples), `0`
  and negatives mean one line, and over the title bar it is the opacity step
  (`:2500`).
- **`ScrollThreshold` is a repaint coalescer in lines** (`vtdisp.c:3132`) —
  the 8 ms frame floor in another unit. Carried, acting on nothing.

URLs — one plausible master switch is really three:

- **`EnableClickableUrl` does not enable URL recognition** — the write path
  always sets `AttrURL`; `EnableURLColor`/`URLUnderline` (both ship on)
  decide paint, `EnableClickableUrl` (ships off) gates only cursor + launch.
- **`MouseCursor` looks like an enum and is not** — the raw spelling is
  kept; unknown values change nothing (`vtwin.cpp:159`). The hand over a URL
  is temporary; moving away re-applies the setting, so don't hardcode
  I-beam.
- **A URL starting at buffer pointer zero loses its marking when it grows**
  (`buffer.c:2658`; `sftp://`/`tftp://` escape via their `ftp://` suffix).
  Case 130 pins it — do not replace the incremental detector with a regex.
- **A wrapped URL is copied with the clipboard setting** —
  `EnableContinuedLineCopy=off` inserts `CR CR LF`. `JoinSplitURL` and
  `JoinSplitURLIgnoreEOLChar` are read, written and never consulted.

The parser's own switches:

- **`CRReceive` has five values here and upstream's AUTO is not the one that
  ships.** A bare CR is a cursor motion far more often than a line ending — an
  interactive shell redrawing its prompt sends one per keystroke — and upstream's
  AUTO takes every one of them as a line ending for the whole session (defect 38
  in `docs/upstream-bugs.md`), so it puts each keystroke on a new line. It is reproduced exactly, and
  `DETECT` is Sterna's own and the default (deviation 9): the first LF resolves
  it — a CR immediately before means `CR LF` and the mode becomes `CR`, anything
  else means `LF` alone. VT and FF call `LineFeed` directly upstream and are not
  evidence. `Session::connect` clears the decision because a new far end need
  not agree with the last one; explicitly changing the mode clears it too, while
  applying an unrelated setting must not. Differential case 33 is AUTO, so it
  stays comparable; nothing in `oracle/` can run DETECT.
- **Debug display restores the wrong attribute** — `PutDebugChar` saves
  `svCharAttr` and restores `char_attr` (`charset.cpp:757`). Reproduced;
  the obvious fix changes what the next character looks like.
- **A broken multi-byte sequence is one U+FFFD per *byte*** upstream, one
  per run in vte (WHATWG). Fixed in `rewrite_c1` (it already tracks the
  sequence start). Case 128 stays divergent: a sequence cut by an OSC
  terminator is decoded at stream level here, string level upstream.
- **An OSC's string is everything after the FIRST semicolon**
  (`vtterm.c:5297`) — vte splits on all of them; `Vt::osc_string` is the
  join, bounded one byte *short* of `MaxOSCBufferSize` (`StrLen + 1 <`).
- **`=` is a private marker, not an intermediate** — vte reports `?`/`>`/`=`
  where intermediates go, so "any intermediate = unported" silently ate the
  tertiary DA. `CSI > Ps c` and `CSI = Ps c` insist on parameter zero.
- **DECSCUSR's space is a real intermediate and needs an explicit arm** —
  `csi_plain`'s refusal of intermediates is load-bearing. With
  `CursorCtrlSequence=on` it rewrites `CursorShape`/`NonblinkingCursor`
  themselves (`vtterm.c:3966`): read the live style, not the file settings.
  `KillFocusCursor` is separate (full-cell outline when unfocused, or none).
- **HTS is the one C1 that must not be folded to its 7-bit form** —
  `TABF_HTS7`/`TABF_HTS8` are separate bits, so `0x88` goes through raw to
  `Perform::execute` and the refusal stays in `rewrite_c1`.
- **`VTCompatTab` off: a tab breaks the line like a printed character; on:
  `buffer.c:5211` restores `Wrap` after the move.** CHT never sees the first
  half.
- **`BackWrap` lands on the right *margin*, not the last column**, and does
  not scroll (`vtterm.c:664`).
- **`LockTUID` defaults on, so DECSTUI does nothing as shipped**;
  `TerminalUID` is validated at both boundaries (8 hex chars, upper-cased;
  invalid keeps the old value) — validation at the boundary, not the schema.
- **`AutoInvoke`'s locking shift runs outside the switch and outside the
  ISO-2022 gate** (`vtterm.c:1409`) — `ESC ( Z` still invokes, and
  `ISO2022ShiftFunction=off` does not stop it.
- **`UseInvalidDECRQSSResponse` flips the digit and keeps the body**
  (`vtterm.c:4400`) — the one setting whose purpose is to lie.
- **`Perform::execute` is a control byte's only channel here, and it is not
  the byte stream.** A C0 *inside* a sequence does execute — `ESC [ 1 BEL m`
  rings the bell and still turns bold on, which is the same fact as the
  printer's `ESC [ 12 BEL m` trap — but `ESC` itself, a sequence's parameter
  and final bytes, and an OSC's terminating BEL never arrive; `rewrite_c1`
  folds 8-bit C1 before vte sees it; and DEL reaches `Perform::print` as a
  character rather than arriving here at all. That is the whole limit of
  `terminal.show_control_chars` (deviation 25), and the reason it is not debug
  display mode. **Anything that must see every byte wants a tap on the
  transport**, not this — the marks are also the one thing in the grid the
  clipboard, the printer's dump, Find, `ttctl` and DECRQCRA all have to skip,
  by `ATTR_CONTROL`, so a new reader of grid *text* needs that skip too.
- **A caret mark is ASCII because DejaVu Sans Mono has no Control Pictures.**
  `fc-match -f '%{family}\n' 'DejaVu Sans Mono:charset=240d'` answers a
  different family; CI's runner carries only `fonts-dejavu-core`, so `␍` would
  render from a fallback or as a box. `¶` (U+00B6) is there and is what the
  line-end mark uses. Check any new glyph this way before a pixel test depends
  on it.

The painter (the differential dump cannot see any of this):

- **`VTFontSpace` is four signed margins, not letter spacing** — left/top
  move the glyph, sums expand the cell; negative clamps are commented out.
  `DrawingResizedFont` stretches to `FontWidth`, not padded `CellWidth`.
- **Bold and underline each have a font switch and a colour switch**
  (`EnableBold`/`UnderlineAttrFont`; `EnableBoldAttrColor`/
  `UnderlineAttrColor`), independent, all four ship on. The attribute stays
  in the cell regardless.
- **`UseTextColor` repairs only three exact same-colour pairs after
  reversal** (fg 0, 7 or 15, `vtdisp.c:2542`); red-on-red stays invisible,
  and under selection/SGR 7/DECSCNM the repair uses the reverse pair even
  when `EnableReverseAttrColor=off`. Don't build a broad "ensure contrast".
- **`UseNormalBGColor` substitutes only an attribute pair's background**
  (reverse puts it in the foreground); a later explicit SGR background wins.
- **A highlight rule's bold and underline must not join `cell.attrs` before
  `Theme::resolve`'s pair chain** — upstream's bold/blink/underline each carry
  a *colour pair*, so OR-ing them in makes "underline this" repaint the text
  the configured magenta. A rule's mark reaches the font and the stroke
  (`paintsBold`/`paintsUnderline` take the combined word) and its colours are
  the only colours it decides; its `reverse` is the one attribute that joins
  the reverse count instead.
- **A highlight span applies after the `UseTextColor` repair**, not before —
  the repair tests the *cell's* two indices and would otherwise discard a
  colour the user asked for on a cell the host had made invisible.
- **Highlight matching runs while painting, and its per-line memo is keyed on
  a damage counter** — `Session::mark_damage` moves it, so a new path that
  edits the grid must go through that rather than pushing `Event::Damage`
  itself, or the screen keeps last frame's colours.
- **A `Session` in a test has connected to nothing, so its background carries
  `color.disconnected_shade`** — every `bgAt` and every `defaultBackground()`
  in `render_test` moves by 12%, on a change that looks like it only touched a
  disconnected window. `Harness` says `setConnected(true)` for that reason, and
  a test that builds a `MainWindow` of its own has to say it too. The shade is
  applied last in `Theme::resolve` and skips any background the host or a rule
  chose (the `hostBackground` flag) — so a bold run, whose *configured* pair
  carries a background of its own, shades with everything around it while
  `SGR 41` does not.

The colour OSCs (the whole family lives in `vtdisp.c`, which the oracle does
not compile — `stubs_manual.c` is the transcription; diff it, don't invent):

- **A host cannot read back a colour it just set** — `DispSetColor` writes
  the live pair, `DispGetColor` reads `ts` (defect 31 in `docs/upstream-bugs.md`). Only the
  palette round-trips (both halves are `vt->ANSIColor`), and Tek by
  accident. Why esctest's `ChangeDynamicColor` family cannot pass.
- **`XsParseColor` accepts `rgb:` case-insensitively and parses it
  case-sensitively** (`RGB:0/0/0` fails in silence). Two forms only;
  `#RGB` scales `<< 4` (0xF0, not xterm's 0xFF).
- **`OSC 10;a;b;c` walks its number along the list** (`vtterm.c:5156`) — fg,
  bg, then a cursor colour with no arm. `OSC 12`/`13`/`14`/`18` and their
  resets do nothing (`XtColor2TTColor` has no case).
- **`OSC 104;` is not `OSC 104`** — an empty parameter string resets entry 0
  alone; only a wholly absent one resets the table. `OSC 105`'s "all" is
  three colours, not the four `OSC 5` can set; `OSC 110-119` reads its
  parameter string as further OSC numbers.
- **`CS_UNSPEC` is a sentinel, not a flag** — `OSC 105;4294967295` is a bare
  `OSC 105`; an `Option` model loses that.
- **The termcap query answers from the colour flags and `EnableANSIColor`
  silences it** (`vtterm.c:4444`) — the one place that setting shows on the
  wire.
- **Applying settings does not refresh live colours upstream** (`#if 0` at
  `vtwin.cpp:1348`, kept for a theme feature this port lacks) — this port
  diverges deliberately; copying it makes the colour tab do nothing.

Window operations — the reports and the actions share one switch and nothing
else:

- **Reports are answered from a snapshot** — the frontend pushes
  `WindowMetrics` on every move/resize/state change and the engine reads
  what it was last told; the *actions* are a queue. Either one built the
  other way round does not work.
- **A frontend that pushes nothing gets a notional window, and the oracle's
  stubs answer the same numbers on purpose** (origin, 8x16 cells, 1920x1080
  work area) — so `esctest/run_diff.sh` adjudicates flags and parameter
  meanings, not furniture. Change both sides or neither.
- **`CSI 13 t` reports x then y; every size report is height then width** —
  and the sub-parameters swap meaning between 13 and 14 (13: 2 = text area;
  14: 2 = frame). Upstream's and xterm's.
- **An unknown sub-parameter answers nothing at all** (`default: return`) —
  `CSI 13;3 t` is silence, not a fallback.
- **`CSI 10 t` is maximise, not full screen** (upstream's comment admits
  it); 9 and 10 are one operation except 10 has a toggle — `CSI 9;2 t`
  falls off the end of its own switch.
- **`CSI 8 t` resizes the grid in the engine and the window must be told** —
  `Vt::take_terminal_resized` is the flag, deliberately not set by
  `Session::resize` (or the frontend's own resize echoes forever).
- **`CSI 4 t`'s zero axis means "leave alone"; `CSI 8 t`'s means "use the
  default"** (0 *or 1* → 24/80, where xterm reads maximum). One sequence
  apart, opposite rules.
- **`GetDesktopRect` is one monitor's work area** — Qt:
  `QScreen::availableGeometry()`. Full geometry over-reports `CSI 15/19 t`.
- **Raise does not take focus, on purpose** — `BringWindowToTop` + flash
  (`QApplication::alert`); the `SetForegroundWindow` version is behind a
  dead `#if`.
- **Wayland cannot honour `CSI 3 t` and must not pretend to** — a dropped
  move that `CSI 13 t` then reports as done puts a lie on the wire.

The clipboard:

- **OSC 52 has two permission bits and notification is neither**:
  `ClipboardAccessFromRemote` read/write independent, `on` = both, anything
  else = neither (`ttset.c:1742`); access ships off, `NotifyClipboardAccess`
  ships on. Notification off must quiet an allowed action, not refuse it.
- **OSC 52 base64 is deliberately permissive** (`ttlib.c:b64decode`: skips
  whitespace, stops at the first invalid byte including `=`, decodes a
  final short group) — a strict decoder is observably different. `Pc`
  accepts only `cps01234567`; only a payload of exactly `?` is a read.
- **An OSC 52 read reply fits thirteen selector bytes, not fourteen**
  (`hdr[20]` minus `ESC ] 52 ;` and the appended `;`) — longer is accepted,
  notified, and never answered. A response always ends in ST; `IsTextW`
  permits empty, refuses binary controls. The terminal owns these rules.
- **A paste is a keyboard: every line break goes out as a single `CR`**
  (`NormalizeLineBreakCR`, `clipboar.c:289`, before the brackets). Queueing
  the clipboard's own bytes reads as correct and puts a byte on the wire no
  key produces. Same trap as `Vt::encode_text`.
- **`BracketedSupport` is a second gate on `DECSET 2004`**
  (`clipboar.c:265`); ships on. `BracketedControlOnly` narrows to pastes
  containing a control character.
- **`EnableContinuedLineCopy` is upstream's `logFlag`** — with it on, only
  the wrap-invented `CR LF` is kept out of the log *and the macro tap*, so a
  copying key decides whether a script sees a wrapped line as one line or
  two (same shape as `LogTypePlainText`).
- **The two mouse-paste keys ship opposite to Linux expectations** —
  `DisablePasteMouseMButton` on, `...RButton` off: Tera Term pastes on the
  right button. Neither default is a bug.
- **A paste happens on button-up**, and so does `AutoTextCopy`'s copy —
  which `SelectOnlyByLButton` *suppresses* for middle/right release
  (`vtwin.cpp:819`); that half is not in the name.
- **Select screen and Select all end at the last column, not at column 0 of
  the line after** — upstream's `SelectEnd` is the other form and its own
  alternative is commented out beside it (`buffer.c:709`, `:726`). The line
  after the live page is not one `Session::line` can answer for, so upstream's
  spelling puts a trailing break in a scrolled-back Select screen and in
  nothing else. `render_test` pins the choice; the live case cannot see it.
- **`PasteDelayPerLine` is the only setting clamped at both ends**
  (`ttset.c:1633`) — why `int_clamp` exists beside `int` and `int_min`.

The bell — a beep is a state machine:

- **The bell that trips the over-used limit still sounds** (six audible for
  `BeepOverUsedCount=5`), and the suppression measures *quiet*, extended by
  every further bell (`vtterm.c:5791`, `:5796`). The manual says five and a
  fixed delay; the port follows the code here.
- **`BEL` is gated by the setting; `ESC g` is not** (`vtterm.c:1077` vs
  `:1561`) — a muted terminal still spends its allowance on `ESC g`.
- **The governor needs a clock, so it is not in the engine** — `Vt` is a
  function of its bytes; `Vt::take_bells` hands `tt-session` a **count**,
  one state-machine step per BEL.
- **`BeepOnConnect` never fires on serial** (`PortType==IdTCPIP` first,
  `vtwin.cpp:3018`, `:3658`); always audible, never visual, outside the
  governor.
- **The visual bell is DECSCNM's own flag toggled twice** — a flash on an
  already-reversed screen shows it the *normal* way round. Upstream sleeps
  on the parsing thread; ours is a timer.
- **`Answerback`/`DelimList` are hex-encoded and `Hex2Str` is
  default-biased** (`$ZZ` is NUL, `$A` is 0xA0). Two decoders, not one: the
  answerback is *bytes* (`hex_decode`), the delimiter list is *characters*
  (`Hex2StrW`/`hex_decode_str`). Never read these through
  `tt_session_setting` — `tt_session_word_delimiters` exists for that.
- **`DelimDBCS` is a width-run splitter, not a DBCS switch**
  (`buffer.c:4479`, `b->cell == 1`; consulted only in the non-delimiter
  arm). Ships on.
- **`IniAutoBackup` covers only Setup > Save over an existing file**
  (`vtwin.cpp:4738`); backup failure does not stop the save; first copy in a
  second wins. Don't move the switch into the generic INI writer.
- **`AlphaBlendActive` defaults to the loaded `AlphaBlend`, not 255**
  (`ttset.c:1471`) — the schema's `default-from=`.
- **`windowOpacity()` succeeding does not mean Wayland changed a pixel** —
  Qt's Wayland client sends no alpha request; the property round-trips
  anyway. Inspect via xcb.
- **`BPAuto=on` silently discards `Answerback=`** (`ttset.c:1132`) — the
  only setting another setting takes over.

The printer — controller mode does not take the stream away:

- **Printer controller mode is not a diverter**: controls go to the printer
  uninterpreted, but printables still reach the screen, their printer copy
  riding `OutputLogUTF32` — and the two halves must be *interleaved* or
  `A LF B` prints as `LF A B`.
- **`CSI 5 i` turns the mode on from inside the parser**, so `Vt::feed`
  cuts the chunk after every `i` while `PrinterCtrlSequence` is on — or
  everything after the sequence in the same segment is lost.
- **Four media-copy sequences are gated; `CSI ? 4 i` (auto print off) is
  deliberately not** — a host can always stop line printing.
- **`ts.PrnDev` gates parsing, not printing** — `DirectPrn` decides whether
  ISO-2022 designations during the job are interpreted or printed (cases
  133/134 are the same input under the two answers).
- **The spool holds code points, not bytes** (`teraprn.cpp:527`;
  `UTF32ToMBCP(u32, CP_ACP)` on the way out) — encoding decided at the
  device.
- **A control inside a half-read sequence flushes that sequence out ahead of
  itself** (`ESC [ 12 BEL m` prints as-is); the exit works only because
  `CSI 4 i`'s arm *discards* the buffer (the clear form of
  `WriteToPrnFile`).
- **A host cannot reach `ResetTerminal`'s `PrinterMode` clear** (while the
  controller has the stream, `ESC c` is printer data); a RIS mid-job leaves
  an open spool. Menu reset also does not close the job or clear
  `AutoPrintMode`.
- **Whether a wrapped line breaks in the printer's copy depends on whether a
  log or macro is running** (`NeedsOutputBufs()`, `vtterm.c:512`).
  Reproduced.
- **Auto print's byte argument is the whole selection**: LF, VT, FF dump the
  line; IND and NEL pass zero and do not; the wrap's `LineFeed(LF, FALSE)`
  prints one.
- **The dump is of the grid, not the stream** (`hello\rH` prints `Hello`).
  Upstream's own version is defect 32 in `docs/upstream-bugs.md`; this port prints what it meant.
- **An `Option<String>` local put a destructor in every character's frame —
  4% of `core.plain`.** Auto print's snapshot is a `State` field assigned
  only under the flag; anything added to `Perform::print` wants
  `./bench/bench.py --core` against the previous commit.

The title — two strings in three places:

- **OSC 1 sets the window title** (`vtterm.c:5109`: cases 0/1/2 fall into
  one arm) — case 107 pins it.
- **`gettitle`/`settitle` use `ts.Title`; an OSC writes `cv.TitleRemoteW`**
  (`ttdde.c:646`, `:636`); the window shows both combined. `settitle` via
  the parser puts the string in the wrong half — visible under `ahead`/
  `last`.
- **The window title and the title report disagree about an empty host
  title** — `ttwinman.c:101` always falls back to `ts.Title`;
  `vtterm.c:2677` only under `overwrite`, so `ahead` answers `CSI 21 t`
  with a leading space. Don't share one function.
- **`TitleFormat` is a wrapping WORD, not six booleans** (default 13 =
  endpoint + VT + swapped order). Connecting/disconnected captions do not
  take the swap arm.
- **A displayed serial speed comes from the live port** — upstream posts
  `WM_USER_CHANGETITLE` after a reset; the shell re-reads the transport on
  the caption edge. Caching the opening parameters goes stale at the first
  `setbaud`.

The menu:

- **`PopupMenu` hides the menu bar; `EnablePopupMenu` gates its
  replacement** (Ctrl+left-click, only when the bar is absent; `HideTitle`
  also hides it without touching `PopupMenu`). The gesture runs before
  mouse reporting — a host cannot capture the route back.
- **`EnableShowMenu` adds a recovery command** (upstream: Win32 system
  menu). Qt cannot reach the compositor's menu, so it lives in the popup
  and clears only `PopupMenu`.
- **The popup reuses the menu bar's `QAction`s** — a second tree would
  drift. Destroying the temporary `QMenu` only removes the association.
- **A disabled `QAction` refuses `trigger()` as well as a click**, silently —
  so an enabled state computed only in the menu's `aboutToShow` is an item
  nothing but a mouse can reach, and a test or a script that triggers it does
  nothing and reports nothing. View > Reset line counter sets its state on the
  settings edge *and* refreshes it as the menu opens: the first is what makes
  `trigger()` work, the second is what makes the answer the front tab's, since
  `terminal.line_numbers` belongs to a session and a tab switch is not a
  settings change.

The keyboard:

- **`MetaKey` chooses whether Alt is Meta; `Meta8Bit` chooses what Meta
  does** (ESC prefix when off; `raw` ORs 0x80 into the byte, `text` U+0080
  into the character — they cannot share the UTF-8 send path). Left/right
  modes must remember the native Alt press.
- **`StrictKeyMapping` removes fallbacks, it does not validate**
  (`keyboard.c:960`); `DeleteKey=on` still sends 0x7f first. With no
  `KEYBOARD.CNF` reader yet, strict mode quiets the built-in special keys —
  faithfully incomplete.
- **Line edit is not telnet LINEMODE.** It is a frontend editor shared by every
  transport: printable input and Ctrl+A/X/Z/Y stay local, while other control,
  function and `KEYBOARD.CNF` keys remain immediate. Sending the accepted line
  forces one echo without assigning `LocalEcho` or SRM.
- **With local echo, sending is also receiving** — the same bytes go through
  the receive parser, so a keystroke damages the screen and only draining the
  core's events says so. The frontend's input paths call `Session::dispatch`,
  never bare `rearm`; and a path that emits `damaged` by hand instead leaves
  its own events in the queue for whoever drains next — `setSetting`, `resize`
  and the three settings loaders all did, so a keystroke's repaint arrived
  carrying the last settings change's. Pumping is the obvious call
  and the wrong one — a pump reads once whatever the budget, so a quiet serial
  or telnet line stalls the UI thread for its 50 ms read timeout on every key.
  Undrained, a typed character waited for the next thing the host said or for
  the cursor's own blink: half a second a keystroke on an idle line, and
  nothing at all with a steady cursor.

File-shaped settings:

- **The read key is `CygwinDirectory ` with a trailing space; the written
  key has none** (`ttset.c:1476`, `:2250`). Backtick-quoted schema keys
  keep the space; `write-key=` records the writer's spelling.
- **`FileSendFilter` reaches raw send and every protocol send picker;
  `FileReceiveFilter` is raw receive only** (`filesys_proto.cpp:727`).
- **`DrawingResizedFont` is glyph fitting, not cell measurement** — turning
  it off must not remove the spacing correction that keeps a batched run on
  the grid.

Remembered window geometry:

- **`SaveVTWinPos` gates writes, not reads** — `VTPos` is applied even when
  off; off means the old line stays byte-for-byte (schema `write-if=`).
- **`GetNthNum` writes zero for an omitted field; `GetNthNum2` takes the
  caller's fallback** (`VTPos=12` is `(12,0)`; `XmodemTimeouts=5` keeps four
  defaults). Schema spellings `int_zero` vs `int`.
- **Close-time `SaveVTPos` is not Save setup** — only `VTPos` and the *live*
  `TerminalSize`, only when `SaveVTWinPos` is on (`ttset.c:3338`).
- **Wayland's `(0,0)` is not a position to remember** — both restore and
  overwrite are skipped there; elsewhere upstream rejects off-desktop points
  and clamps the top/left fringe (`vtdisp.c:1517`).

The command line — two parsers, one of them a plugin:

- **A bare host name cancels `/C=`** (`ttset.c:3954` assigns
  `ParamPort = IdTCPIP` outright) — word order decides, nothing warns.
- **`/AUTOWINCLOSE=1` means off** — `_wcsicmp` against `on` with an `else`
  (`ttset.c:3716`), not `GetOnOff`.
- **`/C=` out of range against `ts.MaxComPort` is dropped, not clamped** —
  serial transport with no port, dialog up.
- **`_ParseParam` discards its first token** — `ttdde.c:617` prepends a
  literal `"a "` and passes NULL for the DDE topic (so `/D=` inside a
  `connect` does nothing). Both facts: `CommandLine::parse_argument`.
- **A `/D=` topic frees `ts.MacroFNW` unconditionally** (`ttset.c:3963`) — a
  macro-launched terminal does not run the startup macro.
- **`TT_MACRO_UNSET` means inherit `StartupMacro`, not run nothing** — four
  launch states: inherit, cancel (`/D=`), prompt, file (`/M`). The relative
  path resolves beside the active INI (no global `chdir` here);
  `StartupMacro=*anything` is a picker (`ttmmain.cpp:285` tests only
  `FileName[0]`).
- **`/ssh` and friends are not in `ttset.c`** — TTSSH hooks the parser,
  runs first, blanks what it consumed, and rewrites `ssh://user@host/` into
  a bare `host:22` token (`ttxssh.c:1521`). Reading only `ttset.c` gives a
  line that cannot open SSH.
- **In TTSSH `-` leads a switch and `ssh` is case-sensitive** — `/SSH` does
  nothing, silently. `/t=2` is consumed; `/t=0` deliberately left.
- **A forwarding letter is given once for the whole list**
  (`ttxssh.c:1556`): `/ssh-L1:h:2,3:h:4` is two `L` specs, and the
  documented `;` separator only works quoted.
- **Nothing sets the SSH port — upstream sends SSH to port 23** on a fresh
  install (TTSSH never assigns `ts.TCPPort`). `Target::of` diverges: 22
  when no port was asked for, tested by upstream's own `TCPPort == TelPort`.
- **Two of `OnCommStart`'s three arms open nothing** (`vtwin.cpp:3708`): a
  host name decides for non-serial, `ComAutoConnect` for serial — and an
  in-range `/C=` re-enables auto-connect *after* the option loop.
- **A macro's `connect` gets TTSSH's half too** — `(*ParseParam)` is a
  function pointer re-hooked by `LoadTTSET` → `TTXGetSetupHooks`
  (`ttdde.c:620`). Read the DDE arm alone and `connect 'myhost /ssh'`
  cannot work.
- **`cygconnect`'s argument is CygTerm's command line** (`cyglaunch -o`),
  split by *two* rules: the line by cygwin's CRT (backslash ordinary),
  `-s`'s shell string by `get_argv` (backslash escapes). One splitter
  mangles the manual's own example.
- **CygTerm's default directory is the launcher's, not home** (`home_chdir`
  false without `-cd`) — `PtyParams::cwd = None` means home, so don't pass
  the default through.

The macro language (TTL):

- **Every TTL argument is a whole expression — a space is not a separator**:
  `fileseek fh -3 2` parses as `(fh - 3)`. Bracket the negative. Same rule
  makes `listbox`'s keywords quoted strings (bare `listboxsize=40x10` is a
  variable → "Variable not initialized").
- **A macro path with no dot gets `.TTL` fitted onto it**
  (`ttmmain.cpp:253`, mutating `FileName` itself) — give test macros a
  `.ttl`.
- **`SendCmnd` holds the link check** — after argument parsing, so
  `sendbreak junk` is a syntax error where `send 'x'` unlinked is
  `ErrLinkFirst`. Port the check, not just the command body.
- **`DDE_FNOTPROCESSED` reads as success** — `setdtr`/`setrts`/`setbaud`/
  `setflowctrl` are silent no-ops over SSH, not errors.
- **`ttl.cpp` bounds-checks its handle arrays in about half the places** —
  assume the next array is unchecked until you have looked (seven OOBs
  found, listed in `docs/upstream-bugs.md`).
- **Rust's whole-file lock is not TTL's Windows lock** — upstream pairs
  `LockFile`/`UnlockFile` over `(0,0,DWORD_MAX,DWORD_MAX)`; Wine accepts
  `LockFileEx` and refuses its unlock. `tt-ttl::files` uses the exact Win32
  pair on Windows.
- **Stable Rust cannot set `STARTUPINFO.wShowWindow`** — TTL's `exec` needs
  it, so the Windows launcher calls `CreateProcessW` directly (keeping the
  raw command line).
- **The upstream script suite contains a blocking GUI program**
  (`#35797.ttl`, `notepad` + `wait=1`) — the harness substitutes a
  guaranteed-missing name on both targets; keep that isolation.
- **A path through the transcript's `esc` has doubled backslashes** —
  normalise both spellings and the separator after `<dir>`/`<home>`/
  `<exedir>`, or six Windows-only golden diffs appear.
- **Do not bless the TTL goldens on Windows** — five scripts expose
  drive/separator/shell-folder answers and Wine is not their authority;
  `TTL_BLESS` is refused there.
- **A BOM-less TTL file means the machine's ACP on Windows** — the
  harness's private copies get a BOM; `source.rs` tests real ACP conversion
  separately. Don't grow the five-name allowlist to absorb a locale.
- **`ToU8W` is not `WideCharToMultiByte` for UTF-8** — invalid UTF-16 comes
  back as ASCII `?`, not U+FFFD (`WideCharToMBCP`). `from_utf16_lossy`
  agrees with the API name and disagrees with the code.
- **`expandenv` is `ExpandEnvironmentStringsW`**: an unknown name's closing
  `%` is *not* consumed — it opens the next name, so `%UNSET%KNOWN%` is
  `%UNSET` + KNOWN's value. Unix mirrors it. (This entry said the opposite
  until a native Windows run corrected it — Wine is not the platform a
  Win32 parser rule is measured on.)
- **A missing reserved word is diagnosed as a bad assignment** ("Unknown
  command." from `ExecCmnd`'s else, `ttl.cpp:6480`) — on a perfectly good
  line. `rsv.rs` transcribes `CheckReservedWord`; check transcriptions by
  extracting and diffing both lists.
- **Commands are not named after their documentation page** —
  `logautoclosemode`, not `logautoclose`. Spelling from `CheckReservedWord`,
  never prose.
- **`waitregex` matches whole lines with the CR still on them** (match runs
  at the LF, before it joins the buffer) — `$` never matches a CRLF line;
  and an empty line matches nothing at all.
- **`onig` needs `default-features = false`** or the build wants
  `libclang` — the failure reads as a missing toolchain.
- **A macro that shows a dialog must not run on the UI thread** — host
  methods block, the frontend answers with a nested event loop (see
  `Session::m_sshWaiting` for what re-entry costs).
- **`getpassword` can report success with an empty password** — `Encrypt`
  output can end in a quote pair that `GetPrivateProfileString` strips
  (~1 in 4000). Reproduced; the symptom points into the cipher.
- **The v2 password format's HMAC key derives from `EncSalt` as stored
  (its own ciphertext), and the three fields share one keystream** (MAC at
  offset 219). Doing either the clean way round-trips here and nowhere
  else.
- **A `/V` before the macro name is a switch; after it, a parameter**
  (`ttmdlg.cpp:112`, `ParamCnt == 0`). `macroparam.bat` is the spec. No
  `--`.
- **`params[0]` is the whole command line; `params[1]` is `ShortName`**
  (basename, `.TTL` fitted), not the path (`ttl.cpp:243`).
- **`GetParam` is not `CommandLineToArgvW`**: backslash ordinary, `""` in a
  quoted run is one literal quote, unquoted `;` ends the line
  (`ttlib.c:888`); quotes come off later in `DequoteParam`.
- **A macro does not read the wire** — `wait`/`waitln`/`waitregex`/`recvln`
  match the *text session log's* tap (`OutputLogUTF32`): printed characters,
  executed `CR`/`LF`/`BS`/`HT`, invented `CR LF` at wraps, **no escape
  sequences**. Teeing the transport is the obvious build and is wrong.
  `Vt::set_macro_tap_enabled` is the seam.
- **`CheckEOLCheckLog` drops a lone CR** (`checkeol.cpp:105`): `abc\rdef`
  reaches a macro as `abcdef`. The parked space before a wrapped wide glyph
  is not in the stream.
- **The macro ring drops the OLDEST byte when full** (`ttdde.c:107`,
  64 KiB) — deliberate: a lagging macro wants the newest prompt.
- **The macro thread must not hold a lock on the session** — `tt-macro`
  posts owned closures; a `Mutex<Session>` dies at the first modal dialog.
- **`sendln` puts a bare CR on the wire by default** — the *text* send path
  expands the newline by `ts.CRSend` (`ttcmn.c:814`), the binary path does
  not. `send_bytes` vs `send_text` are two paths; `looks_like_text`
  decides (its last-byte quirk is upstream's NUL count).
- **The shell has already done half of `ParseParam` on Unix** — take argv
  tokens as given (`CmdLine::from_args`); re-tokenising quote-processes
  twice. `CmdLine::parse` is for a genuine command line.
- **A `recvfile` that receives nothing waits forever** — the auto-stop
  timer arms at the *first byte* (`raw.c:168`), and `raw.c:184` discards
  what was buffered when the transfer starts.
- **Three receives are told their name, four hear it from the wire** —
  XMODEM carries none, `raw.c` writes what it is handed, Kermit `GET` asks
  by the remote *basename* (`kermit.c:1160`). `Job::needs_name` is the
  list; wrong means a file called nothing.
- **A transfer's clock is `GetTickCount64` on Windows** (~15.6 ms
  resolution) — a one-second auto-stop can measure 993 ms to an `Instant`.
  Assert with a tick of slack.
- **A transfer is the one blocking command that cannot notice a dead
  frontend** (its outcome is posted from the other thread) — `PROBE` in
  `tt-macro/src/host.rs` is the quarter-second knock, for that and nothing
  else.

The other language (Lua):

- **`pcall` catches the cancellation raised from the debug hook** — Lua has
  no uncatchable error, so `Script::run` asks the host again at the
  boundary; the *answer* is honest even when the script swallowed the stop.
- **The hook cannot capture the host** (`mlua` hooks are `'static`) — it
  calls a scoped function out of the registry; safe because Lua clears
  `allowhook` during a hook.
- **A success returning two values breaks nesting** (Lua expands the last
  argument) — every function returns one value on success, `nil` + detail
  on failure (`io.open`'s shape); `tt.waitln` is the deliberate exception.
- **`set_name` without a leading `@`** renders errors as
  `[string "login.lua"]:12:` — editors cannot jump there.

The session log:

- **There are two `strftime` expanders and they disagree both ways** — file
  names go through `IsValidStrftimeCode` + the CRT; timestamps through
  `ttstrftime` (twelve conversions, unknown `%x` emitted literally). `%N`
  works in a timestamp and is deleted from a name. One expander for both is
  wrong twice.
- **`LogRotateSize` is in bytes whatever `LogRotateSizeType` says** (the
  dialog pre-multiplies) — scaling by the type turns 1 MB into a terabyte.
- **A `LogRotateStep` of zero is ten thousand generations, not none**
  (`filesys_log.cpp:507`).
- **`LogRotate` is not a bool and takes no range** — 0 none, 1 by-size,
  anything else "do not rotate"; `int(0..1)` would clamp a 2 to on.
- **`LogTimestampType`'s empty value is a value** — absent/empty consults
  Tera Term 4's `LogTimestampUTC`; a present value does not. Both keys are
  in real files; the schema gives the empty spelling its own variant.
- **`LogTypePlainText` gates the tapped BS** (`vtterm.c:666`, `:671`) — and
  the tap is shared with the macro buffer, so a log setting changes what
  every `wait` matches.
- **The file-transfer directory decides where a log goes** —
  `GetTermLogDir` falls back to `FileDir` before the per-user dir, so
  `/FD=` moves the log.
- **A relative `/L=` lands in the log directory, not the working
  directory** (`filesys_log.cpp:964`).
- **A paused log discards, it does not hold** — a binary log drops the byte at
  the input (`filesys_log.cpp:1038`), a text one drops it draining the ring
  (`:647`). `logwrite` deliberately writes anyway, which is the manual against
  the code; the divergence is stated at `SessionLog::write_str`.
- **The BOM has a three-way gate and a fourth place it appears** — new file,
  text mode, asked for (`filesys_log.cpp:382`), and again at the head of every
  rotated generation (`:565`, which does *not* re-test `Append`, because a
  rotated file is new whatever `Append` said). It has no INI key in either
  program: upstream carries it in `FLogDlgInfo_t.bom` and forgets it when the
  dialog closes, so it rides `LogOptions`/`TtLogOptions` here.
- **`LogIncludeScreenBuffer` is text-mode only** (`vtwin.cpp:4145`), and
  `FLogOutputAllBuffer` is not worth transcribing — it walks
  `BuffGetAnyLineDataW`, which is upstream bugs 1 and 2 on file (every line
  stops at its first full-width character; the budget is columns, not code
  points), under a 512-wchar cap. `Session::buffer_text` reads the grid.
- **Tera Term 5's log dialog is not Tera Term 4's** — TT4 customised the common
  save dialog; TT5's is a plain `IDD_LOGDLG` (`logdlg.cpp:267`) with a `...`
  button. Two of its own defects are not reproduced: it writes
  `LogTimestampType` as `GetCurSel() - 1` against the plain index it reads back
  (`:106` vs `:322`), and "New / Overwrite" is a `DeleteFileW` before the open
  (`vtwin.cpp:4142`), so an undeletable file silently becomes an append.
- **`PageStatusBar::setLogging` returns early on an unchanged state, and the
  pause is part of that state** — it is reached from `Session::damaged`, so it
  runs on every read on every open session and the early return is
  load-bearing. A flag left out of the comparison never repaints: the same
  shape as `ConnectBar::Entry::operator==`.
- **Logging is suppressed for the duration of a file transfer** —
  `ProtoGetProtoFlag()` sits in the same two `if`s as the pause
  (`filesys_log.cpp:646`, `:1038`). The same outcome falls out here for a
  different reason: the transfer arm of the pump `continue`s before
  `log_bytes_in`, so a protocol's traffic never reaches the log in either mode.
  Nothing tests it as a log property — if the transfer arm ever stops owning
  the stream, this goes with it silently.

A macro reached from outside the process:

- **A macro that ends without asking for anything never wakes its
  frontend** — `tt_macro_start`'s thread knocks once on its way out, and
  sets its own "done" flag *before* knocking (`JoinHandle::is_finished` is
  still false at that point — don't read it instead).
- **The `QSocketNotifier` must be disabled across `tt_macro_service`** —
  level-triggered + a `messagebox`'s nested loop = a second dialog inside
  the first. "The core drains the pipe first" is not a guard.
- **Qt cannot tell No from the close box** — a closed `yesnobox` reads as
  No and the script carries on (stated in `Macro.cpp`); `listbox`'s Closed
  is -2.
- **`tt_macro_free` cannot detach the terminal** (it has no session) —
  `tt_session_unlink_macro` is the other half; the frontend calls both.

The control socket:

- **`bind` cannot tell a leftover socket file from a collision** — connect
  to it: `ECONNREFUSED` means unlink and take it. The same probe prunes
  dead names, without which "exactly one window" refuses a real session.
- **Windows will not identify a named-pipe client before it has spoken** —
  `ImpersonateNamedPipeClient` before the first read refuses *everyone*.
  The check runs after the first line is read, before it is parsed. Unix
  keeps the stricter `SO_PEERCRED`-first order; deliberately not one path.
- **A refusal and a broken check must not be the same `false`** —
  `peer_check` keeps the reason (`cfg(test)` asserts through it); without
  it the symptom was five tests reporting a hang-up.
- **`FindFirstFile` on `\\.\pipe` says `ERROR_NO_MORE_FILES`** where a
  directory says `ERROR_FILE_NOT_FOUND` — an empty machine must be an
  empty list, not an error. Wine gives the directory spelling.
- **A modal dialog inside `tt_ctl_service` holds the client open** — a
  nameless `connect` is refused outright; a failed open's message box is
  queued to the next event-loop turn. In a test this is a hang, not a
  failure.
- **A claim is read through the endpoint list, not on its own** — a window
  that crashed holding a port would otherwise grey it out for ever. `claims()`
  keeps only the names `addr::live()` answers for and unlinks the rest, so the
  claim expires exactly when the port is free again.
- **`Vt::encode_text` translates CR, not LF** — `sendln` appends `\r`;
  `\n` reads as correct and is wrong under every `CRSend` setting.
- **A `spin(predicate, ms)` helper calls the predicate once more for its
  return value** — fatal when the predicate consumes (accepting a pending
  connection). Latch the result.
- **`QLocalSocket` is in Qt6::Network, which the shell does not link** —
  test clients use `sockaddr_un` + `poll(2)`.

The C ABI:

- **cbindgen parses files, not crates** — `pub(crate)` and private modules
  are invisible, so any `pub const` in a parsed file lands in the header
  (exclude by name in `cbindgen.toml`); the committed-header diff catches
  it.
- **`Builder::with_crate` runs `cargo metadata` inside the build script**
  (can deadlock on the package cache) and, combined with `with_src`,
  parses twice. `tt-ffi/build.rs` lists files, never `with_crate`.
- **The header is the only place an ABI break shows up** — `TtKey` etc.
  come straight from the core crates, so reordering `tt_vt::Key` renumbers
  the ABI. CI regenerates and diffs.

The shell's Windows build:

- **`CMAKE_SHARED_LIBRARY_PREFIX` is `lib` for MinGW and cargo does not use
  it** — the composed `libsterna.dll` exists nowhere. Names are cargo's on
  every platform, import library included (`sterna.dll.lib` MSVC,
  `libsterna.dll.a` MinGW).
- **The import library must be a `BYPRODUCTS` entry too** — `IMPORTED_IMPLIB`
  alone gives Ninja a dependency with no rule.
- **`--target` and no `--target` are different builds** — `TT_CARGO_TARGET`
  is empty unless cross-compiling; every path derives from it.
- **A GUI-subsystem binary has no stderr, and that is the right subsystem**
  — `MainWindow::note` uses `QCommandLineParser`'s console test and a
  parentless box otherwise. Same shape as the qWarning-to-journald trap:
  the message is never written.
- **A Winsock `SOCKET` is unsigned** — `fd < 0` can never be true; compare
  `INVALID_SOCKET`. And `write` is a file call there; send with `send`.

Wine — the harness lies before the code does:

- **`WINEPATH` takes *Windows* paths; a Unix path replaces `PATH`** — spell
  it `Z:\...\mingw\bin;C:\windows\system32;C:\windows`.
- **`wineboot` does not finish in these containers** — run the executable to
  create the prefix; a killed `wineboot` leaves a `wineserver` the next run
  waits on forever. Kill that too.
- **What that looks like from inside Sterna is a missing `cmd.exe`** —
  `portable-pty`'s `search_path` with no `PATH` hands the bare name to
  `CreateProcessW`, which does not search. Broken environment, not broken
  pty.
- **Wine's fonts are not Windows' fonts** — `render_test`'s six metric
  assertions fail there, it faults on exit, and the crash handler starts
  `winedbg`, wedging scripts. Not Wine's question to answer.
- **Wine's ConPTY opens and delivers nothing** — connect-only tests pass,
  shell-driving ones stare at a blank screen. `ResizePseudoConsole` is
  `E_NOTIMPL` and that error surfaces from an unrelated settings change.
- **The two containers have different Wines** — `sterna-fedora`: full pair,
  wedges in `wineboot`. Ubuntu: only `/usr/lib/wine/wine64`, with an
  already-booted `~/.wine` from `ini-audit` (copy that prefix rather than
  boot). Ask which you are in first.
- **That `wine64` has no WOW64** — a 32-bit binary fails with a missing
  `syswow64\rundll32.exe`, which reads as a broken program. Why the
  installer's stub is amd64.

The Windows installer:

- **The finish page must not start the program** — it would run as
  Administrator and write `sterna.ini` into the wrong profile, permanently.
  `StartSterna` goes through `explorer.exe` to get the user's token back.
- **No `RMDir /r "$INSTDIR"`** — that path is user-typed. `build.sh`
  generates the uninstall list from the staging tree: files by name, plain
  `RMDir` (refuses non-empty). Verified under Wine.
- **An upgrade in place leaves stale Qt DLLs that kill the program before
  `main`** — `.onInit` runs the previous uninstaller first; `_?=` keeps it
  waitable.
- **The licence page is RichEdit and needs CRLF** — user-read text files
  get CRLF; the `.lng` files do not (we read those).
- **`windeployqt` does not exist for this target** — the DLL closure is
  `objdump -p` walked to a fixed point; ship only what the MinGW sysroot
  provides (76 DLLs), never a system DLL.
- **Fedora's MinGW packages ship unstripped** — 154 MB staged, 106 after
  `--strip-unneeded`. Stripping PE is safe (exports are in the image).
- **`.ttl` registration must not point at our `ttpmacro.exe`** (it is a
  client, not the interpreter) — the verb is `sterna.exe /M="%1"`. And
  `.ttl` is Turtle too: register under `.ttl\OpenWithProgids`, remove
  `/ifempty`.

The desktop side:

- **This container's Qt is 6.4.2; the desktop's is 6.11.1** — opening
  windows here proves plumbing, not behaviour. Anything version-sensitive
  needs `sterna-fedora`; the host ships no Qt devel files.
- **Never measure Qt behaviour in the Ubuntu container** — its 6.4.2
  Wayland stack manufactured a whole false optimisation (62 MB Mesa
  mapping, a magic env var, ~2x-flattering startup/RSS; none true on
  6.11.1).
- **But CI's Qt is the Ubuntu container's** — a CI paint failure reproduces
  here, in the gitignored `build-ubuntu` tree. Measurements in
  `sterna-fedora`; CI verdicts where CI gave them.
- **...except for the fonts, and that gap hides whole failures.** CI runs on a
  bare `ubuntu-24.04` runner whose only added font is `fonts-dejavu-core`;
  both containers have hundreds. DejaVu's cell is wide enough that 80 columns
  do not fit the offscreen screen, so the window opens short of its configured
  size — a state neither container ever reaches, and the one the layout cycle
  below only shows up in. Reproduce with a `FONTCONFIG_FILE` naming just
  `/usr/share/fonts/truetype/dejavu`; it is harsher than CI (six metric
  assertions fail that CI passes), so read *which* checks failed, not how
  many.
- **A glyph can put ink outside its own advance** — a margin measured from
  column 0 clamps at the image edge. Measure a column with a blank one
  beside it and let the answer be negative.
- **...and a widget clips its own painting, so text that overruns a
  right-aligned field is cut, not spilled** — and cut at the *start*, which is
  where the significant digits are. `LineNumberGutter` drew a number too wide
  for its column from a negative x and got `0001` for line 10001: a wrong
  number, on screen, with nothing saying so. The rule for any fixed field: ask
  whether it fits and draw nothing when it does not. There is nowhere for it to
  spill — the gutter is the leftmost widget in the page, and the thing on the
  other side of a field's far end is usually somebody else's pixels.
- **You can screenshot your own widgets, not the desktop** —
  `QWidget::grab()` works everywhere (offscreen re-render, which is what we
  want); Shell screenshot D-Bus is `AccessDenied`, `grabWindow(0)` is
  blank/NULL.
- **Cargo gives a cdylib no `DT_SONAME`** — linkers record the path they
  were handed. Fixed in `tt-ffi/build.rs` with `rustc-cdylib-link-arg`
  (through `RUSTFLAGS` it would hit every test binary).
- **A batched text run drifts off the grid** — advances are not whole
  pixels; `Theme::recomputeMetrics` rounds to the cell via
  `QFont::AbsoluteSpacing`. Wide characters draw alone in their two-cell
  box. Symptom: cursor stops lining up.
- **`QWidget::grab()` and focus work under offscreen** — but only after
  `show()` **and** `activateWindow()`, or a cursor test measures the
  unfocused form. And `adjustSize()` caps at two thirds of the *screen*,
  which offscreen says is 800x800: put the wanted size back after it.
- **A widget's metrics are wrong until it is polished** — the style's font
  arrives as a `changeEvent` on first show, *after* a dialog has asked how
  big it wants to be, so `TabRows` measured 1.5x wide and the dialog opened
  too narrow for its own tabs. `ensurePolished()` in `sizeHint` is the fix
  (`QComboBox` does it); remeasure *lazily* from there, because rewriting
  geometry inside that event runs while the layout above is mid-computation.
- **A wrapping widget must not quote its wrapped height as its minimum** —
  a layout takes minimum width and minimum height independently, so
  "narrowest width, and the height that needs" made the settings dialog 900
  pixels tall at every width. The height belongs to `heightForWidth`.
- **Sample a cell's corner, not its middle, to read a background** — the
  middle is ink, and a CJK glyph can overhang a pixel: assert fill *width*,
  not the neighbour. **And the corner is not safe either at nine pixels
  wide**: a `j`'s descender reaches the bottom-left one and antialiases to
  neither colour. `highlight_test`'s `filledWith` counts the cell's pixels
  and asks for a majority, which no monospace glyph can reach.
- **`git add -A` from the root sweeps in-progress work from other
  subtrees** — stage the paths the commit is about.
- **CJK is deferred indefinitely** (2026-08-07, `PLAN.md`) — no IME work;
  "IME untested" is not an open risk. **Input only**: wide/combining
  handling in the grid stays in scope. If revived: the ibus plumbing is
  present but no CJK input source is configured, so an empty result means
  "no source", not "Qt is broken".

## Bugs found upstream, not yet reported

**`docs/upstream-bugs.md` is the ledger**: the five a differential run
*proved* (drafted ready to paste, patches in `oracle/patches/`; the two
memory-safety ones — ECH and DECSED — go first) and everything found by
reading since, numbered up to 38. Filing needs a GitHub account (an open
item), and a found-by-reading entry wants demonstrating against a real
`ttpmacro.exe`/Tera Term before it is filed — the exception worth hurrying is
`ParseParam`'s command-line overflow, the only defect an attacker reaches
without already running a macro. The bug in `vte` is `docs/vte-bug.md`.

## Layout

```
PLAN.md          roadmap + status — read first
AGENTS.md        this file: the working agreements and the traps
CLAUDE.md        one `@AGENTS.md` import, so Claude Code reads the same text
ATTRIBUTION.md   licensing, and what still needs clearing before vendoring
docs/            the ledgers: deviations, upstream bugs, the build record
                 (`history.md`), and per-feature notes
oracle/          Tera Term's real VT engine, headless on Linux (see its README)
esctest/         the conformance suite, run inside our own terminal
packaging/       the AppImage and the NSIS installer — all of Linux/Windows packaging
xfer/            Stage 0 spike 2 — ttpfile's protocols, running and interoperating
serial-audit/    Stage 0 spike 4 — serialport-rs vs commlib.c, on real hardware
telnet-audit/    a real telnetd, so the telnet port has an independent check
ssh-audit/       Stage 0 spike 5 — russh vs legacy SSH algorithms and auth
ini-audit/       what GetPrivateProfile* really does, asked of Wine
vendor/ttpfile/  Tera Term's file-transfer protocols, verbatim — the only
                 upstream code the distribution ships
crates/          Rust core — tt-grid, tt-vt, tt-conn, tt-session, tt-config,
                 tt-xfer, tt-ttl, tt-lua, tt-macro, tt-ctl, tt-ffi
crates/tt-fuzz/  the properties, and what they found
crates/fuzz/     the libFuzzer targets — nightly, weekly in CI
bench/           the perf gate: a floor in CI, a baseline locally
run_diff.sh      the differential gate: Rust engine vs Tera Term, every case
shell/           Qt 6 shell — one window on the C ABI
winshim/         what Tera Term's C needs from Windows — shared by the three
                 things that compile it
```

None of `xfer/`, `serial-audit/`, `ssh-audit/` is throwaway — they are the
regression suites for `tt-xfer` and `tt-conn`, and every spike claim in
`docs/history.md` is reproducible from them.

**There is a second hardware rig, and it lives outside this repository.**
`~/Projects/sterna-rig` is an ESP32-S3 presenting two USB CDC interfaces: one is
a port under test, the other answers questions about what the first was asked
for. It records every `SET_LINE_CODING` and every DTR/RTS change with
microsecond stamps, describes itself however a test tells it to, and drops its
own USB pullup on a schedule — so it can be unplugged without anybody touching
a cable.

That closes three things this file lists as untested or human-driven: real
hotplug for `tt-session/src/reopen.rs` (`serial-audit/src/bin/hotplug.rs` needs
a person, and `tests/serial_loopback.rs` fakes the *return* with a symlink
because nothing could fake the departure); line settings read back through
something other than the driver that applied them (the FTDI accepts `CS5` and
transmits eight bits); and the rule that enumeration must never open a port,
which is stated here and had nothing behind it.

It complements the FTDI pair rather than replacing it — with native USB only
there is no wire, so break, framing errors, parity errors, RTS/CTS gating and
modem-line input stay here. It depends on this repository by path and this
repository knows nothing about it, which is the right direction and also means
**it is not in CI and cannot be**: no runner has the board. Run it before a
release, and after anything that touches `tt-conn::serial` or
`tt-session::reopen` — it will not tell you it has gone stale.

`ssh-audit/servers.sh` needs `sudo` (sshd + dropbear on localhost, a
throwaway `sterna-test` account). **Run `./servers.sh stop` when done** — that
removes the account.

**`winshim/` is shared by `oracle/`, `xfer/` and `crates/tt-xfer/`** — a
shipped crate must not reach into the test harness for its build. Adding to it
is usually right, but **re-run `oracle/run_tests.sh` after touching it**.

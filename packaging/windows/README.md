# packaging/windows

One artifact on Windows: an **NSIS installer**, cross-built on Linux. Release
builds run on GitHub in the Fedora 44 container defined by
[`release.yml`](../../.github/workflows/release.yml); a native Windows job also
builds and runs the updater tests before the installer can reach a draft.

```sh
# Local packaging work still lives in sterna-fedora. See AGENTS.md for why.
distrobox-host-exec distrobox enter sterna-fedora --no-tty -- bash -lc '
  cd ~/Projects/Sterna/packaging/windows
  ./build.sh              # → build/sterna-0.2.5-x86_64-setup.exe
  ./build.sh --clean      # ...from scratch
  ./build.sh --stage      # ...stopping at the file tree, without makensis
'
```

Measured 2026-08-12 after signed updates landed: a **33 MB installer** puts
**114 MB across 55 files** on disk. The pre-updater build was 31 MB / 106 MB;
the on-demand updater DLL, Qt Network and its dependency are the increase. A
third of the staged tree was symbol tables before `--strip-unneeded`;
`libstdc++-6.dll` alone came to 29.7 MB, because Fedora ships its MinGW
packages unstripped.

## Why NSIS and not Inno Setup

Upstream Tera Term ships an Inno Setup script (`installer/teraterm.iss`), so
matching it would have been the obvious move. `iscc.exe` is a Windows program:
building with it means Wine on the release path. **NSIS's compiler is a Linux
binary** — `makensis`, from Fedora's `mingw32-nsis`/`mingw64-nsis` — so the
whole artifact is produced by native tools that have no opinion about Windows.

Wine has manufactured several false findings on this project already (see
`AGENTS.md`), and a release artifact is the last place to want another.

## What goes in

| | |
|---|---|
| `sterna.exe`, `sterna.dll` | the shell and the Rust core |
| `ttctl.exe`, `ttpmacro.exe` | the two control-socket clients |
| `lang\*.lng` | all 14 languages, verbatim from upstream |
| `platforms\`, `styles\`, `tls\` | the Qt plugins that are not optional; see below |
| ~30 DLLs | Qt, and what Qt needs, and the MinGW runtime |
| `doc\` | the licences, and `BUILD-INFO.txt` naming the exact Qt |

The clients are in there for the same reason they are in the AppImage: the
control socket exists so a shell script can drive the terminal, and shipping
the socket with nothing that speaks to it is half a feature.

**The DLL set is closed out of the import tables, not from a list.** Qt's own
deployment tooling is not available for this target — `windeployqt` is a
Windows program and the MinGW package ships no `qtpaths`, which CMake says out
loud during configuration ("No qtpaths executable found for deployment
purposes"). So `build.sh` walks `objdump -p` output to a fixed point. The rule
for *ours to ship* versus *Windows'* is whether the MinGW sysroot has the file:
that tree holds only the 76 DLLs the cross toolchain provides, and none of
`kernel32`, `msvcrt`, `shell32`, `user32`, `advapi32` or `ole32` is among them.
Checked rather than assumed, because shipping a private copy of a system DLL is
worse than shipping none.

What is left unresolved after the walk is 45 names, every one a genuine part of
Windows: the API sets, `d3d11`/`d3d12`/`dxgi`/`DWrite` that Qt's graphics
backends load, `WINSPOOL.DRV` for printing, `WTSAPI32`, `UxTheme`.

## The stub is amd64, which is not the convention

An x86 installer stub runs on any Windows and is what nearly every installer
uses, including for 64-bit programs. It costs two things here and buys nothing:

- A 32-bit process writing `HKLM\Software` lands in `Wow6432Node` unless every
  write is wrapped in `SetRegView 64`. Add > Remove Programs reads both views,
  so this works — until something else goes looking.
- **The only Wine in this environment is 64-bit with no WOW64**, so an x86 stub
  cannot be started here at all. It fails with `failed to open
  C:\windows\syswow64\rundll32.exe`, which reads as a broken installer.

A release artifact that cannot be run before release is the wrong trade for
supporting a 32-bit Windows that could not run the 64-bit program inside it
either. What it costs: on 32-bit Windows the refusal now comes from Windows
("this app can't run on your PC") rather than from a message of ours.

## The `.ttl` association, which is off by default

Upstream offers one and ships it unchecked (`teraterm.iss:285`), and that is
the right default for a reason its comment does not give: **`.ttl` is also
Turtle**, the RDF serialisation, so on a machine that edits those the extension
is already spoken for. Both halves of the answer follow from that — off by
default, and registered through `.ttl\OpenWithProgids` rather than by writing
`.ttl`'s own default value, which is the additive form. It offers Sterna in
Open with and in the Windows 8+ "how do you want to open this" list, and takes
the extension away from nobody.

The components page is where it is normally turned on. A silent install has no
page, so `/S /ASSOC` asks for it — upstream's installer takes
`/TASKS=macroassoc` for exactly that reason, and Inno supplies it where NSIS
does not.

**The command is `sterna.exe /M=`, and upstream's is `ttpmacro.exe "%1"`.**
That is the port's architecture showing through: upstream's `ttpmacro.exe` is
the interpreter and runs the script in its own process, whereas here the
interpreter is inside the window and `ttpmacro` is a *client* of one. The
literal registration would therefore fail on a machine with no window open —
which is most of what double-clicking a file means — and fail invisibly, since
`ttpmacro` is a console-subsystem program and Explorer would give it a console
window that flashes and is gone with the diagnostic in it. `/M=` is upstream's
own `ttermpro /M=script.ttl`. What it costs is that a macro opens a *new*
window rather than running in one that is already up.

The uninstall guards the same collision from the other side: the ProgID key
goes unconditionally, our value under `OpenWithProgids` goes by name, and the
two extension keys go only `/ifempty`. Verified — a `.ttl` seeded with another
program's ProgID and its own `OpenWithProgids` entry comes through an install
and uninstall untouched. Upstream leaves `.ttl` behind outright.

## Two things the installer deliberately does not do

- **It does not put anything on `PATH`.** `ttctl` and `ttpmacro` sit beside
  `sterna.exe` and a script can name them. Editing the system `PATH` from NSIS
  is the classic way to *truncate* somebody's `PATH`: the naive `ReadRegStr`
  into a 1024-byte buffer silently loses everything past it, and NSIS's own
  documentation says so.
- **It does not touch `sterna.ini`.** Settings are under the user's AppData
  rather than the program folder, there is one per user on a machine that may
  have several, and an uninstall that is really an upgrade would take them.

## Signed in-app updates

Help > Check for Updates — and the once-a-day startup check, while
`[Sterna] CheckUpdatesOnStartup` is on — downloads a signed release manifest
and, once the offer is accepted, a signed copy of this same NSIS installer. The application starts it with
`/S /UPDATEPID=<pid> /RESTART` after both signatures and the SHA-256 have been
verified. `.onInit` waits up to two minutes for that process to leave before
the old uninstaller runs; a timeout aborts without touching the installation.
After a successful silent upgrade, Explorer starts Sterna with the desktop
user's token and the temporary installer is queued for deletion.

The pid wait is load-bearing. Windows will not replace the running
`sterna.exe`; mixing it with the new Qt DLL set produces the same pre-`main`
missing-entry-point failure an ordinary upgrade's stale DLL cleanup prevents.
The updater therefore also refuses a loose executable: `uninstall.exe` beside
the running program is its proof that there is an installed tree to upgrade.

The same rule applies one step earlier and to this installer itself: the
terminal must let go of the file it downloaded before Windows can run it. A
`QTemporaryFile` holds its file open for the object's lifetime whatever
`close()` suggests, and an image section cannot be created for a file another
handle has open for writing — so the download is detached before
`ShellExecuteExW`, and until it was, this installer never started. The failure
is worth recognising because the message is the shell's rather than Sterna's: a
box about another program using the file. If it appears again, the error code is
now in Sterna's own message behind it — 32 is that sharing violation, 1223 is
the elevation prompt being declined.

See [`../update/`](../update/README.md) for the signing key and manifest.

`Qt6Network.dll` is not enough to speak HTTPS. TLS is a dynamically loaded Qt
plugin, so the installer carries `tls\qschannelbackend.dll`; Schannel uses
Windows' own certificate store and crypto, avoiding a bundled OpenSSL whose
security updates would become Sterna's job. The import-table closure cannot
discover this plugin because no PE import points at it.

## Traps

- **The finish page must not start the program itself.** The installer asks for
  administrator rights, so anything it launches inherits them — and Sterna's
  settings live under the *running user's* AppData. A first run as
  Administrator writes `sterna.ini` into the administrator's profile, and the
  user's own later runs start from defaults, permanently, with nothing to see.
  `StartSterna` goes through `explorer.exe`, which is already running as the
  user and hands the program back its proper token.
- **An upgrade in place leaves the previous version's files behind, and for a
  Qt DLL that is not inert.** The loader finds the stale one first and the
  program dies before `main` with a missing-entry-point box naming a symbol
  nobody has heard of. `.onInit` runs the old uninstaller first; `_?=` is what
  keeps it in place long enough to be waited on, rather than having it copy
  itself to the temp directory and return immediately.
- **`RMDir /r "$INSTDIR"` is a recursive delete of a path the user typed into
  the directory page.** So `build.sh` generates the uninstall list from the
  staging tree: every file by name, every directory with a plain `RMDir`, which
  refuses a directory that is not empty. Verified — a file left in the program
  folder survives the uninstall, and so does the folder.
- **The licence page is a RichEdit control and renders LF-only text as one
  unreadable line.** Every text file a user reads gets CRLF on the way in. The
  `.lng` files do not, because they are read by us.
- **Fedora's MinGW packages are shipped unstripped.** 154 MB staged before
  `--strip-unneeded`, 106 after. Safe on a PE file: the export table a DLL is
  loaded through is part of the image, not part of the symbol table.

## Verifying a build

Wine cannot tell you how this behaves on Windows, and the traps file lists what
it gets wrong. What it *can* answer is the question deployment actually fails
on — did every DLL resolve — and it answers it in a second. Note that the Wine
in `sterna-fedora` wedges in `wineboot` on a fresh prefix (AGENTS.md); the one
in the Ubuntu container works, and a copy of an already-booted `~/.wine` avoids
the boot entirely.

```sh
export WINEPREFIX=$(mktemp -d)/wp && cp -r ~/.wine "$WINEPREFIX"
setup=packaging/windows/build/sterna-0.2.5-x86_64-setup.exe

# 1. it installs, silently, and lands 55 files
/usr/lib/wine/wine64 "$setup" /S
find "$WINEPREFIX/drive_c/Program Files/Sterna" -type f | wc -l

# 2. every DLL resolved and Qt started — the failure this whole exercise is
#    about. A missing one kills it in milliseconds, so surviving is the signal.
cd "$WINEPREFIX/drive_c/Program Files/Sterna"
timeout 12 /usr/lib/wine/wine64 sterna.exe -platform offscreen; echo $?   # 124

# 3. the uninstaller removes what it installed and nothing else
echo hello > notes.txt
/usr/lib/wine/wine64 uninstall.exe /S && sleep 3 && ls        # only notes.txt

# 4. the association, which is registry-only and so is checkable here. Wine
#    flushes the registry when the last process goes, hence the -k.
/usr/lib/wine/wine64 "$setup" /S /ASSOC; sleep 4; /usr/lib/wine/wineserver -k
grep -A3 'Classes\\\\Sterna.MacroFile' "$WINEPREFIX/system.reg"
```

Whether Explorer then *offers* it is not a question Wine can answer, and it is
not one this recipe pretends to ask.

## Not done yet

- **Code signing.** Unsigned, so SmartScreen will warn on first run and the UAC
  prompt says "Unknown publisher". This needs a certificate, which needs a
  legal entity; `signtool` has an `osslsigncode` equivalent that runs on Linux,
  so the build does not have to move when there is something to sign with.
- **A macro opened from Explorer starts a new window.** Upstream would run it
  in the session already on screen. Closing that gap means `ttpmacro` finding a
  running window and falling back to launching one, which changes a shipped
  program's contract — a `.bat` wrapper that today reports "no window" would
  start opening terminals — so it wants deciding rather than assuming.

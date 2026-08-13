# packaging

This file is the AppImage's. The Windows installer is a second artifact with a
README of its own — see [`windows/`](windows/README.md), which is where the
NSIS build and its traps are written down.

One artifact on Linux: an **AppImage**, and nothing else. No rpm, no deb —
decided 2026-08-08, recorded in `PLAN.md`. One thing to build, one thing to
test, and no per-distro packaging to keep alive alongside the Windows installer
while this is one person's project. It also suits the machine it is written for:
the host is Bluefin, an image-based Fedora where layering an rpm is the awkward
path and a self-contained binary is the ordinary one.

Release binaries are built by [`.github/workflows/release.yml`](../.github/workflows/release.yml).
A manual run produces downloadable workflow artifacts without making a release;
pushing a matching `vX.Y.Z` tag builds both platforms and creates a draft
release. The workflow uses a Fedora 44 job container, the same base and package
set as the `sterna-fedora` development container.

The local build remains useful while changing the package:

```sh
distrobox-host-exec distrobox enter sterna-fedora --no-tty -- bash -lc '
  cd ~/Projects/Sterna/packaging/appimage
  ./build.sh --clean      # → build/sterna-x86_64.AppImage + .zsync
  ./build.sh --run        # ...and start it
'
```

Measured from the image on the desktop after signed updates landed, 2026-08-12:
**48 MB on disk**, **46 MB RSS / 39 MB PSS** with a shell attached under
Wayland. The earlier image was 37 MB; Qt Network, its TLS backend and the
on-demand updater library are the increase. They are not mapped until Check for
Updates is chosen; the direct-link prototype used about 5 MB more idle PSS.
Startup was previously measured at about **144 ms** to a mapped window —
including the SquashFS mount, which the build tree does not pay.

## One file, three programs

The control socket needs a client, and on this platform the AppImage *is* the
installation — so `ttctl` and `ttpmacro` are staged next to `sterna` and
reached through AppRun's first argument:

```sh
./sterna-x86_64.AppImage --shell &        # a window
./sterna-x86_64.AppImage ttctl status     # ...and something to ask it
./sterna-x86_64.AppImage ttctl sendln 'uptime'
./sterna-x86_64.AppImage ttpmacro login.ttl
```

Argument dispatch rather than three files, because a single self-contained
binary is the whole point of the format. They go through AppRun rather than
being run out of a mounted image directly, for the reason everything else here
does: they are built against this tree's glibc and need the same
`LD_LIBRARY_PATH`.

Shipping the window without them would be half a feature — the socket exists so
a shell script can drive the terminal, and a user with only the image would have
had a socket and nothing that speaks to it.

## The base is the decision, and it is not settled

An AppImage's floor is the glibc it was linked against. This one is built in
`sterna-fedora`, so:

| | |
|---|---|
| glibc floor | 2.43 — **Fedora 44 and newer, and not much else yet** |
| Qt | 6.11.1, the version the desktop runs |

That is deliberate and temporary. It matches the target desktop exactly, and
everything in `build.sh` except the base is what a portable build will need
anyway. **The follow-up is an older base plus a Qt fetched separately** —
older base for reach, separate Qt because the old distributions that give reach
also ship old Qt.

The Ubuntu 24.04 container was considered as a base and rejected: glibc 2.39
would reach much further, but its Qt 6.4.2 loads Mesa's gallium driver under
Wayland and costs 62 MB of extra private memory (`AGENTS.md`). Bundling that
would ship a regression to every user of a terminal whose claim is being light.

## GitHub builds the release image

The ordinary test workflow still runs on Ubuntu 24.04. The release workflow
uses a Fedora 44 job container instead, so moving the build to GitHub did not
quietly change the glibc floor or substitute Ubuntu's older Qt. It builds from
an empty runner and smoke-tests the completed AppImage before making it
available to the draft-release job.

The update-signing key is deliberately absent from GitHub. The workflow stops
at a draft containing the AppImage, zsync data and Windows installer; the local
[`release.sh`](release.sh) procedure signs those exact downloaded bytes and
publishes only after the six expected assets are present.

## Qt is bundled, and that has consequences

`PLAN.md`'s licensing posture assumed Linux would be an rpm depending on the
distribution's Qt, which costs nothing. An AppImage bundles it, so Linux now
carries the same LGPLv3 obligations Windows does, and `build.sh` discharges
them: Qt stays dynamically linked as separate shared libraries, `LGPL-3.0.txt`
and `GPL-3.0.txt` and `QT-LGPL-NOTICE.md` go **inside** the image under
`usr/share/doc/sterna/`, and `BUILD-INFO.txt` records the exact Qt version and
base so the offer of source points at something specific.

Substituting your own Qt is `--appimage-extract`, drop the library in, repack —
spelled out in `QT-LGPL-NOTICE.md`, which ships in the image rather than only
living here.

## Traps

These cost the whole first afternoon. Each is a place the failure looks like
something other than what it is.

- **linuxdeploy corrupts every library it bundles on this base, silently.** It
  rewrites each one's rpath with its own `patchelf`, and that patchelf predates
  `.relr.dyn`, the compact relocation format Fedora 44 uses everywhere. Its
  `strip` hits the same wall and *says so* — "unknown type [0x13] section
  `.relr.dyn`" — which is why `NO_STRIP=1` is set. `patchelf` says nothing: the
  file comes out about 2 KB larger and segfaults in its own `_init`, before
  `main`, before Qt can log a word. Whichever bundled library the loader reaches
  first is the one in the backtrace, so the crash appears to move between
  libgomp, libicudata and whatever else, and to be about that library.
  **The build lets linuxdeploy do the discovery and then puts the originals
  back**, resolving by `LD_LIBRARY_PATH` from our own `AppRun` instead of by
  patched rpath. A newer tool against an older base would be fine; this is the
  other way round.
- **A Wayland window that never appears is not an error.** Qt's Wayland platform
  plugin loads *more* plugins to do anything: without
  `wayland-shell-integration/libxdg-shell.so` it binds the registry, creates no
  `xdg_toplevel`, and sits there. No warning, no non-zero exit — it looks
  exactly like a working headless run, and it survived the first round of
  testing here for that reason. `WAYLAND_DEBUG=1` and a grep for
  `get_xdg_surface` is the check that actually distinguishes them, and it is
  what the verification below uses. `EXTRA_PLUGINS` is meant to cover this and
  was ignored by the plugin, so the directories are copied by hand — and
  `printsupport` is copied with them, for the same reason one step quieter:
  without `libcupsprintersupport.so`, `QPrinter` finds no printers and the
  window says "no printer is configured", which is a true sentence on a machine
  that has none and indistinguishable from one that has.
- **Distributions do not agree on the platform plugin's name.** Upstream Qt
  splits Wayland into `libqwayland-generic.so` and `libqwayland-egl.so`; Fedora
  ships one `libqwayland.so`. Naming a missing one is a hard error from the Qt
  plugin — after it has already deployed everything else. `build.sh` asks what
  is on disk rather than asserting a spelling, which is also what will keep it
  working when the base moves.
- **linuxdeploy names the deployed icon after the file.** Handing it
  `sterna-256.png` installs an icon called `sterna-256`, and then it fails
  with "could not find suitable icon for Icon entry: sterna". Each size is
  staged as its own `sterna.png` under its own directory.
- **`--appimage-extract-and-run` is required in the container.** linuxdeploy,
  its Qt plugin and appimagetool are all AppImages themselves, and a rootless
  podman container has `/dev/fuse` but no working `fusermount` helper.

## Verifying a build

The three checks worth running, because two of the failures above are silent:

```sh
# 1. it starts on the HOST, where no build tree and no Qt devel exist
QT_QPA_PLATFORM=offscreen timeout 8 ./sterna-x86_64.AppImage --shell -- /bin/echo hi

# 2. a window really maps — not just "the process stayed alive"
WAYLAND_DEBUG=1 timeout 8 ./sterna-x86_64.AppImage --shell -- /bin/echo hi 2>&1 |
    grep -E 'get_xdg_surface|set_title'

# 3. it is OUR Qt that loaded, not the host's
./sterna-x86_64.AppImage --shell -- /bin/sleep 30 & sleep 3
grep -o '/[^ ]*libQt6Core[^ ]*' /proc/$(pgrep -n -f usr/bin/sterna)/maps | sort -u
#   → /tmp/.mount_sterna*/usr/lib/libQt6Core.so.6, not /usr/lib64/...
```

```sh
# 4. and the clients are in there and can find the window
D=$(mktemp -d); export XDG_RUNTIME_DIR=$D
QT_QPA_PLATFORM=offscreen ./sterna-x86_64.AppImage --shell & sleep 2
./sterna-x86_64.AppImage ttctl ls        # → one row, with a pid and a title
./sterna-x86_64.AppImage ttctl close
```

Check 3 matters because the desktop this is developed on *has* Qt 6.11.1
installed. An image that quietly used the host's would pass every other test
here and fail on the first machine that has none.

## Signed in-app updates

Replacing the image works only when Sterna is running from an AppImage in a
writable directory; anything else is offered the release page instead. A check
happens on Help > Check for Updates, and once a day at startup while
`[Sterna] CheckUpdatesOnStartup` is on — the startup one silently unless there
is something to offer. Either verifies the detached manifest signature, then
verifies both SHA-256 and a second Ed25519 signature over the complete
downloaded image. `QSaveFile` writes beside the
old image, restores its execute permissions on the temporary file and renames
it atomically; the running session stays on its mounted old image and the new
one is used at the next start.

The release metadata and key procedure are in
[`update/`](update/README.md). The build also embeds GitHub zsync update
information and produces `sterna-x86_64.AppImage.zsync` for Gear Lever,
AppImageUpdate and other external tools. Sterna's own updater still uses the
same signed full-artifact path on Linux and Windows, so there is one trust
format and one UI inside the application.

Qt Network is deliberately in `libsterna_updater.so`, which the terminal loads
by name only for this action. Linking it into the main shell measured about 5
MB of extra idle PSS before a request existed. The packaged binary's maps are
part of the verification: `libQt6Network.so.6` must be absent before the action
and present after the updater library is loaded.

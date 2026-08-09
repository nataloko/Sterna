# packaging

One artifact on Linux: an **AppImage**, and nothing else. No rpm, no deb —
decided 2026-08-08, recorded in `PLAN.md`. One thing to build, one thing to
test, and no per-distro packaging to keep alive alongside the Windows installer
while this is one person's project. It also suits the machine it is written for:
the host is Bluefin, an image-based Fedora where layering an rpm is the awkward
path and a self-contained binary is the ordinary one.

```sh
# Qt work happens in the sterna-fedora container. See CLAUDE.md for why.
distrobox-host-exec distrobox enter sterna-fedora --no-tty -- bash -lc '
  cd ~/Projects/Sterna/packaging/appimage
  ./build.sh              # → build/sterna-x86_64.AppImage
  ./build.sh --clean      # ...from scratch
  ./build.sh --run        # ...and start it
'
```

Measured from the image on the desktop, 2026-08-08: **37 MB on disk**, **43 MB
RSS / 33 MB PSS** with a shell attached under Wayland, and about **144 ms** from
exec to a mapped window — which includes mounting the SquashFS, a cost the build
tree does not pay.

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
Wayland and costs 62 MB of extra private memory (`CLAUDE.md`). Bundling that
would ship a regression to every user of a terminal whose claim is being light.

## Not in CI, and that is deliberate

CI runs on `ubuntu-24.04`, which is precisely the base this build rejects — a
job there would produce, on every push, the artifact the section above explains
why not to ship. So the AppImage is a release step against a base CI does not
have, and it stays a manual build until the portable base exists. What CI *does*
cover is everything the image is made of: the shell compiles and its render,
telnet, SSH and pty tests run there already.

The install rules the image needs (`shell/CMakeLists.txt`) are exercised by
every build, so the half of this that can rot silently does not.

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
  was ignored by the plugin, so the directories are copied by hand.
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

Check 3 matters because the desktop this is developed on *has* Qt 6.11.1
installed. An image that quietly used the host's would pass every other test
here and fail on the first machine that has none.

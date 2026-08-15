# packaging

This file is the AppImage's. The Windows installer is a second artifact with a
README of its own — see [`windows/`](windows/README.md), which is where the
NSIS build and its traps are written down. **Cutting a release is
[`RELEASING.md`](RELEASING.md)**, which is the order of operations and the two
scripts; this file is what the Linux artifact is made of.

One artifact on Linux: an **AppImage**, and nothing else. No rpm, no deb —
decided 2026-08-08, recorded in `docs/history.md`. One thing to build, one thing to
test, and no per-distro packaging to keep alive alongside the Windows installer
while this is one person's project. It also suits the machine it is written for:
the host is Bluefin, an image-based Fedora where layering an rpm is the awkward
path and a self-contained binary is the ordinary one.

Release binaries are built by [`.github/workflows/release.yml`](../.github/workflows/release.yml).
A manual run produces downloadable workflow artifacts without making a release;
pushing a matching `vX.Y.Z` tag builds both platforms and creates a draft
release. The Linux job uses the maintained `manylinux_2_28` x86-64 image.

## The Qt in the image is downloaded, not built and not cached

Qt 6.11.1 is compiled from its verified source archives on that base — the
official Linux binaries need glibc 2.39 and would raise the floor by eleven
releases of it. Compiling it takes forty minutes on one worker, so it is done
**once, by hand**, and published as a release asset that every build downloads
in seconds:

| | |
|---|---|
| pinned in | [`appimage/toolchain.env`](appimage/toolchain.env) — image digest, tag, file, SHA-256 |
| fetched by | [`appimage/fetch-qt.sh`](appimage/fetch-qt.sh), which verifies the digest and then `build-qt.sh --check` |
| rebuilt by | [`appimage/publish-qt.sh`](appimage/publish-qt.sh), when Qt, the recipe or the base image moves |
| checked by | `ci.yml`'s `qt-toolchain` job, on every push |

An Actions cache was tried first and is the wrong shape twice over. A cache
belongs to the ref that wrote it and is readable only from that ref or the
default branch — so a tag build saved forty minutes of Qt that no later release
could ever open, four times. Warming it on `main` fixes the scope but not the
10 GB repository limit, which evicts least-recently-used: 26 MiB of Qt goes
before anything else in a repository full of 400 MiB Rust caches, and the
symptom is a release that takes fifty minutes with no explanation. A release
asset is scoped to nobody and evicted by nothing.

Nothing is trusted because it was downloaded. The digest ties the bytes to the
publish that was reviewed, and `build-qt.sh --check` then holds the unpacked
tree to the same contract a fresh build passes — the exact version, and the
four plugins without which the window never appears (`AGENTS.md` has the story
of the missing one). If any of that fails, the release job builds Qt from
source and takes fifty minutes instead. Slower, never blocked.

The local release-equivalent build uses Podman directly. `fetch-qt.sh` needs no
credentials, so the toolchain arrives the same way it does in CI:

```sh
./packaging/appimage/fetch-qt.sh
podman run --rm --security-opt label=disable \
  -v "$PWD:/repo:rw" \
  -v "$HOME/.cargo:/root/.cargo:rw" \
  -v "$HOME/.rustup:/root/.rustup:ro" \
  -w /repo \
  quay.io/pypa/manylinux_2_28_x86_64@sha256:f854c50adf7b7a325bc4794316f3758d387a41d61f9e2ebca0f26c7dc8f761d4 \
  bash -lc '
    ./packaging/appimage/install-build-deps.sh
    ./packaging/appimage/build-qt.sh
    export PATH=$PWD/packaging/appimage/toolchain/qt-6.11.1/bin:$PATH
    export CMAKE_BUILD_PARALLEL_LEVEL=4 CARGO_BUILD_JOBS=4
    ./packaging/appimage/build.sh --clean
  '
```

`build-qt.sh` inside the container then finds the prefix, checks it and exits;
drop the fetch to build Qt yourself instead.

The two Rust mounts reuse a normal rustup installation; compilation and
linking still happen entirely against the container's glibc 2.28 userspace.

The portable local release-equivalent image is **32 MB on disk**. The earlier
Fedora-built release measured **46 MB RSS / 39 MB PSS** with a shell attached
under Wayland and about **144 ms** to a mapped window, including its SquashFS
mount. Remeasure those runtime figures once the portable image ships.

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

## Portable baseline

An AppImage's floor is the newest glibc symbol imported by any executable or
library it ships. Sterna targets the same baseline as `manylinux_2_28`:

| | |
|---|---|
| runtime glibc floor | 2.28 — Debian 10+, Ubuntu 20.04+, RHEL 8+, Fedora 29+ |
| Qt | 6.11.1, built from official source on that baseline |

`build.sh` inspects every packaged ELF and fails above `GLIBC_2.28`,
`GLIBCXX_3.4.25`, or `CXXABI_1.3.11`. The C++ gates matter because AppImage
correctly leaves the host's `libstdc++` in place. Together these make the stated
reach a release gate rather than an assumption based only on the container's
`ldd --version`. Musl distributions still need a glibc compatibility layer.

## GitHub builds the release image

The ordinary test workflow still runs on Ubuntu 24.04. The release workflow
uses the manylinux container, builds or restores its pinned Qt prefix, then
smoke-tests the completed AppImage before making it available to the
draft-release job. A clean Qt bootstrap is split into resumable 30-minute
steps: the hosted runner abandons a single long container step at about fifty
minutes without preserving its log. The installed prefix is cached only after
the final slice verifies all required plugins.

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

- **Keep the deployed Qt libraries unmodified.** The build lets linuxdeploy do
  dependency discovery, restores the original files from the pinned Qt prefix,
  and resolves them through `LD_LIBRARY_PATH` in `AppRun`. Besides avoiding
  packaging-tool rpath differences, this leaves the LGPL replacement seam
  literal: the shipped shared libraries are the build's shared libraries.
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

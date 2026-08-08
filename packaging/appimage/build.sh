#!/usr/bin/env bash
# Build termitta as an AppImage — the only Linux artifact there is going to be.
#
#   ./build.sh              build it into build/
#   ./build.sh --run        ...and then start it, to prove it does
#   ./build.sh --clean      throw the build tree away first
#
# **This must be run inside a container, not on the host, and which container is
# the whole question.** An AppImage's floor is the glibc it was linked against,
# so the base decides who can run the result:
#
#   termitta-fedora   glibc 2.43, Qt 6.11.1   Fedora 44+ and not much else
#   the agents box    glibc 2.39, Qt 6.4.2    wide reach, old Qt — see below
#
# Built in `termitta-fedora` today (decision 2026-08-08): it matches the desktop
# this is being written for, and everything below except the base is what a
# portable build will need anyway. The Ubuntu box was considered and rejected as
# a base: its Qt 6.4.2 loads Mesa's gallium driver under Wayland and costs 62 MB
# of extra private memory (CLAUDE.md), and bundling that would ship a regression
# to every user of a terminal whose claim is being light. Reaching older distros
# means an older base *and* a Qt fetched separately, which is the follow-up.
#
# What the licence requires of this script, since an AppImage bundles Qt rather
# than depending on the distribution's: never static-link Qt, keep it as
# separate shared libraries a user can substitute, and put the LGPL text and an
# offer of source *inside the image*. See QT-LGPL-NOTICE.md, which is copied in.
set -uo pipefail

cd "$(dirname "$0")"
root=$(cd ../.. && pwd)

RUN=0
CLEAN=0
for a in "$@"; do
	case "$a" in
		--run) RUN=1 ;;
		--clean) CLEAN=1 ;;
		-h|--help) sed -n '2,27p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
		*) echo "build.sh: unknown option '$a'" >&2; exit 2 ;;
	esac
done

build=$PWD/build
appdir=$build/AppDir
tools=$build/tools

[ "$CLEAN" = 1 ] && rm -rf "$build"
mkdir -p "$tools"

# --- the tools ---------------------------------------------------------------
#
# Pinned to release tags rather than to "continuous", for the same reason the
# oracle pins Tera Term and esctest pins esctest2: a build tool that changes
# under us turns the artifact red with no change on our side.
LD_URL=https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20250213-2/linuxdeploy-x86_64.AppImage
LDQT_URL=https://github.com/linuxdeploy/linuxdeploy-plugin-qt/releases/download/1-alpha-20250213-1/linuxdeploy-plugin-qt-x86_64.AppImage
AT_URL=https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage

fetch() {
	local url=$1 dest=$2
	[ -x "$dest" ] && return 0
	echo "appimage: fetching $(basename "$dest")" >&2
	curl -fsSL "$url" -o "$dest" || { echo "appimage: cannot fetch $url" >&2; return 1; }
	chmod +x "$dest"
}
fetch "$LD_URL" "$tools/linuxdeploy" || exit 2
fetch "$LDQT_URL" "$tools/linuxdeploy-plugin-qt" || exit 2
fetch "$AT_URL" "$tools/appimagetool" || exit 2

# linuxdeploy, its plugin and appimagetool are themselves AppImages, and a
# rootless container has /dev/fuse but no working fusermount setuid helper.
# Extract-and-run is the documented way out and costs a tmpdir per invocation.
export APPIMAGE_EXTRACT_AND_RUN=1

# linuxdeploy carries its own binutils, and they are older than the libraries
# they are pointed at. See "the repair" below: this switches off the half of
# that which fails loudly.
export NO_STRIP=1

# cargo is on PATH only for login shells in the dev container.
export PATH="$HOME/.cargo/bin:$tools:$PATH"
command -v cargo >/dev/null || { echo "appimage: cargo not found" >&2; exit 2; }
command -v qmake6 >/dev/null || { echo "appimage: qmake6 not found — is this the Qt container?" >&2; exit 2; }

qt_plugins=$(qmake6 -query QT_INSTALL_PLUGINS)
qt_libs=$(qmake6 -query QT_INSTALL_LIBS)

# --- build and install -------------------------------------------------------
#
# Release, which also builds the Rust core with --release: CMakeLists drives
# cargo and picks the profile from CMAKE_BUILD_TYPE.
echo "appimage: building" >&2
cmake -S "$root/shell" -B "$build/cmake" -G Ninja \
	-DCMAKE_BUILD_TYPE=Release \
	-DCMAKE_INSTALL_PREFIX="$appdir/usr" >/dev/null || exit 2
cmake --build "$build/cmake" --target termitta || exit 2

rm -rf "$appdir"
cmake --install "$build/cmake" >/dev/null || exit 2

# --- what the licences oblige ------------------------------------------------
docs=$appdir/usr/share/doc/termitta
mkdir -p "$docs"
cp "$root/LICENSE" "$docs/LICENSE"
cp "$root/ATTRIBUTION.md" "$docs/ATTRIBUTION.md"
cp QT-LGPL-NOTICE.md LGPL-3.0.txt GPL-3.0.txt "$docs/"

# Named in QT-LGPL-NOTICE.md as where to look for the exact Qt this was built
# against, so the offer of source points at something specific rather than at
# "some Qt 6".
{
	echo "termitta AppImage build"
	echo
	echo "built:      $(date -u +%Y-%m-%dT%H:%M:%SZ)"
	echo "commit:     $(git -C "$root" rev-parse HEAD 2>/dev/null || echo unknown)"
	echo "base:       $(. /etc/os-release && echo "$PRETTY_NAME")"
	echo "glibc:      $(ldd --version | head -1)"
	echo "Qt:         $(qmake6 -query QT_VERSION), as packaged by the base above"
	echo "Qt source:  https://download.qt.io/official_releases/qt/"
	echo
	echo "The glibc line is this image's floor: it will not start on a system"
	echo "with an older one."
} > "$docs/BUILD-INFO.txt"

# --- bundle ------------------------------------------------------------------
#
# The Qt plugin is what pulls in the platform plugins — without it the image
# starts on the build machine, where Qt is installed, and nowhere else, which
# is the failure this whole exercise is about.
QMAKE=$(command -v qmake6)
export QMAKE
export EXTRA_QT_MODULES="waylandclient"

# Asked for by what is actually on disk, not by name, because distributions do
# not agree on the spelling: upstream Qt splits the Wayland platform plugin into
# `libqwayland-generic.so` and `libqwayland-egl.so`, and Fedora ships one
# `libqwayland.so`. Naming the wrong one is a hard error from the plugin —
# "cannot deploy non-existing library file" — after it has already deployed
# everything else. `offscreen` is here because a terminal that can run headless
# is testable headless.
want=
for p in libqwayland.so libqwayland-generic.so libqwayland-egl.so libqxcb.so libqoffscreen.so; do
	[ -e "$qt_plugins/platforms/$p" ] && want="$want${want:+;}$p"
done
[ -n "$want" ] || { echo "appimage: no usable platform plugin in $qt_plugins" >&2; exit 2; }
export EXTRA_PLATFORM_PLUGINS="$want"

# linuxdeploy names the deployed icon after the *file*, and then looks for the
# desktop entry's `Icon=` among those names — so handing it `termitta-256.png`
# installs an icon called `termitta-256` and then fails with "could not find
# suitable icon for Icon entry: termitta". Each size is staged under its own
# directory so they can all be called `termitta.png` without colliding; the
# size in the AppDir comes from the image, not from the path.
icons=$build/icons
rm -rf "$icons"
icon_args=()
for size in 64 128 256 512; do
	src=$root/assets/branding/termitta/icons/termitta-$size.png
	[ -e "$src" ] || continue
	mkdir -p "$icons/$size"
	cp "$src" "$icons/$size/termitta.png"
	icon_args+=(--icon-file "$icons/$size/termitta.png")
done

echo "appimage: bundling ($want)" >&2
"$tools/linuxdeploy" \
	--appdir "$appdir" \
	--executable "$appdir/usr/bin/termitta" \
	--library "$appdir/usr/lib/libtermitta.so" \
	--desktop-file termitta.desktop \
	"${icon_args[@]}" \
	--plugin qt || exit 2

# The Wayland platform plugin loads *more* plugins to do anything at all, and
# linuxdeploy does not know that. Without
# `wayland-shell-integration/libxdg-shell.so` it binds the registry, creates no
# `xdg_toplevel`, and the process sits there with **no window and no error** —
# which looks exactly like a working headless run and is how it survived the
# first round of testing here. `EXTRA_PLUGINS` is meant to cover this and was
# ignored, so they are copied straight from the Qt tree, which is also where
# the repair below would have fetched them from.
for d in wayland-shell-integration wayland-decoration-client \
         wayland-graphics-integration-client; do
	[ -d "$qt_plugins/$d" ] && cp -r "$qt_plugins/$d" "$appdir/usr/plugins/"
done
[ -e "$appdir/usr/plugins/wayland-shell-integration/libxdg-shell.so" ] || {
	echo "appimage: no xdg-shell integration — the window would never map" >&2
	exit 2
}

# And every Qt library those plugins ask for has to come along with them. A
# target system has glibc and Mesa; it does not have Qt, which is the whole
# point. Closed transitively rather than in one pass, because the plugins pull
# `libQt6WlShellIntegration` which pulls more again.
qt_needed() {
	find "$appdir/usr/plugins" -name '*.so' -print0 |
		xargs -0 -r readelf -d 2>/dev/null |
		sed -n 's/.*NEEDED.*\[\(libQt6[^]]*\)\].*/\1/p'
	readelf -d "$appdir"/usr/lib/libQt6*.so.6 2>/dev/null |
		sed -n 's/.*NEEDED.*\[\(libQt6[^]]*\)\].*/\1/p'
}
for _ in 1 2 3 4 5; do
	missing=
	while read -r so; do
		[ -n "$so" ] || continue
		[ -e "$appdir/usr/lib/$so" ] && continue
		if [ -e "$qt_libs/$so" ]; then
			cp -f "$qt_libs/$so" "$appdir/usr/lib/$so"
		else
			missing="$missing $so"
		fi
	done < <(qt_needed | sort -u)
	[ -z "$missing" ] || {
		echo "appimage: plugins need Qt libraries that are not on this system:$missing" >&2
		exit 2
	}
	# Nothing new was copied, so the closure is complete.
	[ -z "$(qt_needed | sort -u | while read -r so; do
		[ -n "$so" ] && [ ! -e "$appdir/usr/lib/$so" ] && echo "$so"; done)" ] && break
done

# --- the repair --------------------------------------------------------------
#
# **linuxdeploy corrupts every library it bundles on this base, silently.**
#
# It rewrites each one's rpath with its own `patchelf`, and that patchelf is
# older than `.relr.dyn`, the compact relocation format Fedora 44 uses
# everywhere. Its `strip` says so out loud — "unknown type [0x13] section
# `.relr.dyn`" — and NO_STRIP above silences that. Its `patchelf` hits the same
# wall and does not say anything: the file comes out ~2 KB larger and segfaults
# in its own `_init`, before `main`, before Qt can log a word. Whichever
# bundled library the loader reaches first is the one in the backtrace, so the
# crash appears to move around and to be about that library.
#
# So: let linuxdeploy do the discovery, which it does well, and then put the
# originals back. Nothing then has an rpath, which is why AppRun below sets
# LD_LIBRARY_PATH — resolution by environment instead of by patched binary.
# It also means the image is no longer sensitive to which patchelf is around.
echo "appimage: restoring the libraries linuxdeploy rewrote" >&2
restored=0
for f in "$appdir"/usr/lib/*.so*; do
	[ -f "$f" ] || continue
	b=$(basename "$f")
	# libtermitta.so is ours and is not on the system to restore from.
	[ "$b" = "libtermitta.so" ] && continue
	for d in "$qt_libs" /usr/lib64 /usr/lib /lib64; do
		if [ -f "$d/$b" ]; then
			cp -f "$d/$b" "$f"
			restored=$((restored + 1))
			break
		fi
	done
done
while IFS= read -r p; do
	rel=${p#"$appdir"/usr/plugins/}
	if [ -f "$qt_plugins/$rel" ]; then
		cp -f "$qt_plugins/$rel" "$p"
		restored=$((restored + 1))
	fi
done < <(find "$appdir/usr/plugins" -name '*.so' 2>/dev/null)
echo "appimage: restored $restored" >&2

# Our own AppRun, replacing linuxdeploy's, for one reason: the restored
# libraries have no rpath and have to find each other. linuxdeploy's version
# sources exactly the hooks that existed when it ran, so adding one afterwards
# does nothing — this sources whatever is there.
cat > "$appdir/AppRun" <<'APPRUN'
#!/usr/bin/env bash
# Resolution is by LD_LIBRARY_PATH rather than by rpath: the bundled libraries
# are byte-for-byte the distribution's, un-patched, because the tool that would
# have patched them corrupts this base's. Plugins are found through usr/bin/qt.conf.
here=$(readlink -f "$(dirname "$0")")
export LD_LIBRARY_PATH="$here/usr/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
for hook in "$here"/apprun-hooks/*.sh; do
	[ -e "$hook" ] && . "$hook"
done
exec "$here/usr/bin/termitta" "$@"
APPRUN
chmod +x "$appdir/AppRun"
rm -f "$appdir/AppRun.wrapped"

# --- pack --------------------------------------------------------------------
echo "appimage: packing" >&2
out=termitta-x86_64.AppImage
rm -f "$build/$out"
"$tools/appimagetool" "$appdir" "$build/$out" >/dev/null 2>&1 || {
	echo "appimage: appimagetool failed" >&2; exit 2; }

echo
echo "appimage: $build/$out"
echo "          $(du -h "$build/$out" | cut -f1), glibc floor $(ldd --version | head -1 | grep -o '[0-9]\+\.[0-9]\+$')"

if [ "$RUN" = 1 ]; then
	echo "appimage: starting it" >&2
	"$build/$out" --shell
fi

#!/usr/bin/env bash
# Build Sterna as an AppImage — the only Linux artifact there is going to be.
#
#   ./build.sh              build the AppImage and its zsync metadata into build/
#   ./build.sh --run        ...and then start it, to prove it does
#   ./build.sh --clean      throw the build tree away first
#
# Run this inside the maintained manylinux_2_28 x86-64 container. The old Fedora
# 44 build required glibc 2.43, which made a supposedly portable artifact run on
# Fedora 44 and little else. This build has an enforced glibc 2.28 ceiling and
# uses Qt 6.11.1 compiled on that same baseline by `build-qt.sh`.
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
command -v qmake6 >/dev/null || { echo "appimage: qmake6 not found — run build-qt.sh first" >&2; exit 2; }
command -v readelf >/dev/null || { echo "appimage: readelf not found — dnf install binutils" >&2; exit 2; }
command -v zsyncmake >/dev/null || { echo "appimage: zsyncmake not found — run install-build-deps.sh first" >&2; exit 2; }

version=$(sed -n '/^\[workspace\.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' \
	"$root/crates/Cargo.toml" | head -1)
[ -n "$version" ] || { echo "appimage: no version in crates/Cargo.toml" >&2; exit 2; }

qt_plugins=$(qmake6 -query QT_INSTALL_PLUGINS)
qt_libs=$(qmake6 -query QT_INSTALL_LIBS)
qt_version=$(qmake6 -query QT_VERSION)
[ "$qt_version" = 6.11.1 ] || {
	echo "appimage: Qt 6.11.1 is required, found $qt_version" >&2
	exit 2
}
# This Qt lives in a private, cached prefix rather than the base image's linker
# cache. Make that prefix visible to linuxdeploy's dependency resolver.
export LD_LIBRARY_PATH="$qt_libs${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

# --- build and install -------------------------------------------------------
#
# Release, which also builds the Rust core with --release: CMakeLists drives
# cargo and picks the profile from CMAKE_BUILD_TYPE.
echo "appimage: building" >&2
cmake -S "$root/shell" -B "$build/cmake" -G "Unix Makefiles" \
	-DCMAKE_BUILD_TYPE=Release \
	-DCMAKE_INSTALL_PREFIX="$appdir/usr" >/dev/null || exit 2
cmake --build "$build/cmake" --target sterna || exit 2

rm -rf "$appdir"
cmake --install "$build/cmake" >/dev/null || exit 2

# The two control-socket clients, which CMake does not build: they are cargo
# binaries in the core's own workspace and the shell does not link them. An
# AppImage that shipped a window with a control socket and no way to talk to it
# would be half of a feature — the socket exists so that a shell script can
# drive the terminal, and on this platform the AppImage *is* the installation.
#
# They go beside `sterna` and are reached through AppRun's first argument; see
# the dispatch there.
echo "appimage: building the clients" >&2
client_target=$build/cmake/cargo
CARGO_TARGET_DIR=$client_target cargo build --release \
	--manifest-path "$root/crates/Cargo.toml" \
	-p tt-ctl --bins || exit 2
for client in ttctl ttpmacro; do
	cp "$client_target/release/$client" "$appdir/usr/bin/$client" || exit 2
done

# --- what the licences oblige ------------------------------------------------
docs=$appdir/usr/share/doc/sterna
mkdir -p "$docs"
cp "$root/LICENSE" "$docs/LICENSE"
cp "$root/ATTRIBUTION.md" "$docs/ATTRIBUTION.md"
cp QT-LGPL-NOTICE.md LIBGLVND-LICENSE.txt LGPL-3.0.txt GPL-3.0.txt "$docs/"

# Named in QT-LGPL-NOTICE.md as where to look for the exact Qt this was built
# against, so the offer of source points at something specific rather than at
# "some Qt 6".
{
	echo "Sterna AppImage build"
	echo
	echo "built:      $(date -u +%Y-%m-%dT%H:%M:%SZ)"
	echo "commit:     $(git -C "$root" rev-parse HEAD 2>/dev/null || echo unknown)"
	echo "base:       $(. /etc/os-release && echo "$PRETTY_NAME")"
	echo "glibc:      $(ldd --version | head -1)"
	echo "Qt:         $qt_version, built from the official source on the base above"
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
# desktop entry's `Icon=` among those names — so handing it `sterna-256.png`
# installs an icon called `sterna-256` and then fails with "could not find
# suitable icon for Icon entry: sterna". Each size is staged under its own
# directory so they can all be called `sterna.png` without colliding; the
# size in the AppDir comes from the image, not from the path.
icons=$build/icons
rm -rf "$icons"
icon_args=()
for size in 64 128 256 512; do
	src=$root/assets/branding/sterna/icons/sterna-$size.png
	[ -e "$src" ] || continue
	mkdir -p "$icons/$size"
	cp "$src" "$icons/$size/sterna.png"
	icon_args+=(--icon-file "$icons/$size/sterna.png")
done

echo "appimage: bundling ($want)" >&2
"$tools/linuxdeploy" \
	--appdir "$appdir" \
	--executable "$appdir/usr/bin/sterna" \
	--library "$appdir/usr/lib/libsterna.so" \
	--desktop-file sterna.desktop \
	"${icon_args[@]}" \
	--plugin qt || exit 2

# linuxdeploy classifies every OpenGL-shaped library as part of the host's
# driver stack. QtGui, however, is directly linked to GLVND's driver-neutral
# ABI frontends even when the application uses only the raster painter. A
# machine without those four frontends cannot load Sterna far enough to select
# the offscreen plugin. Bundle the dispatch ABI, never a Mesa/NVIDIA driver.
for so in libOpenGL.so.0 libEGL.so.1 libGLX.so.0 libGLdispatch.so.0; do
	found=
	for d in /usr/lib64 /usr/lib /lib64; do
		if [ -e "$d/$so" ]; then
			cp -Lf "$d/$so" "$appdir/usr/lib/$so"
			found=1
			break
		fi
	done
	[ -n "$found" ] || {
		echo "appimage: required GLVND frontend $so is missing" >&2
		exit 2
	}
done

# The Wayland platform plugin loads *more* plugins to do anything at all, and
# linuxdeploy does not know that. Without
# `wayland-shell-integration/libxdg-shell.so` it binds the registry, creates no
# `xdg_toplevel`, and the process sits there with **no window and no error** —
# which looks exactly like a working headless run and is how it survived the
# first round of testing here. `EXTRA_PLUGINS` is meant to cover this and was
# ignored, so they are copied straight from the Qt tree, which is also where
# the repair below would have fetched them from.
#
# `printsupport` is the same shape of gap for the same reason: File > Print and
# the media copy sequences reach `QPrinter`, which finds no printers at all
# without `libcupsprintersupport.so` — and says nothing, because "no printer is
# configured" is a real answer on a machine that has none.
for d in wayland-shell-integration wayland-decoration-client \
         wayland-graphics-integration-client printsupport; do
	[ -d "$qt_plugins/$d" ] && cp -r "$qt_plugins/$d" "$appdir/usr/plugins/"
done
[ -e "$appdir/usr/plugins/wayland-shell-integration/libxdg-shell.so" ] || {
	echo "appimage: no xdg-shell integration — the window would never map" >&2
	exit 2
}
# GNOME draws no title bars, so this plugin *is* the title bar. Falling back to
# Qt Base's `bradient` is not an error and not visible from a log: the window
# opens, wearing a title bar from 1995 that cannot recognise a double click.
[ -e "$appdir/usr/plugins/wayland-decoration-client/libadwaita.so" ] || {
	echo "appimage: no Adwaita decoration — the title bar would be bradient's" >&2
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

# --- restore the source libraries -------------------------------------------
#
# Keep the original Qt files rather than linuxdeploy's rpath-rewritten copies.
# AppRun deliberately supplies their search path. This also preserves the LGPL
# substitution seam and keeps the package recipe identical across build bases.
echo "appimage: restoring the libraries linuxdeploy rewrote" >&2
restored=0
for f in "$appdir"/usr/lib/*.so*; do
	[ -f "$f" ] || continue
	b=$(basename "$f")
	# libsterna.so is ours and is not on the system to restore from.
	[ "$b" = "libsterna.so" ] && continue
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

# Do not let a new Rust, C++ or Qt dependency silently undo the portable base.
# Search every versioned import in every shipped ELF. libstdc++ is deliberately
# supplied by the host, so its ABI ceiling matters just as much as glibc's.
# These limits are the common denominator of the documented Debian 10, RHEL 8
# and Fedora 29 floor.
version_info=$(find "$appdir/usr" -type f -print0 | xargs -0 -r file | \
	sed -n 's/: .*ELF.*//p' | while read -r file; do
		readelf --version-info "$file" 2>/dev/null || true
	done)
glibc_floor=2.28
glibcxx_ceiling=3.4.25
cxxabi_ceiling=1.3.11
newest_glibc=$(printf '%s\n' "$version_info" | grep -o 'GLIBC_[0-9][0-9.]*' | \
	sed 's/GLIBC_//' | sort -Vu | tail -1)
newest_glibcxx=$(printf '%s\n' "$version_info" | grep -o 'GLIBCXX_[0-9][0-9.]*' | \
	sed 's/GLIBCXX_//' | sort -Vu | tail -1)
newest_cxxabi=$(printf '%s\n' "$version_info" | grep -o 'CXXABI_[0-9][0-9.]*' | \
	sed 's/CXXABI_//' | sort -Vu | tail -1)
[ -n "$newest_glibc" ] || {
	echo "appimage: could not determine the packaged glibc requirement" >&2
	exit 2
}
[ -n "$newest_glibcxx" ] && [ -n "$newest_cxxabi" ] || {
	echo "appimage: could not determine the packaged libstdc++ requirement" >&2
	exit 2
}
if [ "$(printf '%s\n%s\n' "$glibc_floor" "$newest_glibc" | sort -Vu | tail -1)" != "$glibc_floor" ]; then
	echo "appimage: packaged ELF requires GLIBC_$newest_glibc, above $glibc_floor" >&2
	exit 2
fi
if [ "$(printf '%s\n%s\n' "$glibcxx_ceiling" "$newest_glibcxx" | sort -Vu | tail -1)" != "$glibcxx_ceiling" ]; then
	echo "appimage: packaged ELF requires GLIBCXX_$newest_glibcxx, above $glibcxx_ceiling" >&2
	exit 2
fi
if [ "$(printf '%s\n%s\n' "$cxxabi_ceiling" "$newest_cxxabi" | sort -Vu | tail -1)" != "$cxxabi_ceiling" ]; then
	echo "appimage: packaged ELF requires CXXABI_$newest_cxxabi, above $cxxabi_ceiling" >&2
	exit 2
fi
printf '\nverified max imports: GLIBC_%s, GLIBCXX_%s, CXXABI_%s\n' \
	"$newest_glibc" "$newest_glibcxx" "$newest_cxxabi" \
	>> "$docs/BUILD-INFO.txt"

# Our own AppRun, replacing linuxdeploy's, for one reason: the restored
# libraries have no rpath and have to find each other. linuxdeploy's version
# sources exactly the hooks that existed when it ran, so adding one afterwards
# does nothing — this sources whatever is there.
cat > "$appdir/AppRun" <<'APPRUN'
#!/usr/bin/env bash
# Resolution is by LD_LIBRARY_PATH rather than by rpath: the bundled libraries
# are byte-for-byte the build inputs, un-patched, preserving the LGPL
# substitution seam. Plugins are found through usr/bin/qt.conf.
here=$(readlink -f "$(dirname "$0")")
# The programs take their own libraries back out of LD_LIBRARY_PATH before
# they start anything, so that a shell opened in the terminal gets the host's
# libraries and not ours; they find them by APPDIR. The AppImage runtime sets
# it, an extracted AppDir run directly has nobody to, and the scrub is keyed on
# it — so it is set here rather than assumed.
export APPDIR="${APPDIR:-$here}"
export LD_LIBRARY_PATH="$here/usr/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
for hook in "$here"/apprun-hooks/*.sh; do
	[ -e "$hook" ] && . "$hook"
done

# One AppImage, three programs. `sterna.AppImage ttctl status` runs the client
# rather than the terminal, which is the only way to reach a second binary
# inside a single-file artifact — and the clients need the same
# LD_LIBRARY_PATH, since they are built against this tree's glibc rather than
# the host's.
case ${1-} in
ttctl | ttpmacro)
	prog=$1
	shift
	exec "$here/usr/bin/$prog" "$@"
	;;
esac
exec "$here/usr/bin/sterna" "$@"
APPRUN
chmod +x "$appdir/AppRun"
rm -f "$appdir/AppRun.wrapped"

# --- pack --------------------------------------------------------------------
echo "appimage: packing" >&2
out=sterna-x86_64.AppImage
zsync=$out.zsync
update_info='gh-releases-zsync|nataloko|Sterna|latest|sterna-x86_64.AppImage.zsync'
rm -f "$build/$out" "$build/$zsync" "$PWD/$zsync"
export VERSION="$version"
"$tools/appimagetool" -u "$update_info" "$appdir" "$build/$out" >/dev/null 2>&1 || {
	echo "appimage: appimagetool failed" >&2; exit 2; }
# appimagetool versions disagree on whether zsyncmake's output follows the
# destination or the current directory. Put it beside the AppImage either way.
[ ! -f "$PWD/$zsync" ] || mv "$PWD/$zsync" "$build/$zsync"
[ -f "$build/$zsync" ] || { echo "appimage: appimagetool produced no $zsync" >&2; exit 2; }
readelf --string-dump=.upd_info "$build/$out" | grep -Fq "$update_info" || {
	echo "appimage: update information is missing from $out" >&2; exit 2; }

echo
echo "appimage: $build/$out"
echo "          $(du -h "$build/$out" | cut -f1), glibc floor $newest_glibc (ceiling $glibc_floor)"
echo "appimage: $build/$zsync"
echo "          $(du -h "$build/$zsync" | cut -f1)"

if [ "$RUN" = 1 ]; then
	echo "appimage: starting it" >&2
	"$build/$out" --shell
fi

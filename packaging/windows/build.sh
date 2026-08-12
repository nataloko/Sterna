#!/usr/bin/env bash
# Build the Windows installer — the only Windows artifact there is going to be.
#
#   ./build.sh              build it into build/
#   ./build.sh --clean      throw the build tree away first
#   ./build.sh --stage      stop after staging, and do not run makensis
#
# **This runs on Linux and produces a Windows program.** The shell is
# cross-compiled with `mingw64-cmake`, and NSIS is assembled by a native
# `makensis`, so nothing here needs Wine. That is the reason NSIS is used at
# all: upstream Tera Term ships an Inno Setup script (`installer/teraterm.iss`)
# and `iscc.exe` is a Windows program, so an Inno build would put Wine on the
# release path. Wine has already manufactured several false findings on this
# project — see AGENTS.md — and a release artifact is the last place to want
# one. NSIS is the format whose compiler is a Linux binary.
#
# **Run it in `sterna-fedora`, not the Ubuntu box.** That is where
# `mingw64-qt6-qtbase` and the MinGW toolchain live:
#
#   distrobox-host-exec distrobox enter sterna-fedora --no-tty -- bash -lc '
#     cd ~/Projects/Sterna/packaging/windows && ./build.sh'
#
# What the licence requires of this script is the same as the AppImage's, and
# for the same reason: Qt is bundled rather than depended on. Never static-link
# it, keep it as separate DLLs a user can substitute, and put the LGPL text and
# an offer of source *inside the installed tree*. See QT-LGPL-NOTICE.md.
set -uo pipefail

cd "$(dirname "$0")"
root=$(cd ../.. && pwd)

CLEAN=0
STAGE_ONLY=0
for a in "$@"; do
	case "$a" in
		--clean) CLEAN=1 ;;
		--stage) STAGE_ONLY=1 ;;
		-h|--help) sed -n '2,26p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
		*) echo "build.sh: unknown option '$a'" >&2; exit 2 ;;
	esac
done

build=$PWD/build
stage=$build/stage

[ "$CLEAN" = 1 ] && rm -rf "$build"
mkdir -p "$build"

# cargo is on PATH only for login shells in the dev container.
export PATH="$HOME/.cargo/bin:$PATH"

need() {
	command -v "$1" >/dev/null || { echo "windows: $1 not found — $2" >&2; exit 2; }
}
need cargo "export \$HOME/.cargo/bin"
need mingw64-cmake "dnf install mingw64-qt6-qtbase mingw64-gcc-c++ — and is this sterna-fedora?"
need x86_64-w64-mingw32-objdump "dnf install mingw64-binutils"
[ "$STAGE_ONLY" = 1 ] || need makensis "dnf install mingw32-nsis"

objdump=x86_64-w64-mingw32-objdump
sysroot=$(x86_64-w64-mingw32-gcc -print-sysroot)/mingw
mingw_bin=$sysroot/bin
qt_plugins=$sysroot/lib/qt6/plugins
[ -d "$qt_plugins/platforms" ] || {
	echo "windows: no Qt plugins under $qt_plugins — install mingw64-qt6-qtbase" >&2
	exit 2
}

version=$(sed -n '/^\[workspace\.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' \
	"$root/crates/Cargo.toml" | head -1)
[ -n "$version" ] || { echo "windows: no version in crates/Cargo.toml" >&2; exit 2; }

# --- build and install -------------------------------------------------------
#
# Release, which also builds the Rust core with --release: CMakeLists drives
# cargo and picks the profile from CMAKE_BUILD_TYPE. It also supplies
# `--target x86_64-pc-windows-gnu` on its own for a cross build, and puts
# cargo's output inside this tree rather than in `crates/target`.
echo "windows: building" >&2
mingw64-cmake -S "$root/shell" -B "$build/cmake" -G Ninja \
	-DCMAKE_BUILD_TYPE=Release \
	-DCMAKE_INSTALL_PREFIX="$build/prefix" >/dev/null || exit 2
cmake --build "$build/cmake" --target sterna || exit 2

rm -rf "$build/prefix" "$stage"
cmake --install "$build/cmake" >/dev/null || exit 2

# The two control-socket clients, which CMake does not build: they are cargo
# binaries in the core's own workspace and the shell does not link them. An
# installation that shipped a window with a control socket and nothing that
# speaks to it would be half a feature.
#
# Built into the CMake tree's own cargo directory rather than the workspace's,
# so they reuse the dependency graph `tt-ffi` has just compiled for this target
# instead of building russh and aws-lc a second time.
echo "windows: building the clients" >&2
CARGO_TARGET_DIR="$build/cmake/cargo" cargo build --release \
	--target x86_64-pc-windows-gnu \
	--manifest-path "$root/crates/Cargo.toml" -p tt-ctl --bins || exit 2
clients=$build/cmake/cargo/x86_64-pc-windows-gnu/release

# --- lay it out the way Windows expects --------------------------------------
#
# CMake's install prefix is a Unix one — bin/ and share/ — because the AppImage
# needs it to be. A Windows program folder is flat: the executable at the root,
# because that is where the loader looks for its DLLs and where Qt looks for
# its plugin directories. `I18n::bundledDirectory` already has the matching
# `lang` beside the executable as its Windows case; the `../share/sterna/lang`
# it prefers cannot exist here, which is what makes the fallback the answer.
mkdir -p "$stage"
cp "$build/prefix/bin/sterna.exe" "$build/prefix/bin/sterna.dll" "$stage/" || exit 2
cp "$clients/ttctl.exe" "$clients/ttpmacro.exe" "$stage/" || exit 2
mkdir -p "$stage/lang"
cp "$build/prefix/share/sterna/lang"/*.lng "$stage/lang/" || exit 2

# `platforms` is not optional: Qt with no platform plugin prints "This
# application failed to start because no Qt platform plugin could be
# initialized" and exits, which is the single most common way a deployed Qt
# program fails. `styles` is: without `qmodernwindowsstyle.dll` the window
# still opens, wearing the Fusion look on a desktop where every other program
# is native — visible to a user and invisible to a test.
mkdir -p "$stage/platforms" "$stage/styles"
cp "$qt_plugins/platforms/qwindows.dll" "$stage/platforms/" || exit 2
cp "$qt_plugins/platforms/qoffscreen.dll" "$stage/platforms/" || exit 2
cp "$qt_plugins/platforms/qminimal.dll" "$stage/platforms/" || exit 2
cp "$qt_plugins/styles"/*.dll "$stage/styles/" 2>/dev/null

# --- the DLLs Windows does not have ------------------------------------------
#
# Closed transitively out of the import tables, rather than from a list that
# would be wrong the first time Qt changed a dependency. The rule for "is this
# ours to ship" is whether the MinGW sysroot has it: that tree holds only the
# 76 DLLs the cross toolchain provides, and none of `kernel32`, `msvcrt`,
# `shell32`, `user32`, `advapi32` or `ole32` is among them — checked, because
# shipping a private copy of a system DLL is worse than shipping none.
imports() {
	"$objdump" -p "$1" 2>/dev/null | sed -n 's/^[[:space:]]*DLL Name: \(.*\)$/\1/p'
}

echo "windows: closing the DLL set" >&2
copied=1
while [ "$copied" != 0 ]; do
	copied=0
	while IFS= read -r f; do
		while IFS= read -r dll; do
			dll=${dll%$'\r'}
			[ -n "$dll" ] || continue
			[ -e "$stage/$dll" ] && continue
			[ -e "$mingw_bin/$dll" ] || continue
			cp "$mingw_bin/$dll" "$stage/$dll" || exit 2
			copied=$((copied + 1))
		done < <(imports "$f")
	done < <(find "$stage" \( -name '*.dll' -o -name '*.exe' \) | sort)
	[ "$copied" = 0 ] || echo "windows:   +$copied" >&2
done

# A plugin that cannot resolve its own imports is not reported as a missing
# DLL — Qt reports it as "no platform plugin", the same message an absent one
# gives, so the two failures are indistinguishable from the message alone.
for dll in Qt6Core.dll Qt6Gui.dll Qt6Widgets.dll Qt6PrintSupport.dll; do
	[ -e "$stage/$dll" ] || { echo "windows: $dll was not resolved" >&2; exit 2; }
done

# Fedora's MinGW packages are shipped unstripped, and so is everything cargo
# and this CMake tree produce, so a third of what has just been staged is
# symbol tables — 154 MB down to 106 MB, most of it `libstdc++-6.dll` alone.
# Safe on a PE file: the export table a DLL is loaded through is part of the
# image, not part of the symbol table, which is why `--strip-unneeded` can take
# the latter without touching the former.
echo "windows: stripping" >&2
find "$stage" \( -name '*.dll' -o -name '*.exe' \) -exec \
	x86_64-w64-mingw32-strip --strip-unneeded {} + || exit 2

# --- what the licences oblige ------------------------------------------------
docs=$stage/doc
mkdir -p "$docs"
cp "$root/LICENSE" "$docs/LICENSE.txt"
cp "$root/ATTRIBUTION.md" "$docs/ATTRIBUTION.md"
cp ../appimage/QT-LGPL-NOTICE.md ../appimage/LGPL-3.0.txt ../appimage/GPL-3.0.txt "$docs/"

# The licence page is a RichEdit control and renders LF-only text as one
# unreadable line. Every text file that a user reads in Notepad or in the
# installer gets CRLF; the .lng files do not, because they are read by us.
for f in "$docs"/*.txt "$docs"/*.md; do
	[ -e "$f" ] || continue
	sed -i 's/\r*$/\r/' "$f"
done

{
	echo "Sterna for Windows"
	echo
	echo "built:      $(date -u +%Y-%m-%dT%H:%M:%SZ)"
	echo "commit:     $(git -C "$root" rev-parse HEAD 2>/dev/null || echo unknown)"
	echo "version:    $version"
	echo "built on:   $(. /etc/os-release && echo "$PRETTY_NAME"), cross-compiled"
	echo "toolchain:  $(x86_64-w64-mingw32-gcc -dumpversion) (x86_64-w64-mingw32)"
	echo "Qt:         $(basename "$(ls "$sysroot"/include/qt6/QtCore/[0-9]* -d 2>/dev/null | tail -1)" 2>/dev/null || echo unknown), as packaged for MinGW"
	echo "Qt source:  https://download.qt.io/official_releases/qt/"
	echo
	echo "Qt is bundled as separate DLLs and may be substituted: replace the"
	echo "Qt6*.dll files in this directory. See QT-LGPL-NOTICE.md."
} | sed 's/$/\r/' > "$docs/BUILD-INFO.txt"

# --- the file lists the installer and the uninstaller use --------------------
#
# Generated rather than written, for the uninstaller's sake. The alternative is
# `RMDir /r "$INSTDIR"` on a directory the *user* chose on the directory page —
# which is how installers delete a Program Files that somebody typed into the
# box. Everything here is removed by name, and each directory with a plain
# `RMDir`, which refuses a directory that is not empty. Anything a user put in
# the install folder therefore survives being uninstalled, and so does the
# folder holding it.
files=$build/files.nsh
uninstall=$build/uninstall.nsh
: > "$files"
: > "$uninstall"
: > "$uninstall.tmp"

# Deepest first for the uninstaller, so a directory is emptied before RMDir
# reaches it; shallowest first for the installer, which does not care.
dirs=$( (echo .; cd "$stage" && find . -mindepth 1 -type d -printf '%P\n') | sort )
for d in $dirs; do
	if [ "$d" = "." ]; then
		out='$INSTDIR'
		src=$stage
	else
		out='$INSTDIR\'$(echo "$d" | tr '/' '\\')
		src=$stage/$d
	fi
	printf 'SetOutPath "%s"\n' "$out" >> "$files"
	for f in "$src"/*; do
		[ -f "$f" ] || continue
		printf '  File "%s"\n' "$f" >> "$files"
		printf 'Delete "%s\\%s"\n' "$out" "$(basename "$f")" >> "$uninstall.tmp"
	done
done
# The installer's own uninstaller is written into $INSTDIR by the script and so
# is not in the staging tree; it is removed last, by hand, after this list.
tac "$uninstall.tmp" > "$uninstall"
rm -f "$uninstall.tmp"
for d in $(echo "$dirs" | tac); do
	[ "$d" = "." ] && continue
	printf 'RMDir "$INSTDIR\\%s"\n' "$(echo "$d" | tr '/' '\\')" >> "$uninstall"
done

n=$(grep -c '^  File ' "$files")
size=$(du -sh "$stage" | cut -f1)
echo "windows: staged $n files, $size" >&2

if [ "$STAGE_ONLY" = 1 ]; then
	echo
	echo "windows: $stage"
	exit 0
fi

# --- assemble ----------------------------------------------------------------
out=$build/sterna-$version-x86_64-setup.exe
rm -f "$out"
makensis -V2 \
	-DVERSION="$version" \
	-DSTAGE="$stage" \
	-DFILES_NSH="$files" \
	-DUNINSTALL_NSH="$uninstall" \
	-DOUTFILE="$out" \
	sterna.nsi || exit 2

echo
echo "windows: $out"
echo "         $(du -h "$out" | cut -f1), from $size staged"

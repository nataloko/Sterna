#!/usr/bin/env bash
# Build the small shared Qt distribution used by the portable AppImage.
#
# Qt's official Linux binaries are built on Ubuntu 24.04 and require glibc
# 2.39. Building Qt Base on the manylinux_2_28 base keeps the application
# usable on glibc 2.28 systems without falling back to an old distro Qt.
set -euo pipefail

cd "$(dirname "$0")"

version=6.11.1
resume=0
if [ "${1:-}" = --resume ]; then
	resume=1
	shift
fi
[ "$#" -le 1 ] || {
	echo "usage: $0 [--resume] [PREFIX]" >&2
	exit 2
}
prefix=${1:-${STERNA_QT_PREFIX:-$PWD/toolchain/qt-$version}}
work=${STERNA_QT_BUILD_ROOT:-$PWD/toolchain/source-$version}
downloads=$work/downloads
src=$work/src
build=$work/build
export CMAKE_BUILD_PARALLEL_LEVEL=${CMAKE_BUILD_PARALLEL_LEVEL:-4}

qmake=$prefix/bin/qmake6
if [ -x "$qmake" ] \
    && [ "$($qmake -query QT_VERSION)" = "$version" ] \
	&& [ -e "$prefix/plugins/platforms/libqxcb.so" ] \
	&& [ -e "$prefix/plugins/platforms/libqwayland.so" ] \
	&& [ -e "$prefix/plugins/wayland-shell-integration/libxdg-shell.so" ] \
	&& [ -e "$prefix/plugins/wayland-decoration-client/libadwaita.so" ]; then
	printf 'qt: using cached Qt %s in %s\n' "$version" "$prefix"
	exit 0
fi

for command in cmake curl make sha256sum tar; do
	command -v "$command" >/dev/null || {
		echo "qt: $command is required" >&2
		exit 2
	}
done

mkdir -p "$downloads" "$src" "$build"

fetch_module() {
	local module=$1 sha=$2
	local archive=$downloads/$module-everywhere-src-$version.tar.xz
	local url=https://download.qt.io/official_releases/qt/6.11/$version/submodules/$(basename "$archive")
	if [ ! -f "$archive" ] || ! echo "$sha  $archive" | sha256sum -c - >/dev/null 2>&1; then
		echo "qt: fetching $(basename "$archive")" >&2
		curl --fail --location --retry 3 "$url" -o "$archive"
	fi
	echo "$sha  $archive" | sha256sum -c - >/dev/null || {
		echo "qt: checksum failed for $archive" >&2
		exit 2
	}
	printf '%s\n' "$archive"
}

if [ "$resume" = 1 ]; then
	[ -f "$build/qtbase/CMakeCache.txt" ] || {
		echo "qt: no configured Qt build to resume in $build/qtbase" >&2
		exit 2
	}
	echo "qt: resuming Qt Base $version in $build/qtbase" >&2
else
	base_archive=$(fetch_module qtbase d9594a31228aa23ad6b531719a29b45f0f3989fe6c136d45767ea179f233c1ac)

	rm -rf "$src/qtbase" "$build/qtbase" "$prefix"
	mkdir -p "$src/qtbase" "$build/qtbase" "$prefix"
	tar -xf "$base_archive" -C "$src/qtbase" --strip-components=1

	echo "qt: configuring Qt Base $version" >&2
	(
		cd "$build/qtbase"
		"$src/qtbase/configure" \
			-prefix "$prefix" \
			-release \
			-opensource -confirm-license \
			-nomake examples -nomake tests \
			-openssl-linked \
			-no-feature-gtk3 \
			-no-feature-vulkan \
			-- \
			-G "Unix Makefiles" \
			-DOPENSSL_INCLUDE_DIR=/usr/include/openssl3 \
			-DOPENSSL_SSL_LIBRARY=/usr/lib64/libssl.so.3 \
			-DOPENSSL_CRYPTO_LIBRARY=/usr/lib64/libcrypto.so.3
	)
fi

echo "qt: building Qt Base" >&2
cmake --build "$build/qtbase" --parallel "$CMAKE_BUILD_PARALLEL_LEVEL"
cmake --install "$build/qtbase"

# Two more modules, for one plugin: `wayland-decoration-client/libadwaita.so`.
#
# GNOME offers no server-side decorations — its compositor advertises no
# `zxdg_decoration_manager_v1` at all — so on a GNOME desktop the title bar
# above this window is drawn by Qt, by whichever decoration plugin is
# installed. Qt Base ships only `bradient`, which draws a title bar out of
# 1995 and handles exactly two gestures: a click on a button, and a drag to
# move. It has no clock in it, so it cannot recognise a double click, and
# double-clicking the title bar of a Qt-decorated window does nothing.
#
# The `adwaita` decoration is Qt's own, matches the desktop's own title bars,
# and toggles maximised on a double click
# (`qwaylandadwaitadecoration.cpp:673`). It lives in Qt Wayland rather than Qt
# Base, and `QT_FEATURE_wayland_decoration_adwaita` turns itself off unless Qt
# Svg is already installed — which is why the order here is load-bearing and
# why an AppImage built without these two stages is silently the old title bar.
#
# Both are built against the Qt Base just installed, with `qt-cmake` so they
# take its toolchain file, and both install into the same prefix. They are
# purely additive: Qt Base's own Wayland client library and platform plugin
# come out byte-identical. Between them they cost about ten minutes, most of
# which is Qt Wayland's compositor — which nothing here ships, and which
# cannot be skipped without patching its build.
for module in qtsvg qtwayland; do
	case $module in
	qtsvg) sha=7f3cf02f4824bf03c2c5859ea6db173bf1482a1daf24e6cdf7bc78cfa26a8a94 ;;
	qtwayland) sha=95788aa502f75441d4edf65932b235f76523084e13dbbb7b9ee2d207b32bd9b3 ;;
	esac
	archive=$(fetch_module "$module" "$sha")
	echo "qt: building $module $version" >&2
	rm -rf "${src:?}/$module" "${build:?}/$module"
	mkdir -p "$src/$module" "$build/$module"
	tar -xf "$archive" -C "$src/$module" --strip-components=1
	"$prefix/bin/qt-cmake" -S "$src/$module" -B "$build/$module" \
		-G "Unix Makefiles" \
		-DCMAKE_BUILD_TYPE=Release \
		-DCMAKE_INSTALL_PREFIX="$prefix" \
		-DQT_BUILD_EXAMPLES=OFF \
		-DQT_BUILD_TESTS=OFF
	cmake --build "$build/$module" --parallel "$CMAKE_BUILD_PARALLEL_LEVEL"
	cmake --install "$build/$module"
done

[ -e "$prefix/plugins/platforms/libqxcb.so" ] || {
	echo "qt: the X11 platform plugin was not built" >&2
	exit 2
}
[ -e "$prefix/plugins/platforms/libqwayland.so" ] || {
	echo "qt: the Wayland platform plugin was not built" >&2
	exit 2
}
[ -e "$prefix/plugins/wayland-shell-integration/libxdg-shell.so" ] || {
	echo "qt: the Wayland shell integration was not built" >&2
	exit 2
}
# Silence is this one's failure mode: without it the window still opens, still
# has a title bar, and only a double click on that bar behaves differently.
[ -e "$prefix/plugins/wayland-decoration-client/libadwaita.so" ] || {
	echo "qt: the Adwaita window decoration was not built" >&2
	exit 2
}

echo "qt: installed Qt $($qmake -query QT_VERSION) in $prefix"

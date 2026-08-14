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
	&& [ -e "$prefix/plugins/wayland-shell-integration/libxdg-shell.so" ]; then
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

echo "qt: installed Qt $($qmake -query QT_VERSION) in $prefix"

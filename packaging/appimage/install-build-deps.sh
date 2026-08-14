#!/usr/bin/env bash
# Prepare the maintained manylinux_2_28 image for Sterna's AppImage build.
set -euo pipefail

if [ "$(id -u)" -ne 0 ] || ! grep -q '^PLATFORM_ID="platform:el8"' /etc/os-release; then
	echo "appimage: install-build-deps.sh requires the manylinux_2_28 container" >&2
	exit 2
fi

dnf install -y -q epel-release
dnf install -y -q \
	autoconf automake binutils bzip2 cmake curl file gcc-c++ git gzip libtool \
	make tar xz zstd \
	at-spi2-core-devel cups-devel dbus-devel fontconfig-devel freetype-devel \
	glib2-devel libdrm-devel libX11-devel libXext-devel libXfixes-devel \
	libXi-devel libXrender-devel libxcb-devel libxkbcommon-devel \
	libxkbcommon-x11-devel mesa-libGL-devel openssl3-devel systemd-devel \
	wayland-devel wayland-protocols-devel xcb-util-cursor-devel \
	xcb-util-devel xcb-util-image-devel xcb-util-keysyms-devel \
	xcb-util-renderutil-devel xcb-util-wm-devel

# EPEL has the zsync client but not zsyncmake, which appimagetool needs to emit
# the release's delta metadata. Build the last C release: 0.7 moved to Go,
# which would add an otherwise-unused compiler to this image.
if ! command -v zsyncmake >/dev/null; then
	archive=$(mktemp)
	source=$(mktemp -d)
	trap 'rm -f "$archive"; rm -rf "$source"' EXIT
	curl --fail --location --retry 3 \
		https://zsync.moria.org.uk/download/zsync-0.6.5.tar.bz2 -o "$archive"
	echo "9df90e71b17204ff41b3885edd7e3601cc2f2b113de7c15320fc20da76ab85f9  $archive" \
		| sha256sum -c -
	tar -xf "$archive" -C "$source" --strip-components=1
	(
		cd "$source"
		./configure --prefix=/usr/local
		make -j2
		make install
	)
fi

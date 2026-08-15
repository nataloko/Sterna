#!/usr/bin/env bash
# Build the portable Qt and publish it as the release asset every build reads.
#
# Run this when `QT_VERSION` moves, when `build-qt.sh`'s recipe changes, or when
# `MANYLINUX_IMAGE` is repinned — the three things that change what the
# toolchain *is*. It is deliberately manual and deliberately rare: forty minutes
# once, against forty minutes per release, which is the arithmetic that put the
# toolchain in a release asset instead of a cache.
#
#   ./packaging/appimage/publish-qt.sh            # build in the pinned image
#   ./packaging/appimage/publish-qt.sh --keep     # ...reuse a prefix already there
#
# It prints the three lines to paste into `toolchain.env`. Nothing reads the new
# asset until they are committed, so a bad publish is not a broken build.
set -euo pipefail

cd "$(dirname "$0")"
. ./toolchain.env

repository=${GH_REPO:-nataloko/Sterna}
keep=0
[ "${1:-}" = --keep ] && keep=1

prefix=$PWD/toolchain/qt-$QT_VERSION
archive=$PWD/toolchain/$QT_TOOLCHAIN_FILE

for command in gh tar xz sha256sum; do
	command -v "$command" >/dev/null || {
		echo "publish-qt: $command is required" >&2
		exit 2
	}
done

if [ "$keep" = 0 ] || ! ./build-qt.sh --check "$prefix" 2>/dev/null; then
	command -v podman >/dev/null && engine=podman || engine=docker
	echo "publish-qt: building Qt $QT_VERSION in $MANYLINUX_IMAGE" >&2
	# The same image the release job runs, so the asset carries that image's
	# glibc floor and not this machine's. `--security-opt label=disable` is
	# podman on a SELinux host; harmless to docker.
	"$engine" run --rm --security-opt label=disable \
		--volume "$PWD/../..:/repo:rw" \
		--workdir /repo \
		"$MANYLINUX_IMAGE" \
		bash -lc '
			set -euo pipefail
			./packaging/appimage/install-build-deps.sh
			./packaging/appimage/build-qt.sh
		'
fi

./build-qt.sh --check "$prefix"

# Reproducible enough to compare two builds: sorted, no owners, one timestamp.
# Not bit-identical — Qt's own build is not — so the pin is a digest of what was
# published rather than a claim that it can be recreated byte for byte.
echo "publish-qt: packing $QT_TOOLCHAIN_FILE" >&2
rm -f "$archive"
tar --sort=name --owner=0 --group=0 --numeric-owner \
	--mtime="@${SOURCE_DATE_EPOCH:-0}" \
	-cf - -C toolchain "qt-$QT_VERSION" | xz -T0 -6 >"$archive"
sha=$(sha256sum "$archive" | cut -d' ' -f1)

if gh release view "$QT_TOOLCHAIN_TAG" --repo "$repository" >/dev/null 2>&1; then
	echo "publish-qt: $QT_TOOLCHAIN_TAG exists; uploading beside it" >&2
	gh release upload "$QT_TOOLCHAIN_TAG" --repo "$repository" --clobber "$archive"
else
	# A prerelease, so it can never take the "Latest" badge from a real one, and
	# titled for somebody who lands on the releases page wondering what it is.
	gh release create "$QT_TOOLCHAIN_TAG" --repo "$repository" \
		--prerelease \
		--title "Qt $QT_VERSION for the portable AppImage" \
		--notes "Qt $QT_VERSION built from its verified source archives on
\`$MANYLINUX_IMAGE\`, which is what gives the Linux AppImage its glibc 2.28
floor. This is a build input, not a Sterna release — see
\`packaging/README.md\`. Pinned by SHA-256 in \`packaging/appimage/toolchain.env\`." \
		"$archive"
fi
rm -f "$archive"

cat >&2 <<EOF

publish-qt: published. Paste into packaging/appimage/toolchain.env:

QT_TOOLCHAIN_TAG=$QT_TOOLCHAIN_TAG
QT_TOOLCHAIN_FILE=$QT_TOOLCHAIN_FILE
QT_TOOLCHAIN_SHA256=$sha
EOF

#!/usr/bin/env bash
# Put the published portable Qt in place, or say why it could not.
#
# Downloading a prebuilt Qt is what keeps a release under ten minutes; building
# it is forty on one worker (`build-qt.sh`, and the OOM note in AGENTS.md that
# says why it is one worker). This script never builds. It fetches, checks the
# bytes against the pin in `toolchain.env`, unpacks, and asks `build-qt.sh
# --check` whether what arrived is usable.
#
# **A failure here is not fatal on purpose.** Exit 1 means "no toolchain", and
# the caller's next step is `build-qt.sh`, which builds one. A deleted asset, a
# GitHub outage or an unauthenticated runner costs a slow release, never a
# blocked one.
set -euo pipefail

cd "$(dirname "$0")"
. ./toolchain.env

repository=${GH_REPO:-nataloko/Sterna}
prefix=$PWD/toolchain/qt-$QT_VERSION
archive=$PWD/toolchain/$QT_TOOLCHAIN_FILE

if ./build-qt.sh --check "$prefix" 2>/dev/null; then
	exit 0
fi

mkdir -p toolchain
rm -f "$archive"

# `gh` on a runner, plain HTTPS anywhere else — a public release asset needs no
# credentials, and a contributor building this locally should not need `gh`.
if command -v gh >/dev/null && [ -n "${GITHUB_TOKEN:-${GH_TOKEN:-}}" ]; then
	gh release download "$QT_TOOLCHAIN_TAG" --repo "$repository" \
		--pattern "$QT_TOOLCHAIN_FILE" --dir toolchain || {
		echo "qt: could not download $QT_TOOLCHAIN_TAG/$QT_TOOLCHAIN_FILE" >&2
		exit 1
	}
else
	url=https://github.com/$repository/releases/download/$QT_TOOLCHAIN_TAG/$QT_TOOLCHAIN_FILE
	curl --fail --location --retry 3 "$url" -o "$archive" || {
		echo "qt: could not download $url" >&2
		exit 1
	}
fi

# Before unpacking, not after: this archive becomes the Qt inside a signed
# release, and the digest is the only thing tying it to the build that was
# reviewed. `--status` so a mismatch prints this message and not sha256sum's.
echo "$QT_TOOLCHAIN_SHA256  $archive" | sha256sum --check --status - || {
	echo "qt: $QT_TOOLCHAIN_FILE does not match its pinned SHA-256" >&2
	echo "qt: expected $QT_TOOLCHAIN_SHA256" >&2
	echo "qt: got      $(sha256sum "$archive" | cut -d' ' -f1)" >&2
	rm -f "$archive"
	exit 1
}

rm -rf "$prefix"
tar -xf "$archive" -C toolchain
rm -f "$archive"

# The digest says the bytes are the ones that were published. This says the
# thing they unpack to is a Qt that can open a window — same four plugins a
# fresh build is held to, so a downloaded toolchain and a built one enter the
# AppImage having passed the same test.
./build-qt.sh --check "$prefix" || {
	echo "qt: the published toolchain unpacked to something unusable" >&2
	exit 1
}

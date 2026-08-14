#!/usr/bin/env bash
# Keep a prepared Qt bootstrap container below GitHub's long-step failure window.
set -euo pipefail

cd "$(dirname "$0")"
mode=${1:-}
case "$mode" in
	prepare|start|resume|finish) ;;
	*) echo "usage: $0 prepare|start|resume|finish" >&2; exit 2 ;;
esac

(
	while sleep 60; do
		echo "qt: CI heartbeat"
		df -h "$PWD"
		free -h
	done
) &
heartbeat=$!
trap 'kill "$heartbeat" 2>/dev/null || true; wait "$heartbeat" 2>/dev/null || true' EXIT

if [ "$mode" = prepare ]; then
	./install-build-deps.sh
	exit 0
fi

if [ "$mode" = finish ]; then
	./build-qt.sh --resume
	rm -rf toolchain/source-6.11.1
	exit 0
fi

work=${STERNA_QT_BUILD_ROOT:-$PWD/toolchain/source-6.11.1}
build=$work/build/qtbase
if [ "$mode" = start ]; then
	./build-qt.sh --configure-only
else
	[ -f "$build/CMakeCache.txt" ] || {
		echo "qt: no configured Qt build to resume in $build" >&2
		exit 2
	}
fi

limit=${STERNA_QT_SLICE_LIMIT:-20m}
set +e
# Put CMake and all of its descendants directly under timeout. Wrapping the
# build-qt shell instead leaves Make holding the Docker exec stream after the
# shell has gone away.
timeout --kill-after=30s --signal=TERM "$limit" \
	cmake --build "$build" --parallel
status=$?
set -e
case "$status" in
	0) echo "qt: build completed within the $mode slice" ;;
	124) echo "qt: $mode slice ended after $limit; the next slice will resume" ;;
	*) exit "$status" ;;
esac

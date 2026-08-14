#!/usr/bin/env bash
# Keep a clean Qt bootstrap below GitHub's long-container-step failure window.
set -euo pipefail

cd "$(dirname "$0")"
mode=${1:-}
case "$mode" in
	start|resume|finish) ;;
	*) echo "usage: $0 start|resume|finish" >&2; exit 2 ;;
esac

./install-build-deps.sh

(
	while sleep 60; do
		echo "qt: CI heartbeat"
		df -h "$PWD"
		free -h
	done
) &
heartbeat=$!
trap 'kill "$heartbeat" 2>/dev/null || true; wait "$heartbeat" 2>/dev/null || true' EXIT

if [ "$mode" = finish ]; then
	./build-qt.sh --resume
	rm -rf toolchain/source-6.11.1
	exit 0
fi

build=(./build-qt.sh)
[ "$mode" = resume ] && build+=(--resume)
set +e
# Make and the compiler clean up interrupted targets on SIGINT. The KILL bound
# prevents a descendant which failed to stop from carrying the step across the
# hosted runner's communication window.
timeout --kill-after=30s --signal=INT 30m "${build[@]}"
status=$?
set -e
case "$status" in
	0) echo "qt: build completed within the $mode slice" ;;
	124) echo "qt: $mode slice ended after 30 minutes; the next slice will resume" ;;
	*) exit "$status" ;;
esac

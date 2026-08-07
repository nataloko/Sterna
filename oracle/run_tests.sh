#!/usr/bin/env bash
# qtterm oracle regression suite.
#
# Each case is a directory under cases/ holding:
#   cmd       one line of oracle arguments (optional, default: --cols 40 --rows 6)
#   input     the byte stream fed to the terminal
#   expected  the golden dump
#
# Regenerate goldens after an intentional change with: ./run_tests.sh --bless
set -uo pipefail

cd "$(dirname "$0")"
ORACLE=./build/oracle
BLESS=0
[ "${1:-}" = "--bless" ] && BLESS=1

if [ ! -x "$ORACLE" ]; then
	echo "run_tests: $ORACLE not built; run make" >&2
	exit 2
fi

pass=0; fail=0; blessed=0
for dir in cases/*/; do
	name=$(basename "$dir")
	[ -f "$dir/input" ] || continue
	args="--cols 40 --rows 6"
	[ -f "$dir/cmd" ] && args=$(cat "$dir/cmd")

	# shellcheck disable=SC2086
	if ! actual=$("$ORACLE" $args "$dir/input" 2>&1); then
		printf '  FAIL %-28s (oracle exited %d)\n' "$name" "$?"
		fail=$((fail + 1))
		continue
	fi

	if [ "$BLESS" = 1 ]; then
		printf '%s\n' "$actual" > "$dir/expected"
		blessed=$((blessed + 1))
		continue
	fi

	if [ ! -f "$dir/expected" ]; then
		printf '  FAIL %-28s (no golden; run --bless)\n' "$name"
		fail=$((fail + 1))
		continue
	fi

	if diff -q <(printf '%s\n' "$actual") "$dir/expected" >/dev/null; then
		printf '  ok   %s\n' "$name"
		pass=$((pass + 1))
	else
		printf '  FAIL %s\n' "$name"
		diff -u "$dir/expected" <(printf '%s\n' "$actual") | sed 's/^/       /' | head -20
		fail=$((fail + 1))
	fi
done

if [ "$BLESS" = 1 ]; then
	echo "blessed $blessed golden file(s)"
	exit 0
fi

echo "$pass passed, $fail failed"
[ "$fail" -eq 0 ]

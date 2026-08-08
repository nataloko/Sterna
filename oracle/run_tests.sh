#!/usr/bin/env bash
# Sterna oracle regression suite.
#
# Each case is a directory under cases/ holding:
#   cmd       one line of oracle arguments (optional, default: --cols 40 --rows 6)
#   input     the byte stream fed to the terminal
#   expected  the golden dump (optional)
#
# The cases are shared with ../run_diff.sh, which needs no golden because it
# diffs the two engines against each other. So a case with no `expected` is
# REPORTED AND SKIPPED here rather than failed: it is a differential-only case,
# which is the default and the one to prefer. Adding a golden opts that case
# into this suite as well, whose separate job is catching the *oracle* drifting
# when upstream is bumped or the stub layer changes.
#
# Regenerate goldens after an intentional change with: ./run_tests.sh --bless —
# and read what it produces. A wrong golden is worse than no test.
set -uo pipefail

cd "$(dirname "$0")"
ORACLE=./build/oracle
BLESS=0
[ "${1:-}" = "--bless" ] && BLESS=1

if [ ! -x "$ORACLE" ]; then
	echo "run_tests: $ORACLE not built; run make" >&2
	exit 2
fi

pass=0; fail=0; skip=0; blessed=0
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
		printf '  skip %-28s (differential-only; no golden)\n' "$name"
		skip=$((skip + 1))
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

echo "$pass passed, $fail failed, $skip differential-only"
[ "$fail" -eq 0 ]

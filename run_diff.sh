#!/usr/bin/env bash
# Sterna differential suite: the Rust VT engine against Tera Term's real one.
#
# For every case in oracle/cases/ this runs BOTH engines with the same arguments
# and diffs their output against each other. There is no golden file in the
# middle — the oracle is the expected output — so adding a case means adding an
# `input` (and optionally a `cmd`) and nothing else. Nothing to bless, nothing
# to read and approve, nothing that can silently encode a wrong answer.
#
#   ./run_diff.sh              run every case
#   ./run_diff.sh 09 12        run the cases whose names contain 09 or 12
#   ./run_diff.sh -v 09        also print the full dump from both engines
#
# A case directory containing an `xfail` file is a KNOWN divergence: the file
# says why, the diff is reported but not counted as a failure, and if the two
# engines ever agree it is reported as XPASS and *does* fail, so the marker
# cannot outlive the bug it describes.
#
# oracle/run_tests.sh still exists and is still the golden-file suite for the
# oracle itself. That one guards against the oracle drifting; this one measures
# how far the port has got.
set -uo pipefail

cd "$(dirname "$0")"

ORACLE=oracle/build/oracle
RUST=crates/target/debug/tt-dump
VERBOSE=0

filters=()
for a in "$@"; do
	case "$a" in
		-v|--verbose) VERBOSE=1 ;;
		-h|--help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
		*) filters+=("$a") ;;
	esac
done

if [ ! -x "$ORACLE" ]; then
	echo "run_diff: building the oracle" >&2
	make -C oracle >/dev/null || { echo "run_diff: oracle build failed" >&2; exit 2; }
fi

echo "run_diff: building tt-dump" >&2
# cargo is on PATH only for login shells in the dev container.
export PATH="$HOME/.cargo/bin:$PATH"
(cd crates && cargo build --quiet) || { echo "run_diff: cargo build failed" >&2; exit 2; }

pass=0; fail=0; skip=0; xfail=0
failed_names=()

for dir in oracle/cases/*/; do
	name=$(basename "$dir")
	[ -f "$dir/input" ] || continue

	if [ ${#filters[@]} -gt 0 ]; then
		matched=0
		for f in "${filters[@]}"; do
			case "$name" in *"$f"*) matched=1 ;; esac
		done
		[ "$matched" = 1 ] || { skip=$((skip + 1)); continue; }
	fi

	args="--cols 40 --rows 6"
	[ -f "$dir/cmd" ] && args=$(cat "$dir/cmd")

	# shellcheck disable=SC2086
	if ! want=$("$ORACLE" $args "$dir/input" 2>&1); then
		printf '  FAIL %-28s (oracle exited %d)\n' "$name" "$?"
		fail=$((fail + 1)); failed_names+=("$name")
		continue
	fi
	# shellcheck disable=SC2086
	if ! got=$("$RUST" $args "$dir/input" 2>&1); then
		printf '  FAIL %-28s (tt-dump exited %d)\n' "$name" "$?"
		fail=$((fail + 1)); failed_names+=("$name")
		continue
	fi

	if [ "$want" = "$got" ]; then
		if [ -f "$dir/xfail" ]; then
			printf '  XPASS %-26s (expected to differ but matched — delete %sxfail)\n' \
				"$name" "$dir"
			fail=$((fail + 1)); failed_names+=("$name")
			continue
		fi
		printf '  ok   %s\n' "$name"
		pass=$((pass + 1))
		[ "$VERBOSE" = 1 ] && printf '%s\n' "$got" | sed 's/^/       /'
	elif [ -f "$dir/xfail" ]; then
		printf '  xfail %-26s %s\n' "$name" "$(head -1 "$dir/xfail")"
		xfail=$((xfail + 1))
		[ "$VERBOSE" = 1 ] && diff -u <(printf '%s\n' "$want") <(printf '%s\n' "$got") \
			--label "oracle (Tera Term)" --label "tt-vt (Rust)" | sed 's/^/       /'
	else
		printf '  FAIL %s\n' "$name"
		fail=$((fail + 1)); failed_names+=("$name")
		diff -u <(printf '%s\n' "$want") <(printf '%s\n' "$got") \
			--label "oracle (Tera Term)" --label "tt-vt (Rust)" | sed 's/^/       /'
	fi
done

echo
echo "$pass matched, $fail differed, $xfail known-divergent, $skip skipped"
if [ "$fail" -gt 0 ]; then
	printf 'differing: %s\n' "${failed_names[*]}"
fi
[ "$fail" -eq 0 ]

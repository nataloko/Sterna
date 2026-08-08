#!/usr/bin/env bash
# Adjudication: what does Tera Term do with esctest's own byte streams?
#
# `run_tests.sh` says whether we satisfy esctest. It cannot say whose decision a
# failure is, because esctest is measuring against xterm and this is a port of
# Tera Term. This does say: it runs the suite with `--test-case-dir`, which
# writes each test's *stimulus* to a file, and then feeds every one of those
# files to BOTH engines and diffs them — the root `run_diff.sh` treatment, over
# a corpus nobody had to write.
#
#   ./run_diff.sh            record and diff every test
#   ./run_diff.sh -v DECIC   also print the diff, for the matching tests only
#   ./run_diff.sh --keep     reuse the recordings from last time
#
# A test that FAILS in run_tests.sh and matches here is Tera Term not being
# xterm, and its `expected` entry should say which upstream decision it is. One
# that differs here is **ours**, and wants a case in `oracle/cases/` and a fix.
#
# The recordings are the stimulus alone: esctest excludes its own queries from
# the side channel, and the per-test reset it does first happens before the
# channel is attached. So a stream replayed here starts from the engines' own
# defaults rather than from esctest's 80x25 soft-reset state. That is fine for
# the question being asked — both engines start from the same place — and it is
# why this is an adjudicator and not a second conformance suite.
set -uo pipefail

cd "$(dirname "$0")"

ORACLE=../oracle/build/oracle
RUST=../crates/target/debug/tt-dump
ARGS="--cols 80 --rows 25 --term vt525"

VERBOSE=0
KEEP=0
filters=()
for a in "$@"; do
	case "$a" in
		-v|--verbose) VERBOSE=1 ;;
		--keep) KEEP=1 ;;
		-h|--help) sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
		*) filters+=("$a") ;;
	esac
done

build=$PWD/build
cases=$build/cases

if [ "$KEEP" = 0 ] || [ ! -d "$cases" ]; then
	rm -rf "$cases"
	mkdir -p "$cases"
	echo "esctest: recording every test's byte stream" >&2
	TT_ESCTEST_RECORD="$cases" ./run_tests.sh --record >/dev/null || {
		echo "esctest: the recording run did not finish" >&2; exit 2; }
fi

if [ ! -x "$ORACLE" ]; then
	echo "esctest: building the oracle" >&2
	make -C ../oracle >/dev/null || { echo "esctest: oracle build failed" >&2; exit 2; }
fi
export PATH="$HOME/.cargo/bin:$PATH"
(cd ../crates && cargo build --quiet -p tt-dump) || {
	echo "esctest: cargo build failed" >&2; exit 2; }

same=0; differ=0; skip=0
differing=()

for f in "$cases"/*.txt; do
	[ -f "$f" ] || continue
	name=$(basename "$f" .txt)

	if [ ${#filters[@]} -gt 0 ]; then
		matched=0
		for p in "${filters[@]}"; do
			case "$name" in *"$p"*) matched=1 ;; esac
		done
		[ "$matched" = 1 ] || { skip=$((skip + 1)); continue; }
	fi

	# shellcheck disable=SC2086
	want=$("$ORACLE" $ARGS "$f" 2>&1) || { differing+=("$name"); differ=$((differ + 1)); continue; }
	# shellcheck disable=SC2086
	got=$("$RUST" $ARGS "$f" 2>&1) || { differing+=("$name"); differ=$((differ + 1)); continue; }

	if [ "$want" = "$got" ]; then
		same=$((same + 1))
	else
		differ=$((differ + 1))
		differing+=("$name")
		if [ "$VERBOSE" = 1 ]; then
			printf '  differs %s\n' "$name"
			diff -u <(printf '%s\n' "$want") <(printf '%s\n' "$got") \
				--label "oracle (Tera Term)" --label "tt-vt (Rust)" | sed 's/^/       /'
		fi
	fi
done

echo
echo "$same agree, $differ differ, $skip not run"
if [ "$differ" -gt 0 ]; then
	printf '%s\n' "${differing[@]}" | sed 's/^/  /'
fi
[ "$differ" -eq 0 ]

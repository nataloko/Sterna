#!/usr/bin/env bash
# Sterna conformance suite: iTerm2's esctest, run against the Rust engine.
#
# esctest is not a recording. It runs as an ordinary program on a pty, writes an
# escape sequence and then *reads the answer back* — cursor position, mode
# state, and the contents of any cell, via DECRQCRA. `tt-host` is the terminal
# on the other end of that pty.
#
#   ./run_tests.sh                run every test
#   ./run_tests.sh CUPTests       run the ones whose name matches (a regex)
#   ./run_tests.sh --bless        rewrite `expected` from what just happened
#   ./run_tests.sh -v             leave the log where it can be read
#
# This is NOT the differential suite and does not replace it. The oracle is
# Tera Term, and where Tera Term and xterm disagree the oracle wins; esctest
# measures against "xterm, without the bugs George Nachman minded", which is a
# different target and a deliberately stricter one. So a test that fails here is
# a *question*, not a verdict, and `expected` is where the answers are written
# down.
#
# `expected` lists every test that does not pass, with a reason. A test that
# starts failing is a diff; so is a test that starts passing, because a stale
# entry must not outlive the thing it describes — the same rule the differential
# suite's `xfail` files follow.
set -uo pipefail

cd "$(dirname "$0")"

# ThomasDickey/esctest2, the maintained fork of gnachman/esctest.
ESCTEST_URL=https://github.com/ThomasDickey/esctest2
ESCTEST_REF=664be3cf2c1e3f06bc93a8bafb48a0db83c607db

# 80x25 because that is what esctest's own per-test reset asks for
# (`XTERM_WINOPS(WINOP_RESIZE_CHARS, 25, 80)`); starting anywhere else just
# means every test resizes on its way in.
COLS=80
ROWS=25

# VT525, the highest identity Tera Term has, and the level told to match it.
# esctest refuses to attempt anything above the level it is given — DECRQCRA
# itself is gated at 4 — and it also *asserts* against it: its DECRQSS test
# asks for level 5 and expects the terminal to report back the level named
# here, so claiming 4 while identifying as a VT525 fails a test for no reason
# beyond the harness contradicting itself.
TERM_ID=vt525
VT_LEVEL=5

BLESS=0
VERBOSE=0
RECORD=0
INCLUDE='.*'

for a in "$@"; do
	case "$a" in
		--bless) BLESS=1 ;;
		# Used by run_diff.sh: write each test's byte stream to
		# $TT_ESCTEST_RECORD and skip the comparison, because the point of
		# that run is the recordings rather than the verdict.
		--record) RECORD=1 ;;
		-v|--verbose) VERBOSE=1 ;;
		-h|--help) sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
		*) INCLUDE="$a" ;;
	esac
done

command -v python3 >/dev/null || { echo "esctest: python3 is needed to run it" >&2; exit 2; }

# Absolute, because the child runs with its own working directory — a pty is a
# terminal, not a shell, and `tt-host` starts the program in the user's home.
build=$PWD/build
src=$build/esctest2

# Pinned to a SHA for the same reason the oracle pins Tera Term: this suite is
# the expectation, so an upstream commit must not be able to turn the gate red
# with no change on our side.
if [ "$(git -C "$src" rev-parse HEAD 2>/dev/null)" != "$ESCTEST_REF" ]; then
	echo "esctest: fetching $ESCTEST_REF" >&2
	rm -rf "$src"
	mkdir -p "$src"
	git -C "$src" init --quiet
	git -C "$src" remote add origin "$ESCTEST_URL"
	if ! git -C "$src" fetch --quiet --depth 1 origin "$ESCTEST_REF"; then
		echo "esctest: cannot fetch $ESCTEST_REF from $ESCTEST_URL" >&2
		exit 2
	fi
	git -C "$src" checkout --quiet FETCH_HEAD
fi

# Reapplied on every run, from a pristine tree, because the checkout survives
# between runs and a half-applied patch is worse than an unpatched one. Same
# convention as `oracle/patches/`: the reference is never edited in place, and
# a change that a run needs lives in a file that says why.
if [ -d patches ] && [ -n "$(echo patches/*.patch)" ]; then
	git -C "$src" checkout --quiet -- . 2>/dev/null || true
	for patch in "$PWD"/patches/*.patch; do
		[ -e "$patch" ] || continue
		git -C "$src" apply "$patch" || {
			echo "esctest: cannot apply $(basename "$patch")" >&2; exit 2; }
	done
fi

# cargo is on PATH only for login shells in the dev container.
export PATH="$HOME/.cargo/bin:$PATH"
echo "esctest: building tt-host" >&2
(cd ../crates && cargo build --quiet -p tt-host) || {
	echo "esctest: cargo build failed" >&2; exit 2; }

log=$build/esctest.log
rm -f "$log"

record=()
if [ "$RECORD" = 1 ]; then
	[ -n "${TT_ESCTEST_RECORD:-}" ] || {
		echo "esctest: --record needs TT_ESCTEST_RECORD" >&2; exit 2; }
	record=(--test-case-dir "$TT_ESCTEST_RECORD")
fi

# --expected-terminal has only three values and none of them is "something
# else", so xterm it is, and the xterm-specific expectations come with it.
#
# --xterm-checksum 334 picks the two conventions our DECRQCRA implements: the
# plain sum rather than xterm's pre-#279 two's complement, and an erased cell
# counting as a space rather than as a distinguishable "empty". Tera Term's
# grid has no empty-versus-blank distinction to report, so the alternative is
# not available to us.
../crates/target/debug/tt-host \
	--cols "$COLS" --rows "$ROWS" --term-id "$TERM_ID" --decrqcra \
	--timeout 1200 -- \
	python3 "$src/esctest/esctest.py" \
		--expected-terminal xterm \
		--xterm-checksum 334 \
		--max-vt-level "$VT_LEVEL" \
		--logfile "$log" \
		--no-print-logs \
		"${record[@]+"${record[@]}"}" \
		--include "$INCLUDE" || {
	echo "esctest: the run did not finish" >&2; exit 2; }

[ -s "$log" ] || { echo "esctest: no log was written" >&2; exit 2; }

# esctest's own exit status is always 0 and its counts are prose, so the log's
# per-test lines are the result. "Fails as expected" is esctest's own
# known-bug annotation for the terminal we claimed to be; "skipped" is a test
# that wanted a higher VT level than we asked for.
results=$build/results
awk '
	/^Run test: / { name = $3; next }
	/^Passed\.$/ { print "pass " name; next }
	/^Fails as expected: / { print "known-bug " name; next }
	/^Skipped because terminal lacks/ { print "skipped " name; next }
	/^\*\*\* TEST .* FAILED:$/ { print "fail " name; next }
' "$log" | sort -k2 > "$results"

# Everything that is not a plain pass, which is what `expected` records.
notpass=$build/notpass
awk '$1 != "pass" { print }' "$results" > "$notpass"

total=$(wc -l < "$results")
passed=$(awk '$1 == "pass"' "$results" | wc -l)
echo
echo "$passed of $total passed"

# The recordings were the point of the run; the verdict belongs to a run that
# was not also writing files.
[ "$RECORD" = 1 ] && exit 0

if [ "$BLESS" = 1 ]; then
	# A reason already written down survives a bless; a line that is new
	# arrives bare and has to be given one by hand, which is the point of
	# keeping them in the same file.
	if [ -f expected ]; then
		awk 'NR==FNR { why[$1 " " $2] = $0; next }
		     { key = $1 " " $2; print (key in why) ? why[key] : $0 }' \
			expected "$notpass" > expected.new
	else
		cp "$notpass" expected.new
	fi
	mv expected.new expected
	bare=$(grep -cv '#' expected || true)
	echo "esctest: wrote expected, $(wc -l < expected) entries, $bare without a reason."
	echo "         Write those before committing: an unexplained failure is a"
	echo "         bug nobody has looked at yet."
	exit 0
fi

if [ ! -f expected ]; then
	echo "esctest: no expected file; run --bless once and write the reasons" >&2
	exit 2
fi

# Compare on name and status only. The reason is a comment for humans.
if diff -u <(awk '{ print $1, $2 }' expected | sort -k2) \
           <(sort -k2 "$notpass") \
           --label 'expected' --label 'this run' > "$build/drift"; then
	[ "$VERBOSE" = 1 ] && echo "esctest: log in $log"
	echo "no drift from expected"
	exit 0
fi

echo
echo "esctest: the results moved. A line only in 'this run' is a new failure;"
echo "         a line only in 'expected' now passes and its entry must go."
cat "$build/drift"
echo
echo "esctest: log in $log"
exit 1

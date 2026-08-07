#!/usr/bin/env bash
# Differential suite, driven by Tera Term's OWN escape-sequence exercisers.
#
# ../teraterm/tests holds 33 scripts upstream uses to exercise its terminal by
# hand. Running them headless and diffing the two engines over their output
# gets a corpus written by people who knew where the bodies are buried, for the
# price of a shell loop — PLAN.md's verification item 4.
#
# Nothing is copied into this repository: the scripts are executed from the
# read-only sibling checkout, so the corpus tracks the pinned upstream SHA.
# `oracle/upstream.cases` says which are run, with which arguments, and records
# a reason for every one that is skipped or expected to differ.
#
#   ./run_upstream.sh              run everything the manifest lists
#   ./run_upstream.sh irm          just the entries matching "irm"
#   ./run_upstream.sh -v irm       and print the diff for known divergences too
#   ./run_upstream.sh -k irm       keep the captured input for inspection
set -uo pipefail

cd "$(dirname "$0")"

TESTS=../teraterm/tests
MANIFEST=oracle/upstream.cases
ORACLE=oracle/build/oracle
RUST=crates/target/debug/tt-dump
VERBOSE=0
KEEP=0

filters=()
for a in "$@"; do
	case "$a" in
		-v|--verbose) VERBOSE=1 ;;
		-k|--keep) KEEP=1 ;;
		-h|--help) sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
		*) filters+=("$a") ;;
	esac
done

if [ ! -d "$TESTS" ]; then
	echo "run_upstream: $TESTS not found — this needs the sibling Tera Term checkout" >&2
	exit 2
fi

if [ ! -x "$ORACLE" ]; then
	make -C oracle >/dev/null || { echo "run_upstream: oracle build failed" >&2; exit 2; }
fi
export PATH="$HOME/.cargo/bin:$PATH"
(cd crates && cargo build --quiet) || { echo "run_upstream: cargo build failed" >&2; exit 2; }

work=$(mktemp -d)
trap '[ "$KEEP" = 1 ] && echo "kept: $work" || rm -rf "$work"' EXIT

# Several scripts pace themselves with sleep(1) between screens. That is for a
# human watching; here it only decides how much of the stream a timeout
# captures, which would make the corpus depend on machine speed. Stub it out.
mkdir -p "$work/bin"
printf '#!/bin/sh\nexit 0\n' > "$work/bin/sleep"
chmod +x "$work/bin/sleep"

# The pauses read a line from stdin. Forty is more than any script asks for;
# after that they see EOF, which every script here treats as "carry on".
feed=$(printf '\n%.0s' $(seq 1 40))

pass=0; fail=0; xfail=0; skip=0; miss=0
failed_names=()

while IFS='|' read -r script args note; do
	script=$(echo "$script" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
	args=$(echo "$args" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
	note=$(echo "$note" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')

	# Comments start with '#' — and so does every bug-number script name
	# (`#38168-deccara-range.sh`). Filenames have no spaces, prose does, so
	# that is the discriminator.
	[ -z "$script" ] && continue
	case "$script" in '#' | \#*[[:space:]]*) continue ;; esac

	if [ ${#filters[@]} -gt 0 ]; then
		matched=0
		for f in "${filters[@]}"; do
			case "$script" in *"$f"*) matched=1 ;; esac
		done
		[ "$matched" = 1 ] || continue
	fi

	case "$note" in
		SKIP:*)
			printf '  skip  %-38s %s\n' "$script" "${note#SKIP: }"
			skip=$((skip + 1)); continue ;;
	esac

	if [ ! -f "$TESTS/$script" ]; then
		printf '  MISS  %-38s (not in %s — upstream moved or renamed it)\n' "$script" "$TESTS"
		miss=$((miss + 1)); fail=$((fail + 1)); failed_names+=("$script")
		continue
	fi

	case "$script" in
		*.pl) interp=perl ;;
		*) interp=sh ;;
	esac

	input="$work/$(echo "$script" | tr -c 'A-Za-z0-9._-' '_').bin"
	printf '%s' "$feed" | PATH="$work/bin:$PATH" timeout 20 "$interp" "$TESTS/$script" \
		> "$input" 2>/dev/null
	if [ ! -s "$input" ]; then
		printf '  MISS  %-38s (produced no output)\n' "$script"
		miss=$((miss + 1)); fail=$((fail + 1)); failed_names+=("$script")
		continue
	fi

	# shellcheck disable=SC2086
	want=$("$ORACLE" $args "$input" 2>&1)
	# shellcheck disable=SC2086
	got=$("$RUST" $args "$input" 2>&1)

	if [ "$want" = "$got" ]; then
		case "$note" in
			XFAIL:*)
				printf '  XPASS %-38s (matched — drop the XFAIL in %s)\n' "$script" "$MANIFEST"
				fail=$((fail + 1)); failed_names+=("$script") ;;
			*)
				printf '  ok    %s\n' "$script"
				pass=$((pass + 1)) ;;
		esac
	else
		case "$note" in
			XFAIL:*)
				printf '  xfail %-38s %s\n' "$script" "${note#XFAIL: }"
				xfail=$((xfail + 1))
				[ "$VERBOSE" = 1 ] && diff -u <(printf '%s\n' "$want") <(printf '%s\n' "$got") \
					--label "oracle (Tera Term)" --label "tt-vt (Rust)" | sed 's/^/        /' ;;
			*)
				printf '  FAIL  %s\n' "$script"
				fail=$((fail + 1)); failed_names+=("$script")
				diff -u <(printf '%s\n' "$want") <(printf '%s\n' "$got") \
					--label "oracle (Tera Term)" --label "tt-vt (Rust)" | sed 's/^/        /' | head -40 ;;
		esac
	fi
done < "$MANIFEST"

echo
echo "$pass matched, $fail differed, $xfail known-divergent, $skip not run"
if [ "$fail" -gt 0 ]; then
	printf 'differing: %s\n' "${failed_names[*]}"
fi
[ "$fail" -eq 0 ]

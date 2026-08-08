#!/usr/bin/env bash
# Build the fuzz corpora out of what the repository already has.
#
# Starting libFuzzer from nothing means spending the first minutes rediscovering
# that `ESC [` introduces a sequence. The differential cases already are that
# knowledge — 105 hand-written streams aimed at the engine's corners — so they
# seed `vt_stream` directly, and `vt_chunks`/`telnet` get their structured
# equivalents.
#
#   ./seed.sh            build corpus/ from oracle/cases/ (idempotent)
#
# corpus/ is gitignored. artifacts/ is NOT: a crash file committed there becomes
# a permanent regression case, replayed on stable by `cargo test -p tt-fuzz`.
set -euo pipefail

cd "$(dirname "$0")"
root=../..

mkdir -p corpus/vt_stream corpus/vt_chunks corpus/telnet

n=0
for dir in "$root"/oracle/cases/*/; do
	[ -f "$dir/input" ] || continue
	cp "$dir/input" "corpus/vt_stream/$(basename "$dir")"
	n=$((n + 1))
done

# `vt_chunks` and `telnet` take an `arbitrary`-encoded Vec<Vec<u8>>, not a flat
# stream, so a raw case file is not a valid input for them. Rather than encode
# that format by hand — it is `arbitrary`'s to change — they start from the
# same bytes and let libFuzzer find the structure. A wrong-shaped seed costs one
# wasted iteration; a hand-rolled encoder that drifts costs a corpus that
# silently decodes to nothing.
for dir in "$root"/oracle/cases/*/; do
	[ -f "$dir/input" ] || continue
	cp "$dir/input" "corpus/vt_chunks/$(basename "$dir")"
done

# Telnet's own corner cases, which no VT case contains: escaped IAC, the CR NUL
# line ending, a subnegotiation, and a command cut in half.
printf '\377\375\030\377\373\001hello\377\377world\r\000' > corpus/telnet/negotiate
printf '\377\372\037\000\120\000\060\377\360' > corpus/telnet/naws
printf '\377\372\030\001\377\360' > corpus/telnet/termtype-send
printf '\377' > corpus/telnet/lone-iac

echo "seeded $n VT cases and 4 telnet cases"

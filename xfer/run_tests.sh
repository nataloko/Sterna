#!/bin/bash
# Interop matrix: Tera Term's protocol C against the reference Unix
# implementations. A protocol that passes here is one the Rust core can call
# through FFI and trust.
#
#   lrzsz  (sx/rx, sb/rb, sz/rz)  -- x/y/zmodem
#   C-Kermit (kermit)             -- kermit
#
# Usage: ./run_tests.sh [-v] [pattern]

set -u
cd "$(dirname "$0")"
XFER=$PWD/build/xfer
[ -x "$XFER" ] || { echo "build first: make"; exit 1; }

VERBOSE=""
[ "${1:-}" = "-v" ] && { VERBOSE="-v"; shift; }
FILTER="${1:-}"

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
pass=0; fail=0; skip=0

# A payload containing the bytes that break naive implementations: the
# ZMODEM/kermit control set (0x11 0x13 0x18 0x0d 0x0a 0x1a) and a run of 0xff.
make_payload() {
	head -c "$2" /dev/urandom > "$1"
	printf '\x11\x13\x18\x0d\x0a\x1a\xff\xff\xff\x00' >> "$1"
}

# XMODEM has no length field: the receiver gets whole 128/1024-byte blocks and
# trailing padding is expected, not a failure. Compare only the sent length.
compare() {
	local orig=$1 got=$2 padded=$3
	[ -f "$got" ] || return 1
	if [ "$padded" = pad ]; then
		head -c "$(stat -c%s "$orig")" "$got" | cmp -s - "$orig"
	else
		cmp -s "$orig" "$got"
	fi
}

run_case() {
	local name=$1 proto=$2 dir=$3 peer=$4 padded=${5:-exact} size=${6:-4096}
	[ -n "$FILTER" ] && [[ "$name" != *"$FILTER"* ]] && return

	local d="$WORK/$name"
	mkdir -p "$d/out"
	make_payload "$d/payload.bin" "$size"

	# The peer shares the pty, so its diagnostics would land in the protocol
	# stream. ymodem.c meets an unexpected byte with assert(0), so this
	# redirect is load-bearing, not tidiness.
	local cmd="${peer//@FILE@/$d/payload.bin} 2>/dev/null"
	local extra=""
	[ "$proto" = x ] && extra="--recv-name payload.bin"

	local out rc
	if [ "$dir" = recv ]; then
		out=$(cd "$d" && timeout 120 "$XFER" --proto "$proto" --recv "$d/out" \
		      $extra --pty "$cmd" --limit 90 $VERBOSE 2>&1)
	else
		out=$(cd "$d/out" && timeout 120 "$XFER" --proto "$proto" \
		      --send "$d/payload.bin" --pty "$cmd" --limit 90 $VERBOSE 2>&1)
	fi
	rc=$?

	if [ $rc -eq 0 ] && compare "$d/payload.bin" "$d/out/payload.bin" "$padded"; then
		printf '  ok   %-22s %s\n' "$name" "$(echo "$out" | grep -o 'in=[0-9]* out=[0-9]*')"
		pass=$((pass+1))
	else
		printf '  FAIL %-22s rc=%d\n' "$name" "$rc"
		[ -n "$VERBOSE" ] && echo "$out" | sed 's/^/         /'
		if [ -f "$d/out/payload.bin" ]; then
			printf '       sizes: sent %s got %s\n' \
			  "$(stat -c%s "$d/payload.bin")" "$(stat -c%s "$d/out/payload.bin")"
		else
			printf '       no file received\n'
		fi
		fail=$((fail+1))
	fi
}

echo "=== xfer interop: Tera Term protocol C vs lrzsz / C-Kermit ==="

# Tera Term receives; the reference implementation sends.
run_case xmodem-recv    x recv "sx @FILE@"   pad
run_case ymodem-recv    y recv "sb @FILE@"   exact
run_case zmodem-recv    z recv "sz -b @FILE@" exact

# Tera Term sends; the reference implementation receives.
run_case xmodem-send    x send "rx payload.bin" pad
run_case ymodem-send    y send "rb"           exact
run_case zmodem-send    z send "rz -b"        exact

# 1 MB: exercises zmodem windowing and the multi-packet paths.
run_case zmodem-recv-1m z recv "sz -b @FILE@" exact 1048576
run_case zmodem-send-1m z send "rz -b"        exact 1048576

if command -v kermit >/dev/null; then
	run_case kermit-recv kermit recv "kermit -i -s @FILE@" exact
	run_case kermit-send kermit send "kermit -i -r"        exact
else
	echo "  skip kermit (C-Kermit not installed)"; skip=$((skip+1))
fi

echo
echo "$pass passed, $fail failed, $skip skipped"
[ $fail -eq 0 ]

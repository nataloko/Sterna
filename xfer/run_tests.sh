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
	# Kermit transmits names in its "common form", which is uppercase, so the
	# received file is PAYLOAD.BIN. That is correct protocol behaviour on both
	# sides, not a defect — accept either spelling.
	[ -f "$got" ] || got="$(dirname "$got")/$(basename "$got" | tr 'a-z' 'A-Z')"
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

	# rc 4 is "the peer closed before the final handshake". lrzsz's `rb` exits
	# without acknowledging the closing null block of a YMODEM batch, so the
	# file is complete and correct but the protocol never sees its last ACK.
	# The byte-comparison below is the real assertion either way, so accept 4 —
	# but only alongside a byte-identical file, never on its own.
	if { [ $rc -eq 0 ] || [ $rc -eq 4 ]; } \
	   && compare "$d/payload.bin" "$d/out/payload.bin" "$padded"; then
		printf '  ok   %-22s %s\n' "$name" "$(echo "$out" | grep -o 'in=[0-9]* out=[0-9]*')"
		pass=$((pass+1))
	else
		printf '  FAIL %-22s rc=%d\n' "$name" "$rc"
		[ -n "$VERBOSE" ] && echo "$out" | sed 's/^/         /'
		local any; any=$(ls "$d/out" 2>/dev/null | head -1)
		if [ -n "$any" ]; then
			printf '       sizes: sent %s got %s (%s)\n' \
			  "$(stat -c%s "$d/payload.bin")" "$(stat -c%s "$d/out/$any")" "$any"
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

# G-Kermit rather than C-Kermit: C-Kermit on a pty sees a tty and drops into
# interactive command mode instead of speaking the protocol.
if command -v gkermit >/dev/null; then
	run_case kermit-recv kermit recv "gkermit -s @FILE@" exact
	run_case kermit-send kermit send "gkermit -r"        exact
else
	echo "  skip kermit (gkermit not installed)"; skip=$((skip+1))
fi

echo
echo "$pass passed, $fail failed, $skip skipped"
[ $fail -eq 0 ]

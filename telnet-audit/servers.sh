#!/bin/bash
# Start a real telnet server for the transport's interop tests, then stop it.
#
#   :2323  GNU inetutils telnetd, with /bin/cat where /bin/login goes — so it
#          needs no account, no password and no root-owned pty, and whatever
#          comes back is what went out.
#   :2324  A raw echo, speaking no telnet at all. What a console server's
#          per-line port looks like, and the thing TelnetMode::Raw is for.
#
# telnetd has no standalone listening mode — it expects inetd to hand it an
# accepted socket on stdin — so `inetd.py` is fifteen lines of exactly that.
#
# Usage: ./servers.sh start | stop | status
#
# Unlike ssh-audit's, this needs no sudo and creates no accounts: both servers
# run as the invoking user on loopback ports.

set -u
DIR="${XDG_RUNTIME_DIR:-/tmp}/termitta-telnet-audit"
TELNET_PORT=2323
RAW_PORT=2324
HERE="$(cd "$(dirname "$0")" && pwd)"

TELNETD=""
for candidate in /usr/sbin/telnetd /usr/sbin/in.telnetd /usr/libexec/telnetd; do
	[ -x "$candidate" ] && TELNETD="$candidate" && break
done

case "${1:-status}" in
start)
	mkdir -p "$DIR"
	if [ -z "$TELNETD" ]; then
		echo "no telnetd found — install inetutils-telnetd (Debian/Ubuntu)" >&2
		echo "or telnet-server (Fedora). The tests skip without it." >&2
		exit 1
	fi
	"$HERE/inetd.py" "$TELNET_PORT" -- "$TELNETD" -E /bin/cat \
		> "$DIR/telnetd.log" 2>&1 &
	echo $! > "$DIR/telnetd.pid"
	# The raw counterparty: no telnet, so an 0xFF is data. `cat` on the socket
	# rather than on a pty, which is the point — no line discipline either.
	"$HERE/inetd.py" "$RAW_PORT" -- /bin/cat > "$DIR/raw.log" 2>&1 &
	echo $! > "$DIR/raw.pid"
	sleep 1
	echo "telnetd on :$TELNET_PORT, raw echo on :$RAW_PORT"
	;;
stop)
	for name in telnetd raw; do
		if [ -f "$DIR/$name.pid" ]; then
			kill "$(cat "$DIR/$name.pid")" 2>/dev/null
			rm -f "$DIR/$name.pid"
		fi
	done
	echo "stopped"
	;;
status)
	ss -tln 2>/dev/null | grep -E ":$TELNET_PORT|:$RAW_PORT" || echo "not running"
	;;
esac

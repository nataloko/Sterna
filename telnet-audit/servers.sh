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
DIR="${XDG_RUNTIME_DIR:-/tmp}/sterna-telnet-audit"
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
	# Wait for the listeners rather than for a second, and say so if they
	# never arrive. `inetd.py` is a PEP 723 script run through `uv`, so on a
	# machine without `uv` both children die immediately — and printing
	# "telnetd on :2323" anyway is how a CI job spent three minutes building a
	# Qt frontend to discover a connection refused. The error is in the log
	# file; put it where whoever ran this will see it.
	for name in telnetd:$TELNET_PORT raw:$RAW_PORT; do
		port="${name#*:}"
		for _ in $(seq 50); do
			(exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null && break
			sleep 0.2
		done
		if ! (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null; then
			echo "${name%%:*} never listened on :$port" >&2
			cat "$DIR/${name%%:*}.log" >&2
			exit 1
		fi
	done
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

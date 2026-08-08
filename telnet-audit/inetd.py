#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# ///
"""A one-line inetd, so a real `telnetd` can be interoped against.

GNU inetutils' `telnetd` has no standalone listening mode — it expects to be
launched by inetd with the accepted socket already on stdin and stdout. That is
the only thing standing between this project and a genuinely independent telnet
counterparty, and it is about fifteen lines.

    ./inetd.py 2323 -- /usr/sbin/telnetd -E /bin/cat

`-E` replaces `/bin/login`, so nothing here needs an account, a password or a
pty owned by root. `/bin/cat` makes the far end an echo service, which is
exactly what a protocol test wants: whatever comes back is what went out, after
both sides' framing.

Kill it with SIGTERM; it reaps its children and exits.
"""

import os
import signal
import socket
import sys


def main() -> int:
    if "--" not in sys.argv:
        print(__doc__)
        return 2
    split = sys.argv.index("--")
    port = int(sys.argv[1])
    command = sys.argv[split + 1 :]

    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", port))
    listener.listen(8)
    # Reaped by the kernel: this never waits, and a zombie per connection would
    # otherwise pile up for as long as the test suite runs.
    signal.signal(signal.SIGCHLD, signal.SIG_IGN)
    print(f"listening on 127.0.0.1:{port} -> {' '.join(command)}", flush=True)

    while True:
        try:
            conn, _ = listener.accept()
        except OSError:
            return 0
        if os.fork() == 0:
            listener.close()
            # What inetd does, and the whole trick: the accepted socket becomes
            # the child's stdin, stdout and stderr.
            for fd in (0, 1, 2):
                os.dup2(conn.fileno(), fd)
            conn.close()
            os.execvp(command[0], command)
        conn.close()


if __name__ == "__main__":
    raise SystemExit(main())

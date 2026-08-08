# telnet-audit

A real telnet server, so the transport's interop tests have a counterparty
that was not written here.

```sh
./servers.sh start     # :2323 telnetd, :2324 raw echo
./servers.sh status
./servers.sh stop
```

Unlike `ssh-audit/servers.sh` this needs **no `sudo`** and creates no accounts.
Both servers run as the invoking user on loopback ports.

## Why a whole directory for fifteen lines

`crates/tt-conn/src/telnet/protocol.rs` is ported from Tera Term's `telnet.c`
and the IAC handling in `ttpcmn/ttcmn.c`, and its unit tests are byte strings
derived from that C. They prove the port matches upstream and **nothing** about
whether upstream matches the world. Only a server somebody else wrote can close
that, and the first thing this one does is prove the point:

```
WILL 37 (AUTHENTICATION), WILL 38 (ENCRYPT), DO TERMINAL-TYPE,
DO TERMINAL-SPEED, DO XDISPLOC (35), DO NEW-ENVIRON (39), DO NEW-ENVIRON (36)
```

Four of those seven are above upstream's `MaxTelOpt` of 34 and get a flat
refusal. So the very first exchange of every session exercises the path that
would otherwise never be reached by a test written against our own idea of a
server.

## `inetd.py`, and why it exists

GNU inetutils' `telnetd` has no standalone listening mode — it expects `inetd`
to hand it an accepted socket on stdin. `inetd.py` is exactly that and nothing
else: listen, accept, fork, `dup2` the socket onto 0/1/2, `exec`.

`-E /bin/cat` replaces `/bin/login`, so no account, no password and no
root-owned pty are involved, and whatever comes back is what went out.

The **raw** port on 2324 runs `cat` straight on the socket — no telnet, no pty,
no line discipline. That is what a console server's per-line port looks like,
and it is the case `TelnetMode::Raw` exists for: a byte of `0xFF` in a firmware
upload is data, and a client that eats it corrupts the transfer.

## Installing a server

| | |
|---|---|
| Debian / Ubuntu | `apt install inetutils-telnetd` |
| Fedora | `dnf install telnet-server` |

`servers.sh start` looks for `/usr/sbin/telnetd`, `/usr/sbin/in.telnetd` and
`/usr/libexec/telnetd`, and says which package to install if it finds none. The
tests skip loudly rather than failing when the servers are not running.

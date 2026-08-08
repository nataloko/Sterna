#!/bin/bash
# Start the SSH servers the audit runs against, then stop them again.
#
#   openssh-legacy  :2222  OpenSSH 9.6 with the pre-2020 algorithms explicitly
#                          re-enabled — ssh-rsa host keys, SHA-1 kex, CBC
#                          ciphers, hmac-sha1. Models what old gear offers.
#   dropbear        :2223  A genuinely different implementation, and the one
#                          actually found on console servers and embedded kit.
#
# Usage: ./servers.sh start | stop | status

set -u
DIR="${XDG_RUNTIME_DIR:-/tmp}/sterna-ssh-audit"
OPENSSH_PORT=2222
DROPBEAR_PORT=2223
# Throwaway account for the password / keyboard-interactive cases. Old gear
# rarely does public keys, so those auth paths need covering. Both servers
# listen on 127.0.0.1 only and the account is removed by `stop`.
TESTUSER=sterna-test
TESTPASS=spike5-not-a-secret

setup_keys() {
	mkdir -p "$DIR"
	chmod 700 "$DIR"
	# RSA host key: old gear predates ed25519, so ssh-rsa is the realistic case.
	[ -f "$DIR/hostkey_rsa" ] || \
		ssh-keygen -q -t rsa -b 2048 -N '' -f "$DIR/hostkey_rsa"
	[ -f "$DIR/hostkey_ed25519" ] || \
		ssh-keygen -q -t ed25519 -N '' -f "$DIR/hostkey_ed25519"
	# Client keys, one of each type the audit exercises.
	[ -f "$DIR/id_rsa" ] || ssh-keygen -q -t rsa -b 2048 -N '' -f "$DIR/id_rsa"
	[ -f "$DIR/id_ed25519" ] || ssh-keygen -q -t ed25519 -N '' -f "$DIR/id_ed25519"
	cat "$DIR/id_rsa.pub" "$DIR/id_ed25519.pub" > "$DIR/authorized_keys"
	chmod 600 "$DIR/authorized_keys" "$DIR"/id_* "$DIR"/hostkey_*

	# Dropbear needs its host key in its own format.
	[ -f "$DIR/dropbear_rsa" ] || \
		dropbearkey -t rsa -s 2048 -f "$DIR/dropbear_rsa" >/dev/null 2>&1
}

write_sshd_config() {
	cat > "$DIR/sshd_config" <<EOF
Port $OPENSSH_PORT
ListenAddress 127.0.0.1
HostKey $DIR/hostkey_rsa
HostKey $DIR/hostkey_ed25519
PidFile $DIR/sshd.pid
AuthorizedKeysFile $DIR/authorized_keys
StrictModes no
# UsePAM is what makes keyboard-interactive available at all.
UsePAM yes
PasswordAuthentication yes
KbdInteractiveAuthentication yes
PubkeyAuthentication yes
PermitRootLogin no
Subsystem sftp internal-sftp
LogLevel VERBOSE

# The point of this config: offer what a 2005-era device offers. OpenSSH still
# implements all of it, just disabled by default — '+' re-enables rather than
# replaces, so the modern set stays available too.
HostKeyAlgorithms +ssh-rsa
PubkeyAcceptedAlgorithms +ssh-rsa
KexAlgorithms +diffie-hellman-group1-sha1,diffie-hellman-group14-sha1,diffie-hellman-group-exchange-sha1
Ciphers +3des-cbc,aes128-cbc,aes192-cbc,aes256-cbc
MACs +hmac-sha1
EOF
}

case "${1:-status}" in
start)
	setup_keys
	write_sshd_config
	if ! id "$TESTUSER" >/dev/null 2>&1; then
		sudo useradd -m -s /bin/sh "$TESTUSER"
		echo "$TESTUSER:$TESTPASS" | sudo chpasswd
	fi
	# sshd refuses to start without its privsep directory, and says so in a way
	# that reads like a config error rather than a missing mkdir.
	sudo mkdir -p /run/sshd && sudo chmod 755 /run/sshd
	sudo /usr/sbin/sshd -f "$DIR/sshd_config" -E "$DIR/sshd.log"
	echo "openssh-legacy on :$OPENSSH_PORT"

	# No -E: it keeps dropbear attached to stderr instead of daemonising, which
	# silently hangs whatever started it.
	sudo /usr/sbin/dropbear -r "$DIR/dropbear_rsa" -p 127.0.0.1:$DROPBEAR_PORT \
	     -P "$DIR/dropbear.pid"
	echo "dropbear       on :$DROPBEAR_PORT"

	# Dropbear reads ~/.ssh/authorized_keys of the target user, not our file.
	mkdir -p "$HOME/.ssh" && chmod 700 "$HOME/.ssh"
	touch "$HOME/.ssh/authorized_keys"
	grep -qFf "$DIR/id_rsa.pub" "$HOME/.ssh/authorized_keys" 2>/dev/null || \
		cat "$DIR/authorized_keys" >> "$HOME/.ssh/authorized_keys"
	chmod 600 "$HOME/.ssh/authorized_keys"
	sleep 0.5
	;;
stop)
	[ -f "$DIR/sshd.pid" ] && sudo kill "$(cat "$DIR/sshd.pid")" 2>/dev/null
	[ -f "$DIR/dropbear.pid" ] && sudo kill "$(cat "$DIR/dropbear.pid")" 2>/dev/null
	id "$TESTUSER" >/dev/null 2>&1 && sudo userdel -r "$TESTUSER" 2>/dev/null
	echo "stopped"
	;;
status)
	ss -tlnp 2>/dev/null | grep -E ":$OPENSSH_PORT|:$DROPBEAR_PORT" || echo "not running"
	;;
esac

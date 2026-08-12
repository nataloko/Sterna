//! The four proxy relays against real servers on a real socket.
//!
//! The unit tests beside the module assert the bytes; these assert the thing
//! the bytes are for, and the property they all share is the one a byte
//! assertion cannot reach: **after `dial` returns, the next byte read must be
//! the session's first byte.** A handshake that leaves one byte of the
//! proxy's reply on the socket produces a terminal whose first screen has a
//! stray character on it and whose SSH key exchange fails with a protocol
//! error — neither of which points at the proxy.
//!
//! The servers are in-process rather than a real `squid` or `dante` for the
//! reason `oracle/` exists: a test that needs a daemon installed is a test
//! that does not run. The interop check against a genuine SOCKS5 server is
//! `ssh -D`, in `ssh-audit/`.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use tt_conn::proxy::{dial, ProxyKind, ProxyParams, Resolve, TelnetPrompts};

/// What the server writes once the handshake is done, and what the client
/// must see as the very first byte of the session.
const MARKER: &[u8] = b"SESSION";

fn params(kind: ProxyKind, port: u16) -> ProxyParams {
    ProxyParams {
        kind,
        host: "127.0.0.1".into(),
        port,
        timeout: Duration::from_secs(5),
        prompts: TelnetPrompts::default(),
        ..Default::default()
    }
}

/// Start a one-connection server and answer with `serve`.
fn serve<F>(serve: F) -> u16
where
    F: FnOnce(&mut TcpStream) + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    thread::spawn(move || {
        if let Ok((mut sock, _)) = listener.accept() {
            sock.set_read_timeout(Some(Duration::from_secs(5))).ok();
            serve(&mut sock);
            // Hold the socket open long enough for the client to read; a
            // close here would race the read on a fast machine.
            thread::sleep(Duration::from_millis(50));
        }
    });
    port
}

fn read_exact_n(sock: &mut TcpStream, n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    sock.read_exact(&mut buf).expect("server read");
    buf
}

/// Read to the blank line, which is where an HTTP request ends.
fn read_http_request(sock: &mut TcpStream) -> String {
    let mut all = Vec::new();
    let mut byte = [0u8; 1];
    while !all.ends_with(b"\r\n\r\n") {
        match sock.read(&mut byte) {
            Ok(0) | Err(_) => break,
            Ok(_) => all.push(byte[0]),
        }
    }
    String::from_utf8_lossy(&all).into_owned()
}

fn session_first_bytes(mut stream: TcpStream) -> Vec<u8> {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let mut got = vec![0u8; MARKER.len()];
    stream.read_exact(&mut got).expect("session read");
    got
}

#[test]
fn http_connect_tunnels() {
    let port = serve(|sock| {
        let req = read_http_request(sock);
        assert!(
            req.starts_with("CONNECT host.example:2222 HTTP/1.1\r\n"),
            "{req}"
        );
        assert!(req.contains("Host: host.example:2222\r\n"), "{req}");
        sock.write_all(b"HTTP/1.1 200 Connection established\r\nVia: t\r\n\r\n")
            .unwrap();
        sock.write_all(MARKER).unwrap();
    });

    let stream = dial(
        Some(&params(ProxyKind::Http, port)),
        "host.example",
        2222,
        Duration::from_secs(5),
    )
    .expect("dial");
    assert_eq!(session_first_bytes(stream), MARKER);
}

#[test]
fn http_credentials_reach_the_proxy() {
    let port = serve(|sock| {
        let req = read_http_request(sock);
        // base64("bob:s3cret")
        assert!(
            req.contains("Proxy-Authorization: Basic Ym9iOnMzY3JldA==\r\n"),
            "{req}"
        );
        sock.write_all(b"HTTP/1.1 200 OK\r\n\r\n").unwrap();
        sock.write_all(MARKER).unwrap();
    });

    let mut p = params(ProxyKind::Http, port);
    p.user = Some("bob".into());
    p.pass = Some("s3cret".into());
    let stream = dial(Some(&p), "host.example", 23, Duration::from_secs(5)).expect("dial");
    assert_eq!(session_first_bytes(stream), MARKER);
}

#[test]
fn http_refusal_carries_the_status() {
    let port = serve(|sock| {
        read_http_request(sock);
        sock.write_all(b"HTTP/1.1 502 Bad Gateway\r\nX: y\r\n\r\n")
            .unwrap();
    });

    let e = dial(
        Some(&params(ProxyKind::Http, port)),
        "host.example",
        23,
        Duration::from_secs(5),
    )
    .expect_err("a 502 must not look like a connection");
    assert!(format!("{e}").contains("502"), "{e}");
}

#[test]
fn socks5_tunnels_with_no_authentication() {
    let port = serve(|sock| {
        assert_eq!(read_exact_n(sock, 3), vec![5, 1, 0]);
        sock.write_all(&[5, 0]).unwrap();

        // VER CMD RSV ATYP, then a 12-byte name and the port.
        assert_eq!(read_exact_n(sock, 4), vec![5, 1, 0, 3]);
        let len = read_exact_n(sock, 1)[0] as usize;
        assert_eq!(read_exact_n(sock, len), b"host.example".to_vec());
        assert_eq!(read_exact_n(sock, 2), vec![0, 22]);

        sock.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 22]).unwrap();
        sock.write_all(MARKER).unwrap();
    });

    let mut p = params(ProxyKind::Socks5, port);
    p.resolve = Resolve::Remote;
    let stream = dial(Some(&p), "host.example", 22, Duration::from_secs(5)).expect("dial");
    assert_eq!(session_first_bytes(stream), MARKER);
}

#[test]
fn socks5_authenticates() {
    let port = serve(|sock| {
        assert_eq!(read_exact_n(sock, 4), vec![5, 2, 0, 2]);
        sock.write_all(&[5, 2]).unwrap();

        // RFC 1929: VER, ULEN, UNAME, PLEN, PASSWD.
        assert_eq!(read_exact_n(sock, 2), vec![1, 3]);
        assert_eq!(read_exact_n(sock, 3), b"bob".to_vec());
        assert_eq!(read_exact_n(sock, 1), vec![6]);
        assert_eq!(read_exact_n(sock, 6), b"s3cret".to_vec());
        sock.write_all(&[1, 0]).unwrap();

        read_exact_n(sock, 4 + 4 + 2);
        sock.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).unwrap();
        sock.write_all(MARKER).unwrap();
    });

    let mut p = params(ProxyKind::Socks5, port);
    p.user = Some("bob".into());
    p.pass = Some("s3cret".into());
    let stream = dial(Some(&p), "203.0.113.7", 22, Duration::from_secs(5)).expect("dial");
    assert_eq!(session_first_bytes(stream), MARKER);
}

#[test]
fn socks5_bad_credentials_are_not_a_connection() {
    let port = serve(|sock| {
        read_exact_n(sock, 4);
        sock.write_all(&[5, 2]).unwrap();
        let ulen = read_exact_n(sock, 2)[1] as usize;
        read_exact_n(sock, ulen);
        let plen = read_exact_n(sock, 1)[0] as usize;
        read_exact_n(sock, plen);
        sock.write_all(&[1, 1]).unwrap(); // any non-zero is a failure
    });

    let mut p = params(ProxyKind::Socks5, port);
    p.user = Some("bob".into());
    p.pass = Some("wrong".into());
    let e = dial(Some(&p), "203.0.113.7", 22, Duration::from_secs(5)).expect_err("rejected");
    assert!(format!("{e}").contains("credentials"), "{e}");
}

/// The reply's bound address is variable-length and has to come off the
/// socket. A domain-name one is the case that gets this wrong.
#[test]
fn socks5_drains_a_domain_bound_address_before_the_session() {
    let port = serve(|sock| {
        read_exact_n(sock, 3);
        sock.write_all(&[5, 0]).unwrap();
        read_exact_n(sock, 4 + 4 + 2);
        sock.write_all(&[5, 0, 0, 3, 5]).unwrap();
        sock.write_all(b"proxy").unwrap();
        sock.write_all(&[0, 22]).unwrap();
        sock.write_all(MARKER).unwrap();
    });

    let stream = dial(
        Some(&params(ProxyKind::Socks5, port)),
        "203.0.113.7",
        22,
        Duration::from_secs(5),
    )
    .expect("dial");
    assert_eq!(session_first_bytes(stream), MARKER);
}

#[test]
fn socks4_tunnels() {
    let port = serve(|sock| {
        assert_eq!(read_exact_n(sock, 9), vec![4, 1, 0, 22, 203, 0, 113, 7, 0]);
        sock.write_all(&[0, 90, 0, 22, 203, 0, 113, 7]).unwrap();
        sock.write_all(MARKER).unwrap();
    });

    let stream = dial(
        Some(&params(ProxyKind::Socks4, port)),
        "203.0.113.7",
        22,
        Duration::from_secs(5),
    )
    .expect("dial");
    assert_eq!(session_first_bytes(stream), MARKER);
}

#[test]
fn socks4a_sends_the_name() {
    let port = serve(|sock| {
        assert_eq!(read_exact_n(sock, 8), vec![4, 1, 0, 23, 0, 0, 0, 1]);
        assert_eq!(read_exact_n(sock, 4), b"bob\0".to_vec());
        assert_eq!(read_exact_n(sock, 13), b"host.example\0".to_vec());
        sock.write_all(&[0, 90, 0, 0, 0, 0, 0, 0]).unwrap();
        sock.write_all(MARKER).unwrap();
    });

    let mut p = params(ProxyKind::Socks4, port);
    p.user = Some("bob".into());
    p.resolve = Resolve::Remote;
    let stream = dial(Some(&p), "host.example", 23, Duration::from_secs(5)).expect("dial");
    assert_eq!(session_first_bytes(stream), MARKER);
}

#[test]
fn socks4_refusal_carries_the_code() {
    let port = serve(|sock| {
        read_exact_n(sock, 9);
        sock.write_all(&[0, 91, 0, 0, 0, 0, 0, 0]).unwrap();
    });

    let e = dial(
        Some(&params(ProxyKind::Socks4, port)),
        "203.0.113.7",
        22,
        Duration::from_secs(5),
    )
    .expect_err("rejected");
    assert!(format!("{e}").contains("CD 91"), "{e}");
}

#[test]
fn the_telnet_proxy_is_answered_like_a_person() {
    let port = serve(|sock| {
        sock.write_all(b"Terminal server, port 3\r\n").unwrap();
        sock.write_all(b"Username:").unwrap();
        // No newline after the prompt: upstream reads to one, so the answer
        // only comes after the server ends the line.
        sock.write_all(b"\r\n").unwrap();
        assert_eq!(read_exact_n(sock, 4), b"bob\n".to_vec());

        sock.write_all(b"Password:\r\n").unwrap();
        assert_eq!(read_exact_n(sock, 7), b"s3cret\n".to_vec());

        sock.write_all(b">> Host name: \r\n").unwrap();
        assert_eq!(read_exact_n(sock, 16), b"host.example:23\n".to_vec());

        sock.write_all(b"-- Connected to host.example\r\n").unwrap();
        sock.write_all(MARKER).unwrap();
    });

    let mut p = params(ProxyKind::Telnet, port);
    p.user = Some("bob".into());
    p.pass = Some("s3cret".into());
    let stream = dial(Some(&p), "host.example", 23, Duration::from_secs(5)).expect("dial");
    assert_eq!(session_first_bytes(stream), MARKER);
}

#[test]
fn the_telnet_proxys_error_string_ends_it() {
    let port = serve(|sock| {
        sock.write_all(b">> Host name: \r\n").unwrap();
        read_exact_n(sock, 16);
        sock.write_all(b"!!!!!!!! host unreachable\r\n").unwrap();
    });

    let e = dial(
        Some(&params(ProxyKind::Telnet, port)),
        "host.example",
        23,
        Duration::from_secs(5),
    )
    .expect_err("refused");
    assert!(format!("{e}").contains("telnet proxy refused"), "{e}");
}

/// No proxy, and a proxy with no host, must both be the ordinary direct
/// connection rather than an error or a dial to nowhere.
#[test]
fn an_inactive_proxy_connects_directly() {
    let port = serve(|sock| {
        sock.write_all(MARKER).unwrap();
    });

    let stream = dial(None, "127.0.0.1", port, Duration::from_secs(5)).expect("direct");
    assert_eq!(session_first_bytes(stream), MARKER);

    let port = serve(|sock| {
        sock.write_all(MARKER).unwrap();
    });
    let mut p = params(ProxyKind::Socks5, 1);
    p.host.clear();
    let stream = dial(Some(&p), "127.0.0.1", port, Duration::from_secs(5)).expect("direct");
    assert_eq!(session_first_bytes(stream), MARKER);
}

/// A proxy that cannot be reached must say it was the proxy. "Connection
/// refused" about the host name the user typed sends them to check a host
/// that was never dialled.
#[test]
fn an_unreachable_proxy_names_itself() {
    // Bind and drop, so the port is almost certainly closed.
    let dead = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = dead.local_addr().unwrap().port();
    drop(dead);

    let e = dial(
        Some(&params(ProxyKind::Socks5, port)),
        "host.example",
        22,
        Duration::from_secs(2),
    )
    .expect_err("nothing is listening");
    let msg = format!("{e}");
    assert!(msg.contains("SOCKS5 proxy"), "{msg}");
    assert!(msg.contains("127.0.0.1"), "{msg}");
}

/// The four relays above are checked against servers written from the same
/// reading of the protocols that wrote the client, which proves the two halves
/// agree and nothing about whether either matches the world. This one closes
/// that: it drives a **real** SOCKS5 server — OpenSSH's `ssh -D`, which speaks
/// SOCKS4 and SOCKS5 both and auto-detects by the version byte.
///
/// ```sh
/// D=$(mktemp -d)
/// ssh-keygen -q -t ed25519 -N '' -f $D/hostkey
/// ssh-keygen -q -t ed25519 -N '' -f $D/id && cp $D/id.pub $D/authorized_keys
/// printf 'Port 2299\nListenAddress 127.0.0.1\nHostKey %s/hostkey\n\
/// AuthorizedKeysFile %s/authorized_keys\nStrictModes no\nUsePAM no\n' $D $D \
///   > $D/sshd_config
/// /usr/sbin/sshd -f $D/sshd_config -D &
/// ssh -q -N -D 127.0.0.1:11080 -p 2299 -i $D/id -o StrictHostKeyChecking=no \
///   -o UserKnownHostsFile=$D/known_hosts -o IdentitiesOnly=yes 127.0.0.1 &
/// TT_SOCKS_PROXY=127.0.0.1:11080 cargo test -p tt-conn --test proxy
/// ```
///
/// Skipped without `TT_SOCKS_PROXY`, the way the SSH suite skips without its
/// rig — a test that needs a daemon and fails when it is absent is a test
/// somebody turns off.
#[test]
fn a_real_socks_server_agrees() {
    let Ok(addr) = std::env::var("TT_SOCKS_PROXY") else {
        eprintln!("skipped: set TT_SOCKS_PROXY=host:port to a real SOCKS server");
        return;
    };
    let (phost, pport) = addr.rsplit_once(':').expect("TT_SOCKS_PROXY is host:port");
    let pport: u16 = pport.parse().expect("a port");

    for (kind, resolve, host) in [
        (ProxyKind::Socks5, Resolve::Local, "127.0.0.1"),
        (ProxyKind::Socks5, Resolve::Remote, "localhost"),
        (ProxyKind::Socks4, Resolve::Local, "127.0.0.1"),
        (ProxyKind::Socks4, Resolve::Remote, "localhost"),
    ] {
        // The target is ours, so what is being tested is the client and the
        // server's agreement about the handshake in front of it. One per case:
        // `serve` accepts once.
        let target = serve(|sock| {
            sock.write_all(MARKER).unwrap();
            let mut echo = [0u8; 4];
            sock.read_exact(&mut echo).unwrap();
            sock.write_all(&echo).unwrap();
        });
        let mut p = params(kind, pport);
        p.host = phost.to_string();
        p.resolve = resolve;

        let mut stream = dial(Some(&p), host, target, Duration::from_secs(5))
            .unwrap_or_else(|e| panic!("{kind:?}/{resolve:?} through {addr}: {e}"));
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();

        let mut got = vec![0u8; MARKER.len()];
        stream
            .read_exact(&mut got)
            .unwrap_or_else(|e| panic!("{kind:?}/{resolve:?} read: {e}"));
        assert_eq!(got, MARKER, "{kind:?}/{resolve:?}");

        // ...and back the other way, which is the half a handshake test can
        // pass without: a tunnel that only ever carries the server's greeting.
        stream.write_all(b"ping").expect("session write");
        let mut back = [0u8; 4];
        stream.read_exact(&mut back).expect("echo");
        assert_eq!(&back, b"ping", "{kind:?}/{resolve:?}");
    }
}

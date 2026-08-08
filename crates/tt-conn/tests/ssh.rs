//! The SSH transport, against a real server.
//!
//! ```sh
//! cd ssh-audit && ./servers.sh start        # :2222 OpenSSH, :2223 dropbear
//! D=$XDG_RUNTIME_DIR/sterna-ssh-audit
//! TT_SSH_HOST=127.0.0.1 TT_SSH_PORT=2222 \
//!   TT_SSH_USER=$USER TT_SSH_KEY=$D/id_ed25519 \
//!   TT_SSH_PW_USER=sterna-test TT_SSH_PASS=spike5-not-a-secret \
//!   cargo test -p tt-conn --test ssh -- --test-threads=1
//! cd ssh-audit && ./servers.sh stop         # removes the throwaway account
//! ```
//!
//! **`--test-threads=1`, and for a different reason from the serial rig.**
//! There is no shared device here — the limit is the *server's*. OpenSSH's
//! `MaxStartups` defaults to `10:30:100` and starts randomly refusing above
//! ten concurrent unauthenticated connections; dropbear's ceiling is lower
//! still. Run these in parallel and a handful fail with what looks like a
//! connection bug and is actually the server declining to be hammered — the
//! symptom is a scatter of unrelated failures that all pass on their own,
//! and on dropbear it is most of the file.
//!
//! **Two accounts, and that is the rig rather than a quirk of these tests.**
//! `servers.sh` appends the client keys to the *invoking* user's
//! `authorized_keys`, and creates a throwaway `sterna-test` account with a
//! password because old gear rarely does public keys. So the key cases
//! authenticate as whoever is running them and the password cases do not.
//!
//! Without those variables the tests **skip loudly** rather than pass quietly,
//! the same rule the serial hardware tests follow: a machine with no server
//! still gets a green `cargo test` without pretending SSH was exercised.
//!
//! These are integration tests in the strict sense — real key exchange, real
//! authentication, a real pty with a real shell in it — because every part of
//! this module that could be unit-tested is a part that was never going to be
//! wrong. What breaks in an SSH client is the ordering: which method is tried
//! when the server offers three, what happens when the recorded host key
//! stops matching, whether the pty actually got the size it was told.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use tt_conn::ssh::{
    HostKeyDecision, HostKeyPolicy, KnownHosts, SshConn, SshConnect, SshParams, Step, Verdict,
};
use tt_conn::{Error, Transport};

/// What the servers need, or a reason there is nothing to test against.
struct Server {
    host: String,
    port: u16,
    /// The account the key opens.
    user: String,
    key: Option<PathBuf>,
    /// The account the password opens, which is a different one — see above.
    password_user: String,
    password: Option<String>,
}

fn server() -> Option<Server> {
    let host = std::env::var("TT_SSH_HOST").ok()?;
    let user = std::env::var("TT_SSH_USER").unwrap_or_else(|_| "root".into());
    Some(Server {
        host,
        port: std::env::var("TT_SSH_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(22),
        key: std::env::var("TT_SSH_KEY").ok().map(PathBuf::from),
        password_user: std::env::var("TT_SSH_PW_USER").unwrap_or_else(|_| user.clone()),
        password: std::env::var("TT_SSH_PASS").ok(),
        user,
    })
}

macro_rules! server_or_skip {
    () => {
        match server() {
            Some(s) => s,
            None => {
                eprintln!("SKIPPED: set TT_SSH_HOST/PORT/USER to run this (see the module docs)");
                return;
            }
        }
    };
}

/// A scratch directory keyed by test name, so tests do not share a
/// `known_hosts`.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("tt-ssh-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        Scratch(dir)
    }

    fn known_hosts(&self) -> KnownHosts {
        KnownHosts::with_files(vec![self.0.join("known_hosts")])
    }

    fn known_hosts_text(&self) -> String {
        std::fs::read_to_string(self.0.join("known_hosts")).unwrap_or_default()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn params(s: &Server, scratch: &Scratch) -> SshParams {
    let mut p = SshParams::new(&s.host, s.port, &s.user);
    p.known_hosts = scratch.known_hosts();
    // The agent belongs to whoever is running the tests and has no business
    // deciding whether they pass.
    p.use_agent = false;
    p.identities = s.key.iter().cloned().collect();
    p.connect_timeout = Duration::from_secs(15);
    p
}

/// What driving the connect state machine produced.
#[derive(Debug, Default)]
struct Seen {
    host_key: Vec<(String, Verdict)>,
    auth: Vec<String>,
}

/// Drive a connection to completion, answering as instructed.
///
/// This is what a frontend does, minus the event loop: poll, answer, poll.
/// Written out here rather than hidden in a helper crate because the shape of
/// it *is* the API being tested.
fn drive(
    mut c: SshConnect,
    host_key: HostKeyDecision,
    answers: &[&str],
) -> (std::result::Result<SshConn, Error>, Seen) {
    let mut seen = Seen::default();
    let mut next_answer = 0;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match c.poll() {
            Step::Working => {
                if Instant::now() > deadline {
                    return (Err(Error::Ssh("test timed out".into())), seen);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Step::HostKey(p) => {
                seen.host_key
                    .push((p.fingerprint.clone(), p.verdict.clone()));
                c.answer_host_key(host_key);
            }
            Step::Auth(p) => {
                seen.auth.push(format!("{:?}", p.kind));
                let a = answers.get(next_answer).copied().unwrap_or("");
                next_answer += 1;
                c.answer_auth(vec![a.to_string(); p.prompts.len().max(1)]);
            }
            Step::Ready(conn) => return (Ok(conn), seen),
            Step::Failed(e) => return (Err(e), seen),
        }
    }
}

/// Read until `needle` shows up, or give up.
fn read_until(conn: &mut SshConn, needle: &str, how_long: Duration) -> String {
    let deadline = Instant::now() + how_long;
    let mut out = Vec::new();
    let (mut data, mut events) = (Vec::new(), Vec::new());
    while Instant::now() < deadline {
        data.clear();
        match conn.read(&mut data, &mut events) {
            Ok(0) => std::thread::sleep(Duration::from_millis(10)),
            Ok(_) => {
                out.extend_from_slice(&data);
                if String::from_utf8_lossy(&out).contains(needle) {
                    break;
                }
            }
            Err(e) if e.is_disconnected() => break,
            Err(e) => panic!("read: {e}"),
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Read until the far end has been quiet for a moment, and throw it away.
///
/// A login shell is not ready the instant `request_shell` returns: the MOTD,
/// `Last login:` and the first prompt all arrive first, and anything typed
/// before `bash` starts reading is echoed by the pty and then dropped on the
/// floor. Every test that types something needs this, and a fixed sleep would
/// be both slower and less reliable.
fn settle(conn: &mut SshConn) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut quiet_since = Instant::now();
    let (mut data, mut events) = (Vec::new(), Vec::new());
    while Instant::now() < deadline && quiet_since.elapsed() < Duration::from_millis(500) {
        data.clear();
        match conn.read(&mut data, &mut events) {
            Ok(0) => std::thread::sleep(Duration::from_millis(20)),
            Ok(_) => quiet_since = Instant::now(),
            Err(e) => panic!("read while settling: {e}"),
        }
    }
}

fn send(conn: &mut SshConn, line: &str) {
    conn.write(line.as_bytes(), Duration::from_secs(1))
        .expect("write");
}

#[test]
fn a_public_key_gets_a_shell() {
    let s = server_or_skip!();
    let scratch = Scratch::new("pubkey");
    if s.key.is_none() {
        eprintln!("SKIPPED: no TT_SSH_KEY");
        return;
    }
    let c = SshConnect::start(params(&s, &scratch)).expect("start");
    let (conn, seen) = drive(c, HostKeyDecision::AcceptAndSave, &[]);
    let mut conn = conn.expect("connect");

    // The key answered, so nothing should have been asked for.
    assert!(seen.auth.is_empty(), "prompted for {:?}", seen.auth);
    assert_eq!(seen.host_key.len(), 1);
    assert_eq!(seen.host_key[0].1, Verdict::Unknown);

    settle(&mut conn);
    send(&mut conn, "echo tt-marker-$((6*7))\n");
    let out = read_until(&mut conn, "tt-marker-42", Duration::from_secs(10));
    assert!(out.contains("tt-marker-42"), "got {out:?}");
}

#[test]
fn accepting_a_key_makes_the_next_connection_silent() {
    let s = server_or_skip!();
    let scratch = Scratch::new("learn");
    if s.key.is_none() {
        eprintln!("SKIPPED: no TT_SSH_KEY");
        return;
    }

    let c = SshConnect::start(params(&s, &scratch)).expect("start");
    let (conn, first) = drive(c, HostKeyDecision::AcceptAndSave, &[]);
    drop(conn.expect("first connect"));
    assert_eq!(first.host_key.len(), 1);
    assert!(!scratch.known_hosts_text().is_empty(), "nothing recorded");

    let c = SshConnect::start(params(&s, &scratch)).expect("start");
    let (conn, second) = drive(c, HostKeyDecision::Refuse, &[]);
    conn.expect("second connect");
    assert!(
        second.host_key.is_empty(),
        "asked again about a key it had recorded: {:?}",
        second.host_key
    );
}

#[test]
fn accepting_once_records_nothing() {
    let s = server_or_skip!();
    let scratch = Scratch::new("once");
    if s.key.is_none() {
        eprintln!("SKIPPED: no TT_SSH_KEY");
        return;
    }
    let c = SshConnect::start(params(&s, &scratch)).expect("start");
    let (conn, _) = drive(c, HostKeyDecision::AcceptOnce, &[]);
    conn.expect("connect");
    assert!(
        scratch.known_hosts_text().is_empty(),
        "AcceptOnce wrote to known_hosts"
    );
}

#[test]
fn a_changed_key_is_reported_as_changed() {
    let s = server_or_skip!();
    let scratch = Scratch::new("changed");
    if s.key.is_none() {
        eprintln!("SKIPPED: no TT_SSH_KEY");
        return;
    }
    // Record the real key, then corrupt the recorded blob. This is the
    // man-in-the-middle case as the file sees it, and the one place the
    // verdict has to be more than "unknown".
    let c = SshConnect::start(params(&s, &scratch)).expect("start");
    drop(
        drive(c, HostKeyDecision::AcceptAndSave, &[])
            .0
            .expect("connect"),
    );

    let path = scratch.0.join("known_hosts");
    let text = std::fs::read_to_string(&path).unwrap();
    let mut fields: Vec<&str> = text.trim().split(' ').collect();
    // Same algorithm, different key: swap the base64 for another valid one.
    let other = "AAAAC3NzaC1lZDI1NTE5AAAAIGb5f8Vb1DzWn8Yc9k3Nl4Pv2Qw6Rt8Uy0Ia2Cs4Ee6G";
    fields[2] = other;
    std::fs::write(&path, format!("{}\n", fields.join(" "))).unwrap();

    let c = SshConnect::start(params(&s, &scratch)).expect("start");
    let (conn, seen) = drive(c, HostKeyDecision::Refuse, &[]);
    assert!(conn.is_err(), "connected to a host whose key had changed");
    assert_eq!(seen.host_key.len(), 1);
    assert!(
        matches!(seen.host_key[0].1, Verdict::Changed { .. }),
        "got {:?}",
        seen.host_key[0].1
    );
}

#[test]
fn refusing_a_host_key_fails_the_connection() {
    let s = server_or_skip!();
    let scratch = Scratch::new("refuse");
    let c = SshConnect::start(params(&s, &scratch)).expect("start");
    let (conn, seen) = drive(c, HostKeyDecision::Refuse, &[]);
    assert!(conn.is_err(), "connected after refusing the host key");
    assert_eq!(seen.host_key.len(), 1);
}

#[test]
fn a_password_is_asked_for_and_accepted() {
    let s = server_or_skip!();
    let scratch = Scratch::new("password");
    let Some(password) = s.password.clone() else {
        eprintln!("SKIPPED: no TT_SSH_PASS");
        return;
    };
    let mut p = params(&s, &scratch);
    p.user = s.password_user.clone();
    // No key, so the only thing left is what has to be typed.
    p.identities = vec![PathBuf::from("/nonexistent/id_none")];
    let c = SshConnect::start(p).expect("start");
    let (conn, seen) = drive(c, HostKeyDecision::AcceptOnce, &[&password]);
    let mut conn = conn.expect("connect");
    assert!(!seen.auth.is_empty(), "never asked for anything");

    settle(&mut conn);
    send(&mut conn, "echo tt-pw-ok\n");
    let out = read_until(&mut conn, "tt-pw-ok", Duration::from_secs(10));
    assert!(out.contains("tt-pw-ok"), "got {out:?}");
}

#[test]
fn a_wrong_password_ends_in_an_auth_error() {
    let s = server_or_skip!();
    let scratch = Scratch::new("badpassword");
    if s.password.is_none() {
        eprintln!("SKIPPED: no TT_SSH_PASS");
        return;
    }
    let mut p = params(&s, &scratch);
    p.user = s.password_user.clone();
    p.identities = vec![PathBuf::from("/nonexistent/id_none")];
    let c = SshConnect::start(p).expect("start");
    let (conn, seen) = drive(c, HostKeyDecision::AcceptOnce, &["not-the-password"; 8]);
    match conn {
        Err(Error::Auth { offered }) => {
            // The message is only useful if it can say what to try instead.
            assert!(!offered.is_empty(), "no methods reported");
        }
        Err(e) => panic!("wrong error: {e}"),
        Ok(_) => panic!("a wrong password authenticated"),
    }
    // Asked three times, not forever: OpenSSH's NumberOfPasswordPrompts.
    let passwords = seen.auth.iter().filter(|k| k.contains("Password")).count();
    assert!((1..=3).contains(&passwords), "asked {passwords} times");
}

#[test]
fn the_remote_pty_gets_the_size_it_was_told() {
    let s = server_or_skip!();
    let scratch = Scratch::new("size");
    if s.key.is_none() {
        eprintln!("SKIPPED: no TT_SSH_KEY");
        return;
    }
    let mut p = params(&s, &scratch);
    p.cols = 132;
    p.rows = 43;
    let c = SshConnect::start(p).expect("start");
    let mut conn = drive(c, HostKeyDecision::AcceptOnce, &[])
        .0
        .expect("connect");

    settle(&mut conn);
    send(&mut conn, "stty size\n");
    let out = read_until(&mut conn, "43 132", Duration::from_secs(10));
    assert!(out.contains("43 132"), "pty size wrong: {out:?}");

    // And a resize must reach the far end, or a remote `vim` draws to the old
    // size for the rest of the session.
    conn.resize(100, 30).expect("resize");
    settle(&mut conn);
    send(&mut conn, "stty size\n");
    let out = read_until(&mut conn, "30 100", Duration::from_secs(10));
    assert!(out.contains("30 100"), "resize did not reach it: {out:?}");
}

#[test]
fn the_far_end_hanging_up_reads_as_a_disconnect() {
    let s = server_or_skip!();
    let scratch = Scratch::new("exit");
    if s.key.is_none() {
        eprintln!("SKIPPED: no TT_SSH_KEY");
        return;
    }
    let c = SshConnect::start(params(&s, &scratch)).expect("start");
    let mut conn = drive(c, HostKeyDecision::AcceptOnce, &[])
        .0
        .expect("connect");

    settle(&mut conn);
    send(&mut conn, "exit\n");
    let deadline = Instant::now() + Duration::from_secs(10);
    let (mut data, mut events) = (Vec::new(), Vec::new());
    loop {
        assert!(Instant::now() < deadline, "never reported a disconnect");
        match conn.read(&mut data, &mut events) {
            Ok(0) => std::thread::sleep(Duration::from_millis(10)),
            Ok(_) => data.clear(),
            Err(e) => {
                assert!(e.is_disconnected(), "wrong error: {e}");
                break;
            }
        }
    }
}

#[test]
fn a_break_says_so_rather_than_pretending() {
    let s = server_or_skip!();
    let scratch = Scratch::new("break");
    if s.key.is_none() {
        eprintln!("SKIPPED: no TT_SSH_KEY");
        return;
    }
    let c = SshConnect::start(params(&s, &scratch)).expect("start");
    let mut conn = drive(c, HostKeyDecision::AcceptOnce, &[])
        .0
        .expect("connect");
    // On a console server reached over SSH a break is a real function.
    // Silently doing nothing would look like the far end ignoring it.
    match conn.send_break(Duration::from_millis(250)) {
        Err(Error::Unsupported(_)) => {}
        other => panic!("expected Unsupported, got {other:?}"),
    }
}

#[test]
fn the_descriptor_survives_the_handover() {
    let s = server_or_skip!();
    let scratch = Scratch::new("fd");
    if s.key.is_none() {
        eprintln!("SKIPPED: no TT_SSH_KEY");
        return;
    }
    // A frontend registers one notifier and keeps it; if the fd changed when
    // the session started, it would go deaf at exactly the moment output
    // begins.
    let c = SshConnect::start(params(&s, &scratch)).expect("start");
    let before = c.poll_fd();
    let conn = drive(c, HostKeyDecision::AcceptOnce, &[])
        .0
        .expect("connect");
    assert_eq!(conn.poll_fd(), Some(before));
}

#[test]
fn accept_new_records_a_first_seen_host_without_asking() {
    let s = server_or_skip!();
    let scratch = Scratch::new("acceptnew");
    if s.key.is_none() {
        eprintln!("SKIPPED: no TT_SSH_KEY");
        return;
    }
    let mut p = params(&s, &scratch);
    p.host_key_policy = HostKeyPolicy::AcceptNew;
    let c = SshConnect::start(p).expect("start");
    // `Refuse` is what the prompt would be answered with if one were raised,
    // so a connection proves no prompt happened.
    let (conn, seen) = drive(c, HostKeyDecision::Refuse, &[]);
    conn.expect("connect");
    assert!(seen.host_key.is_empty(), "asked: {:?}", seen.host_key);
    assert!(!scratch.known_hosts_text().is_empty(), "nothing recorded");
}

#[test]
fn accept_new_refuses_a_changed_key_without_asking() {
    let s = server_or_skip!();
    let scratch = Scratch::new("acceptnew-changed");
    if s.key.is_none() {
        eprintln!("SKIPPED: no TT_SSH_KEY");
        return;
    }
    let mut p = params(&s, &scratch);
    p.host_key_policy = HostKeyPolicy::AcceptNew;
    drop(
        drive(
            SshConnect::start(p.clone()).expect("start"),
            HostKeyDecision::Refuse,
            &[],
        )
        .0
        .expect("first connect"),
    );
    corrupt_recorded_key(&scratch);

    let c = SshConnect::start(p).expect("start");
    // `AcceptAndSave` would connect if a prompt were raised, so a failure
    // proves the policy refused on its own.
    let (conn, seen) = drive(c, HostKeyDecision::AcceptAndSave, &[]);
    assert!(seen.host_key.is_empty(), "asked: {:?}", seen.host_key);
    match conn {
        Err(Error::HostKey(_)) => {}
        Err(e) => panic!("wrong error: {e}"),
        Ok(_) => panic!("connected to a host whose key had changed"),
    }
}

#[test]
fn strict_refuses_an_unknown_host_before_asking() {
    let s = server_or_skip!();
    let scratch = Scratch::new("strict");
    let mut p = params(&s, &scratch);
    p.host_key_policy = HostKeyPolicy::Strict;
    let c = SshConnect::start(p).expect("start");
    let (conn, seen) = drive(c, HostKeyDecision::AcceptAndSave, &[]);
    assert!(
        seen.host_key.is_empty(),
        "asked despite StrictHostKeyChecking yes"
    );
    match conn {
        Err(Error::HostKey(_)) => {}
        Err(e) => panic!("wrong error: {e}"),
        Ok(_) => panic!("connected to an unrecorded host under Strict"),
    }
    assert!(
        scratch.known_hosts_text().is_empty(),
        "Strict wrote to known_hosts"
    );
}

#[test]
fn accept_any_connects_to_a_changed_key_and_keeps_the_evidence() {
    let s = server_or_skip!();
    let scratch = Scratch::new("acceptany");
    if s.key.is_none() {
        eprintln!("SKIPPED: no TT_SSH_KEY");
        return;
    }
    let mut p = params(&s, &scratch);
    p.host_key_policy = HostKeyPolicy::AcceptAny;
    drop(
        drive(
            SshConnect::start(p.clone()).expect("start"),
            HostKeyDecision::Refuse,
            &[],
        )
        .0
        .expect("first connect"),
    );
    let corrupted = corrupt_recorded_key(&scratch);

    let (conn, seen) = drive(
        SshConnect::start(p).expect("start"),
        HostKeyDecision::Refuse,
        &[],
    );
    conn.expect("connect");
    assert!(
        seen.host_key.is_empty(),
        "asked despite StrictHostKeyChecking no"
    );
    // The old line is still there and no new one was added: overwriting it
    // would destroy the only evidence that the key changed.
    assert_eq!(scratch.known_hosts_text(), corrupted);
}

/// Replace the recorded key with a different one of the same algorithm, and
/// return the file's new contents.
fn corrupt_recorded_key(scratch: &Scratch) -> String {
    let path = scratch.0.join("known_hosts");
    let text = std::fs::read_to_string(&path).unwrap();
    let mut fields: Vec<&str> = text.trim().split(' ').collect();
    let other = "AAAAC3NzaC1lZDI1NTE5AAAAIGb5f8Vb1DzWn8Yc9k3Nl4Pv2Qw6Rt8Uy0Ia2Cs4Ee6G";
    fields[2] = other;
    let out = format!("{}\n", fields.join(" "));
    std::fs::write(&path, &out).unwrap();
    out
}

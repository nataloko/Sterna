//! A macro driving a terminal, with a far end that answers.
//!
//! Everything below is the real thing on both sides: `tt_ttl::Interp` parsing
//! real TTL on its own thread, `tt_session::Session` parsing real VT on this
//! one, and a `MemoryTransport` between them playing the host. No part of it is
//! stubbed, which is the point — the pieces have unit tests each and what they
//! could not prove is that they fit.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use tt_macro::{channel, NullUi, SessionHost};
use tt_session::{MemoryTransport, Session};
use tt_ttl::Interp;
use tt_vt::Config;

/// How long a test will wait for a macro before calling it hung.
const LIMIT: Duration = Duration::from_secs(10);

/// What a run produced: the bytes the far end received, the screen, and the
/// terminal itself for the things a macro changes that neither of those shows.
struct Run {
    sent: Vec<u8>,
    screen: Vec<String>,
    session: Session,
}

/// Run a script against a session with a far end.
///
/// `respond` is the host: it is handed each batch the macro sent and whatever
/// it returns is fed back. The loop it sits in is the frontend's — service the
/// macro's jobs, pump the line — which is exactly the two calls the Qt shell
/// will make from its notifiers.
fn drive(script: &str, mut respond: impl FnMut(&[u8]) -> Vec<u8>) -> Run {
    let mut session = Session::new(Config {
        cols: 40,
        rows: 8,
        ..Config::default()
    });
    let (transport, far) = MemoryTransport::new();
    session.connect(Box::new(transport));
    let link = session.link_macro();
    let (tx, rx) = channel().unwrap();

    let (done_tx, done_rx) = mpsc::channel();
    let body = script.as_bytes().to_vec();
    let thread = std::thread::spawn(move || {
        let mut host = SessionHost::new(tx, link);
        let mut it = Interp::new("t.ttl", body, &mut host);
        it.run(&mut host);
        let _ = done_tx.send(());
    });

    let mut sent = Vec::new();
    let start = Instant::now();
    let mut finished = false;
    loop {
        rx.service(&mut session, &mut NullUi);
        session.pump(Duration::from_millis(1)).unwrap();

        // Taken rather than read: `outbound` clones and leaves the buffer
        // alone, so reading it every turn would count every byte again.
        let out = far.with(|s| std::mem::take(&mut s.outbound));
        if !out.is_empty() {
            sent.extend_from_slice(&out);
            let reply = respond(&out);
            if !reply.is_empty() {
                far.feed(&reply);
            }
        }

        if finished {
            break;
        }
        // One more turn after it ends, so a last `sendln` is not left queued.
        if done_rx.try_recv().is_ok() {
            finished = true;
        }
        if start.elapsed() > LIMIT {
            rx.cancel();
            let _ = thread.join();
            panic!("macro did not finish within {LIMIT:?}");
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    thread.join().unwrap();

    Run {
        screen: (0..session.grid().rows())
            .map(|y| row(&session, y))
            .collect(),
        sent,
        session,
    }
}

fn row(s: &Session, y: usize) -> String {
    let mut out = String::new();
    for cell in s.row(y) {
        if cell.width_class == tt_grid::WIDTH_PAD {
            continue;
        }
        let mut any = false;
        for cp in cell.codepoints() {
            out.push(char::from_u32(cp).unwrap_or('?'));
            any = true;
        }
        if !any {
            out.push(' ');
        }
    }
    out.trim_end().to_string()
}

/// The smallest whole thing: a script sends, the host answers, the script
/// waits for the answer and sends again. That is a login, and it is what the
/// macro language is for.
#[test]
fn a_script_holds_a_conversation() {
    let r = drive(
        "sendln 'hello'\nwait 'world'\nsendln 'goodbye'",
        |out| match out {
            b"hello\r" => b"world\r\n".to_vec(),
            _ => Vec::new(),
        },
    );
    // **`sendln` puts a bare CR on the wire**, because `ts.CRSend` defaults to
    // CR and the text path expands the newline by it (`ttcmn.c:814`). A
    // terminal configured for CRLF sends both; this is the shipping default,
    // and a test asserting `\r\n` would be asserting a setting.
    assert_eq!(r.sent, b"hello\rgoodbye\r");
    assert_eq!(r.screen[0], "world");
}

/// A `wait` matches what was *printed*, so a prompt dressed in colour matches
/// anyway — and a script that waited for the escape sequence instead would
/// wait for ever, because the parser eats it before the macro can see it.
#[test]
fn a_wait_matches_through_the_escape_sequences() {
    let r = drive(
        "sendln 'go'\ntimeout = 2\nwait 'login:'\nsendln 'root'",
        |out| match out {
            // A prompt in bold green, positioned, with the cursor saved and
            // restored around it — none of which a macro can see.
            b"go\r" => b"\x1b[2J\x1b[H\x1b7\x1b[1;32mlogin:\x1b[0m \x1b8".to_vec(),
            _ => Vec::new(),
        },
    );
    assert_eq!(r.sent, b"go\rroot\r");
    assert_eq!(r.screen[0], "login:");
}

/// `timeout` is in seconds and a `wait` that does not match ends with
/// `result` 0 — which is how every script tells "the host answered" from "the
/// host did not".
#[test]
fn a_wait_that_matches_nothing_times_out() {
    let r = drive(
        "timeout = 1\nwait 'never'\nif result = 0 then\n  sendln 'gave up'\nendif",
        |_| Vec::new(),
    );
    assert_eq!(r.sent, b"gave up\r");
}

/// `waitln` hands the matched line back in `inputstr` **without** its
/// terminator — `GetRecvLnBuff` drops the LF and the CR before it.
#[test]
fn waitln_hands_the_line_back_without_its_terminator() {
    let r = drive(
        "sendln 'go'\nwaitln 'ok'\nsendln inputstr",
        |out| match out {
            b"go\r" => b"status ok now\r\n".to_vec(),
            _ => Vec::new(),
        },
    );
    assert_eq!(r.sent, b"go\rstatus ok now\r");
}

/// `waitregex` is the other half of that, and the two disagree: the *pattern*
/// is matched before the LF joins the buffer, so the CR is the last byte it
/// sees and a `$` never matches a line from an ordinary host.
///
/// This is the trap `CLAUDE.md` records, driven end to end for the first time
/// — and it needs the tap to have put the CR there, which is the thing a macro
/// reading the wire would have got for free and a macro reading the screen
/// nearly did not.
#[test]
fn a_regex_anchored_at_the_end_needs_the_cr() {
    fn matched(pattern: &str) -> bool {
        let script =
            format!("sendln 'go'\ntimeout = 2\nwaitregex '{pattern}'\nint2str s result\nsendln s");
        let r = drive(&script, |out| match out {
            b"go\r" => b"status ok now\r\n".to_vec(),
            _ => Vec::new(),
        });
        assert!(r.sent.starts_with(b"go\r"), "{:?}", r.sent);
        r.sent.ends_with(b"1\r")
    }
    assert!(!matched("now$"), "a bare `$` should not have matched");
    assert!(matched("now\\r$"), "`\\r$` should have matched");
    // And the anchorless form matches either way, which is what most scripts
    // are written with and why this is not noticed until it is.
    assert!(matched("now"));
}

/// `send` puts bytes on the wire unchanged, which is why the session needed a
/// raw write: `#255` is not UTF-8 and must not be made into it.
#[test]
fn send_is_bytes_and_not_text() {
    let r = drive("send 'a' #255 'b'", |_| Vec::new());
    assert_eq!(r.sent, b"a\xffb");
}

/// `dispstr` writes to the screen without touching the line, and a macro can
/// then `wait` for what it wrote — because the tap sees the parser's output
/// whoever fed it.
#[test]
fn dispstr_reaches_the_screen_and_the_macro_can_see_it() {
    let r = drive(
        "dispstr 'local text'\ntimeout = 2\nwait 'local'\nif result = 1 then\n  sendln 'saw it'\nendif",
        |_| Vec::new(),
    );
    assert_eq!(r.screen[0], "local text");
    assert_eq!(r.sent, b"saw it\r");
}

/// **`gettitle` cannot see the title the host set**, which is the opposite of
/// what the name suggests and is upstream: `CmdGetTitle` answers with
/// `ts.Title` (`ttdde.c:646`), the one out of `TERATERM.INI`, while an OSC
/// writes `cv.TitleRemoteW`. The window shows the two combined; a macro is
/// told only its own half.
#[test]
fn a_macro_reads_the_files_title_and_not_the_hosts() {
    let r = drive(
        "settitle 'mine'\nsendln 'go'\ntimeout = 2\nwait 'ready'\ngettitle t\nsendln t",
        |out| match out {
            b"go\r" => b"\x1b]2;buildbox\x07ready\r\n".to_vec(),
            _ => Vec::new(),
        },
    );
    assert!(
        r.sent.ends_with(b"mine\r"),
        "{:?}",
        String::from_utf8_lossy(&r.sent)
    );
}

/// `settitle` goes the other way, and the frontend sees it as an ordinary
/// title change.
#[test]
fn settitle_reaches_the_terminal() {
    let r = drive("settitle 'from a macro'\ngettitle t\nsendln t", |_| {
        Vec::new()
    });
    assert_eq!(r.sent, b"from a macro\r");
}

/// `getipv4addr` fills a string array and says how many there were, and the
/// count is the *whole* answer while `result` says whether the array was big
/// enough — which is how a script learns it has to ask again with a bigger
/// one.
///
/// The addresses themselves are this machine's and change; what is asserted
/// is the shape, and `tt-conn`'s own tests check the rendering.
#[test]
fn a_macro_can_read_this_machines_addresses() {
    let r = drive(
        "strdim a 8\ngetipv4addr a n\nint2str s result\nsendln s\n\
         int2str c n\nsendln c\nsendln a[0]",
        |_| Vec::new(),
    );
    let sent = String::from_utf8_lossy(&r.sent).into_owned();
    let mut lines = sent.split('\r');
    assert_eq!(lines.next(), Some("1"), "the array was big enough");
    let count: usize = lines.next().unwrap().parse().expect("a count");
    let first = lines.next().unwrap();
    if count == 0 {
        // A machine with no address up is not an error, and `a[0]` is then
        // the empty string it was declared as.
        assert_eq!(first, "");
    } else {
        first
            .parse::<std::net::Ipv4Addr>()
            .unwrap_or_else(|_| panic!("not an address: {first:?}"));
    }
}

/// The serial control lines over something that has none.
///
/// The assertion is that the script *finishes*: upstream's terminal answers
/// `DDE_FNOTPROCESSED` for a connection that is not serial and the macro
/// reads that as success, so five commands in a row that do nothing must not
/// stop a login script that was written for a modem and is being run over
/// SSH. `getmodemstatus` is the one that reports, and it reports all four
/// lines low and `result` 0 — which is upstream's answer too, for a reason
/// `Interp::cmd_get_modem_status` spells out.
///
/// What the pins do when there *is* a port is in `tt-session`'s loopback
/// tests, where there is a cable to watch.
#[test]
fn the_serial_commands_are_quiet_over_a_connection_that_has_no_lines() {
    let r = drive(
        "setflowctrl 3\nsetdtr 0\nsetrts 0\nsetbaud 19200\n\
         getmodemstatus m\nint2str s m\nsendln s\nint2str n result\nsendln n",
        |_| Vec::new(),
    );
    assert_eq!(r.sent, b"0\r0\r");
    // And the settings are untouched, because the guard is ahead of the
    // assignment: a `setbaud` over a memory transport must not leave 19200
    // behind for the next serial connection.
    assert_eq!(r.session.settings().serial_baud, 9600);
}

/// `setecho` changes a setting *and* a mode, because upstream's are one
/// variable: `ts.LocalEcho` is what SRM assigns (`vtterm.c:2053`), so a macro
/// setting it is indistinguishable from the host setting it.
///
/// It is asserted against the terminal rather than against the screen or the
/// wire because nothing else can see it yet — and that is exactly how this
/// went wrong: the write named the *INI key* where `Settings::set_str` wants
/// the dotted name, so it resolved to nothing, answered `false`, and left
/// `setecho` a command that ran and did nothing at all.
#[test]
fn setecho_reaches_the_terminals_own_mode() {
    let r = drive("setecho 1\nsendln 'done'", |_| Vec::new());
    assert_eq!(r.sent, b"done\r");
    assert!(r.session.vt().local_echo(), "setecho 1 left the mode off");

    let r = drive("setecho 1\nsetecho 0\nsendln 'done'", |_| Vec::new());
    assert!(!r.session.vt().local_echo(), "setecho 0 left the mode on");
}

/// `clearscreen` is a terminal operation and not a wire one — nothing goes
/// out, and the screen is empty afterwards.
#[test]
fn clearscreen_empties_the_screen_without_sending_anything() {
    let r = drive(
        "dispstr 'something'\nclearscreen 0\nsendln 'cleared'",
        |_| Vec::new(),
    );
    assert_eq!(r.sent, b"cleared\r");
    assert_eq!(r.screen[0], "");
}

/// End stops a script blocked in a `wait` with no timeout, which is the one
/// thing a user must always be able to do.
#[test]
fn cancelling_releases_a_macro_blocked_for_ever() {
    let mut session = Session::new(Config::default());
    let (transport, _far) = MemoryTransport::new();
    session.connect(Box::new(transport));
    let link = session.link_macro();
    let (tx, rx) = channel().unwrap();

    let thread = std::thread::spawn(move || {
        let mut host = SessionHost::new(tx, link);
        // `timeout` defaults to 0, which upstream documents as "wait for ever".
        let mut it = Interp::new("t.ttl", b"wait 'never arrives'".to_vec(), &mut host);
        it.run(&mut host);
    });

    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(100) {
        rx.service(&mut session, &mut NullUi);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!thread.is_finished(), "it should still be waiting");

    rx.cancel();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !thread.is_finished() && Instant::now() < deadline {
        rx.service(&mut session, &mut NullUi);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(thread.is_finished(), "End did not stop it");
    thread.join().unwrap();
}

/// A frontend that goes away while a macro is blocked must not leave the
/// thread wedged — the window closed, and there is nothing left to answer to.
#[test]
fn a_macro_ends_when_the_frontend_does() {
    let mut session = Session::new(Config::default());
    let link = session.link_macro();
    let (tx, rx) = channel().unwrap();
    let thread = std::thread::spawn(move || {
        let mut host = SessionHost::new(tx, link);
        let mut it = Interp::new("t.ttl", b"sendln 'anyone there'".to_vec(), &mut host);
        it.run(&mut host);
    });
    drop(rx);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !thread.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(thread.is_finished(), "it hung waiting for a dead frontend");
    thread.join().unwrap();
}

/// A script driving the terminal's log, which is upstream's arrangement too:
/// `logopen` from a macro and `File > Log` are one log, and everything a macro
/// says about it goes to `filesys_log.cpp`.
///
/// The pause is the interesting part. What arrives while it is shut is thrown
/// away rather than held, so the "during" line is not in the file — but the
/// note the script writes to explain the gap is, which is where this port and
/// upstream part company. See `SessionLog::write_str`.
#[test]
fn a_script_can_open_pause_annotate_and_close_the_terminals_log() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.log");
    let script = format!(
        "logopen '{}' 0 0\n\
         sendln 'one'\n\
         wait 'one'\n\
         logpause\n\
         sendln 'two'\n\
         wait 'two'\n\
         logwrite '-- quiet --'#13#10\n\
         logstart\n\
         sendln 'three'\n\
         wait 'three'\n\
         loginfo s\n\
         logclose\n\
         sendln s",
        path.display()
    );

    // The host echoes each line back with an LF added, which is what puts it
    // on the screen and so into a text log: `sendln` sends a bare CR, and a CR
    // executes as a carriage return rather than as a line break.
    let r = drive(&script, |out| {
        let mut echo = out.to_vec();
        echo.push(b'\n');
        echo
    });

    // `'…'#13#10` has no space in it because a space would end the string
    // expression: the pieces of a concatenation have to abut, and `logwrite`
    // takes one string rather than a parameter list the way `sendln` does.
    //
    // The CR in the middle is not a mistake. The tap normalises the line
    // ending it saw into one LF; `logwrite` writes the string it was given,
    // character for character, which is `FLogPutUTF32_` and is why `#13#10` is
    // what upstream's own examples pass it.
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "one\n-- quiet --\r\nthree\n"
    );
    // `loginfo` handed back the open log's path — read while it was still
    // open, since `logclose` comes after it.
    let sent = String::from_utf8_lossy(&r.sent).into_owned();
    assert!(
        sent.ends_with(&format!("{}\r", path.display())),
        "loginfo did not report the path: {sent:?}"
    );
}

/// `logrotate` reconfigures and rotates nothing now, and none of the family is
/// an error with no log open — `FLogPause` and the three rotation setters all
/// return on a NULL `LogVar`, so a macro cannot tell.
#[test]
fn the_log_commands_are_quiet_with_no_log_open() {
    let r = drive(
        "logpause\nlogwrite 'nowhere'\nlogrotate 'size' '1M'\nlogrotate 'halt'\n\
         logstart\nloginfo s\nint2str n result\nsendln n",
        |_| Vec::new(),
    );
    // -1 is `loginfo`'s answer for "not logging", and reaching the line at all
    // is the assertion: any of the five refusing would have ended the script.
    assert_eq!(r.sent, b"-1\r");
}

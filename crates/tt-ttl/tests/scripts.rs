//! The 53 `.ttl` scripts in upstream's own `tests/` directory, run.
//!
//! `PLAN.md` has said since Stage 2 that the target for the macro language is
//! "the 53 `.ttl` scripts in `teraterm/tests/` pass", and that sentence needed
//! unpacking before it could be a gate. **Almost none of them is
//! self-checking.** They report to a human through `messagebox`, several are
//! deliberately full of errors so as to exercise the error dialog, one asks the
//! user to pick a file and most are Shift-JIS. Three do assert —
//! `test_file.ttl`, `macroparam.ttl` and `params_array.ttl` compare what they
//! got against what they expected and set an exit code — and for the other
//! fifty there is nothing to test.
//!
//! So "pass" here means what it means in `oracle/`: a **transcript** — every
//! send, every dialog, every action the macro asked the world for, in order —
//! and a golden per script. What that catches is a *change*: a command that
//! stops firing, an argument that starts arriving mangled, an error that moves
//! to a different line. What it cannot catch is the two of us being wrong the
//! same way, which is what `run_diff.sh` is for on the terminal side and what
//! nothing is for here, because there is no headless `ttpmacro.exe` to diff
//! against. Every golden was read before it was blessed; the rule in
//! `AGENTS.md` applies here exactly as it does to `oracle/cases/`.
//!
//! Eight decisions make a run reproducible. They are choices, not discoveries,
//! and a script's transcript only means anything against them:
//!
//! 1. **The terminal is connected.** `linked` and `com_ready` both start true,
//!    because these scripts were written to be run from a Tera Term with a
//!    session in it. Start them false and 34 of the 53 stop at their first
//!    `sendln` with `ErrLinkFirst`, which tests the link check and nothing
//!    else.
//! 2. **Nothing ever arrives.** `read_byte` answers `None`, so every `wait`,
//!    `waitln`, `waitn`, `waitrecv` and `waitregex` ends as a timeout. Feeding
//!    canned input would mean inventing a far end per script, and the far end
//!    is the one thing these scripts do not describe.
//! 3. **The error dialog answers Continue.** It has two buttons — `IDOK` is
//!    Stop and `IDCANCEL` is Continue (`errdlg.cpp:73`, `ttmparse.cpp:165`) —
//!    and Continue is what a human testing these presses, which
//!    `gui_commands_test.ttl` says out loud in a label named
//!    `this_line_is_error_push_continue`. Stopping instead would truncate
//!    every script whose point is the error.
//! 4. **A dialog takes its default answer unless the script needs otherwise**:
//!    OK, Yes, the preselected list item, a fixed string typed into `inputbox`,
//!    a fixed name chosen in the file dialogs. That is not a policy that
//!    terminates on its own — see [`User`] — so the two scripts that loop until
//!    a human does something specific get a scripted one.
//! 5. **The clock, the entropy and the machine are fixed**, and so is `HOME`
//!    and the XDG environment `getspecialfolder` reads. Paths that are still
//!    the running machine's — the scratch directory, the home under it, the
//!    test binary's own directory, which is what `getttdir` answers — are
//!    substituted out of the transcript, so a golden is the same on any
//!    machine.
//! 6. **Each script gets its own directory**, seeded with upstream's non-`.ttl`
//!    files so that the two scripts reading a data file find it, and the macro
//!    is named by an absolute path inside it so that every relative filename
//!    resolves there rather than into the repository. What it left behind is
//!    part of the transcript, because otherwise the file-handling scripts
//!    record nothing at all.
//! 7. **The command line is upstream's own.** `macroparam.ttl` and
//!    `params_array.ttl` are about `params[]` rather than about the language,
//!    and the `.bat` files next to them are the specification — the switches
//!    `/V`, `/i` and `/vxx` are eaten, kept or passed on depending only on
//!    which side of the macro's name they fall. [`launches`] transcribes those
//!    two files, so `macroparam.ttl` runs four times and each run is a separate
//!    stretch of the same golden. Every other script is launched with its own
//!    path and nothing else.
//! 8. **`exec` is isolated.** It runs in the macro process upstream and does
//!    here too, so there is no host method to record it. The one script using
//!    it starts Notepad and waits for a human to close it; [`isolated_source`]
//!    changes that program name to one which cannot exist on either platform,
//!    leaving the command's parse and failure path under test without letting
//!    the unattended suite start an external program.
//!
//! The scripts themselves are **not** copied into this tree: `../teraterm` is a
//! read-only reference checkout and stays the one copy. A tree without it skips
//! this test rather than failing it, which is the same bargain `oracle/` makes.
//!
//! ```sh
//! cargo test -p tt-ttl --test scripts
//! TTL_BLESS=1 cargo test -p tt-ttl --test scripts   # ...then read the diff
//! ```
//!
//! The reviewed goldens are the portable/Linux answers. Windows compares all
//! 53 against them too, then permits exactly [`WINDOWS_PLATFORM_DIFFS`] to
//! differ. That is deliberately not a set of Wine-blessed Windows goldens: the
//! allowlist makes the common 48 a gate and makes any new divergence a failure,
//! while native Windows remains the authority for the five platform-shaped
//! transcripts. Blessing is refused on Windows so it cannot overwrite the
//! reviewed set. A BOM is added to the harness's private copy of a BOM-less
//! script on Windows: these are language transcripts, so the machine's ACP
//! must not turn CP932 or UTF-8 fixture bytes into a new expected language.
//! `source.rs` tests the real ACP branch separately.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tt_ttl::host::{
    BeepSound, ClearScreen, DebugMode, DialogEnd, DialogPos, ErrorReport, FlowControl, ListBoxOpts,
    LogInfo, LogOpen, LogRotate, MacroWindow, ModemLines, ScriptHost, SendMode, ShowWindow,
    WindowGeometry, WindowState, Xfer,
};
use tt_ttl::{CmdLine, Interp, TtlError};

/// How far a script is allowed to run before the harness stops it.
///
/// Two of these are not conversations with a user at all. `array.ttl` fills a
/// 100,000-element array and reads it back, which is about 300,000 lines, and
/// `#35822-random.ttl` draws **eleven million** random numbers to check their
/// distribution — that one is a benchmark, it would take minutes, and its
/// transcript says `stopped` rather than pretending otherwise. The limit is set
/// above the first and below the second on purpose.
const MAX_LINES: usize = 500_000;

/// The clock every run believes in: 2023-11-14 22:13:20 UTC.
const NOW: i64 = 1_700_000_000;

/// Scripts whose purpose or output includes a Windows filesystem or shell
/// answer. Keep this exact: a missing member is as interesting as a new one.
#[cfg(windows)]
const WINDOWS_PLATFORM_DIFFS: &[&str] =
    &["#31050", "#31971", "#39452", "getspecialfolder", "spfolder"];

// ---------------------------------------------------------------------------
// The tape
// ---------------------------------------------------------------------------

/// A host that answers everything the same way twice and writes down what it
/// was asked, in order.
///
/// The order is the whole point, and it is why this is not
/// [`tt_ttl::RecordingHost`]: that one files each family of calls in its own
/// vector, which is what a unit test wants — assert on the dialogs, ignore the
/// sends — and which loses the interleaving that makes a transcript readable.
/// `sendln`, `messagebox`, `sendln` has to come out in that order or the golden
/// says nothing about when the box went up.
///
/// Questions with no side effect — `linked`, `cancelled`, the clock, the byte
/// reader — are deliberately silent. `cancelled` alone is asked once per line.
struct Tape {
    log: Vec<String>,
    dir: PathBuf,
    exit_code: i32,
    linked: bool,
    com_ready: bool,
    random: u32,
    clipboard: Vec<u8>,
    logging: Option<LogInfo>,
    user: User,
}

/// What the person at the keyboard does, for the scripts that ask.
///
/// Always pressing the default button is not a policy that terminates.
/// `gui_commands_test.ttl` loops on `yesnobox` until it has been told **both**
/// yes and no, on `inputbox` until the string is `ok`, on `passwordbox` until
/// it is `password`, and on `listbox` until every one of seven items has been
/// picked and then the dialog cancelled — so a host that always says yes runs
/// it forever, which is what the first blessing of this suite produced: a
/// 57,000-line transcript that stopped on the line limit.
///
/// So the harness plays a user. Each queue is consumed in order and falls back
/// to the default answer once it is empty, which is why only the two scripts
/// that need one have an entry in [`user`].
#[derive(Debug, Default)]
struct User {
    yes_no: VecDeque<bool>,
    input: VecDeque<Vec<u8>>,
    password: VecDeque<Vec<u8>>,
    /// `None` is Cancel, which `listbox` reports as -1.
    list: VecDeque<Option<usize>>,
}

/// The scripted user for a script, by file name. Everything not named here
/// gets the defaults.
fn user(script: &str) -> User {
    let mut u = User::default();
    if script == "gui_commands_test.ttl" {
        // Yes once and no once, which is what its `while` is waiting for.
        u.yes_no.extend([true, false]);
        // Something that does not match `^ok$`, so the retry arm runs too.
        u.input.extend([b"typed".to_vec(), b"ok".to_vec()]);
        u.password
            .extend([b"hunter2".to_vec(), b"password".to_vec()]);
        // Each of the seven items once — the script toggles a flag per item
        // and waits for all seven — and then a cancel, which is the other half
        // of its condition.
        u.list.extend((0..7).map(Some).chain([None]));
    }
    u
}

impl Tape {
    fn new(dir: PathBuf, user: User) -> Tape {
        Tape {
            log: Vec::new(),
            dir,
            exit_code: 0,
            linked: true,
            com_ready: true,
            random: 0,
            clipboard: b"clipboard".to_vec(),
            logging: None,
            user,
        }
    }

    fn note(&mut self, line: impl Into<String>) {
        self.log.push(line.into());
    }
}

/// Bytes as a golden can hold them: printable ASCII stays, everything else is
/// `\xNN`. Not `String::from_utf8_lossy`, which turns fifteen Shift-JIS scripts
/// into a wall of identical replacement characters — the point of a transcript
/// is that a changed byte changes it.
fn esc(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() + 2);
    out.push('"');
    for &b in bytes {
        match b {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            b'\r' => out.push_str("\\r"),
            b'\n' => out.push_str("\\n"),
            b'\t' => out.push_str("\\t"),
            0x20..=0x7E => out.push(b as char),
            _ => out.push_str(&format!("\\x{b:02x}")),
        }
    }
    out.push('"');
    out
}

/// A transfer request, with its path as text.
///
/// Not `{req:?}`: the derived `Debug` prints `path` as `[122, 58, 92, ...]`,
/// so the one field a reader wants to check is the one they have to decode.
fn show_xfer(req: &Xfer<'_>) -> String {
    let (name, path, extra) = match *req {
        Xfer::XmodemSend { path, opt } => ("xmodemsend", Some(path), format!(" {opt:?}")),
        Xfer::XmodemRecv { path, binary, opt } => (
            "xmodemrecv",
            Some(path),
            format!(" binary={} {opt:?}", on(binary)),
        ),
        Xfer::YmodemSend { path } => ("ymodemsend", Some(path), String::new()),
        Xfer::YmodemRecv => ("ymodemrecv", None, String::new()),
        Xfer::ZmodemSend { path, binary } => {
            ("zmodemsend", Some(path), format!(" binary={}", on(binary)))
        }
        Xfer::ZmodemRecv => ("zmodemrecv", None, String::new()),
        Xfer::KmtSend { path } => ("kmtsend", Some(path), String::new()),
        Xfer::KmtRecv => ("kmtrecv", None, String::new()),
        Xfer::KmtGet { path } => ("kmtget", Some(path), String::new()),
        Xfer::KmtFinish => ("kmtfinish", None, String::new()),
        Xfer::BPlusSend { path } => ("bplussend", Some(path), String::new()),
        Xfer::BPlusRecv => ("bplusrecv", None, String::new()),
        Xfer::QuickVanSend { path } => ("quickvansend", Some(path), String::new()),
        Xfer::QuickVanRecv => ("quickvanrecv", None, String::new()),
        Xfer::SendFile { path, binary } => {
            ("sendfile", Some(path), format!(" binary={}", on(binary)))
        }
        Xfer::RecvFile { path, autostop } => (
            "recvfile",
            Some(path),
            format!(" autostop={}ms", autostop.as_millis()),
        ),
    };
    match path {
        Some(p) => format!("{name} {}{extra}", esc(p)),
        None => format!("{name}{extra}"),
    }
}

/// The flags `logopen` and `loginfo` carry, named rather than positional.
fn show_log_flags(
    binary: bool,
    append: bool,
    plain_text: bool,
    timestamp: bool,
    hide_dialog: bool,
) -> String {
    let mut flags: Vec<&str> = Vec::new();
    for (set, name) in [
        (binary, "binary"),
        (append, "append"),
        (plain_text, "plaintext"),
        (timestamp, "timestamp"),
        (hide_dialog, "hidedialog"),
    ] {
        if set {
            flags.push(name);
        }
    }
    if flags.is_empty() {
        "-".to_string()
    } else {
        flags.join(",")
    }
}

fn on(b: bool) -> &'static str {
    if b {
        "on"
    } else {
        "off"
    }
}

impl ScriptHost for Tape {
    // ---- the macro itself ----

    fn error(&mut self, report: &ErrorReport<'_>) -> bool {
        let e = report.error;
        self.note(format!(
            "!! {:?} ({}) at line {} cols {}..{}",
            e,
            e.code(),
            report.line_no,
            report.start,
            report.end
        ));
        // Continue, which is `IDCANCEL` — see the note at the top.
        false
    }

    fn read_macro(&mut self, path: &[u8]) -> Result<Vec<u8>, TtlError> {
        self.note(format!("include {}", esc(path)));
        let name = String::from_utf8(path.to_vec()).map_err(|_| TtlError::CantOpen)?;
        std::fs::read(self.dir.join(name)).map_err(|_| TtlError::CantOpen)
    }

    fn disp_str(&mut self, s: &[u8]) -> Result<(), TtlError> {
        self.note(format!("dispstr {}", esc(s)));
        Ok(())
    }

    fn set_exit_code(&mut self, code: i32) {
        self.exit_code = code;
        self.note(format!("setexitcode {code}"));
    }

    fn cancelled(&mut self) -> bool {
        false
    }

    // ---- what goes out ----

    fn linked(&mut self) -> bool {
        self.linked
    }

    fn send(&mut self, bytes: &[u8], mode: SendMode) -> Result<(), TtlError> {
        let how = match mode {
            SendMode::Compat => "send",
            SendMode::Text => "sendtext",
            SendMode::Binary => "sendbinary",
        };
        self.note(format!("{how} {}", esc(bytes)));
        Ok(())
    }

    fn send_broadcast(&mut self, text: &[u8]) -> Result<(), TtlError> {
        self.note(format!("sendbroadcast {}", esc(text)));
        Ok(())
    }

    fn send_multicast(&mut self, name: &[u8], text: &[u8]) -> Result<(), TtlError> {
        self.note(format!("sendmulticast {} {}", esc(name), esc(text)));
        Ok(())
    }

    fn set_multicast_name(&mut self, name: &[u8]) -> Result<(), TtlError> {
        self.note(format!("setmulticastname {}", esc(name)));
        Ok(())
    }

    fn send_key_code(&mut self, code: u16, repeat: u16) -> Result<(), TtlError> {
        self.note(format!("sendkcode {code} x{repeat}"));
        Ok(())
    }

    fn scp(&mut self, send: bool, path: &[u8], dest: &[u8]) -> Result<(), TtlError> {
        let dir = if send { "scpsend" } else { "scprecv" };
        self.note(format!("{dir} {} {}", esc(path), esc(dest)));
        Ok(())
    }

    fn send_break(&mut self) -> Result<(), TtlError> {
        self.note("sendbreak");
        Ok(())
    }

    // ---- what comes in ----

    fn read_byte(&mut self, _timeout: Option<Duration>) -> Option<u8> {
        None
    }

    fn flush_recv(&mut self) {
        self.note("flushrecv");
    }

    fn wait_for_all(
        &mut self,
        patterns: &[Vec<u8>],
        _timeout: Option<Duration>,
    ) -> Result<usize, TtlError> {
        let shown: Vec<String> = patterns.iter().map(|p| esc(p)).collect();
        self.note(format!("wait4all {}", shown.join(" ")));
        Ok(0)
    }

    fn sleep(&mut self, d: Duration) {
        self.note(format!("sleep {}ms", d.as_millis()));
    }

    // ---- the clock and the dice ----

    fn random_u32(&mut self) -> u32 {
        // A counter, not entropy: `random` has to give the same answer twice
        // for the golden to mean anything. The rejection loop that makes the
        // result uniform is in the interpreter, so this is all it takes.
        self.random = self.random.wrapping_add(0x9E37_79B9);
        self.random
    }

    fn now_unix(&mut self) -> i64 {
        NOW
    }

    fn uptime_ms(&mut self) -> Option<u64> {
        Some(1_234_567)
    }

    fn set_system_date(&mut self, year: i32, month: i32, day: i32) {
        self.note(format!("setdate {year:04}-{month:02}-{day:02}"));
    }

    fn set_system_time(&mut self, hour: i32, minute: i32, second: i32) {
        self.note(format!("settime {hour:02}:{minute:02}:{second:02}"));
    }

    // ---- the session ----

    fn com_ready(&mut self) -> bool {
        self.com_ready
    }

    fn connect(&mut self, cmdline: &[u8], cygwin: bool) -> Result<(), TtlError> {
        let how = if cygwin { "cygconnect" } else { "connect" };
        self.note(format!("{how} {}", esc(cmdline)));
        self.linked = true;
        self.com_ready = true;
        Ok(())
    }

    fn disconnect(&mut self, confirm: bool) -> Result<(), TtlError> {
        self.note(format!("disconnect confirm={}", on(confirm)));
        self.com_ready = false;
        Ok(())
    }

    fn close_terminal(&mut self) -> Result<(), TtlError> {
        self.note("closett");
        self.linked = false;
        self.com_ready = false;
        Ok(())
    }

    fn unlink(&mut self) {
        self.note("unlink");
        self.linked = false;
    }

    fn set_sync(&mut self, sync: bool) {
        self.note(format!("setsync {}", on(sync)));
    }

    // ---- the serial control lines ----

    fn set_dtr(&mut self, up: bool) {
        self.note(format!("setdtr {}", on(up)));
    }

    fn set_rts(&mut self, up: bool) {
        self.note(format!("setrts {}", on(up)));
    }

    fn set_baud(&mut self, baud: u32) {
        self.note(format!("setbaud {baud}"));
    }

    fn set_flow_control(&mut self, flow: FlowControl) {
        self.note(format!("setflowctrl {flow:?}"));
    }

    fn modem_lines(&mut self) -> Option<ModemLines> {
        self.note("getmodemstatus");
        Some(ModemLines {
            cts: true,
            dsr: true,
            ring: false,
            carrier: true,
        })
    }

    // ---- file transfer ----

    fn transfer(&mut self, req: &Xfer<'_>) -> Result<bool, TtlError> {
        self.note(show_xfer(req));
        Ok(true)
    }

    // ---- the dialogs ----

    fn message_box(&mut self, text: &[u8], title: &[u8]) -> Result<DialogEnd, TtlError> {
        self.note(format!("messagebox {} {}", esc(text), esc(title)));
        Ok(DialogEnd::Ok(()))
    }

    fn yes_no_box(&mut self, text: &[u8], title: &[u8]) -> Result<DialogEnd, TtlError> {
        let yes = self.user.yes_no.pop_front().unwrap_or(true);
        let answer = if yes { "yes" } else { "no" };
        self.note(format!("yesnobox {} {} -> {answer}", esc(text), esc(title)));
        Ok(if yes {
            DialogEnd::Ok(())
        } else {
            DialogEnd::Cancel
        })
    }

    fn status_box(&mut self, text: &[u8], title: &[u8]) -> Result<(), TtlError> {
        self.note(format!("statusbox {} {}", esc(text), esc(title)));
        Ok(())
    }

    fn close_status_box(&mut self) -> Result<(), TtlError> {
        self.note("closesbox");
        Ok(())
    }

    fn bringup_status_box(&mut self) -> Result<(), TtlError> {
        self.note("bringupbox");
        Ok(())
    }

    fn list_box(
        &mut self,
        text: &[u8],
        title: &[u8],
        items: &[&[u8]],
        selected: usize,
        opts: &ListBoxOpts,
    ) -> Result<DialogEnd<usize>, TtlError> {
        let shown: Vec<String> = items.iter().map(|i| esc(i)).collect();
        // With no scripted answer: OK on whatever was preselected, which is
        // what a user gives by pressing Return.
        let picked = self.user.list.pop_front().unwrap_or(Some(selected));
        self.note(format!(
            "listbox {} {} [{}] sel={selected} {opts:?} -> {}",
            esc(text),
            esc(title),
            shown.join(" "),
            match picked {
                Some(i) => i.to_string(),
                None => "cancel".to_string(),
            }
        ));
        Ok(match picked {
            Some(i) => DialogEnd::Ok(i),
            None => DialogEnd::Cancel,
        })
    }

    fn input_box(
        &mut self,
        text: &[u8],
        title: &[u8],
        default: &[u8],
        password: bool,
    ) -> Result<DialogEnd<Vec<u8>>, TtlError> {
        let (which, queue, fallback): (_, &mut VecDeque<Vec<u8>>, &[u8]) = if password {
            ("passwordbox", &mut self.user.password, b"hunter2")
        } else {
            ("inputbox", &mut self.user.input, b"typed")
        };
        let typed = queue.pop_front().unwrap_or_else(|| fallback.to_vec());
        self.note(format!(
            "{which} {} {} default={} -> {}",
            esc(text),
            esc(title),
            esc(default),
            esc(&typed)
        ));
        Ok(DialogEnd::Ok(typed))
    }

    fn filename_box(
        &mut self,
        title: &[u8],
        save: bool,
        init_dir: &[u8],
    ) -> Result<Option<Vec<u8>>, TtlError> {
        let chosen = self.dir.join("chosen.txt");
        let chosen = tt_ttl::files::path_to_bytes(&chosen);
        self.note(format!(
            "filenamebox {} save={} dir={} -> {}",
            esc(title),
            on(save),
            esc(init_dir),
            esc(&chosen)
        ));
        Ok(Some(chosen))
    }

    fn dirname_box(&mut self, title: &[u8], init_dir: &[u8]) -> Result<Option<Vec<u8>>, TtlError> {
        let chosen = tt_ttl::files::path_to_bytes(&self.dir);
        self.note(format!(
            "dirnamebox {} dir={} -> {}",
            esc(title),
            esc(init_dir),
            esc(&chosen)
        ));
        Ok(Some(chosen))
    }

    fn set_dialog_pos(&mut self, pos: Option<DialogPos>) {
        match pos {
            Some(p) => self.note(format!("setdlgpos {p:?}")),
            None => self.note("setdlgpos default"),
        }
    }

    // ---- session logging ----

    fn log_open(&mut self, req: &LogOpen<'_>) -> Result<bool, TtlError> {
        self.note(format!(
            "logopen {} {} screen={} clock={:?}",
            esc(req.path),
            show_log_flags(
                req.binary,
                req.append,
                req.plain_text,
                req.timestamp,
                req.hide_dialog
            ),
            on(req.include_screen),
            req.timestamp_type,
        ));
        self.logging = Some(LogInfo {
            path: req.path.to_vec(),
            binary: req.binary,
            append: req.append,
            plain_text: req.plain_text,
            timestamp: req.timestamp,
            hide_dialog: req.hide_dialog,
        });
        Ok(true)
    }

    fn log_close(&mut self) -> Result<(), TtlError> {
        self.note("logclose");
        self.logging = None;
        Ok(())
    }

    fn log_pause(&mut self, paused: bool) -> Result<(), TtlError> {
        self.note(format!("logpause {}", on(paused)));
        Ok(())
    }

    fn log_write(&mut self, s: &[u8]) -> Result<(), TtlError> {
        self.note(format!("logwrite {}", esc(s)));
        Ok(())
    }

    fn log_info(&mut self) -> Result<Option<LogInfo>, TtlError> {
        let info = self.logging.clone();
        self.note(match &info {
            Some(i) => format!(
                "loginfo -> {} {}",
                esc(&i.path),
                show_log_flags(i.binary, i.append, i.plain_text, i.timestamp, i.hide_dialog)
            ),
            None => "loginfo -> none".to_string(),
        });
        Ok(info)
    }

    fn log_rotate(&mut self, how: LogRotate) -> Result<(), TtlError> {
        self.note(format!("logrotate {how:?}"));
        Ok(())
    }

    fn log_auto_close(&mut self, close: bool) -> Result<(), TtlError> {
        self.note(format!("logautoclosemode {}", on(close)));
        Ok(())
    }

    // ---- the terminal's odds and ends ----

    fn beep(&mut self, sound: BeepSound) -> Result<(), TtlError> {
        self.note(format!("beep {sound:?}"));
        Ok(())
    }

    fn call_menu(&mut self, id: i32) -> Result<(), TtlError> {
        self.note(format!("callmenu {id}"));
        Ok(())
    }

    fn set_transfer_dir(&mut self, path: &[u8]) -> Result<(), TtlError> {
        self.note(format!("changedir {}", esc(path)));
        Ok(())
    }

    fn clear_screen(&mut self, what: ClearScreen) -> Result<(), TtlError> {
        self.note(format!("clearscreen {what:?}"));
        Ok(())
    }

    fn enable_keyboard(&mut self, enable: bool) -> Result<(), TtlError> {
        self.note(format!("enablekeyb {}", on(enable)));
        Ok(())
    }

    fn load_key_map(&mut self, path: &[u8]) -> Result<(), TtlError> {
        self.note(format!("loadkeymap {}", esc(path)));
        Ok(())
    }

    fn restore_setup(&mut self, path: &[u8]) -> Result<(), TtlError> {
        self.note(format!("restoresetup {}", esc(path)));
        Ok(())
    }

    fn set_debug_mode(&mut self, mode: DebugMode) -> Result<(), TtlError> {
        self.note(format!("setdebug {mode:?}"));
        Ok(())
    }

    fn set_local_echo(&mut self, echo: bool) -> Result<(), TtlError> {
        self.note(format!("setecho {}", on(echo)));
        Ok(())
    }

    fn set_title(&mut self, title: &[u8]) -> Result<(), TtlError> {
        self.note(format!("settitle {}", esc(title)));
        Ok(())
    }

    fn title(&mut self) -> Result<Vec<u8>, TtlError> {
        self.note("gettitle");
        Ok(b"conformance".to_vec())
    }

    fn show_window(&mut self, which: ShowWindow) -> Result<(), TtlError> {
        self.note(format!("showtt {which:?}"));
        Ok(())
    }

    fn show_macro_window(&mut self, how: MacroWindow) -> Result<(), TtlError> {
        self.note(format!("show {how:?}"));
        Ok(())
    }

    fn terminal_geometry(&mut self) -> Result<Option<WindowGeometry>, TtlError> {
        self.note("getttpos");
        Ok(Some(WindowGeometry {
            state: WindowState::Normal,
            window: (10, 20, 810, 620),
            client: (14, 44, 806, 616),
        }))
    }

    fn set_serial_delay(&mut self, per_line: bool, ms: i32) -> Result<bool, TtlError> {
        let which = if per_line { "line" } else { "char" };
        self.note(format!("setserialdelay{which} {ms}"));
        Ok(true)
    }

    // ---- what the macro process cannot reach on its own ----

    fn hostname(&mut self) -> Result<Vec<u8>, TtlError> {
        self.note("gethostname");
        Ok(b"host.example".to_vec())
    }

    fn clipboard_text(&mut self) -> Option<Vec<u8>> {
        self.note("clipb2var");
        Some(self.clipboard.clone())
    }

    fn set_clipboard_text(&mut self, text: &[u8]) -> bool {
        self.note(format!("var2clipb {}", esc(text)));
        self.clipboard = text.to_vec();
        true
    }

    fn local_ip_addresses(&mut self, v6: bool) -> Option<Vec<Vec<u8>>> {
        self.note(if v6 { "getipv6addr" } else { "getipv4addr" });
        Some(if v6 {
            vec![b"2001:db8::1".to_vec()]
        } else {
            vec![b"192.0.2.1".to_vec()]
        })
    }
}

// ---------------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------------

/// Where upstream's scripts are. `../teraterm` is the sibling read-only
/// checkout every other harness here reads; `TERATERM_TESTS` overrides it.
fn upstream_dir() -> Option<PathBuf> {
    let dir = match std::env::var_os("TERATERM_TESTS") {
        Some(p) => PathBuf::from(p),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../teraterm/tests"),
    };
    dir.is_dir().then_some(dir)
}

/// The environment a run believes in, so that `getspecialfolder` and
/// `expandenv` answer the same thing on every machine.
///
/// Set once, before the single test function runs anything. `set_var` is only
/// sound because this is an integration test with one `#[test]` in it, so there
/// is no second thread to be reading the environment while this writes it.
fn fix_environment(home: &Path) {
    std::env::set_var("HOME", home);
    std::env::set_var("XDG_DATA_HOME", home.join(".local/share"));
    std::env::set_var("XDG_CONFIG_HOME", home.join(".config"));
    for (var, dir) in [
        ("XDG_DESKTOP_DIR", "Desktop"),
        ("XDG_DOCUMENTS_DIR", "Documents"),
        ("XDG_TEMPLATES_DIR", "Templates"),
    ] {
        std::env::set_var(var, home.join(dir));
    }
    // `expandenv` is asked for both of these by `expandenv.ttl`, and neither
    // exists on Linux. Removing them makes "it does not exist" the answer on a
    // machine that happens to have set one.
    std::env::remove_var("WINDIR");
    std::env::remove_var("OS");
}

/// Everything in upstream's `tests/` that is not a macro, so that the two
/// scripts reading a data file find it next to themselves.
fn seed(from: &Path, to: &Path) {
    let Ok(entries) = std::fs::read_dir(from) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.is_file() && path.extension().is_none_or(|x| x != "ttl") {
            if let Some(name) = path.file_name() {
                let _ = std::fs::copy(&path, to.join(name));
            }
        }
    }
}

/// Every file and directory under `dir`, by path relative to it. `None` is a
/// directory.
///
/// Taken before and after a run so that the transcript can say what the macro
/// left behind. Without it a third of these scripts record **nothing**:
/// `#32621.ttl` reads a file, rewrites every line and writes another one, and
/// asks the host for not one thing while doing it — so its transcript was the
/// exit code alone, and any of the file commands could have stopped working
/// without this suite noticing.
fn snapshot(dir: &Path) -> std::collections::BTreeMap<String, Option<Vec<u8>>> {
    let mut out = std::collections::BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            let Ok(rel) = path.strip_prefix(dir) else {
                continue;
            };
            let name = rel.to_string_lossy().into_owned();
            if path.is_dir() {
                out.insert(name, None);
                stack.push(path);
            } else {
                out.insert(name, Some(std::fs::read(&path).unwrap_or_default()));
            }
        }
    }
    out
}

/// What changed between two snapshots, as transcript lines.
///
/// `paths` is the same substitution [`portable`] does, applied to a file's
/// *bytes* rather than to the finished transcript — because the preview below
/// truncates, and a path cut in half is a path the later substitution can no
/// longer find. `getspecialfolder.ttl` writes sixteen absolute paths into a
/// file and is exactly that case.
fn changes(
    before: &std::collections::BTreeMap<String, Option<Vec<u8>>>,
    after: &std::collections::BTreeMap<String, Option<Vec<u8>>>,
    paths: &[(Vec<u8>, &str)],
) -> Vec<String> {
    /// Enough of a file to see it is the right one. Only files the *macro*
    /// wrote can reach here — the seeded ones are in both snapshots — so the
    /// limit is about a `filetruncate` of 10 KB of NULs rather than about
    /// copying upstream's test data. `getspecialfolder.ttl` writes 427 bytes
    /// of answers and all of them matter, so the cap is above that.
    fn preview(body: &[u8], paths: &[(Vec<u8>, &str)]) -> String {
        let mut body = body.to_vec();
        for (from, to) in paths {
            body = replace_bytes(&body, from, to.as_bytes());
        }
        let head = &body[..body.len().min(512)];
        let tail = if head.len() < body.len() { "..." } else { "" };
        format!("{} bytes {}{tail}", body.len(), esc(head))
    }

    let mut out = Vec::new();
    for (name, what) in after {
        match (before.get(name), what) {
            (Some(old), _) if old == what => {}
            (Some(_), Some(body)) => out.push(format!("~ file {name} {}", preview(body, paths))),
            (None, Some(body)) => out.push(format!("+ file {name} {}", preview(body, paths))),
            (None, None) => out.push(format!("+ dir {name}")),
            // A file that has become a directory or the other way round.
            (Some(_), None) => out.push(format!("~ dir {name}")),
        }
    }
    for name in before.keys() {
        if !after.contains_key(name) {
            out.push(format!("- {name}"));
        }
    }
    out.sort();
    out
}

/// `str::replace`, for bytes.
fn replace_bytes(body: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    if from.is_empty() {
        return body.to_vec();
    }
    let mut out = Vec::with_capacity(body.len());
    let mut i = 0;
    while i < body.len() {
        if body[i..].starts_with(from) {
            out.extend_from_slice(to);
            i += from.len();
        } else {
            out.push(body[i]);
            i += 1;
        }
    }
    out
}

/// Remove the one external side effect from upstream's otherwise headless
/// suite.
///
/// `#35797.ttl` says `exec 'notepad' 'show' 1`. It failed to spawn on Linux,
/// which happened to make the old transcript deterministic, but a Windows run
/// correctly opens Notepad and waits forever for a person. Replacing only the
/// executable name makes both targets exercise the same `CreateProcess`
/// failure path. The exact-one assertion keeps an upstream edit from silently
/// weakening the gate.
fn isolated_source(name: &str, raw: Vec<u8>) -> Vec<u8> {
    if name != "#35797.ttl" {
        return raw;
    }
    const FROM: &[u8] = b"exec 'notepad' 'show' 1";
    const TO: &[u8] = b"exec 'sterna-test-no-such-program' 'show' 1";
    assert_eq!(
        raw.windows(FROM.len()).filter(|w| *w == FROM).count(),
        1,
        "upstream #35797.ttl changed its exec line"
    );
    replace_bytes(&raw, FROM, TO)
}

/// Make the interpreter transcript independent of the Windows machine's ACP.
///
/// A user file with no BOM really does use that code page; [`tt_ttl::source`]
/// tests it. These 53 fixtures contain both CP932 and UTF-8 without BOM, so no
/// single system ACP can preserve the reviewed byte transcripts for both.
/// Marking the harness copy as UTF-8 selects `LoadFileU8C`'s pass-through BOM
/// branch and leaves the read-only upstream file untouched.
#[cfg(windows)]
fn stable_source(mut raw: Vec<u8>) -> Vec<u8> {
    if raw.starts_with(b"\xEF\xBB\xBF")
        || raw.starts_with(b"\xFF\xFE")
        || raw.starts_with(b"\xFE\xFF")
    {
        return raw;
    }
    raw.splice(..0, b"\xEF\xBB\xBF".iter().copied());
    raw
}

#[cfg(not(windows))]
fn stable_source(raw: Vec<u8>) -> Vec<u8> {
    raw
}

/// Paths that are this machine's, replaced by names that are not.
fn portable(text: String, dir: &Path, home: &Path, exe_dir: &Path) -> String {
    let mut out = text;
    // Longest first: the script's directory is under the scratch root, which is
    // not under the home, but the exe directory can be anywhere.
    for (path, name) in [(dir, "<dir>"), (home, "<home>"), (exe_dir, "<exedir>")] {
        let raw = path.to_string_lossy().into_owned();
        // Most host calls pass paths through `esc` before they reach the tape.
        // A Unix path contains no escapable byte, so replacing only `raw`
        // worked there by accident; every Windows separator is doubled.
        let escaped = esc(raw.as_bytes());
        out = out.replace(&escaped[1..escaped.len() - 1], name);
        out = out.replace(&raw, name);

        // Once the machine-shaped prefix is gone, make the one separator that
        // follows it portable as well. There are two spellings: raw command
        // lines carry one backslash and `esc` output carries two.
        out = out.replace(&format!(r"{name}\\"), &format!("{name}/"));
        out = out.replace(&format!(r"{name}\"), &format!("{name}/"));
    }
    out
}

#[test]
fn portable_recognises_raw_and_escaped_windows_paths() {
    let text = r#"message "C:\\work\\run\\chosen.txt" raw C:\work\run\macro.ttl"#.to_string();
    assert_eq!(
        portable(
            text,
            Path::new(r"C:\work\run"),
            Path::new(r"C:\users\tester"),
            Path::new(r"C:\bin"),
        ),
        r#"message "<dir>/chosen.txt" raw <dir>/macro.ttl"#
    );
}

/// How a script is launched, as the command line `ttpmacro.exe` would have
/// been given, with `{}` where the macro's own quoted path goes.
///
/// Fifty-one of the 53 are started with the macro and nothing else. The other
/// two are not about the language at all — they are about
/// [`tt_ttl::CmdLine`], and the `.bat` files sitting next to them in
/// upstream's `tests/` are the specification. This is a transcription of those
/// two files: four launches for `macroparam.bat` and one for
/// `params_array.bat`, each of which drives the script down a different arm and
/// asserts a different `paramcnt`. Their point is that `/V` before the macro is
/// a switch and `/V` after it is a parameter, so the four lines have to be run
/// as four runs — a script cannot be launched two ways at once.
fn launches(script: &str) -> &'static [&'static str] {
    match script {
        "macroparam.ttl" => &[
            "{} /vxx /ixx /V /i test1",
            "/V /i {} /v /I test2",
            "/I {} test3 /Vxx /ixx /V /i",
            "/i {} test4 /V /Vxx /ixx",
        ],
        "params_array.ttl" => &[r#"{} /vxx /ixx /V /i test1 "param 7" "" param9 10 eleven"#],
        _ => &["{}"],
    }
}

fn run_one(script: &Path, root: &Path, upstream: &Path, home: &Path) -> String {
    let name = script.file_name().unwrap().to_string_lossy().into_owned();
    let lines = launches(&name);
    // A launch line is only worth writing down when there is something to say:
    // the default one is the same for 51 scripts and adds nothing to a golden.
    let show = lines != ["{}"];
    let mut out = String::new();
    for line in lines {
        out.push_str(&run_once(script, &name, line, show, root, upstream, home));
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn run_once(
    script: &Path,
    name: &str,
    launch: &str,
    show: bool,
    root: &Path,
    upstream: &Path,
    home: &Path,
) -> String {
    let dir = root.join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    seed(upstream, &dir);

    let raw = stable_source(isolated_source(name, std::fs::read(script).unwrap()));
    let path = dir.join(name);
    std::fs::write(&path, &raw).unwrap();

    // `%TTMACRO%` and `%MACROFILE%` are both quoted in upstream's `.bat` files,
    // and the quoting is part of what `ParseParam` is being asked about.
    let quoted = format!("\"{}\"", path.to_string_lossy());
    let cmd =
        CmdLine::parse(format!("\"ttpmacro.exe\" {}", launch.replace("{}", &quoted)).as_bytes());

    let before = snapshot(&dir);
    let mut tape = Tape::new(dir.clone(), user(name));
    if show {
        tape.note(format!("-- launch {}", String::from_utf8_lossy(&cmd.raw)));
    }
    let mut it = Interp::with_cmdline(&cmd, raw, &mut tape);
    let mut lines = 0;
    while it.step(&mut tape) {
        lines += 1;
        if lines >= MAX_LINES {
            tape.note(format!("-- stopped after {MAX_LINES} lines"));
            break;
        }
    }
    tape.note(format!("-- exit {}", tape.exit_code));

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_default();
    let paths: Vec<(Vec<u8>, &str)> = [
        (dir.as_path(), "<dir>"),
        (home, "<home>"),
        (exe_dir.as_path(), "<exedir>"),
    ]
    .into_iter()
    .map(|(p, n)| (p.to_string_lossy().into_owned().into_bytes(), n))
    .collect();

    for line in changes(&before, &snapshot(&dir), &paths) {
        tape.note(line);
    }
    // The home is a directory the run may also have written into —
    // `getspecialfolder`'s answers live there — and it is shared, so it is
    // reported per script and cleaned between them.
    for line in changes(&Default::default(), &snapshot(home), &paths) {
        tape.note(format!("home {line}"));
    }
    let _ = std::fs::remove_dir_all(home);
    std::fs::create_dir_all(home).unwrap();

    let body = tape.log.join("\n") + "\n";
    portable(body, &dir, home, &exe_dir)
}

/// The first place two transcripts differ, with a little either side.
fn diff(want: &str, got: &str) -> String {
    let (want, got): (Vec<&str>, Vec<&str>) = (want.lines().collect(), got.lines().collect());
    let at = (0..want.len().max(got.len()))
        .find(|&i| want.get(i) != got.get(i))
        .unwrap_or(0);
    let from = at.saturating_sub(3);
    let mut out = String::new();
    for i in from..(at + 4).min(want.len().max(got.len())) {
        let mark = if i == at { ">>" } else { "  " };
        out.push_str(&format!(
            "{mark} {:>4} - {}\n{mark}      + {}\n",
            i + 1,
            want.get(i).unwrap_or(&"<end>"),
            got.get(i).unwrap_or(&"<end>")
        ));
    }
    out
}

#[test]
fn the_upstream_macros_run() {
    let Some(upstream) = upstream_dir() else {
        eprintln!(
            "skipped: no ../teraterm/tests — this needs the read-only reference \
             checkout, or TERATERM_TESTS pointing at one"
        );
        return;
    };

    let root = std::env::temp_dir().join("tt-ttl-scripts");
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("home");
    std::fs::create_dir_all(&home).unwrap();
    fix_environment(&home);

    let goldens = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scripts");
    let bless = std::env::var_os("TTL_BLESS").is_some();
    #[cfg(windows)]
    assert!(
        !bless,
        "TTL_BLESS is Linux-only: Windows has five reviewed platform divergences"
    );
    if bless {
        std::fs::create_dir_all(&goldens).unwrap();
    }

    let mut scripts: Vec<PathBuf> = std::fs::read_dir(&upstream)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "ttl"))
        .collect();
    scripts.sort();
    assert!(!scripts.is_empty(), "no .ttl scripts in {upstream:?}");

    let mut failed: Vec<String> = Vec::new();
    let mut failed_names: Vec<String> = Vec::new();
    let mut blessed = 0;
    for script in &scripts {
        let name = script.file_stem().unwrap().to_string_lossy().into_owned();
        let got = run_one(script, &root, &upstream, &home);
        let golden = goldens.join(format!("{name}.txt"));

        if bless {
            let old = std::fs::read_to_string(&golden).unwrap_or_default();
            if old != got {
                std::fs::write(&golden, &got).unwrap();
                blessed += 1;
            }
            continue;
        }
        match std::fs::read_to_string(&golden) {
            Ok(want) if want == got => {}
            Ok(want) => {
                failed_names.push(name.clone());
                failed.push(format!("{name}: differs\n{}", diff(&want, &got)));
            }
            Err(_) => {
                failed_names.push(name.clone());
                failed.push(format!("{name}: no golden at {}\n{got}", golden.display()));
            }
        }
    }

    if bless {
        // Deliberately a failure. Blessing is not a way to make the suite
        // green — the goldens have to be read, and a run that rewrote some
        // should not also report success.
        assert_eq!(blessed, 0, "{blessed} golden(s) rewritten — now read them");
        return;
    }

    #[cfg(windows)]
    {
        assert_eq!(
            failed_names.iter().map(String::as_str).collect::<Vec<_>>(),
            WINDOWS_PLATFORM_DIFFS,
            "Windows script divergences changed:\n\n{}",
            failed.join("\n")
        );
        eprintln!(
            "{} common scripts match; {} platform-shaped scripts remain quarantined",
            scripts.len() - failed.len(),
            failed.len()
        );
    }

    #[cfg(not(windows))]
    assert!(
        failed.is_empty(),
        "{} of {} scripts differ:\n\n{}",
        failed.len(),
        scripts.len(),
        failed.join("\n")
    );
}

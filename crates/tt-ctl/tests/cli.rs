//! The two binaries, against a real socket and a real window.
//!
//! `cargo` builds them for this test and hands over their paths, so what runs
//! here is argv to JSON to a job on the frontend's thread and back — the whole
//! path a shell script takes, with nothing stubbed but the window itself.
//!
//! The window is a thread holding a [`Session`] and a [`Server`], which is
//! exactly the arrangement `shell/src/MainWindow.cpp` has minus Qt. It stops
//! when the test drops its handle.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tt_ctl::host::{CtlHost, MacroStatus, RunError};
use tt_ctl::server::Listener;
use tt_session::Session;
use tt_vt::Config;

/// What the fake window did, so a test can assert on it after the client has
/// gone.
#[derive(Default)]
struct Record {
    macro_path: Option<PathBuf>,
    macro_params: Vec<String>,
    connected: Option<Vec<u8>>,
    closed: bool,
}

/// A window that answers everything and remembers what it was asked.
struct Host {
    record: Arc<Mutex<Record>>,
    /// How many more `macro_status` calls report a macro still running.
    left: u32,
    exit: i32,
}

impl CtlHost for Host {
    fn run_macro(&mut self, path: &Path, params: &[String]) -> Result<(), RunError> {
        let mut r = self.record.lock().unwrap();
        r.macro_path = Some(path.to_path_buf());
        r.macro_params = params.to_vec();
        self.left = 1;
        Ok(())
    }

    fn macro_status(&mut self) -> MacroStatus {
        let running = self.left > 0;
        self.left = self.left.saturating_sub(1);
        MacroStatus {
            running,
            exit: self.exit,
        }
    }

    fn connect(&mut self, line: &[u8]) -> Result<(), String> {
        self.record.lock().unwrap().connected = Some(line.to_vec());
        Ok(())
    }

    fn close_window(&mut self) -> bool {
        self.record.lock().unwrap().closed = true;
        true
    }
}

/// A running window: a socket, a session and a thread servicing both.
struct Window {
    dir: tempfile::TempDir,
    name: String,
    path: PathBuf,
    record: Arc<Mutex<Record>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Window {
    fn new(exit: i32) -> Window {
        let dir = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        let name = String::from("win");
        #[cfg(windows)]
        let name = {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            format!(
                "c{:x}{:x}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )
        };
        // Where `addr::dir()` looks, so that a client given no `--to` finds
        // this window by the same directory scan it would use for a real one.
        #[cfg(unix)]
        std::fs::create_dir_all(dir.path().join("sterna")).unwrap();
        #[cfg(unix)]
        let path = dir.path().join("sterna").join("win.sock");
        #[cfg(windows)]
        let path = tt_ctl::addr::path_of(&name).unwrap();
        let record = Arc::new(Mutex::new(Record::default()));
        let stop = Arc::new(AtomicBool::new(false));

        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let thread = {
            let path = path.clone();
            let record = record.clone();
            let stop = stop.clone();
            std::thread::spawn(move || {
                let server = Listener::bind_path(&path).unwrap().start().unwrap();
                let mut session = Session::new(Config::default());
                session.feed(b"\x1b]0;a window\x07hello\r\nworld");
                let mut host = Host {
                    record,
                    left: 0,
                    exit,
                };
                ready_tx.send(()).unwrap();
                // The frontend's loop, minus the toolkit: this is what a
                // `QSocketNotifier` on `Server::poll_fd` would drive.
                while !stop.load(Ordering::Relaxed) {
                    server.service(&mut session, &mut host);
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            })
        };
        ready_rx.recv().unwrap();
        Window {
            dir,
            name,
            path,
            record,
            stop,
            thread: Some(thread),
        }
    }

    /// Run a binary against this window, with the directory it should look in.
    fn run(&self, exe: &str, args: &[&str]) -> Output {
        let mut command = Command::new(exe);
        command
            .args(args)
            // The endpoint inherited by a child is the deterministic answer
            // on both platforms and isolates parallel Windows tests, whose
            // named pipes share one session namespace.
            .env("STERNA_CTL", &self.path);
        #[cfg(unix)]
        command
            // So that a client with no `--to` finds this window and not the
            // developer's own.
            .env("XDG_RUNTIME_DIR", self.dir.path());
        command.output().expect("the binary runs")
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

const TTCTL: &str = env!("CARGO_BIN_EXE_ttctl");
const TTPMACRO: &str = env!("CARGO_BIN_EXE_ttpmacro");

#[test]
fn ttctl_reads_a_window_it_was_pointed_at() {
    let w = Window::new(0);
    let out = w.run(TTCTL, &["--to", w.path.to_str().unwrap(), "status"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(v["title"], serde_json::json!("a window"));
    assert_eq!(v["connected"], serde_json::json!(false));
}

/// A child inherits the endpoint of the window that launched it.
#[test]
fn ttctl_uses_the_inherited_window_by_itself() {
    let w = Window::new(0);
    let out = w.run(TTCTL, &["ping"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout(&out).contains("\"pid\""));
}

#[test]
fn ttctl_screen_prints_the_terminal_as_text() {
    let w = Window::new(0);
    let out = w.run(TTCTL, &["screen"]);
    let text = stdout(&out);
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("hello"));
    assert_eq!(lines.next(), Some("world"));
}

#[test]
fn ttctl_ls_names_the_window() {
    let w = Window::new(0);
    let out = w.run(TTCTL, &["ls"]);
    assert!(stdout(&out).contains(&w.name), "{}", stdout(&out));
}

#[test]
fn ttctl_connect_passes_the_command_line_through() {
    let w = Window::new(0);
    let out = w.run(TTCTL, &["connect", "myhost /ssh /auth=publickey"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        w.record.lock().unwrap().connected.as_deref(),
        Some(&b"myhost /ssh /auth=publickey"[..])
    );
}

#[test]
fn ttctl_close_reaches_the_window() {
    let w = Window::new(0);
    let out = w.run(TTCTL, &["close"]);
    assert!(out.status.success());
    assert!(w.record.lock().unwrap().closed);
}

/// An error is one line on stderr and a non-zero status, which is what a shell
/// script tests.
#[test]
fn an_unknown_method_is_reported_and_fails() {
    let w = Window::new(0);
    let out = w.run(TTCTL, &["call", "no.such.method"]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("no.such.method"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
#[cfg(unix)]
fn ttctl_says_so_when_no_window_is_listening() {
    let dir = tempfile::tempdir().unwrap();
    let out = Command::new(TTCTL)
        .args(["status"])
        .env("XDG_RUNTIME_DIR", dir.path())
        .env_remove("STERNA_CTL")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no Sterna window"));
}

#[test]
#[cfg(windows)]
fn ttctl_says_so_when_the_named_window_is_not_listening() {
    let name = format!("missing{:x}", std::process::id());
    let out = Command::new(TTCTL)
        .args(["--to", &name, "status"])
        .env_remove("STERNA_CTL")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(!out.stderr.is_empty());
}

/// The whole point of the compatibility binary: a `.bat` wrapper's command
/// line, a macro run in the window, and the exit status a script tests.
#[test]
fn ttpmacro_runs_a_macro_in_the_window_and_carries_its_exit_code() {
    let w = Window::new(3);
    let script = w.dir.path().join("login.ttl");
    std::fs::write(&script, b"; nothing; the window is a fake\n").unwrap();

    let out = w.run(
        TTPMACRO,
        &["/V", script.to_str().unwrap(), "first", "second"],
    );
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let r = w.record.lock().unwrap();
    // The same file, not the same string. The launcher resolves the path
    // before sending it, and on Windows resolving `%TEMP%` also expands the
    // 8.3 name the runner's environment holds — `RUNNER~1` becomes
    // `runneradmin` — so comparing spellings compares the environment.
    let got = r
        .macro_path
        .clone()
        .expect("a macro path reached the window");
    assert_eq!(
        tt_ctl::full_path(&got).unwrap(),
        tt_ctl::full_path(&script).unwrap()
    );
    // And it is a path a person would recognise: `canonicalize` alone would
    // have handed the window `\\?\C:\...`, which is what the macro then sees
    // as its own name.
    assert!(
        !got.to_string_lossy().starts_with(r"\\?\"),
        "the window was given a verbatim path: {}",
        got.display()
    );
    assert_eq!(r.macro_params, vec!["first".to_string(), "second".into()]);
}

/// `/V` before the name is a switch and `/V` after it is a parameter — the
/// distinction `macroparam.bat` exists to pin down, still true when the
/// launcher is a client rather than the interpreter.
#[test]
fn ttpmacro_keeps_the_switch_rule_its_command_line_has() {
    let w = Window::new(0);
    let script = w.dir.path().join("m.ttl");
    std::fs::write(&script, b"\n").unwrap();

    let out = w.run(TTPMACRO, &[script.to_str().unwrap(), "/V", "after"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        w.record.lock().unwrap().macro_params,
        vec!["/V".to_string(), "after".into()],
        "a switch after the file name is a parameter"
    );
}

/// `/D=` names the window, which is the job it does upstream through DDE.
#[test]
fn ttpmacro_takes_its_window_from_the_topic() {
    let w = Window::new(0);
    let script = w.dir.path().join("m.ttl");
    std::fs::write(&script, b"\n").unwrap();

    let topic = format!("/D={}", w.name);
    let out = w.run(TTPMACRO, &[&topic, script.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(w.record.lock().unwrap().macro_path.is_some());

    let out = w.run(TTPMACRO, &["/D=nosuch", script.to_str().unwrap()]);
    assert!(!out.status.success(), "a topic that names nothing fails");
}

#[test]
fn ttpmacro_with_no_file_says_what_it_wanted() {
    let w = Window::new(0);
    let out = w.run(TTPMACRO, &[]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no macro file"));
}

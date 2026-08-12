//! tt-lua — Lua, over the same host the macro language uses.
//!
//! `tt-ttl` is Tera Term's macro language, and it is a *port*: every quirk in
//! it is upstream's, because macros written against those quirks exist and are
//! most of the reason anybody wants this program. This is the other half of the
//! bargain — a language somebody would choose, for the scripts nobody has
//! written yet.
//!
//! **The two share a host and nothing else.** [`ScriptHost`] is the whole of
//! what a script can do to the world: send bytes, wait for them, open a
//! connection, move a file, put a dialog up, drive the log. A frontend
//! implements it once — `tt-macro`'s `SessionHost` is the one that is a real
//! terminal — and both languages run against it. That is what the trait being
//! wide and shallow buys, and it is why this crate is glue rather than a
//! second port.
//!
//! ```no_run
//! use tt_lua::Script;
//! use tt_ttl::RecordingHost;
//!
//! let mut host = RecordingHost::new();
//! host.linked = true;
//! Script::new("login.lua", b"tt.sendln('who')".to_vec()).run(&mut host)?;
//! # Ok::<(), mlua::Error>(())
//! ```
//!
//! # Decisions
//!
//! **Lua is not a second TTL.** There is no `result`, no `inputstr`, no
//! 1-based string indexing and no `goto`; a function returns its answer and a
//! failure raises, which `pcall` catches. Reproducing TTL's shape in Lua would
//! give a language with TTL's drawbacks and none of its compatibility, and
//! anyone who needs TTL's exact behaviour already has TTL. `PLAN.md` made the
//! same call in the other direction when it refused to transpile TTL *into*
//! Lua.
//!
//! **Only the terminal is exposed.** Roughly half of TTL's 231 reserved words
//! exist because the language had no standard library — `strlen`, `sprintf`,
//! `fileopen`, `getenv`, `int2str`. Lua has all of those, and shadowing them
//! with worse versions would be the wrong half of the trade. What this crate
//! adds is the half Lua cannot have on its own, which is [`ScriptHost`].
//!
//! **Strings are bytes.** Lua strings are 8-bit clean and [`ScriptHost`] takes
//! `&[u8]`, so nothing is decoded on the way through and a `tt.send` of
//! arbitrary bytes means those bytes. TTL's 511-byte string ceiling is
//! `ttmdde.c`'s buffer size rather than anything about terminals, so it is not
//! reproduced here — [`Recv`] is the same line buffer with the cap taken off.
//!
//! **`print` writes on the terminal**, through
//! [`disp_str`](ScriptHost::disp_str), with `\n` expanded to `CR LF` because
//! that is what a terminal needs to start a line. Lua's own `print` goes to
//! stdout, which for a window launched from a desktop menu is nowhere — the
//! same silent-diagnostic trap `AGENTS.md` records for `qWarning` under
//! journald. A host with no screen falls back to stderr rather than failing.
//!
//! **`os.exit` is removed.** The macro is a thread inside the terminal, so
//! `os.exit` would take the window with it. A script ends by returning, and
//! [`set_exit_code`](ScriptHost::set_exit_code) is `tt.setexitcode`.
//!
//! **A runaway script is still stoppable.** The interpreter checks
//! [`cancelled`](ScriptHost::cancelled) once per line; here a Lua debug hook
//! does it every few thousand instructions, so `while true do end` answers
//! the End button. See [`Cancelled`] for the part of that which is not
//! obvious.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use mlua::{HookTriggers, Lua, LuaOptions, Scope, StdLib, Table, Value, VmState};
use tt_ttl::{ScriptHost, TtlError};

mod conn;
mod dlg;
mod env;
mod log;
mod plugin;
mod serial;
mod term;
mod xfer;

pub use conn::Recv;
pub use plugin::{
    CallbackId, Hook, KeyBinding, MenuItem, Plugin, StreamDirection, StreamFilterResult,
    StreamFilters, StreamPlugin,
};

/// The host, shared between every callback in one run.
///
/// `mlua` hands each callback a `&Lua` and nothing else, so the host reaches
/// them by capture — and a `&mut` cannot be captured by more than one closure.
/// The [`RefCell`] is what splits it back up. Nothing here re-enters Lua while
/// holding the borrow, so the borrows never overlap; the one place that could
/// is the cancellation hook, which is why it is written the way it is.
pub(crate) type Host<'a> = RefCell<&'a mut dyn ScriptHost>;

/// Where the registry keeps the scoped function the debug hook calls.
const CANCEL_KEY: &str = "sterna.cancelled";

/// How many VM instructions between cancellation checks.
///
/// The interpreter checks once per *line*, which for TTL is also roughly once
/// per command. This is the same order of magnitude for a Lua loop body and
/// costs nothing measurable next to the host calls a real script makes.
const CANCEL_EVERY: u32 = 4096;

/// The run was stopped from outside — `ScriptHost::cancelled` answered true.
///
/// A stopped script is not a failed one, and a frontend that reports every
/// error in a dialog must not put one up because the user pressed End. It is
/// carried as an error because that is the only way out of a Lua chunk;
/// [`is_cancelled`] is the test, and it looks through the wrapping rather than
/// at the top-level variant, because a stop inside a `pcall`ed function arrives
/// wrapped in a `CallbackError` and a stop in the main chunk does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

/// Whether this error is the End button rather than the script's own fault.
pub fn is_cancelled(e: &mlua::Error) -> bool {
    e.chain().any(|c| c.is::<Cancelled>())
}

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("macro cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// A Lua script, and the arguments it was given.
///
/// It holds no Lua state, so it can be built on one thread and run on another
/// — which is what the frontend does, because a script blocks and a window
/// must not. The `Lua` itself is created inside [`run`](Script::run) and dies
/// with it.
pub struct Script {
    name: String,
    body: Vec<u8>,
    args: Vec<Vec<u8>>,
}

impl Script {
    pub fn new(name: impl Into<String>, body: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            body,
            args: Vec::new(),
        }
    }

    /// The parameters after the file name, which reach the script as `tt.args`.
    ///
    /// A 1-based array, which is Lua's convention and happens to be TTL's
    /// `param1`..`param9` renumbered by one: `tt.args[1]` is the first
    /// parameter. Unlike TTL there is no ninth-parameter ceiling and no
    /// `params[0]` holding the command line — see `PLAN.md` on why upstream's
    /// index 0 is the whole line.
    pub fn with_args(mut self, args: Vec<Vec<u8>>) -> Self {
        self.args = args;
        self
    }

    /// Run it to completion.
    ///
    /// Returns the script's error rather than reporting it through
    /// [`ScriptHost::error`]: that method carries a [`TtlError`], one of
    /// upstream's twenty-one numbered codes with upstream's sentence attached,
    /// and a Lua traceback is neither. The caller shows it.
    pub fn run(&self, host: &mut dyn ScriptHost) -> mlua::Result<()> {
        // `ALL_SAFE` is everything but `debug` and the `ffi`/`package` loader
        // for native modules. `io` and `os` stay: a macro language that cannot
        // open a file is not one, and TTL has `fileopen` and `exec` for the
        // same reason.
        let lua = Lua::new_with(StdLib::ALL_SAFE, LuaOptions::default())?;
        let cell: Host<'_> = RefCell::new(host);
        let recv = RefCell::new(Recv::default());

        lua.scope(|scope| {
            let tt = lua.create_table()?;
            conn::install(scope, &tt, &cell, &recv)?;
            serial::install(scope, &tt, &cell)?;
            xfer::install(scope, &tt, &cell)?;
            dlg::install(scope, &tt, &cell)?;
            log::install(scope, &tt, &cell)?;
            term::install(scope, &tt, &cell)?;
            env::install(scope, &tt, &cell)?;

            let args = lua.create_table()?;
            for (i, a) in self.args.iter().enumerate() {
                args.set(i + 1, lua.create_string(a)?)?;
            }
            tt.set("args", args)?;
            tt.set("name", lua.create_string(&self.name)?)?;

            seal(&lua, &tt)?;
            lua.globals().set("tt", &tt)?;
            install_print(&lua, scope, &cell)?;
            let os: Table = lua.globals().get("os")?;
            os.set("exit", Value::Nil)?;

            install_cancel_hook(&lua, scope, &cell)?;
            self.reach_neighbours(&lua)?;

            // `@` in front is Lua's own mark for "this chunk came from a
            // file", and it is what makes an error read `login.lua:12:` rather
            // than `[string "login.lua"]:12:`. The difference is not cosmetic:
            // the second form is what a chunk compiled from a *string* looks
            // like, so an editor jumping to the error would not find it, and
            // the reader is told the wrong thing about where the code is.
            let r = lua
                .load(&self.body[..])
                .set_name(format!("@{}", self.name))
                .exec();
            // The hook holds a reference to a scoped function; taking it down
            // before the scope closes keeps the two from racing at drop.
            lua.remove_hook();
            let _ = lua.unset_named_registry_value(CANCEL_KEY);
            // `pcall` catches errors raised from a hook as readily as any
            // other, so a script wrapping its own loop in one could otherwise
            // report a clean finish after being told to stop. Asking the host
            // again at the boundary is what makes the answer honest; it does
            // not make the script stop sooner, and nothing can — Lua has no
            // uncatchable error.
            match r {
                Ok(()) if cell.borrow_mut().cancelled() => Err(mlua::Error::external(Cancelled)),
                other => other,
            }
        })
    }
}

impl Script {
    /// `require 'helper'` should find the file next to the script.
    ///
    /// Lua's default `package.path` is the *process's* working directory and
    /// the system module directories, neither of which is where a script that
    /// somebody double-clicked lives. TTL's `include` resolves against the
    /// running macro's own directory, which is what a script author expects,
    /// so the directory goes on the front of the path. Nothing is removed —
    /// a script that installs a library in the usual place still finds it.
    fn reach_neighbours(&self, lua: &Lua) -> mlua::Result<()> {
        let Some(dir) = std::path::Path::new(&self.name).parent() else {
            return Ok(());
        };
        if dir.as_os_str().is_empty() {
            return Ok(());
        }
        let package: Table = lua.globals().get("package")?;
        let existing: String = package.get("path")?;
        let dir = dir.display();
        package.set("path", format!("{dir}/?.lua;{dir}/?/init.lua;{existing}"))
    }
}

/// A misspelled command should say so.
///
/// Without this `tt.sendlnn('x')` is "attempt to call a nil value", which names
/// nothing; TTL has the same problem one level down, where an unknown command
/// is read as a variable and reported as a bad assignment. Reading an unknown
/// key is an error rather than `nil`, so `if tt.foo then` fails loudly too —
/// deliberate, since every real key is installed before the script runs.
fn seal(lua: &Lua, tt: &Table) -> mlua::Result<()> {
    let meta = lua.create_table()?;
    meta.set(
        "__index",
        lua.create_function(|_, (_, key): (Table, String)| -> mlua::Result<Value> {
            Err(mlua::Error::runtime(format!("tt.{key} does not exist")))
        })?,
    )?;
    tt.set_metatable(Some(meta))?;
    Ok(())
}

/// `print`, rewritten to reach the screen. See the crate docs.
fn install_print<'s, 'e>(
    lua: &Lua,
    scope: &'s Scope<'s, 'e>,
    host: &'e Host<'e>,
) -> mlua::Result<()> {
    let f = scope.create_function(move |lua, args: mlua::Variadic<Value>| {
        let mut out = Vec::new();
        for (i, v) in args.iter().enumerate() {
            if i > 0 {
                out.push(b'\t');
            }
            // `tostring`, so a table with `__tostring` prints as itself —
            // which is what Lua's own `print` does.
            let s = lua.coerce_string(v.clone())?;
            match s {
                Some(s) => out.extend_from_slice(&s.as_bytes()),
                None => out.extend_from_slice(v.type_name().as_bytes()),
            }
        }
        out.push(b'\n');
        let crlf = expand_lf(&out);
        if host.borrow_mut().disp_str(&crlf).is_err() {
            use std::io::Write;
            let _ = std::io::stderr().write_all(&out);
        }
        Ok(())
    })?;
    lua.globals().set("print", f)
}

/// A bare LF moves down a terminal without returning, so `print` sends both.
fn expand_lf(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() + 8);
    let mut prev = 0u8;
    for &b in s {
        if b == b'\n' && prev != b'\r' {
            out.push(b'\r');
        }
        out.push(b);
        prev = b;
    }
    out
}

/// Stop a script that is not calling anything.
///
/// The hook has to be `'static` — `mlua` stores it in the `Lua` — so it cannot
/// capture the borrowed host the way every other callback here does. It calls
/// a *scoped* function out of the registry instead, which can. Lua clears its
/// own `allowhook` while a hook runs, so that call cannot re-enter this.
fn install_cancel_hook<'s, 'e>(
    lua: &Lua,
    scope: &'s Scope<'s, 'e>,
    host: &'e Host<'e>,
) -> mlua::Result<()> {
    let probe = scope.create_function(move |_, ()| Ok(host.borrow_mut().cancelled()))?;
    lua.set_named_registry_value(CANCEL_KEY, probe)?;
    lua.set_hook(
        HookTriggers::new().every_nth_instruction(CANCEL_EVERY),
        |lua, _| {
            let probe: mlua::Function = lua.named_registry_value(CANCEL_KEY)?;
            if probe.call::<bool>(())? {
                return Err(mlua::Error::external(Cancelled));
            }
            Ok(VmState::Continue)
        },
    )
}

/// A host refusal, as a Lua error.
///
/// `TtlError` keeps upstream's sentence — "Link macro first. Use 'connect'
/// macro." — which is odd wording to meet in a Lua traceback and is still the
/// right one: it is what the host meant, and a second vocabulary for the same
/// twenty-one conditions would be a second thing to keep in step.
pub(crate) fn lua_err(e: TtlError) -> mlua::Error {
    mlua::Error::external(e)
}

/// One of a named set, so a script says `'rts'` rather than `2`.
///
/// TTL numbers these because DDE carries a decimal string and because the
/// numbers are `ttdde.c`'s `switch` labels. Nothing here is a switch label,
/// and a wrong name is reported with the list — where a wrong *number* is
/// dropped in silence, which is upstream's answer to `setflowctrl 7`.
pub(crate) fn choice<T: Copy>(got: &str, what: &str, of: &[(&str, T)]) -> mlua::Result<T> {
    of.iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(got))
        .map(|(_, v)| *v)
        .ok_or_else(|| {
            let names: Vec<&str> = of.iter().map(|(n, _)| *n).collect();
            mlua::Error::runtime(format!(
                "{what} '{got}' is not one of: {}",
                names.join(", ")
            ))
        })
}

/// `tt.timeout`, in seconds, as a deadline. Zero and absent are both "for
/// ever", which is TTL's rule for `timeout` and `mtimeout` together.
pub(crate) fn deadline(tt: &Table) -> mlua::Result<Option<Instant>> {
    let secs: f64 = tt.raw_get::<Option<f64>>("timeout")?.unwrap_or(0.0);
    if secs > 0.0 {
        Ok(Some(Instant::now() + Duration::from_secs_f64(secs)))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tt_ttl::RecordingHost;

    /// Handy in tests: run a snippet and give back the host it ran against.
    pub(crate) fn run(src: &str) -> (RecordingHost, mlua::Result<()>) {
        let mut host = RecordingHost::new();
        host.linked = true;
        let r = Script::new("test.lua", src.as_bytes().to_vec()).run(&mut host);
        (host, r)
    }

    #[test]
    fn a_script_reaches_the_host() {
        let (host, r) = run("tt.sendln('who')");
        r.unwrap();
        assert_eq!(host.sent, b"who\r");
    }

    #[test]
    fn print_goes_to_the_screen_with_the_carriage_return_a_terminal_needs() {
        let (host, r) = run("print('hi', 42)");
        r.unwrap();
        assert_eq!(host.output, b"hi\t42\r\n");
    }

    #[test]
    fn a_misspelled_command_names_itself() {
        let (_, r) = run("tt.sendlnn('x')");
        let msg = r.unwrap_err().to_string();
        assert!(msg.contains("tt.sendlnn does not exist"), "{msg}");
    }

    #[test]
    fn os_exit_is_gone() {
        let (_, r) = run("return os.exit == nil");
        r.unwrap();
        let (_, r) = run("os.exit(1)");
        assert!(r.is_err());
    }

    #[test]
    fn arguments_arrive_one_based() {
        let mut host = RecordingHost::new();
        host.linked = true;
        Script::new("t.lua", b"tt.send(tt.args[1], tt.args[2])".to_vec())
            .with_args(vec![b"one".to_vec(), b"two".to_vec()])
            .run(&mut host)
            .unwrap();
        assert_eq!(host.sent, b"onetwo");
    }

    /// The whole point of the hook: a loop that calls nothing still stops.
    #[test]
    fn a_runaway_loop_answers_the_end_button() {
        struct Impatient(u32);
        impl ScriptHost for Impatient {
            fn cancelled(&mut self) -> bool {
                self.0 += 1;
                self.0 > 2
            }
        }
        let mut host = Impatient(0);
        let r = Script::new("spin.lua", b"while true do end".to_vec()).run(&mut host);
        assert!(is_cancelled(&r.unwrap_err()));
    }

    /// A script that catches the stop still reports having been stopped.
    #[test]
    fn a_pcall_cannot_hide_the_end_button() {
        struct Impatient(u32);
        impl ScriptHost for Impatient {
            fn cancelled(&mut self) -> bool {
                self.0 += 1;
                self.0 > 2
            }
        }
        let mut host = Impatient(0);
        let src = b"pcall(function() while true do end end)".to_vec();
        let r = Script::new("spin.lua", src).run(&mut host);
        assert!(is_cancelled(&r.unwrap_err()));
    }

    /// `require` should find the file next to the script, which is where
    /// `include` looks and where Lua's own `package.path` does not.
    #[test]
    fn a_script_can_require_its_neighbour() {
        let dir = std::env::temp_dir().join(format!("sterna-lua-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("greet.lua"),
            b"return function() tt.sendln('hi') end",
        )
        .unwrap();
        let script = dir.join("main.lua");

        let mut host = RecordingHost::new();
        host.linked = true;
        Script::new(
            script.display().to_string(),
            b"local g = require('greet'); g()".to_vec(),
        )
        .run(&mut host)
        .unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(host.sent, b"hi\r");
    }

    /// Bytes in, bytes out — no UTF-8 anywhere on the path.
    #[test]
    fn a_string_is_bytes() {
        let (host, r) = run(r#"tt.send('\xff\xfe\x00')"#);
        r.unwrap();
        assert_eq!(host.sent, b"\xff\xfe\x00");
    }
}

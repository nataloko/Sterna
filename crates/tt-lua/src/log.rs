//! The session log.
//!
//! The log is the *terminal's*, not the script's: `tt.logopen` and
//! File > Log are one log, and the second of them to run displaces the first
//! — which is why opening one while one is already open reports failure rather
//! than quietly opening a second.
//!
//! **`tt.logopen` reports `true` for success**, which is the opposite of TTL's
//! `result` and the same as everything else here. Upstream's inversion is
//! documented and is upstream's alone: `logopen` is the one command in the
//! language whose 0 means it worked.

use mlua::{BString, Scope, Table};
use tt_ttl::{LogClock, LogOpen, LogRotate};

use crate::{choice, lua_err, Host};

/// The flags after the path, out of an options table. Each defaults to off, so
/// a host sees the same request however few of them a script wrote.
fn open_req<'a>(path: &'a [u8], t: Option<&Table>) -> mlua::Result<LogOpen<'a>> {
    let mut r = LogOpen {
        path,
        binary: false,
        append: false,
        plain_text: false,
        timestamp: false,
        hide_dialog: false,
        include_screen: false,
        timestamp_type: LogClock::Local,
    };
    let Some(t) = t else { return Ok(r) };
    r.binary = t.get::<Option<bool>>("binary")?.unwrap_or(false);
    r.append = t.get::<Option<bool>>("append")?.unwrap_or(false);
    r.plain_text = t.get::<Option<bool>>("plaintext")?.unwrap_or(false);
    r.timestamp = t.get::<Option<bool>>("timestamp")?.unwrap_or(false);
    r.hide_dialog = t.get::<Option<bool>>("hidedialog")?.unwrap_or(false);
    r.include_screen = t.get::<Option<bool>>("includescreen")?.unwrap_or(false);
    // There is no "none" here — that is the `timestamp` flag — and the two
    // elapsed clocks measure from different events.
    if let Some(c) = t.get::<Option<String>>("clock")? {
        r.timestamp_type = choice(
            &c,
            "log clock",
            &[
                ("local", LogClock::Local),
                ("utc", LogClock::Utc),
                ("elapsedlog", LogClock::ElapsedLog),
                ("elapsedconnection", LogClock::ElapsedConnection),
            ],
        )?;
    }
    Ok(r)
}

pub(crate) fn install<'s, 'e>(
    scope: &'s Scope<'s, 'e>,
    tt: &Table,
    host: &'e Host<'e>,
) -> mlua::Result<()> {
    tt.set(
        "logopen",
        scope.create_function(move |_, (path, opts): (BString, Option<Table>)| {
            crate::conn::link(host)?;
            let req = open_req(&path, opts.as_ref())?;
            host.borrow_mut().log_open(&req).map_err(lua_err)
        })?,
    )?;
    tt.set(
        "logclose",
        scope.create_function(move |_, ()| {
            crate::conn::link(host)?;
            host.borrow_mut().log_close().map_err(lua_err)
        })?,
    )?;
    // What arrives while paused is **discarded**, not buffered, so this is not
    // a valve on a queue.
    tt.set(
        "logpause",
        scope.create_function(move |_, ()| {
            crate::conn::link(host)?;
            host.borrow_mut().log_pause(true).map_err(lua_err)
        })?,
    )?;
    tt.set(
        "logstart",
        scope.create_function(move |_, ()| {
            crate::conn::link(host)?;
            host.borrow_mut().log_pause(false).map_err(lua_err)
        })?,
    )?;
    // And this is the exception that makes the pause usable: a note explaining
    // the gap reaches the file even while the gap is open. `logwrite.html`
    // says so and `FLogWriteStr` cannot — the drain loop discards what it
    // pulls while paused — which is one of the three places this port follows
    // the manual instead of the code.
    tt.set(
        "logwrite",
        scope.create_function(move |_, s: BString| {
            crate::conn::link(host)?;
            host.borrow_mut().log_write(&s).map_err(lua_err)
        })?,
    )?;

    // A table, or `nil` when nothing is being logged.
    tt.set(
        "loginfo",
        scope.create_function(move |lua, ()| {
            crate::conn::link(host)?;
            let info = host.borrow_mut().log_info().map_err(lua_err)?;
            match info {
                None => Ok(None),
                Some(i) => {
                    let t = lua.create_table()?;
                    t.set("path", lua.create_string(&i.path)?)?;
                    t.set("binary", i.binary)?;
                    t.set("append", i.append)?;
                    t.set("plaintext", i.plain_text)?;
                    t.set("timestamp", i.timestamp)?;
                    t.set("hidedialog", i.hide_dialog)?;
                    Ok(Some(t))
                }
            }
        })?,
    )?;

    // It reconfigures rotation and does not rotate anything now; the
    // documentation says so twice.
    tt.set(
        "logrotate",
        scope.create_function(move |_, (how, n): (String, Option<i32>)| {
            let how = match how.to_ascii_lowercase().as_str() {
                "size" => LogRotate::Size(n.ok_or_else(|| {
                    mlua::Error::runtime("logrotate 'size' needs a size in bytes")
                })?),
                "keep" => LogRotate::Keep(n.ok_or_else(|| {
                    mlua::Error::runtime("logrotate 'keep' needs a number of files")
                })?),
                "halt" => LogRotate::Halt,
                other => {
                    return Err(mlua::Error::runtime(format!(
                        "logrotate '{other}' is not one of: size, keep, halt"
                    )))
                }
            };
            crate::conn::link(host)?;
            host.borrow_mut().log_rotate(how).map_err(lua_err)
        })?,
    )?;

    // Whether the log closes when the **script** ends — not the connection.
    // It lasts one run and no longer, which the documentation calls out
    // because it surprises people.
    tt.set(
        "logautoclose",
        scope.create_function(move |_, on: bool| {
            crate::conn::link(host)?;
            host.borrow_mut().log_auto_close(on).map_err(lua_err)
        })?,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::tests::run;
    use crate::Script;
    use tt_ttl::{LogInfo, RecordingHost};

    #[test]
    fn logopen_carries_its_flags_and_defaults_them_off() {
        let (host, r) = run("tt.logopen('/tmp/s.log', {append=true, clock='utc'})");
        r.unwrap();
        assert_eq!(
            host.logs,
            ["logopen \"/tmp/s.log\" binary=0 append=1 plain=0 ts=0 hide=0 screen=0 clock=Utc"]
        );
    }

    /// The divergence: TTL's `logopen` reports 0 for success.
    #[test]
    fn logopen_reports_success_as_true_like_everything_else() {
        let (host, r) = run("tt.dispstr(tostring(tt.logopen('/tmp/s.log')))");
        r.unwrap();
        assert_eq!(host.output, b"true");

        let mut host = RecordingHost::new();
        host.linked = true;
        host.log_open_fails = true;
        Script::new("t.lua", b"tt.dispstr(tostring(tt.logopen('x')))".to_vec())
            .run(&mut host)
            .unwrap();
        assert_eq!(host.output, b"false");
    }

    #[test]
    fn loginfo_is_a_table_or_nothing() {
        let (host, r) = run("tt.dispstr(tostring(tt.loginfo()))");
        r.unwrap();
        assert_eq!(host.output, b"nil");

        let mut host = RecordingHost::new();
        host.linked = true;
        host.log_info = Some(LogInfo {
            path: b"/tmp/s.log".to_vec(),
            binary: true,
            append: false,
            plain_text: false,
            timestamp: true,
            hide_dialog: false,
        });
        let src = "local i = tt.loginfo(); tt.dispstr(i.path, tostring(i.binary))";
        Script::new("t.lua", src.as_bytes().to_vec())
            .run(&mut host)
            .unwrap();
        assert_eq!(host.output, b"/tmp/s.logtrue");
    }

    #[test]
    fn the_pause_and_the_note_that_explains_it() {
        let (host, r) = run("tt.logpause(); tt.logwrite('gap: reconnecting'); tt.logstart()");
        r.unwrap();
        assert_eq!(
            host.logs,
            ["logpause", "logwrite \"gap: reconnecting\"", "logstart"]
        );
    }

    #[test]
    fn logrotate_names_its_three_forms() {
        let (host, r) = run("tt.logrotate('size', 1048576); tt.logrotate('halt')");
        r.unwrap();
        assert_eq!(host.logs, ["logrotate Size(1048576)", "logrotate Halt"]);

        let (_, r) = run("tt.logrotate('size')");
        assert!(r.unwrap_err().to_string().contains("needs a size"));
    }

    /// Upstream's `logopen` with no terminal is `ErrLinkFirst` even though its
    /// own body says nothing about a link, and so is every other one here.
    #[test]
    fn the_log_is_the_terminals_so_it_needs_one() {
        let mut host = RecordingHost::new();
        let r = Script::new("t.lua", b"tt.logclose()".to_vec()).run(&mut host);
        assert!(r.unwrap_err().to_string().contains("Link macro first"));
    }
}

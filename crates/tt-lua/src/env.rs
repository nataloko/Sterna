//! The environment a script cannot reach on its own, and the clock.
//!
//! This is the shortest of the surfaces on purpose. TTL has `getenv`,
//! `setenv`, `exec`, `dirnamebox` and a dozen string helpers here because the
//! language had no standard library; Lua has `os` and `io`, so what is left is
//! only what the *terminal* knows — what it is connected to, what is on the
//! clipboard, which machine this is — plus the two clock calls that exist so a
//! host can make a run repeatable.
//!
//! Two deliberate omissions. `random` is not here: Lua's `math.random` is a
//! better generator than the one upstream reaches for, and `math.randomseed`
//! makes a test repeatable without a host having to. And `include` is not
//! here: `require` is Lua's, and [`Script::run`](crate::Script::run) puts the
//! script's own directory on `package.path` so it finds its neighbours the way
//! `include` does.

use mlua::{BString, Scope, Table, Value};

use crate::conn::link;
use crate::{lua_err, Host};

pub(crate) fn install<'s, 'e>(
    scope: &'s Scope<'s, 'e>,
    tt: &Table,
    host: &'e Host<'e>,
) -> mlua::Result<()> {
    // What the terminal is connected **to** — `ts.HostName` for TCP/IP and
    // `COM<n>` for serial. A linked terminal with nothing open answers the
    // empty string rather than refusing, because "is there a terminal" and
    // "is anything open" are two questions and this command asks the first.
    tt.set(
        "gethostname",
        scope.create_function(move |lua, ()| {
            link(host)?;
            let h = host.borrow_mut().hostname().map_err(lua_err)?;
            lua.create_string(h)
        })?,
    )?;

    // `nil` for a clipboard that could not be read *or* holds something that
    // is not text — upstream reports both as `result` 0 and so does this.
    tt.set(
        "getclipboard",
        scope.create_function(move |lua, ()| {
            let text = host.borrow_mut().clipboard_text();
            match text {
                Some(t) => Ok(Value::String(lua.create_string(t)?)),
                None => Ok(Value::Nil),
            }
        })?,
    )?;
    tt.set(
        "setclipboard",
        scope.create_function(move |_, text: BString| {
            Ok(host.borrow_mut().set_clipboard_text(&text))
        })?,
    )?;

    // This machine's own addresses, as a 1-based array. `nil` is "could not
    // retrieve", which is what upstream reports when the enumeration fails.
    //
    // The v6 rendering is upstream's and is **not** RFC 5952: `myInetNtop`
    // prints all sixteen bytes as `%02x` with a colon after every second one,
    // so `::1` comes back as `0000:0000:…:0001`. A host is free to do better;
    // the default does not, so that a script comparing against a stored
    // address keeps working.
    tt.set(
        "getipaddresses",
        scope.create_function(move |lua, v6: Option<bool>| {
            let addrs = host.borrow_mut().local_ip_addresses(v6.unwrap_or(false));
            match addrs {
                None => Ok(None),
                Some(list) => {
                    let t = lua.create_table()?;
                    for (i, a) in list.iter().enumerate() {
                        t.set(i + 1, lua.create_string(a)?)?;
                    }
                    Ok(Some(t))
                }
            }
        })?,
    )?;

    // **Tera Term's version, not Sterna's**, and deliberately: `getver`'s
    // whole use is feature gating, so a version of its own would fail every
    // gate ever written and silently take the old branch.
    tt.set(
        "version",
        scope.create_function(move |_, ()| Ok(host.borrow_mut().version()))?,
    )?;

    // ---- the clock ----
    //
    // `os.time` and `os.date` exist and are fine for most of what a script
    // wants. These are the terminal's clock: a host that makes a run
    // repeatable overrides them, and `tt.date`'s zone argument is applied
    // without going through the environment — upstream sets `TZ` around the
    // call and leaks it on one path, which is a defect rather than something
    // to reproduce.

    tt.set(
        "time",
        scope.create_function(move |_, ()| Ok(host.borrow_mut().now_unix()))?,
    )?;
    tt.set(
        "date",
        scope.create_function(
            move |lua, (fmt, when, tz): (BString, Option<i64>, Option<BString>)| {
                let mut h = host.borrow_mut();
                let when = when.unwrap_or_else(|| h.now_unix());
                let out = h.strftime(when, &fmt, tz.as_ref().map(|t| t.as_slice()));
                match out {
                    Some(s) => Ok(Value::String(lua.create_string(s)?)),
                    None => Ok(Value::Nil),
                }
            },
        )?,
    )?;
    // Milliseconds since the machine booted, or `nil` if it cannot be asked.
    // Upstream assigns `GetTickCount`'s `DWORD` to an `int`, so a machine up
    // more than 24.9 days reports a negative number; nothing here does that,
    // because the wrap is a defect of the C type rather than a promise.
    tt.set(
        "uptime",
        scope.create_function(move |_, ()| Ok(host.borrow_mut().uptime_ms()))?,
    )?;

    // These move the **system** clock, and on most machines they will do
    // nothing: upstream's `SetLocalTime` needs a privilege an ordinary
    // `ttpmacro.exe` does not have, and it discards the result either way. The
    // fields not named are left as they are.
    tt.set(
        "setsystemdate",
        scope.create_function(move |_, (y, m, d): (i32, i32, i32)| {
            host.borrow_mut().set_system_date(y, m, d);
            Ok(())
        })?,
    )?;
    tt.set(
        "setsystemtime",
        scope.create_function(move |_, (h_, m, s): (i32, i32, i32)| {
            host.borrow_mut().set_system_time(h_, m, s);
            Ok(())
        })?,
    )?;

    // `include`'s one job that `require` cannot do for itself is finding a
    // file the host has and the filesystem may not, so it stays reachable —
    // and it is the only way a sandboxed host can hand a script its
    // neighbours.
    tt.set(
        "readmacro",
        scope.create_function(move |lua, path: BString| {
            let body = host.borrow_mut().read_macro(&path).map_err(lua_err)?;
            lua.create_string(body)
        })?,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::tests::run;
    use crate::Script;
    use tt_ttl::RecordingHost;

    #[test]
    fn the_terminal_answers_what_it_is_connected_to() {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.hostname = b"router.example".to_vec();
        Script::new("t.lua", b"tt.dispstr(tt.gethostname())".to_vec())
            .run(&mut host)
            .unwrap();
        assert_eq!(host.output, b"router.example");
    }

    #[test]
    fn the_clipboard_goes_both_ways_and_reports_a_refusal() {
        let (host, r) = run("tt.dispstr(tostring(tt.setclipboard('x')))");
        r.unwrap();
        assert_eq!(host.output, b"false");

        let mut host = RecordingHost::new();
        host.linked = true;
        host.clipboard_writable = true;
        let src = "tt.setclipboard('copied'); tt.dispstr(tt.getclipboard())";
        Script::new("t.lua", src.as_bytes().to_vec())
            .run(&mut host)
            .unwrap();
        assert_eq!(host.output, b"copied");
    }

    #[test]
    fn an_unreadable_clipboard_is_nil_rather_than_empty() {
        let (host, r) = run("tt.dispstr(tostring(tt.getclipboard()))");
        r.unwrap();
        assert_eq!(host.output, b"nil");
    }

    /// The version is Tera Term's, so a feature gate written twenty years ago
    /// still takes the branch its author meant.
    #[test]
    fn the_version_is_tera_terms() {
        let (host, r) = run("local a, b = tt.version(); tt.dispstr(tostring(a), '.', tostring(b))");
        r.unwrap();
        assert_eq!(host.output, b"5.7");
    }

    #[test]
    fn the_clock_is_the_hosts_so_a_run_can_be_repeatable() {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.now = Some(1_000_000_000);
        let src = "tt.dispstr(tostring(tt.time()), ' ', tt.date('%Y-%m-%d'))";
        Script::new("t.lua", src.as_bytes().to_vec())
            .run(&mut host)
            .unwrap();
        assert_eq!(host.output, b"1000000000 2001-09-09");
    }

    #[test]
    fn the_addresses_are_a_one_based_array_or_nothing() {
        let (host, r) = run("tt.dispstr(tostring(tt.getipaddresses()))");
        r.unwrap();
        assert_eq!(host.output, b"nil");

        let mut host = RecordingHost::new();
        host.linked = true;
        host.ipv4 = Some(vec![b"10.0.0.1".to_vec(), b"192.168.1.5".to_vec()]);
        Script::new("t.lua", b"tt.dispstr(tt.getipaddresses()[2])".to_vec())
            .run(&mut host)
            .unwrap();
        assert_eq!(host.output, b"192.168.1.5");
    }

    #[test]
    fn setting_the_system_clock_reaches_the_host() {
        let (host, r) = run("tt.setsystemdate(2026, 8, 9); tt.setsystemtime(13, 30, 0)");
        r.unwrap();
        assert_eq!(host.clock_sets, ["setdate 2026-8-9", "settime 13:30:0"]);
    }
}

//! The terminal's odds and ends, and the commands that reach the *other*
//! terminals.
//!
//! Upstream's are the `TTLCommCmd*` two-liners whose behaviour is all in
//! `ttdde.c`, and three of them switch on the **first character** of a decimal
//! argument — `showtt 100` restores the VT window and any negative number
//! hides it, because `-1` and `-99` are the same `'-'` arm. That fold is in
//! [`ScriptHost`](tt_ttl::ScriptHost)'s enums already, so what reaches Lua is
//! the meaning and never the digit.

use mlua::{BString, Scope, Table, Value, Variadic};
use tt_ttl::{BeepSound, ClearScreen, DebugMode, MacroWindow, ShowWindow, WindowState};

use crate::conn::link;
use crate::{choice, deadline, lua_err, Host};

fn state_name(s: WindowState) -> &'static str {
    match s {
        WindowState::Normal => "normal",
        WindowState::Minimized => "minimized",
        WindowState::Maximized => "maximized",
        WindowState::Hidden => "hidden",
    }
}

pub(crate) fn install<'s, 'e>(
    scope: &'s Scope<'s, 'e>,
    tt: &Table,
    host: &'e Host<'e>,
) -> mlua::Result<()> {
    // A sound, not the terminal bell: upstream's is `MessageBeep` in the macro
    // process, so it works with no terminal attached and the names are
    // Windows' system events. Only `simple` has a portable meaning.
    tt.set(
        "beep",
        scope.create_function(move |_, sound: Option<String>| {
            let sound = match sound {
                None => BeepSound::Simple,
                Some(s) => choice(
                    &s,
                    "sound",
                    &[
                        ("simple", BeepSound::Simple),
                        ("asterisk", BeepSound::Asterisk),
                        ("exclamation", BeepSound::Exclamation),
                        ("criticalstop", BeepSound::CriticalStop),
                        ("question", BeepSound::Question),
                        ("default", BeepSound::Default),
                    ],
                )?,
            };
            host.borrow_mut().beep(sound).map_err(lua_err)
        })?,
    )?;

    // The ids are `teraterm.rc`'s and there is no way to make them portable: a
    // script that says 50210 means Edit > Copy, and a frontend either has that
    // table or it does not. Kept as a number for exactly that reason — naming
    // ninety menu items here would be inventing a second vocabulary for one
    // that is already written down.
    tt.set(
        "callmenu",
        scope.create_function(move |_, id: i32| {
            link(host)?;
            host.borrow_mut().call_menu(id).map_err(lua_err)
        })?,
    )?;

    // The **file transfer** directory, which is not the script's own. Lua's
    // own working directory is `os`'s business; this is what a relative name
    // in `tt.zmodemrecv` and a log file resolve against. Two directories, and
    // upstream's names for them — `setdir` and `changedir` — are the wrong way
    // round for guessing, which is why this one is spelled out.
    tt.set(
        "settransferdir",
        scope.create_function(move |_, path: BString| {
            link(host)?;
            host.borrow_mut().set_transfer_dir(&path).map_err(lua_err)
        })?,
    )?;

    tt.set(
        "clearscreen",
        scope.create_function(move |_, what: Option<String>| {
            let what = match what {
                None => ClearScreen::Screen,
                Some(w) => choice(
                    &w,
                    "clearscreen",
                    &[
                        ("screen", ClearScreen::Screen),
                        ("buffer", ClearScreen::ScreenAndBuffer),
                        ("tek", ClearScreen::TekScreen),
                    ],
                )?,
            };
            link(host)?;
            host.borrow_mut().clear_screen(what).map_err(lua_err)
        })?,
    )?;

    tt.set(
        "enablekeyb",
        scope.create_function(move |_, on: bool| {
            link(host)?;
            host.borrow_mut().enable_keyboard(on).map_err(lua_err)
        })?,
    )?;
    tt.set(
        "loadkeymap",
        scope.create_function(move |_, path: BString| {
            link(host)?;
            host.borrow_mut().load_key_map(&path).map_err(lua_err)
        })?,
    )?;
    tt.set(
        "restoresetup",
        scope.create_function(move |_, path: BString| {
            link(host)?;
            host.borrow_mut().restore_setup(&path).map_err(lua_err)
        })?,
    )?;

    tt.set(
        "setdebug",
        scope.create_function(move |_, mode: String| {
            let mode = choice(
                &mode,
                "debug mode",
                &[
                    ("off", DebugMode::Off),
                    ("normal", DebugMode::Normal),
                    ("hex", DebugMode::Hex),
                    ("silent", DebugMode::Silent),
                ],
            )?;
            link(host)?;
            host.borrow_mut().set_debug_mode(mode).map_err(lua_err)
        })?,
    )?;
    tt.set(
        "setecho",
        scope.create_function(move |_, on: bool| {
            link(host)?;
            host.borrow_mut().set_local_echo(on).map_err(lua_err)
        })?,
    )?;

    tt.set(
        "settitle",
        scope.create_function(move |_, title: BString| {
            link(host)?;
            host.borrow_mut().set_title(&title).map_err(lua_err)
        })?,
    )?;
    tt.set(
        "gettitle",
        scope.create_function(move |lua, ()| {
            link(host)?;
            let t = host.borrow_mut().title().map_err(lua_err)?;
            lua.create_string(t)
        })?,
    )?;

    tt.set(
        "showtt",
        scope.create_function(move |_, which: String| {
            let which = choice(
                &which,
                "window",
                &[
                    ("hide", ShowWindow::VtHide),
                    ("minimize", ShowWindow::VtMinimize),
                    ("restore", ShowWindow::VtRestore),
                    ("tekhide", ShowWindow::TekHide),
                    ("tekminimize", ShowWindow::TekMinimize),
                    ("tekopen", ShowWindow::TekOpen),
                    ("tekclose", ShowWindow::TekClose),
                    ("loghide", ShowWindow::LogHide),
                    ("logminimize", ShowWindow::LogMinimize),
                    ("logrestore", ShowWindow::LogRestore),
                ],
            )?;
            link(host)?;
            host.borrow_mut().show_window(which).map_err(lua_err)
        })?,
    )?;
    // The **script's** own window, which upstream has and this port only has
    // if the frontend made one to show a script running.
    tt.set(
        "show",
        scope.create_function(move |_, how: String| {
            let how = choice(
                &how,
                "window",
                &[
                    ("hide", MacroWindow::Hide),
                    ("minimize", MacroWindow::Minimize),
                    ("restore", MacroWindow::Restore),
                ],
            )?;
            host.borrow_mut().show_macro_window(how).map_err(lua_err)
        })?,
    )?;

    // Where the terminal's window is. `nil` is "cannot say", which upstream
    // reports as -1 in every field.
    tt.set(
        "getttpos",
        scope.create_function(move |lua, ()| {
            link(host)?;
            let g = host.borrow_mut().terminal_geometry().map_err(lua_err)?;
            match g {
                None => Ok(None),
                Some(g) => {
                    let t = lua.create_table()?;
                    t.set("state", state_name(g.state))?;
                    t.set("x", g.window.0)?;
                    t.set("y", g.window.1)?;
                    t.set("width", g.window.2)?;
                    t.set("height", g.window.3)?;
                    t.set("clientx", g.client.0)?;
                    t.set("clienty", g.client.1)?;
                    t.set("clientwidth", g.client.2)?;
                    t.set("clientheight", g.client.3)?;
                    Ok(Some(t))
                }
            }
        })?,
    )?;

    // ---- the other terminals ----

    // The line ending here is **CRLF**, not the bare CR `tt.sendln` uses, and
    // that is not a typo on either side: `sendln` goes through the terminal's
    // own newline setting and a broadcast does not.
    tt.set(
        "sendbroadcast",
        scope.create_function(move |_, args: Variadic<Value>| {
            let bytes = crate::conn::bytes_of(&args)?;
            link(host)?;
            host.borrow_mut().send_broadcast(&bytes).map_err(lua_err)
        })?,
    )?;
    tt.set(
        "sendlnbroadcast",
        scope.create_function(move |_, args: Variadic<Value>| {
            let mut bytes = crate::conn::bytes_of(&args)?;
            bytes.extend_from_slice(b"\r\n");
            link(host)?;
            host.borrow_mut().send_broadcast(&bytes).map_err(lua_err)
        })?,
    )?;
    tt.set(
        "sendmulticast",
        scope.create_function(move |_, (name, args): (BString, Variadic<Value>)| {
            let bytes = crate::conn::bytes_of(&args)?;
            link(host)?;
            host.borrow_mut()
                .send_multicast(&name, &bytes)
                .map_err(lua_err)
        })?,
    )?;
    tt.set(
        "sendlnmulticast",
        scope.create_function(move |_, (name, args): (BString, Variadic<Value>)| {
            let mut bytes = crate::conn::bytes_of(&args)?;
            bytes.extend_from_slice(b"\r\n");
            link(host)?;
            host.borrow_mut()
                .send_multicast(&name, &bytes)
                .map_err(lua_err)
        })?,
    )?;
    tt.set(
        "setmulticastname",
        scope.create_function(move |_, name: BString| {
            link(host)?;
            host.borrow_mut().set_multicast_name(&name).map_err(lua_err)
        })?,
    )?;

    // Not a character: it runs whatever the *keyboard file* has bound to that
    // key. Both arguments reach the terminal as four hex digits, so both are
    // sixteen bits and a larger value wraps — reproduced with the cast rather
    // than a range check, because a range check turns a quiet script into a
    // failing one.
    tt.set(
        "sendkcode",
        scope.create_function(move |_, (code, repeat): (i64, Option<i64>)| {
            link(host)?;
            host.borrow_mut()
                .send_key_code(code as u16, repeat.unwrap_or(1) as u16)
                .map_err(lua_err)
        })?,
    )?;

    // Upstream does **not** wait for these to finish — the documentation's own
    // example polls `ps` to find out — so a host that blocks is being kinder
    // than Tera Term rather than more faithful.
    tt.set(
        "scpsend",
        scope.create_function(move |_, (path, dest): (BString, Option<BString>)| {
            link(host)?;
            host.borrow_mut()
                .scp(true, &path, &dest.unwrap_or_default())
                .map_err(lua_err)
        })?,
    )?;
    tt.set(
        "scprecv",
        scope.create_function(move |_, (path, dest): (BString, Option<BString>)| {
            link(host)?;
            host.borrow_mut()
                .scp(false, &path, &dest.unwrap_or_default())
                .map_err(lua_err)
        })?,
    )?;

    // Waits until **every** terminal running a script has seen one of the
    // patterns, and answers with the index this one matched. The set is
    // whatever was running when the command started.
    let t = tt.clone();
    tt.set(
        "wait4all",
        scope.create_function(move |_, pats: Variadic<BString>| {
            let pats: Vec<Vec<u8>> = pats.iter().map(|p| p.to_vec()).collect();
            link(host)?;
            if pats.is_empty() {
                return Ok(None);
            }
            let timeout =
                deadline(&t)?.map(|d| d.saturating_duration_since(std::time::Instant::now()));
            let found = host
                .borrow_mut()
                .wait_for_all(&pats, timeout)
                .map_err(lua_err)?;
            Ok((found > 0).then_some(found))
        })?,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::tests::run;

    #[test]
    fn the_three_folded_arguments_arrive_as_meanings() {
        let (host, r) = run("tt.showtt('tekopen'); tt.setdebug('hex'); tt.clearscreen('buffer')");
        r.unwrap();
        assert_eq!(
            host.terminal,
            [
                "showtt TekOpen",
                "setdebug Hex",
                "clearscreen ScreenAndBuffer"
            ]
        );
    }

    #[test]
    fn beep_defaults_to_the_speaker() {
        let (host, r) = run("tt.beep(); tt.beep('question')");
        r.unwrap();
        assert_eq!(host.terminal, ["beep Simple", "beep Question"]);
    }

    /// The one place a broadcast differs from a `send`, and it is deliberate
    /// on both sides.
    #[test]
    fn a_broadcast_line_ends_in_crlf_where_sendln_ends_in_cr() {
        let (host, r) = run("tt.sendlnbroadcast('halt'); tt.sendln('halt')");
        r.unwrap();
        assert_eq!(host.sends, [r#"broadcast "halt\r\n""#]);
        assert_eq!(host.sent, b"halt\r");
    }

    #[test]
    fn scp_and_the_multicast_name_reach_the_host() {
        let (host, r) = run("tt.setmulticastname('lab'); tt.scpsend('/tmp/f', 'host:/tmp/')");
        r.unwrap();
        assert_eq!(
            host.sends,
            [r#"multicastname "lab""#, r#"scpsend "/tmp/f" "host:/tmp/""#]
        );
    }

    #[test]
    fn a_wrong_name_lists_the_right_ones() {
        let (_, r) = run("tt.setdebug('hexadecimal')");
        let msg = r.unwrap_err().to_string();
        assert!(msg.contains("off, normal, hex, silent"), "{msg}");
    }

    #[test]
    fn the_window_geometry_is_a_table_or_nothing() {
        let (host, r) = run("tt.dispstr(tostring(tt.getttpos()))");
        r.unwrap();
        assert_eq!(host.output, b"nil");
    }
}

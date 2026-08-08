//! The terminal's odds and ends — the window, the title, the screen, the
//! keyboard and the two setup files.
//!
//! Almost all of them are `TTLCommCmd*` two-liners whose behaviour lives in
//! `teraterm/ttdde.c`, which is the half a reader of `ttl.cpp` alone never
//! sees. Reading only the macro side gives commands that take an argument and
//! do nothing with it.
//!
//! **One quirk runs through the family and is worth knowing before the
//! commands: several of these arguments reach the terminal as text and are
//! then switched on their *first character*.** `CmdShowTT` (`ttdde.c:847`),
//! `CmdClearScreen` (`:593`) and `CmdSetDebug` (`:834`) each read
//! `ParamFileName[0]`, so `showtt 100` is `showtt 1`, `clearscreen 25` is
//! `clearscreen 2`, and any negative value is the `'-'` arm — which `showtt`
//! has and the other two do not. A value with no arm is not an error; nothing
//! happens. All three are reproduced, in the `from_code` functions on the
//! enums, which is the one place that has to know it.

use crate::error::{TtlError, TtlResult};
use crate::expr;
use crate::host::{BeepSound, ClearScreen, DebugMode, MacroWindow, ScriptHost, ShowWindow};
use crate::interp::Interp;
use crate::rsv::Rsv;

impl Interp {
    /// Dispatch for the commands in this file. `None` means "not one of mine".
    pub(crate) fn terminal_command(
        &mut self,
        host: &mut dyn ScriptHost,
        w: Rsv,
    ) -> Option<TtlResult<()>> {
        Some(match w {
            Rsv::Beep => self.cmd_beep(host),
            Rsv::CallMenu => self.comm_cmd_int(host).and_then(|v| host.call_menu(v)),
            Rsv::ChangeDir => self
                .comm_cmd_file(host)
                .and_then(|p| host.set_transfer_dir(&p)),
            Rsv::ClearScreen => {
                self.comm_cmd_int(host)
                    .and_then(|v| match ClearScreen::from_code(v) {
                        Some(what) => host.clear_screen(what),
                        None => Ok(()),
                    })
            }
            Rsv::EnableKeyb => self
                .comm_cmd_int(host)
                .and_then(|v| host.enable_keyboard(v != 0)),
            Rsv::LoadKeyMap => self.comm_cmd_file(host).and_then(|p| host.load_key_map(&p)),
            Rsv::RestoreSetup => self
                .comm_cmd_file(host)
                .and_then(|p| host.restore_setup(&p)),
            Rsv::SetDebug => self
                .comm_cmd_int(host)
                .and_then(|v| match DebugMode::from_code(v) {
                    Some(mode) => host.set_debug_mode(mode),
                    None => Ok(()),
                }),
            Rsv::SetEcho => self
                .comm_cmd_int(host)
                .and_then(|v| host.set_local_echo(v != 0)),
            Rsv::SetTitle => self.comm_cmd_file(host).and_then(|t| host.set_title(&t)),
            Rsv::GetTitle => self.cmd_get_title(host),
            Rsv::ShowTT => self
                .comm_cmd_int(host)
                .and_then(|v| match ShowWindow::from_code(v) {
                    Some(which) => host.show_window(which),
                    None => Ok(()),
                }),
            Rsv::Show => self.cmd_show(host),
            Rsv::GetTTPos => self.cmd_get_tt_pos(host),
            Rsv::GetTTDir => self.cmd_get_tt_dir(),
            Rsv::SetSerialDelayChar => self.cmd_set_serial_delay(host, false),
            Rsv::SetSerialDelayLine => self.cmd_set_serial_delay(host, true),
            _ => return None,
        })
    }

    /// `beep [<sound type>]`.
    ///
    /// The one command in this file with **no link check**: upstream's is
    /// `MessageBeep` in `ttpmacro.exe` itself, so it works with no terminal
    /// attached. Its argument is validated properly, too — an unknown value is
    /// `ErrSyntax` rather than the silent nothing the switched-on-a-character
    /// commands give.
    fn cmd_beep(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let sound = if self.lx.parameter_given() {
            let v = expr::get_int_val(&mut self.lx, &mut self.vars)?;
            BeepSound::from_code(v).ok_or(TtlError::Syntax)?
        } else {
            BeepSound::Default
        };
        self.end_of_line()?;
        host.beep(sound)
    }

    /// `gettitle <strvar>` — the terminal's window title.
    fn cmd_get_title(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let target = expr::get_str_var(&mut self.lx, &mut self.vars)?;
        self.comm_cmd(host)?;
        let title = host.title()?;
        self.vars.set_str(target, &title);
        Ok(())
    }

    /// `show <show flag>` — the **macro's** window, not the terminal's.
    ///
    /// Upstream's is `ttpmacro.exe`'s own dialog, which is why this one is
    /// local rather than a DDE command and why it has no link check. In
    /// process there is no separate window unless the frontend has made one to
    /// show a macro running, so this is the host's to answer or refuse.
    fn cmd_show(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let v = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        // Three-way on the sign, so every negative hides and every positive
        // restores — no first-character reading here.
        host.show_macro_window(match v.signum() {
            0 => MacroWindow::Minimize,
            1 => MacroWindow::Restore,
            _ => MacroWindow::Hide,
        })
    }

    /// `getttpos <showflag> <wx> <wy> <ww> <wh> <cx> <cy> <cw> <ch>` → 0, or
    /// -1 if the terminal could not describe itself.
    ///
    /// Nine integer variables, and upstream's -1 arm is a `sscanf` of the
    /// terminal's reply failing to yield nine fields — which is what a host
    /// with no window is really saying, so [`terminal_geometry`] answers
    /// `None` for it.
    ///
    /// [`terminal_geometry`]: ScriptHost::terminal_geometry
    fn cmd_get_tt_pos(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let mut targets = Vec::with_capacity(9);
        for _ in 0..9 {
            targets.push(expr::get_int_var(&mut self.lx, &mut self.vars)?);
        }
        self.comm_cmd(host)?;

        let Some(g) = host.terminal_geometry()? else {
            self.set_result(-1);
            return Ok(());
        };
        let values = [
            g.state.code(),
            g.window.0,
            g.window.1,
            g.window.2,
            g.window.3,
            g.client.0,
            g.client.1,
            g.client.2,
            g.client.3,
        ];
        for (target, v) in targets.into_iter().zip(values) {
            self.vars.set_int(target, v);
        }
        self.set_result(0);
        Ok(())
    }

    /// `getttdir <strvar>` → 1, with the application's own directory.
    ///
    /// No link check and no host method: upstream reads
    /// `GetModuleFileName(NULL)`, which is the *running executable*, and
    /// `std::env::current_exe` is that exactly. So a frontend gets its
    /// installation directory and a test binary gets the test binary's, which
    /// is the same answer upstream would give in the same position.
    ///
    /// The failure arm writes the empty string and 0, and it is reachable — a
    /// deleted or replaced executable is what makes `current_exe` fail.
    fn cmd_get_tt_dir(&mut self) -> TtlResult<()> {
        let target = expr::get_str_var(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;

        let dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(crate::files::path_to_bytes));
        match dir {
            Some(d) => {
                self.vars.set_str(target, &d);
                self.set_result(1);
            }
            None => {
                self.vars.set_str(target, b"");
                self.set_result(0);
            }
        }
        Ok(())
    }

    /// `setserialdelaychar <ms>` and `setserialdelayline <ms>`.
    ///
    /// The only two commands in this file that wait for a result, and the only
    /// two that report one — `IdTTLWaitCmndResult` where every neighbour uses
    /// 0. Serial-only, like the control lines, so a host with another kind of
    /// connection declines quietly rather than failing.
    fn cmd_set_serial_delay(&mut self, host: &mut dyn ScriptHost, per_line: bool) -> TtlResult<()> {
        let ms = self.comm_cmd_int(host)?;
        let ok = host.set_serial_delay(per_line, ms)?;
        self.set_result(i32::from(ok));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::host::{
        BeepSound, ClearScreen, DebugMode, RecordingHost, ShowWindow, WindowGeometry, WindowState,
    };
    use crate::interp::Interp;
    use crate::TtlError;

    fn run_with(host: &mut RecordingHost, src: &str) {
        let mut it = Interp::new("t.ttl", src.as_bytes().to_vec(), host);
        it.run(host);
    }

    fn run(src: &str) -> RecordingHost {
        let mut host = RecordingHost::new();
        host.linked = true;
        run_with(&mut host, src);
        host
    }

    fn did(src: &str) -> Vec<String> {
        let h = run(src);
        assert!(h.errors.is_empty(), "unexpected errors: {:?}", h.errors);
        h.terminal.clone()
    }

    fn err_of(src: &str) -> TtlError {
        let h = run(src);
        assert_eq!(h.errors.len(), 1, "expected one error: {:?}", h.errors);
        h.errors[0].0
    }

    #[test]
    fn the_thin_ones_reach_the_host_with_their_argument() {
        assert_eq!(
            did("callmenu 50210\nchangedir '/tmp'\nenablekeyb 0\nsetecho 1\nsettitle 'hi'\nloadkeymap 'k.cnf'\nrestoresetup 't.ini'"),
            vec![
                "callmenu 50210",
                "changedir \"/tmp\"",
                "enablekeyb 0",
                "setecho 1",
                "settitle \"hi\"",
                "loadkeymap \"k.cnf\"",
                "restoresetup \"t.ini\"",
            ]
        );
    }

    #[test]
    fn every_one_of_them_but_beep_and_getttdir_wants_a_terminal() {
        for src in [
            "callmenu 1",
            "changedir '/tmp'",
            "clearscreen 0",
            "enablekeyb 1",
            "loadkeymap 'k'",
            "restoresetup 't'",
            "setdebug 1",
            "setecho 1",
            "settitle 'x'",
            "gettitle v",
            "showtt 1",
            "getttpos a b c d e f g h i",
            "setserialdelaychar 10",
        ] {
            let mut host = RecordingHost::new();
            run_with(&mut host, src);
            assert_eq!(
                host.errors.first().map(|e| e.0),
                Some(TtlError::LinkFirst),
                "{src}"
            );
        }

        // `beep` is `MessageBeep` in the macro process and `getttdir` is its
        // own path, so neither asks for a terminal.
        let mut host = RecordingHost::new();
        run_with(&mut host, "beep\ngetttdir v");
        assert!(host.errors.is_empty(), "{:?}", host.errors);
    }

    #[test]
    fn only_the_first_character_of_the_argument_reaches_the_terminal() {
        // `ttdde.c` switches on `ParamFileName[0]`, so these three are the
        // same command written three ways.
        assert_eq!(did("clearscreen 1"), did("clearscreen 199"));
        assert_eq!(did("showtt 1"), did("showtt 100"));
        assert_eq!(
            did("showtt (-1)"),
            did("showtt (-99)"),
            "any negative hides"
        );
        // And a value with no arm is silently nothing, not an error.
        assert_eq!(did("clearscreen 9"), Vec::<String>::new());
        assert_eq!(did("showtt 9"), Vec::<String>::new());
    }

    #[test]
    fn the_three_switched_arguments_map_where_the_documentation_says() {
        assert_eq!(ClearScreen::from_code(0), Some(ClearScreen::Screen));
        assert_eq!(
            ClearScreen::from_code(1),
            Some(ClearScreen::ScreenAndBuffer)
        );
        assert_eq!(ClearScreen::from_code(2), Some(ClearScreen::TekScreen));
        assert_eq!(ClearScreen::from_code(-1), None, "no minus arm here");

        assert_eq!(ShowWindow::from_code(-1), Some(ShowWindow::VtHide));
        assert_eq!(ShowWindow::from_code(0), Some(ShowWindow::VtMinimize));
        assert_eq!(ShowWindow::from_code(1), Some(ShowWindow::VtRestore));
        assert_eq!(ShowWindow::from_code(8), Some(ShowWindow::LogRestore));

        assert_eq!(DebugMode::from_code(0), Some(DebugMode::Off));
        assert_eq!(DebugMode::from_code(2), Some(DebugMode::Hex));
        assert_eq!(DebugMode::from_code(4), None);
    }

    #[test]
    fn beep_validates_its_argument_where_the_others_do_not() {
        assert_eq!(did("beep"), vec!["beep Default"]);
        assert_eq!(did("beep 0"), vec!["beep Simple"]);
        assert_eq!(did("beep 5"), vec!["beep Default"]);
        assert_eq!(err_of("beep 6"), TtlError::Syntax);
        assert_eq!(err_of("beep (-1)"), TtlError::Syntax);
        assert_eq!(BeepSound::from_code(3), Some(BeepSound::CriticalStop));
    }

    #[test]
    fn gettitle_fills_the_variable_and_settitle_sends_it() {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.title = b"a window".to_vec();
        run_with(&mut host, "gettitle t\ndispstr t");
        assert_eq!(host.output, b"a window");
        assert_eq!(err_of("gettitle 'literal'"), TtlError::Syntax);
    }

    #[test]
    fn getttpos_fills_nine_variables_or_reports_minus_one() {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.geometry = Some(WindowGeometry {
            state: WindowState::Maximized,
            window: (1, 2, 3, 4),
            client: (5, 6, 7, 8),
        });
        run_with(
            &mut host,
            "getttpos s wx wy ww wh cx cy cw ch\ndispstr result '|' s wx wy ww wh cx cy cw ch",
        );
        assert!(host.errors.is_empty(), "{:?}", host.errors);
        assert_eq!(host.output, b"0|212345678");

        let h = run("getttpos a b c d e f g h i\ndispstr result");
        assert_eq!(h.output, b"-1", "no window to describe");
        assert_eq!(err_of("getttpos a b c"), TtlError::Syntax);
    }

    #[test]
    fn getttdir_answers_the_running_executables_directory() {
        let h = run("getttdir d\ndispstr result");
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(h.output, b"1");
    }

    #[test]
    fn show_is_three_ways_on_the_sign() {
        assert_eq!(did("show 0"), vec!["show Minimize"]);
        assert_eq!(did("show 1"), vec!["show Restore"]);
        assert_eq!(did("show 99"), vec!["show Restore"]);
        assert_eq!(did("show (-1)"), vec!["show Hide"]);
    }

    #[test]
    fn the_serial_delays_are_the_only_two_that_report() {
        let h = run("setserialdelaychar 5\nsetserialdelayline 10\ndispstr result");
        assert_eq!(
            h.terminal,
            vec!["serialdelay char 5", "serialdelay line 10"]
        );
        assert_eq!(h.output, b"1");

        let mut host = RecordingHost::new();
        host.linked = true;
        host.serial_delay_fails = true;
        run_with(&mut host, "setserialdelaychar 5\ndispstr result");
        assert_eq!(host.output, b"0", "a connection that is not serial");
    }
}

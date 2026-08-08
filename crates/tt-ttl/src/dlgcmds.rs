//! The dialogs — every command that puts a window in front of the user.
//!
//! Upstream these are `ttpmacro.exe`'s own windows, in `ttl_gui.cpp` and
//! `ttmdlg.cpp`, and each is an ordinary `DoModal` on the thread the macro is
//! running on. That is the shape kept here: one host method per dialog, each
//! of which blocks. What changes is who owns the window — the frontend does,
//! so it answers by spinning its own event loop, which is why the interpreter
//! has to be off the UI thread. It already had to be, for `wait`.
//!
//! Three of the family are worth knowing before reading the commands:
//!
//! - **Closing a dialog is not the same as cancelling it.** Every one of these
//!   windows puts a "halt the script?" confirmation in front of its close
//!   button, so `Closed` comes back only when the user has said yes — and the
//!   macro ends. Escape is `Cancel` and ends nothing.
//! - **`messagebox`, `statusbox`, `yesnobox` and `inputbox` each take a
//!   `<special>` flag** that runs [`restore_new_line`] over the *message* and
//!   nothing else. The documentation calls it obsolete and points at
//!   `strspecial`; it is still there because scripts use it.
//! - **`filenamebox` and `dirnamebox` report through `inputstr`**, and both
//!   check that it is still a string variable before opening anything at all.
//!   A macro that has assigned an integer to `inputstr` gets no dialog.

use crate::error::{TtlError, TtlResult};
use crate::expr;
use crate::host::{DialogAnchor, DialogEnd, DialogPos, ListBoxOpts, ScriptHost};
use crate::interp::Interp;
use crate::rsv::Rsv;
use crate::strcmds::{restore_new_line, scan_int};
use crate::vars::VarType;

impl Interp {
    /// Dispatch for the commands in this file. `None` means "not one of mine".
    pub(crate) fn dialog_command(
        &mut self,
        host: &mut dyn ScriptHost,
        w: Rsv,
    ) -> Option<TtlResult<()>> {
        Some(match w {
            Rsv::MessageBox => self.cmd_message_box(host),
            Rsv::YesNoBox => self.cmd_yes_no_box(host),
            Rsv::StatusBox => self.cmd_status_box(host),
            Rsv::CloseSBox => self.cmd_status_box_cmd(host, false),
            Rsv::BringupBox => self.cmd_status_box_cmd(host, true),
            Rsv::ListBox => self.cmd_list_box(host),
            Rsv::InputBox => self.cmd_input_box(host, false),
            Rsv::PasswordBox => self.cmd_input_box(host, true),
            Rsv::FilenameBox => self.cmd_filename_box(host),
            Rsv::DirnameBox => self.cmd_dirname_box(host),
            Rsv::SetDlgPos => self.cmd_set_dlg_pos(host),
            _ => return None,
        })
    }

    /// `MessageCommand`'s shared head (`ttl_gui.cpp:433`): a message, a title,
    /// and the optional `<special>` flag.
    ///
    /// Both strings are read with auto-conversion on, so `messagebox count
    /// 'n'` prints the number rather than reporting a type mismatch. Only the
    /// message is expanded — `<special>` is documented as not affecting the
    /// title, and does not.
    fn message_args(&mut self) -> TtlResult<(Vec<u8>, Vec<u8>)> {
        let text = expr::get_str_val2(&mut self.lx, &mut self.vars, true)?;
        let title = expr::get_str_val2(&mut self.lx, &mut self.vars, true)?;
        let special = if self.lx.parameter_given() {
            expr::get_int_val(&mut self.lx, &mut self.vars)?
        } else {
            0
        };
        self.end_of_line()?;
        let text = if special != 0 {
            restore_new_line(&text)
        } else {
            text
        };
        Ok((text, title))
    }

    /// `messagebox <message> <title> [<special>]` — one button, no `result`.
    ///
    /// The dialog reports `IDCANCEL` for its close button because it has no No
    /// button to spend that code on, where `yesnobox` reports `IDCLOSE`; both
    /// end the macro, so both of the non-`Ok` answers do here.
    fn cmd_message_box(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let (text, title) = self.message_args()?;
        if host.message_box(&text, &title)? != DialogEnd::Ok(()) {
            self.ended = true;
        }
        Ok(())
    }

    /// `yesnobox <message> <title> [<special>]` → 1 for Yes, 0 for No.
    ///
    /// Closing it is a No that also ends the macro: upstream sets `TTLStatus`
    /// and then falls through to the same `IDOK`-or-nothing test, so `result`
    /// is 0 either way.
    fn cmd_yes_no_box(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let (text, title) = self.message_args()?;
        let end = host.yes_no_box(&text, &title)?;
        if end == DialogEnd::Closed {
            self.ended = true;
        }
        self.set_result(i32::from(end == DialogEnd::Ok(())));
        Ok(())
    }

    /// `statusbox <message> <title> [<special>]` — the one modeless dialog.
    fn cmd_status_box(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let (text, title) = self.message_args()?;
        host.status_box(&text, &title)
    }

    /// `closesbox` and `bringupbox`, which take nothing and report nothing.
    fn cmd_status_box_cmd(&mut self, host: &mut dyn ScriptHost, bringup: bool) -> TtlResult<()> {
        self.end_of_line()?;
        if bringup {
            host.bringup_status_box()
        } else {
            host.close_status_box()
        }
    }

    /// `listbox <message> <title> <strary> [<selected>] [<keyword>...]` →
    /// the index chosen, -1 for cancel, -2 for closed.
    ///
    /// `TTLListBox` writes `result` from whatever `MessageCommand` handed back
    /// *before* it looks at the error code, so a listbox that failed to parse
    /// its arguments still leaves 0 there and one given an empty array leaves
    /// -1. Only a host that carries on past an error can see it, which is why
    /// the answer is threaded out separately rather than returned.
    fn cmd_list_box(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let mut code = 0;
        let r = self.list_box_body(host, &mut code);
        self.set_result(code);
        r
    }

    fn list_box_body(&mut self, host: &mut dyn ScriptHost, code: &mut i32) -> TtlResult<()> {
        // No `<special>` here: `MessageCommand` skips the optional-integer arm
        // entirely for a list box, so `sp` stays 0 and the message is never
        // expanded however many arguments follow.
        let text = expr::get_str_val2(&mut self.lx, &mut self.vars, true)?;
        let title = expr::get_str_val2(&mut self.lx, &mut self.vars, true)?;
        let ary = expr::get_ary_var(&mut self.lx, &mut self.vars, VarType::StrArray)?;

        let mut selected = 0;
        let mut opts = ListBoxOpts::default();
        while self.lx.parameter_given() {
            let arg = expr::get_str_val2(&mut self.lx, &mut self.vars, true)?;
            match keyword(&arg) {
                Keyword::DoubleClick => opts.double_click = true,
                Keyword::MinMaxButton => opts.min_max_button = true,
                // Each of these clears the other, so the last one written
                // wins rather than both being set.
                Keyword::Minimize => {
                    opts.minimized = true;
                    opts.maximized = false;
                }
                Keyword::Maximize => {
                    opts.minimized = false;
                    opts.maximized = true;
                }
                Keyword::Size => {
                    // Upstream's prefix test is `_wcsnicmp(..., 12)` written
                    // with a length of **5**, so anything beginning `listb`
                    // lands here and has to parse as a size or be an error.
                    let Some((w, h)) = scan_size(&arg) else {
                        return Err(TtlError::Syntax);
                    };
                    if w < 0 || h < 0 {
                        return Err(TtlError::Syntax);
                    }
                    opts.size = Some((w as u32, h as u32));
                }
                // Not a keyword, so it is the selected index — and this is how
                // the documented integer argument works at all, since the
                // argument was read with auto-conversion and arrives as text.
                Keyword::None => match scan_int(&arg, 10) {
                    Some(v) => selected = v,
                    None => return Err(TtlError::Syntax),
                },
            }
        }
        self.end_of_line()?;

        // An index the array cannot hold is not an error; it selects the first
        // item, which is also what an omitted argument does.
        let len = self.vars.array_len(ary);
        if selected < 0 || selected as usize >= len {
            selected = 0;
        }
        if len == 0 {
            // Upstream reaches this as `s[0] == NULL` after a `calloc` of one
            // element, and answers -1 as well as erroring.
            *code = -1;
            return Err(TtlError::Syntax);
        }

        let items: Vec<&[u8]> = (0..len)
            .map(|i| self.vars.str_at(self.vars.elem(ary, i as i32).unwrap()))
            .collect();
        let end = host.list_box(&text, &title, &items, selected as usize, &opts)?;

        *code = match end {
            DialogEnd::Ok(i) => i as i32,
            DialogEnd::Cancel => -1,
            DialogEnd::Closed => {
                self.ended = true;
                -2
            }
        };
        Ok(())
    }

    /// `inputbox <message> <title> [<default> [<special>]]`, and
    /// `passwordbox <message> <title> [<special>]`.
    ///
    /// Neither writes `result`; the answer is `inputstr`.
    ///
    /// **The third argument is read twice-over.** `inputbox` tries it as a
    /// string, and on a type mismatch rewinds the line and lets the
    /// `<special>` arm have it — which is what makes `inputbox 'a' 'b' 1`
    /// mean the flag and not a default of `"1"`. `passwordbox` has no default
    /// to take, so its optional argument is the flag directly.
    fn cmd_input_box(&mut self, host: &mut dyn ScriptHost, password: bool) -> TtlResult<()> {
        let text = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let title = expr::get_str_val(&mut self.lx, &mut self.vars)?;

        let mut default = Vec::new();
        if !password && self.lx.parameter_given() {
            let mark = self.lx.ptr;
            match expr::get_str_val(&mut self.lx, &mut self.vars) {
                Ok(s) => default = s,
                Err(TtlError::TypeMismatch) => self.lx.ptr = mark,
                Err(e) => return Err(e),
            }
        }

        let special = if self.lx.parameter_given() {
            expr::get_int_val(&mut self.lx, &mut self.vars)?
        } else {
            0
        };
        self.end_of_line()?;
        let text = if special != 0 {
            restore_new_line(&text)
        } else {
            text
        };

        self.set_input_str(b"");
        if !self.input_str_is_string() {
            return Ok(());
        }
        match host.input_box(&text, &title, &default, password)? {
            DialogEnd::Ok(s) => self.set_input_str(&s),
            // Upstream cannot answer this one. `CInpDlg` has no Cancel button,
            // but Escape still reaches `TTCDialog::OnCancel` and ends the
            // dialog with `IDCANCEL` — which `TTLInputBox` treats as OK and
            // copies its **uninitialised** stack buffer into `inputstr`
            // (`ttl_gui.cpp:353`, `:360`). Not reproducible in safe Rust and
            // nothing to be faithful to: the empty string is what the
            // documentation implies and what `getpassword` does with the very
            // same dialog, which initialises its buffer.
            DialogEnd::Cancel => {}
            DialogEnd::Closed => self.ended = true,
        }
        Ok(())
    }

    /// `filenamebox <title> [<dialogtype> [<initialdir>]]` → 1 if a name came
    /// back, with the name in `inputstr`.
    ///
    /// **`<dialogtype>` selects the dialog and upstream's flags contradict
    /// it.** A non-zero value opens the Save dialog with `OFN_FILEMUSTEXIST`
    /// set, which is Win32's "the name must already exist" and stops the user
    /// naming a new file; zero opens the Open dialog with
    /// `OFN_OVERWRITEPROMPT`, which an Open dialog has no use for
    /// (`ttl_gui.cpp:180`). The two flag sets are each other's. Implemented as
    /// documented — a Save dialog that cannot save is not a behaviour a script
    /// can be written against.
    fn cmd_filename_box(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let title = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let mut save = 0;
        let mut init_dir = Vec::new();
        if self.lx.parameter_given() {
            save = expr::get_int_val(&mut self.lx, &mut self.vars)?;
            if self.lx.parameter_given() {
                init_dir = expr::get_str_val(&mut self.lx, &mut self.vars)?;
            }
        }
        self.end_of_line()?;

        self.set_input_str(b"");
        if !self.input_str_is_string() {
            return Ok(());
        }
        let picked = host.filename_box(&title, save != 0, &init_dir)?;
        // Upstream writes the buffer whatever happened, and it is empty when
        // the dialog was cancelled — so `inputstr` is cleared either way.
        self.set_input_str(picked.as_deref().unwrap_or(b""));
        self.set_result(i32::from(picked.is_some()));
        Ok(())
    }

    /// `dirnamebox <title> [<initialdir>]` → 1 if a directory came back, with
    /// the path in `inputstr`.
    fn cmd_dirname_box(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let title = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let mut init_dir = Vec::new();
        if self.lx.parameter_given() {
            init_dir = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        }
        self.end_of_line()?;

        self.set_input_str(b"");
        if !self.input_str_is_string() {
            return Ok(());
        }
        let picked = host.dirname_box(&title, &init_dir)?;
        if let Some(path) = &picked {
            self.set_input_str(path);
        }
        self.set_result(i32::from(picked.is_some()));
        Ok(())
    }

    /// `setdlgpos [<x> <y> [<position> [<offset x> <offset y>]]]`.
    ///
    /// Four shapes, and the arguments come in the groups the documentation
    /// draws: no coordinates at all means centre on the primary display, and
    /// the offsets are only reachable through a `<position>`.
    fn cmd_set_dlg_pos(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        if !self.lx.parameter_given() {
            self.end_of_line()?;
            host.set_dialog_pos(None);
            return Ok(());
        }

        let x = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        let y = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        let mut anchor = None;
        let mut offset_x = 0;
        let mut offset_y = 0;
        if self.lx.parameter_given() {
            let position = expr::get_int_val(&mut self.lx, &mut self.vars)?;
            anchor = DialogAnchor::from_code(position);
            if anchor.is_none() {
                return Err(TtlError::Syntax);
            }
            if self.lx.parameter_given() {
                offset_x = expr::get_int_val(&mut self.lx, &mut self.vars)?;
                offset_y = expr::get_int_val(&mut self.lx, &mut self.vars)?;
            }
        }
        self.end_of_line()?;

        host.set_dialog_pos(Some(DialogPos {
            x,
            y,
            anchor,
            offset_x,
            offset_y,
        }));
        Ok(())
    }

    /// Whether `inputstr` is still a string variable.
    ///
    /// `TTLInputBox`, `TTLFilenameBox` and `TTLDirnameBox` all guard on this
    /// and skip the dialog entirely when it fails, which is a step further
    /// than [`set_input_str`](Interp::set_input_str)'s silence: the user is
    /// not asked a question whose answer has nowhere to go.
    fn input_str_is_string(&self) -> bool {
        matches!(self.vars.find(b"inputstr"), Some((_, VarType::String)))
    }
}

/// Which of `listbox`'s keyword parameters an argument is, if any.
enum Keyword {
    DoubleClick,
    MinMaxButton,
    Minimize,
    Maximize,
    Size,
    None,
}

fn keyword(arg: &[u8]) -> Keyword {
    let eq = |name: &[u8]| arg.eq_ignore_ascii_case(name);
    if eq(b"dblclick=on") {
        Keyword::DoubleClick
    } else if eq(b"minmaxbutton=on") {
        Keyword::MinMaxButton
    } else if eq(b"minimize=on") {
        Keyword::Minimize
    } else if eq(b"maximize=on") {
        Keyword::Maximize
    } else if arg.len() >= 5 && arg[..5].eq_ignore_ascii_case(b"listb") {
        Keyword::Size
    } else {
        Keyword::None
    }
}

/// `swscanf_s(s, L"%[^=]=%d%[xX]%d", ...) == 4` — `listboxsize=WxH`, and the
/// several other spellings that format also accepts.
///
/// The prefix is anything up to the first `=`, so a misspelt keyword that
/// still starts `listb` is taken as a size and works. Both buffers are 24 wide
/// characters, which caps the prefix and the run of `x`s at 23 apiece; longer
/// makes the conversion fail rather than truncate.
fn scan_size(s: &[u8]) -> Option<(i32, i32)> {
    let eq = s.iter().position(|&b| b == b'=')?;
    if eq == 0 || eq > 23 {
        return None;
    }
    let (w, rest) = scan_int_prefix(&s[eq + 1..])?;
    // `%[xX]` does not skip leading whitespace, where `%d` on either side of
    // it does.
    let xs = rest.iter().take_while(|b| matches!(b, b'x' | b'X')).count();
    if xs == 0 || xs > 23 {
        return None;
    }
    let (h, _) = scan_int_prefix(&rest[xs..])?;
    Some((w, h))
}

/// `%d` against the front of a byte string: the value, and what is left.
fn scan_int_prefix(s: &[u8]) -> Option<(i32, &[u8])> {
    let mut i = 0;
    while i < s.len() && s[i].is_ascii_whitespace() {
        i += 1;
    }
    let start = i;
    if matches!(s.get(i), Some(b'-') | Some(b'+')) {
        i += 1;
    }
    let digits = i;
    while i < s.len() && s[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits {
        return None;
    }
    scan_int(&s[start..i], 10).map(|v| (v, &s[i..]))
}

#[cfg(test)]
mod tests {
    use crate::host::{
        DialogAnchor, DialogEnd, DialogOrigin, ListBoxOpts, RecordingHost, ScriptHost,
    };
    use crate::interp::Interp;
    use crate::vars::VarRef;
    use crate::TtlError;

    fn run_with(host: &mut RecordingHost, src: &str) {
        let mut it = Interp::new("t.ttl", src.as_bytes().to_vec(), host);
        it.run(host);
    }

    fn run(src: &str) -> RecordingHost {
        let mut host = RecordingHost::new();
        run_with(&mut host, src);
        host
    }

    /// The one dialog put up, as `RecordingHost` renders it.
    fn asked(src: &str) -> String {
        let h = run(src);
        assert!(h.errors.is_empty(), "unexpected errors: {:?}", h.errors);
        assert_eq!(h.dialogs.len(), 1, "{:?}", h.dialogs);
        h.dialogs[0].clone()
    }

    fn err_of(src: &str) -> TtlError {
        let h = run(src);
        assert_eq!(h.errors.len(), 1, "expected one error: {:?}", h.errors);
        h.errors[0].0
    }

    #[test]
    fn a_host_with_no_dialogs_answers_unknown_command_rather_than_ok() {
        // The refusing default, which `RecordingHost` deliberately does not
        // take: a macro asking a question of nobody must not read as answered.
        #[derive(Default)]
        struct NoUi(Vec<TtlError>);
        impl ScriptHost for NoUi {
            fn error(&mut self, report: &crate::host::ErrorReport<'_>) -> bool {
                self.0.push(report.error);
                true
            }
        }

        for src in [
            "messagebox 'hi' 'there'",
            "yesnobox 'hi' 'there'",
            "statusbox 'hi' 'there'",
            "closesbox",
            "bringupbox",
            "inputbox 'hi' 'there'",
            "filenamebox 'hi'",
            "dirnamebox 'hi'",
        ] {
            let mut host = NoUi::default();
            let mut it = Interp::new("t.ttl", src.as_bytes().to_vec(), &mut host);
            it.run(&mut host);
            assert_eq!(host.0, vec![TtlError::NotSupported], "{src}");
        }

        // `setdlgpos` is the exception: a preference with no user in it, and
        // upstream's cannot fail either.
        let mut host = NoUi::default();
        let mut it = Interp::new("t.ttl", b"setdlgpos 1 2".to_vec(), &mut host);
        it.run(&mut host);
        assert!(host.0.is_empty());
    }

    #[test]
    fn both_message_arguments_take_an_integer_and_spell_it() {
        assert_eq!(asked("messagebox 42 7"), r#"messagebox "42" "7""#);
    }

    #[test]
    fn special_expands_the_message_and_leaves_the_title_alone() {
        assert_eq!(
            asked(r"messagebox 'a\nb' 'c\nd' 1"),
            "messagebox \"a\\nb\" \"c\\\\nd\"",
            "the title keeps its backslash"
        );
        assert_eq!(
            asked(r"messagebox 'a\nb' 'c' 0"),
            r#"messagebox "a\\nb" "c""#,
            "and without the flag so does the message"
        );
    }

    #[test]
    fn a_message_cut_short_by_a_nul_escape_stops_there() {
        assert_eq!(
            asked(r"messagebox 'keep\0drop' 't' 1"),
            r#"messagebox "keep" "t""#
        );
    }

    #[test]
    fn closing_a_message_box_ends_the_macro_and_ok_does_not() {
        let h = run("messagebox 'a' 'b'\ndispstr 'after'");
        assert_eq!(h.output, b"after");

        let mut host = RecordingHost::new();
        host.msg_replies.push_back(DialogEnd::Closed);
        run_with(&mut host, "messagebox 'a' 'b'\ndispstr 'after'");
        assert!(host.output.is_empty(), "the line after it must not run");
    }

    #[test]
    fn yesnobox_reports_yes_as_one_and_a_close_as_a_no_that_ends_the_run() {
        let mut host = RecordingHost::new();
        host.msg_replies.push_back(DialogEnd::Ok(()));
        run_with(&mut host, "yesnobox 'a' 'b'\ndispstr result");
        assert_eq!(host.output, b"1");

        let mut host = RecordingHost::new();
        host.msg_replies.push_back(DialogEnd::Cancel);
        run_with(&mut host, "yesnobox 'a' 'b'\ndispstr result");
        assert_eq!(host.output, b"0", "No is 0 and carries on");

        // Closing it ends the run, so `result` cannot be read from a later
        // line — upstream writes it anyway, on the way out.
        let mut host = RecordingHost::new();
        host.msg_replies.push_back(DialogEnd::Closed);
        let src = b"yesnobox 'a' 'b'\ndispstr 'after'".to_vec();
        let mut it = Interp::new("t.ttl", src, &mut host);
        it.run(&mut host);
        assert!(host.output.is_empty(), "the line after it must not run");
        let (id, _) = it.vars.find(b"result").unwrap();
        assert_eq!(it.vars.int_at(VarRef::Scalar(id)), 0);
    }

    #[test]
    fn the_status_box_is_one_window_and_closesbox_takes_nothing() {
        let h = run("statusbox 'one' 't'\nstatusbox 'two' 't'\nbringupbox\nclosesbox");
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(
            h.dialogs,
            vec![
                r#"statusbox "one" "t""#,
                r#"statusbox "two" "t""#,
                "bringupbox",
                "closesbox",
            ]
        );
        assert_eq!(err_of("closesbox 1"), TtlError::Syntax);
    }

    // ---- listbox ----

    fn list_src(tail: &str) -> String {
        format!("strdim a 3\na[0] = 'x'\na[1] = 'y'\na[2] = 'z'\nlistbox 'm' 't' a {tail}")
    }

    #[test]
    fn a_listbox_hands_over_every_item_and_reports_the_choice() {
        let mut host = RecordingHost::new();
        host.list_replies.push_back(DialogEnd::Ok(2));
        run_with(&mut host, &(list_src("") + "\ndispstr result"));
        assert_eq!(host.output, b"2");
        assert_eq!(
            host.dialogs[0],
            r#"listbox "m" "t" ["x", "y", "z"] sel=0 ListBoxOpts { double_click: false, min_max_button: false, minimized: false, maximized: false, size: None }"#
        );
    }

    #[test]
    fn cancel_is_minus_one_and_close_is_minus_two_and_ends_the_run() {
        let mut host = RecordingHost::new();
        host.list_replies.push_back(DialogEnd::Cancel);
        run_with(&mut host, &(list_src("") + "\ndispstr result"));
        assert_eq!(host.output, b"-1");

        let mut host = RecordingHost::new();
        host.list_replies.push_back(DialogEnd::Closed);
        run_with(&mut host, &(list_src("") + "\ndispstr 'after'"));
        assert!(host.output.is_empty());
    }

    #[test]
    fn the_selected_index_arrives_as_a_string_and_is_folded_when_out_of_range() {
        let h = run(&list_src("1"));
        assert!(h.dialogs[0].contains("sel=1"), "{}", h.dialogs[0]);
        let h = run(&list_src("99"));
        assert!(h.dialogs[0].contains("sel=0"), "an index past the end");
        let h = run(&list_src("-1"));
        assert!(h.dialogs[0].contains("sel=0"), "and a negative one");
    }

    #[test]
    fn the_keywords_are_case_insensitive_and_the_last_of_the_pair_wins() {
        let h = run(&list_src("'DblClick=On' 'minimize=on' 'maximize=on'"));
        let opts = ListBoxOpts {
            double_click: true,
            maximized: true,
            ..Default::default()
        };
        assert!(
            h.dialogs[0].contains(&format!("{opts:?}")),
            "{}",
            h.dialogs[0]
        );
    }

    #[test]
    fn listboxsize_is_matched_on_five_characters_so_a_typo_still_works() {
        // `_wcsnicmp(..., L"listboxsize=", 5)` compares "listb" and nothing
        // more, so this arm swallows anything starting that way — and then
        // insists it parse as a size.
        let h = run(&list_src("'listbee=60x20'"));
        assert!(
            h.dialogs[0].contains("size: Some((60, 20))"),
            "{}",
            h.dialogs[0]
        );
        assert_eq!(err_of(&list_src("'listbee'")), TtlError::Syntax);
        assert_eq!(err_of(&list_src("'listboxsize=wide'")), TtlError::Syntax);
        assert_eq!(err_of(&list_src("'listboxsize=-1x2'")), TtlError::Syntax);
    }

    #[test]
    fn an_unrecognised_option_that_is_not_a_number_is_a_syntax_error() {
        assert_eq!(err_of(&list_src("'dblclick=off'")), TtlError::Syntax);
    }

    #[test]
    fn an_empty_array_answers_minus_one_as_well_as_erroring() {
        let mut host = RecordingHost::new();
        host.stop_on_error = false;
        run_with(&mut host, "strdim a 0\nlistbox 'm' 't' a\ndispstr result");
        assert_eq!(host.errors.len(), 1);
        assert_eq!(host.errors[0].0, TtlError::Syntax);
        assert!(host.dialogs.is_empty(), "no window for an empty list");
        assert_eq!(host.output, b"-1");
    }

    // ---- inputbox ----

    #[test]
    fn an_integer_third_argument_is_the_special_flag_and_not_a_default() {
        assert_eq!(
            asked(r"inputbox 'a\nb' 't' 1"),
            "inputbox \"a\\nb\" \"t\" \"\"",
            "the line is rewound and the 1 read again as the flag"
        );
        assert_eq!(
            asked(r"inputbox 'a\nb' 't' 'd'"),
            r#"inputbox "a\\nb" "t" "d""#,
            "a string third argument is the default, and no expansion"
        );
        assert_eq!(
            asked(r"inputbox 'a\nb' 't' 'd' 1"),
            "inputbox \"a\\nb\" \"t\" \"d\"",
            "and both together"
        );
    }

    #[test]
    fn passwordbox_has_no_default_so_its_third_argument_is_the_flag() {
        assert_eq!(
            asked(r"passwordbox 'a\nb' 't' 1"),
            "passwordbox \"a\\nb\" \"t\" \"\""
        );
    }

    #[test]
    fn what_the_user_typed_lands_in_inputstr() {
        let mut host = RecordingHost::new();
        host.input_replies
            .push_back(DialogEnd::Ok(b"typed".to_vec()));
        run_with(&mut host, "inputbox 'a' 't'\ndispstr inputstr");
        assert_eq!(host.output, b"typed");
    }

    #[test]
    fn escape_leaves_inputstr_empty_and_closing_ends_the_run() {
        let mut host = RecordingHost::new();
        host.input_replies.push_back(DialogEnd::Cancel);
        run_with(
            &mut host,
            "inputstr = 'stale'\ninputbox 'a' 't'\ndispstr 'x' inputstr",
        );
        assert_eq!(
            host.output, b"x",
            "cleared before the dialog, and not refilled"
        );

        let mut host = RecordingHost::new();
        host.input_replies.push_back(DialogEnd::Closed);
        run_with(&mut host, "inputbox 'a' 't'\ndispstr 'after'");
        assert!(host.output.is_empty());
    }

    #[test]
    fn no_dialog_opens_when_inputstr_is_not_a_string_variable() {
        // Not reachable from a macro: `InitTTL` makes `inputstr` a string and
        // TTL will not retype a variable — `inputstr = 5` is a type mismatch
        // and `strdim inputstr` a syntax error. But `TTLInputBox`,
        // `TTLFilenameBox` and `TTLDirnameBox` all guard on it and skip the
        // dialog entirely, so the guard is reproduced and tested from
        // underneath the language rather than through it.
        for src in ["inputbox 'a' 't'", "filenamebox 'f'", "dirnamebox 'd'"] {
            let mut host = RecordingHost::new();
            let mut it = Interp::new("t.ttl", src.as_bytes().to_vec(), &mut host);
            it.vars.new_int(b"inputstr", 0);
            it.run(&mut host);
            assert!(host.errors.is_empty(), "{src}: {:?}", host.errors);
            assert!(host.dialogs.is_empty(), "{src}: {:?}", host.dialogs);
        }
    }

    // ---- filenamebox and dirnamebox ----

    #[test]
    fn filenamebox_reports_through_result_and_inputstr() {
        let mut host = RecordingHost::new();
        host.file_replies.push_back(Some(b"/tmp/f".to_vec()));
        run_with(
            &mut host,
            "filenamebox 'pick' 1 '/tmp'\ndispstr result inputstr",
        );
        assert_eq!(host.output, b"1/tmp/f");
        assert_eq!(host.dialogs[0], r#"filenamebox "pick" save=1 "/tmp""#);

        let mut host = RecordingHost::new();
        host.file_replies.push_back(None);
        run_with(&mut host, "filenamebox 'pick'\ndispstr result '|' inputstr");
        assert_eq!(host.output, b"0|");
        assert_eq!(host.dialogs[0], r#"filenamebox "pick" save=0 """#);
    }

    #[test]
    fn dirnamebox_takes_no_type_argument() {
        let mut host = RecordingHost::new();
        host.file_replies.push_back(Some(b"/tmp".to_vec()));
        run_with(&mut host, "dirnamebox 'pick' '/'\ndispstr result inputstr");
        assert_eq!(host.output, b"1/tmp");
        assert_eq!(host.dialogs[0], r#"dirnamebox "pick" "/""#);
        assert_eq!(err_of("dirnamebox 'a' 'b' 'c'"), TtlError::Syntax);
    }

    // ---- setdlgpos ----

    #[test]
    fn setdlgpos_has_four_shapes() {
        let h = run("setdlgpos");
        assert_eq!(h.dialogs, vec!["setdlgpos default"]);
        assert_eq!(h.dialog_pos, None);

        let h = run("setdlgpos 10 20");
        let p = h.dialog_pos.unwrap();
        assert_eq!(
            (p.x, p.y, p.anchor, p.offset_x, p.offset_y),
            (10, 20, None, 0, 0)
        );

        let h = run("setdlgpos 0 0 10");
        let p = h.dialog_pos.unwrap();
        assert_eq!(
            p.anchor,
            Some((DialogAnchor::Center, DialogOrigin::VtWindow))
        );

        let h = run("setdlgpos 0 0 2 (-5) 6");
        let p = h.dialog_pos.unwrap();
        assert_eq!(
            p.anchor,
            Some((DialogAnchor::TopRight, DialogOrigin::Display))
        );
        assert_eq!((p.offset_x, p.offset_y), (-5, 6));
    }

    #[test]
    fn a_negative_offset_needs_its_brackets() {
        // The language's rule rather than this command's: an argument is a
        // whole expression, so `2 -5` is `2 - 5` and the position becomes -3.
        // Every command taking two integers in a row has this shape.
        assert_eq!(err_of("setdlgpos 0 0 2 -5 6"), TtlError::Syntax);
    }

    #[test]
    fn a_position_outside_one_to_ten_is_a_syntax_error() {
        assert_eq!(err_of("setdlgpos 0 0 0"), TtlError::Syntax);
        assert_eq!(err_of("setdlgpos 0 0 11"), TtlError::Syntax);
        assert_eq!(err_of("setdlgpos 0"), TtlError::Syntax);
    }

    #[test]
    fn the_offsets_come_in_a_pair_and_only_after_a_position() {
        assert_eq!(err_of("setdlgpos 0 0 1 5"), TtlError::Syntax);
    }
}

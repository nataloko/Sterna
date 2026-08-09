//! The interpreter — `ttl.cpp`'s `ExecCmnd` and `Exec`.
//!
//! One line is read, parsed and thrown away; then the next. Blocks are not
//! skipped by seeking past them but by executing every line inside them with a
//! counter that suppresses the effect, which is why a nested `if` still has to
//! be recognised while its enclosing branch is being ignored — and why the four
//! skip flags at the top of [`Interp::exec_cmnd`] are checked before anything
//! else, in the order upstream checks them.
//!
//! The one structural departure is [`ScriptHost`]: upstream's `wait` parks the
//! macro in a state machine driven by the window's message loop, because both
//! run on the same thread. Here the interpreter has its own thread and a host
//! call may simply block, so there is no `TTLStatus` for waiting — only for
//! having finished.

use crate::buffer::Buffers;
use crate::error::{TtlError, TtlResult};
use crate::expr::{self, Eval};
use crate::files::Files;
use crate::host::{ErrorReport, ScriptHost};
use crate::lexer::{check_reserved, Lexer};
use crate::pathcmds::NUM_DIR_HANDLE;
use crate::rsv::Rsv;
use crate::vars::{VarRef, VarType, Vars};
use crate::wait::{RecvLine, WaitSet};

/// A guard where upstream leans on `calloc` failing.
///
/// `TTLDim` passes its size straight to `calloc`, so a negative one becomes a
/// huge `size_t`, the allocation fails and the macro gets `ErrFewMemory`. The
/// same answer is given here, without asking the allocator for it first.
const MAX_ARRAY_LEN: i32 = 1 << 20;

/// A TTL macro, mid-run.
pub struct Interp {
    pub lx: Lexer,
    pub vars: Vars,
    pub buf: Buffers,
    /// `IfNest` — how many `if ... then` blocks are open.
    if_nest: u32,
    /// `ElseFlag` — skipping to the matching `else`, `elseif` or `endif`.
    else_flag: u32,
    /// `EndIfFlag` — skipping to the matching `endif`, no `else` will do.
    end_if_flag: u32,
    /// `ParseAgain` — `execcmnd` has put a new line in the buffer; run it
    /// instead of reading one.
    parse_again: bool,
    /// `TTLStatus == IdTTLEnd`.
    pub(crate) ended: bool,
    /// The `wait` patterns, and the line the far end is part-way through.
    pub(crate) waits: WaitSet,
    pub(crate) recv_line: RecvLine,
    /// The sixteen file handles and the directory relative paths hang off.
    pub(crate) files: Files,
    /// `DirHandle` — eight directory walks at once, each holding the names it
    /// has not handed out yet. Upstream keeps a live `FindFirstFile` handle
    /// and reads the directory lazily; the whole listing is taken up front
    /// here, which a macro can only tell apart by creating a file in a
    /// directory it is already walking.
    pub(crate) finds: [Option<Vec<Vec<u8>>>; NUM_DIR_HANDLE],
    /// `clipb2var`'s copy of the clipboard — `cbbuff`/`cblen`, the two
    /// `static`s in `ttl_gui.cpp:59`. Only an offset of 0 refills it; every
    /// other offset reads out of it. `None` is upstream's NULL, which is what
    /// a clipboard that could not be read leaves behind.
    pub(crate) clipboard: Option<Vec<u8>>,
}

impl Interp {
    /// Load a macro.
    ///
    /// `name` names it in error reports, and — when it is an absolute path —
    /// is also where `CurrentDir` starts, which is what a relative filename in
    /// a file command resolves against. `TTLStart` does the same
    /// (`ttl.cpp:267`).
    pub fn new(name: impl Into<String>, body: Vec<u8>, host: &mut dyn ScriptHost) -> Self {
        let name = name.into();
        let files = Files::new(&name);
        let mut it = Interp {
            lx: Lexer::new(),
            vars: Vars::new(),
            buf: Buffers::new(),
            if_nest: 0,
            else_flag: 0,
            end_if_flag: 0,
            parse_again: false,
            ended: false,
            waits: WaitSet::new(),
            recv_line: RecvLine::new(),
            files,
            finds: Default::default(),
            clipboard: None,
        };
        it.buf.open(name, body);
        it.define_system_variables(&[]);
        it.register_labels(host);
        it
    }

    /// `InitTTL`'s system variables (`ttl.cpp:199-235`).
    ///
    /// `params` is the argument list *without* the macro's own name, which goes
    /// in `param1` and `params[1]`. Upstream counts the name in `paramcnt`, and
    /// makes `paramcnt` at least 1 even when nothing was passed.
    pub fn define_system_variables(&mut self, params: &[Vec<u8>]) {
        let v = &mut self.vars;
        v.new_int(b"result", 0);
        v.new_int(b"timeout", 0);
        v.new_int(b"mtimeout", 0);
        v.new_str(b"inputstr", b"");
        v.new_str(b"matchstr", b"");
        for i in 1..=9 {
            v.new_str(format!("groupmatchstr{i}").as_bytes(), b"");
        }

        let name = self.buf.file_name().to_owned().into_bytes();
        let count = (params.len() + 1).max(1);
        v.new_int(b"paramcnt", count as i32);
        v.new_str(b"param1", &name);
        for i in 2..=9 {
            let val = params.get(i - 2).map(|s| s.as_slice()).unwrap_or(b"");
            v.new_str(format!("param{i}").as_bytes(), val);
        }

        // `params[0]` exists and is never written to, which is upstream's:
        // the array is `ParamCnt + 1` long and the loop starts the names at 1.
        let id = v.new_str_array(b"params", count + 1);
        v.set_str(VarRef::Elem(id, 1), &name);
        for (i, p) in params.iter().enumerate() {
            v.set_str(VarRef::Elem(id, i + 2), p);
        }
    }

    pub fn ended(&self) -> bool {
        self.ended
    }

    /// Run to the end. Returns the number of lines executed, which is only
    /// interesting to a test.
    pub fn run(&mut self, host: &mut dyn ScriptHost) -> usize {
        let mut n = 0;
        while self.step(host) {
            n += 1;
        }
        n
    }

    /// `Exec` — read and run one line. False once the macro has finished.
    pub fn step(&mut self, host: &mut dyn ScriptHost) -> bool {
        if self.ended {
            return false;
        }
        if host.cancelled() {
            self.ended = true;
            return false;
        }
        if !self.parse_again {
            let (ok, closed) = self.buf.new_line(&mut self.lx);
            self.drop_labels(closed);
            if !ok {
                self.ended = true;
                return false;
            }
        }
        self.parse_again = false;

        if let Err(e) = self.exec_cmnd(host) {
            self.report(host, e);
        }
        !self.ended
    }

    fn drop_labels(&mut self, closed: Option<usize>) {
        if let Some(level) = closed {
            self.vars.del_labels_from(level);
        }
    }

    /// `DispErr` — hand the error to the host, and end the run if it says so.
    fn report(&mut self, host: &mut dyn ScriptHost, error: TtlError) {
        let start = self.lx.parse_ptr;
        let mut end = self.lx.ptr;
        if start == end {
            end = self.lx.len();
        }
        let stop = host.error(&ErrorReport {
            error,
            line: self.lx.line(),
            line_no: self.buf.line_no(),
            start,
            end,
            file: self.buf.file_name(),
        });
        if stop {
            self.ended = true;
        }
    }

    /// `RegisterLabels` — walk the top file once and record every `:name`.
    ///
    /// The rest of each line is scanned too, skipping over string literals, so
    /// that the C-comment state machine sees a `/*` that is real and not one
    /// inside a quoted string. Errors are reported and the walk continues.
    fn register_labels(&mut self, host: &mut dyn ScriptHost) {
        let level = self.buf.nest();
        self.buf.rewind();
        while self.buf.raw_line(&mut self.lx) {
            let mut err = None;
            if self.lx.first_char() == b':' {
                match self.lx.label_name() {
                    Some(name) if self.lx.first_char() == 0 => {
                        if self.vars.find(&name).is_some() {
                            err = Some(TtlError::LabelAlreadyDef);
                        } else {
                            let pos = self.buf.pos();
                            self.vars.new_label(&name, pos, level);
                        }
                    }
                    _ => err = Some(TtlError::Syntax),
                }
            } else {
                self.lx.back();
            }

            loop {
                let b = self.lx.first_char();
                if b == 0 {
                    break;
                }
                if b == b'"' || b == b'\'' || b == b'#' {
                    self.lx.back();
                    let _ = self.lx.string();
                }
            }

            if let Some(e) = err {
                self.report(host, e);
            }
        }
        if !self.lx.comment_closed() {
            self.report(host, TtlError::CloseComment);
        }
        self.buf.rewind();
    }

    /// `ExecCmnd`.
    fn exec_cmnd(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let word = self.lx.reserved_word();

        // Skipping the body of a `while`/`until`/`do` whose condition was false.
        // Only the openers and closers matter; nothing else is even looked at.
        if self.buf.end_while_flag > 0 {
            if let Some(w) = word {
                match w {
                    Rsv::While | Rsv::Until | Rsv::Do => self.buf.end_while_flag += 1,
                    Rsv::EndWhile | Rsv::EndUntil | Rsv::Loop => self.buf.end_while_flag -= 1,
                    _ => {}
                }
            }
            return Ok(());
        }

        // Skipping the rest of a loop body after `break` or `continue`. Unlike
        // the case above this one does track `if`, because a `break` inside a
        // branch has to leave `IfNest` where the enclosing `endif` expects it.
        if self.buf.break_flag > 0 {
            let mut err = Ok(());
            if let Some(w) = word {
                match w {
                    Rsv::If => {
                        if self.check_then(&mut err) {
                            self.if_nest += 1;
                        }
                    }
                    Rsv::EndIf => {
                        if self.if_nest < 1 {
                            err = Err(TtlError::InvalidCtl);
                        } else {
                            self.if_nest -= 1;
                        }
                    }
                    Rsv::For | Rsv::While | Rsv::Until | Rsv::Do => self.buf.break_flag += 1,
                    Rsv::Next | Rsv::EndWhile | Rsv::EndUntil | Rsv::Loop => {
                        self.buf.break_flag -= 1
                    }
                    _ => {}
                }
            }
            if self.buf.break_flag > 0 || !self.buf.continue_flag {
                return err;
            }
            // `continue` lands here on the closing line, and falls through so
            // that the `endwhile` or `next` actually runs and jumps back.
            self.buf.continue_flag = false;
        }

        if self.end_if_flag > 0 {
            let mut err = Ok(());
            match word {
                Some(Rsv::If) if self.check_then(&mut err) => self.end_if_flag += 1,
                Some(Rsv::EndIf) => self.end_if_flag -= 1,
                _ => {}
            }
            return err;
        }

        if self.else_flag > 0 {
            let mut err = Ok(());
            match word {
                // A nested `if` here increments `EndIfFlag`, not `ElseFlag` —
                // its `else` belongs to it, and must not end this skip.
                Some(Rsv::If) if self.check_then(&mut err) => self.end_if_flag += 1,
                Some(Rsv::Else) => self.else_flag -= 1,
                Some(Rsv::ElseIf) => match self.check_else_if() {
                    Ok(v) => {
                        if v != 0 {
                            self.else_flag -= 1;
                        }
                    }
                    Err(e) => err = Err(e),
                },
                Some(Rsv::EndIf) => {
                    self.else_flag -= 1;
                    if self.else_flag == 0 {
                        self.if_nest -= 1;
                    }
                }
                _ => {}
            }
            return err;
        }

        match word {
            Some(w) => self.command(host, w),
            None => self.assignment(),
        }
    }

    /// `CheckThen` — scan forward for the word `then`, ignoring everything that
    /// is not a letter on the way. It is used only while skipping, where the
    /// condition has not been evaluated and its text is meaningless.
    fn check_then(&mut self, err: &mut TtlResult<()>) -> bool {
        loop {
            let name = loop {
                let b = self.lx.first_char();
                if b == 0 {
                    return false;
                }
                if b.is_ascii_alphabetic() || b == b'_' {
                    self.lx.back();
                    match self.lx.identifier() {
                        Some(n) => break n,
                        None => return false,
                    }
                }
            };
            if name.eq_ignore_ascii_case(b"then") {
                if self.lx.first_char() != 0 {
                    *err = Err(TtlError::Syntax);
                }
                return true;
            }
        }
    }

    /// `CheckElseIf` — the condition of an `elseif`, which must end in `then`.
    fn check_else_if(&mut self) -> TtlResult<i32> {
        let v = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        if self.lx.reserved_word() != Some(Rsv::Then) || self.lx.first_char() != 0 {
            return Err(TtlError::Syntax);
        }
        Ok(v)
    }

    /// The `else` arm of `ExecCmnd` — `name = value`, the only statement that
    /// is not a command.
    fn assignment(&mut self) -> TtlResult<()> {
        let Some(name) = self.lx.identifier() else {
            return Err(TtlError::Syntax);
        };

        // A bad index is replaced by `ErrNotSupported` rather than reported,
        // because upstream's `if (!Err && GetFirstChar() == '=')` short-circuits
        // into an `else` that overwrites the error. `a[1 = 2` says "unknown
        // command", not "] expected".
        let with_index = match expr::index(&mut self.lx, &mut self.vars) {
            Ok(i) => i,
            Err(_) => return Err(TtlError::NotSupported),
        };
        if self.lx.first_char() != b'=' {
            return Err(TtlError::NotSupported);
        }

        let literal = self.lx.string()?;
        let val = match &literal {
            Some(_) => None,
            None => match expr::get_expression(&mut self.lx, &mut self.vars)? {
                Some(v) => Some(v),
                None => return Err(TtlError::Syntax),
            },
        };
        let val_type = match (&literal, &val) {
            (Some(_), _) => VarType::String,
            (None, Some(v)) => v.var_type(),
            (None, None) => unreachable!(),
        };

        match self.vars.find(&name) {
            Some((id, var_type)) => {
                let (target, var_type) = match with_index {
                    Some(i) => match var_type {
                        VarType::IntArray => (self.vars.elem(id, i)?, VarType::Integer),
                        VarType::StrArray => (self.vars.elem(id, i)?, VarType::String),
                        _ => return Err(TtlError::Syntax),
                    },
                    None => (VarRef::Scalar(id), var_type),
                };
                if var_type != val_type {
                    return Err(TtlError::TypeMismatch);
                }
                match (val_type, &literal, val) {
                    (VarType::Integer, _, Some(Eval::Int(v))) => self.vars.set_int(target, v),
                    (VarType::String, Some(s), _) => self.vars.set_str(target, &s.clone()),
                    (VarType::String, None, Some(Eval::Str(src))) => {
                        let s = self.vars.str_at(src).to_vec();
                        self.vars.set_str(target, &s);
                    }
                    // Two arrays of the same kind pass the type test and then
                    // fall off the end of upstream's switch. Assigning one array
                    // to another is a syntax error, not a copy.
                    _ => return Err(TtlError::Syntax),
                }
            }
            // `a[0] = 1` cannot create `a`: there is no size to create it with.
            None if with_index.is_some() => return Err(TtlError::Syntax),
            None => match (val_type, &literal, val) {
                (VarType::Integer, _, Some(Eval::Int(v))) => {
                    self.vars.new_int(&name, v);
                }
                (VarType::String, Some(s), _) => {
                    self.vars.new_str(&name, s);
                }
                (VarType::String, None, Some(Eval::Str(src))) => {
                    let s = self.vars.str_at(src).to_vec();
                    self.vars.new_str(&name, &s);
                }
                _ => return Err(TtlError::TooManyVar),
            },
        }

        if self.lx.first_char() != 0 {
            return Err(TtlError::Syntax);
        }
        Ok(())
    }

    /// The big `switch`. Everything not listed here is not implemented yet and
    /// says so with the code upstream uses for a word it does not know.
    fn command(&mut self, host: &mut dyn ScriptHost, w: Rsv) -> TtlResult<()> {
        if let Some(r) = self.string_command(host, w) {
            return r;
        }
        if let Some(r) = self.connection_command(host, w) {
            return r;
        }
        if let Some(r) = self.session_command(host, w) {
            return r;
        }
        if let Some(r) = self.file_command(host, w) {
            return r;
        }
        if let Some(r) = self.path_command(host, w) {
            return r;
        }
        if let Some(r) = self.dialog_command(host, w) {
            return r;
        }
        if let Some(r) = self.log_command(host, w) {
            return r;
        }
        if let Some(r) = self.checksum_command(w) {
            return r;
        }
        if let Some(r) = self.terminal_command(host, w) {
            return r;
        }
        if let Some(r) = self.env_command(host, w) {
            return r;
        }
        if let Some(r) = self.clock_command(host, w) {
            return r;
        }
        match w {
            // --- control flow ---
            Rsv::If => self.cmd_if(host),
            Rsv::Then => Err(TtlError::Syntax),
            Rsv::Else => self.cmd_else(),
            Rsv::ElseIf => self.cmd_else_if(),
            Rsv::EndIf => self.cmd_end_if(),
            Rsv::While => self.cmd_while(true),
            Rsv::Until => self.cmd_while(false),
            Rsv::EndWhile => self.cmd_end_while(true),
            Rsv::EndUntil => self.cmd_end_while(false),
            Rsv::Do => self.cmd_do(),
            Rsv::Loop => self.cmd_loop(),
            Rsv::For => self.cmd_for(),
            Rsv::Next => self.cmd_next(),
            Rsv::Break | Rsv::Continue => self.cmd_break(w == Rsv::Continue),
            Rsv::Goto => self.cmd_goto(),
            Rsv::Call => self.cmd_call(),
            Rsv::Return => self.cmd_return(),
            Rsv::End => self.cmd_end(),
            Rsv::Exit => self.cmd_exit(),
            Rsv::Include => self.cmd_include(host),
            Rsv::ExecCmnd => self.cmd_exec_cmnd(),

            // --- variables ---
            Rsv::IntDim => self.cmd_dim(false),
            Rsv::StrDim => self.cmd_dim(true),
            Rsv::IfDefined => {
                let t = expr::get_var_type(&mut self.lx, &mut self.vars);
                self.set_result(t.code());
                Ok(())
            }

            // --- the terminal, so far ---
            Rsv::DispStr => self.cmd_disp_str(host),
            Rsv::SetExitCode => {
                let code = expr::get_int_val(&mut self.lx, &mut self.vars)?;
                self.end_of_line()?;
                host.set_exit_code(code);
                Ok(())
            }

            _ => Err(TtlError::NotSupported),
        }
    }

    /// `SetResult` — write `result`, but only if it is still an integer. A
    /// macro that assigns a string to `result` silently stops being told
    /// anything, which is upstream's behaviour and not obviously wrong.
    pub(crate) fn set_result(&mut self, code: i32) {
        if let Some((id, VarType::Integer)) = self.vars.find(b"result") {
            self.vars.set_int(VarRef::Scalar(id), code);
        }
    }

    pub(crate) fn end_of_line(&mut self) -> TtlResult<()> {
        if self.lx.first_char() != 0 {
            Err(TtlError::Syntax)
        } else {
            Ok(())
        }
    }

    // ---- control flow ----

    /// `TTLIf` — two statements sharing a name. With `then` it opens a block;
    /// without, the rest of the line is a command to run conditionally.
    fn cmd_if(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let Some(v) = expr::get_expression(&mut self.lx, &mut self.vars)? else {
            return Err(TtlError::Syntax);
        };
        let Eval::Int(v) = v else {
            return Err(TtlError::TypeMismatch);
        };

        let save = self.lx.ptr;
        if self.lx.reserved_word() == Some(Rsv::Then) {
            self.end_of_line()?;
            self.if_nest += 1;
            if v == 0 {
                self.else_flag = 1;
            }
            Ok(())
        } else {
            self.lx.ptr = save;
            if !self.lx.parameter_given() {
                return Err(TtlError::Syntax);
            }
            if v == 0 {
                return Ok(());
            }
            self.exec_cmnd(host)
        }
    }

    fn cmd_else(&mut self) -> TtlResult<()> {
        self.end_of_line()?;
        if self.if_nest < 1 {
            return Err(TtlError::InvalidCtl);
        }
        // The `then` branch ran, so everything to the `endif` is skipped — and
        // `IfNest` comes down now because the `endif` will be swallowed.
        self.if_nest -= 1;
        self.end_if_flag = 1;
        Ok(())
    }

    fn cmd_else_if(&mut self) -> TtlResult<()> {
        self.check_else_if()?;
        if self.if_nest < 1 {
            return Err(TtlError::InvalidCtl);
        }
        self.if_nest -= 1;
        self.end_if_flag = 1;
        Ok(())
    }

    fn cmd_end_if(&mut self) -> TtlResult<()> {
        self.end_of_line()?;
        if self.if_nest < 1 {
            return Err(TtlError::InvalidCtl);
        }
        self.if_nest -= 1;
        Ok(())
    }

    /// `TTLWhile` — and `until`, which is the same code with the test flipped.
    /// A bare `while` with no condition is an infinite loop; a bare `until` is
    /// a block that runs once.
    fn cmd_while(&mut self, mode: bool) -> TtlResult<()> {
        let mut val = i32::from(mode);
        if self.lx.parameter_given() {
            val = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        }
        self.end_of_line()?;
        if (val != 0) == mode {
            self.buf.set_while_loop()
        } else {
            self.buf.end_while_loop();
            Ok(())
        }
    }

    fn cmd_end_while(&mut self, mode: bool) -> TtlResult<()> {
        let mut val = i32::from(mode);
        if self.lx.parameter_given() {
            val = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        }
        self.end_of_line()?;
        let closed = self.buf.back_to_while((val != 0) == mode)?;
        self.drop_labels(closed);
        Ok(())
    }

    /// `TTLDo` — `do`, `do while <expr>` or `do until <expr>`.
    fn cmd_do(&mut self) -> TtlResult<()> {
        let val = self.do_condition()?;
        if val != 0 {
            self.buf.set_while_loop()
        } else {
            self.buf.end_while_loop();
            Ok(())
        }
    }

    fn cmd_loop(&mut self) -> TtlResult<()> {
        let val = self.do_condition()?;
        let closed = self.buf.back_to_while(val != 0)?;
        self.drop_labels(closed);
        Ok(())
    }

    fn do_condition(&mut self) -> TtlResult<i32> {
        if !self.lx.parameter_given() {
            return Ok(1);
        }
        let val = match self.lx.reserved_word() {
            Some(Rsv::While) => expr::get_int_val(&mut self.lx, &mut self.vars)?,
            Some(Rsv::Until) => i32::from(expr::get_int_val(&mut self.lx, &mut self.vars)? == 0),
            _ => return Err(TtlError::Syntax),
        };
        self.end_of_line()?;
        Ok(val)
    }

    /// `TTLFor` — the loop variable steps *towards* the end value, so a `for`
    /// counts down as readily as up and `for i = 3 3` runs exactly once.
    fn cmd_for(&mut self) -> TtlResult<()> {
        let var = expr::get_int_var(&mut self.lx, &mut self.vars)?;
        let start = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        let end = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;

        if !self.buf.check_next() {
            self.buf.set_for_loop()?;
            self.vars.set_int(var, start);
            if start == end {
                self.buf.last_for_loop();
            }
        } else {
            let mut i = self.vars.int_at(var);
            match i.cmp(&end) {
                std::cmp::Ordering::Less => i += 1,
                std::cmp::Ordering::Greater => i -= 1,
                std::cmp::Ordering::Equal => {}
            }
            self.vars.set_int(var, i);
            if i == end {
                self.buf.last_for_loop();
            }
        }
        Ok(())
    }

    fn cmd_next(&mut self) -> TtlResult<()> {
        self.end_of_line()?;
        let closed = self.buf.next_loop()?;
        self.drop_labels(closed);
        Ok(())
    }

    fn cmd_break(&mut self, is_continue: bool) -> TtlResult<()> {
        let closed = self.buf.break_loop(is_continue)?;
        self.drop_labels(closed);
        Ok(())
    }

    fn cmd_goto(&mut self) -> TtlResult<()> {
        let (pos, level) = self.label_operand()?;
        let closed = self.buf.jump_to(pos, level);
        self.drop_labels(closed);
        Ok(())
    }

    fn cmd_call(&mut self) -> TtlResult<()> {
        let (pos, level) = self.label_operand()?;
        self.buf.call_to(pos, level)
    }

    fn label_operand(&mut self) -> TtlResult<(usize, usize)> {
        let Some(name) = self.lx.label_name() else {
            return Err(TtlError::Syntax);
        };
        if self.lx.first_char() != 0 {
            return Err(TtlError::Syntax);
        }
        match self.vars.find(&name) {
            Some((id, VarType::Label)) => Ok(self.vars.label(id).unwrap()),
            _ => Err(TtlError::LabelReq),
        }
    }

    fn cmd_return(&mut self) -> TtlResult<()> {
        self.end_of_line()?;
        let closed = self.buf.return_from_sub()?;
        self.drop_labels(closed);
        Ok(())
    }

    fn cmd_end(&mut self) -> TtlResult<()> {
        self.end_of_line()?;
        self.ended = true;
        Ok(())
    }

    /// `TTLExit` — leave the included file, or the macro if there is none.
    fn cmd_exit(&mut self) -> TtlResult<()> {
        self.end_of_line()?;
        let level = self.buf.nest();
        if self.buf.exit_buffer() {
            self.vars.del_labels_from(level);
        } else {
            self.ended = true;
        }
        Ok(())
    }

    fn cmd_include(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let path = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let body = host.read_macro(&path)?;
        let name = String::from_utf8_lossy(&path).into_owned();
        if !self.buf.include(name, body) {
            return Err(TtlError::CantOpen);
        }
        self.register_labels(host);
        Ok(())
    }

    /// `TTLExecCmnd` — replace the line with the contents of a string and run
    /// that instead. The one place a TTL program can build a statement.
    fn cmd_exec_cmnd(&mut self) -> TtlResult<()> {
        let next = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        self.end_of_line()?;
        self.lx.set_line(&next);
        let b = self.lx.first_char();
        self.lx.back();
        self.parse_again = b != 0 && b != b':' && b != b';';
        Ok(())
    }

    // ---- variables ----

    /// `TTLDim` — `intdim name size` / `strdim name size`.
    fn cmd_dim(&mut self, string: bool) -> TtlResult<()> {
        let Some(name) = self.lx.identifier() else {
            return Err(TtlError::Syntax);
        };
        if check_reserved(&name).is_some() || self.vars.find(&name).is_some() {
            return Err(TtlError::Syntax);
        }
        let size = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        if !(0..=MAX_ARRAY_LEN).contains(&size) {
            return Err(TtlError::FewMemory);
        }
        if string {
            self.vars.new_str_array(&name, size as usize);
        } else {
            self.vars.new_int_array(&name, size as usize);
        }
        Ok(())
    }

    // ---- the terminal ----

    fn cmd_disp_str(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        // `dispstr` takes any number of strings and integers and joins them.
        let mut out = Vec::new();
        while self.lx.parameter_given() {
            out.extend_from_slice(&expr::get_str_val2(&mut self.lx, &mut self.vars, true)?);
        }
        host.disp_str(&out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::RecordingHost;

    fn run(src: &str) -> RecordingHost {
        let mut host = RecordingHost::new();
        let mut it = Interp::new("t.ttl", src.as_bytes().to_vec(), &mut host);
        it.run(&mut host);
        host
    }

    fn out(src: &str) -> String {
        let h = run(src);
        assert!(h.errors.is_empty(), "unexpected errors: {:?}", h.errors);
        String::from_utf8_lossy(&h.output).into_owned()
    }

    fn err(src: &str) -> TtlError {
        let h = run(src);
        assert!(!h.errors.is_empty(), "expected an error");
        h.errors[0].0
    }

    #[test]
    fn assignment_creates_and_types_a_name() {
        assert_eq!(out("a = 1\nb = 'x'\ndispstr a b"), "1x");
        assert_eq!(err("a = 1\na = 'x'"), TtlError::TypeMismatch);
        assert_eq!(err("a = 'x'\na = 1"), TtlError::TypeMismatch);
    }

    #[test]
    fn a_string_assignment_copies_rather_than_aliases() {
        assert_eq!(out("a = 'one'\nb = a\na = 'two'\ndispstr b"), "one");
    }

    #[test]
    fn a_line_that_is_not_a_command_says_so() {
        assert_eq!(err("nosuch 1"), TtlError::NotSupported);
        assert_eq!(err("1 = 2"), TtlError::Syntax);
    }

    #[test]
    fn if_then_else_endif() {
        assert_eq!(out("if 1 then\ndispstr 'y'\nelse\ndispstr 'n'\nendif"), "y");
        assert_eq!(out("if 0 then\ndispstr 'y'\nelse\ndispstr 'n'\nendif"), "n");
        assert_eq!(out("if 0 then\ndispstr 'y'\nendif\ndispstr '.'"), ".");
    }

    #[test]
    fn elseif_takes_the_first_true_arm_and_only_that_one() {
        let src = "\
a = 2
if a = 1 then
dispstr 'one'
elseif a = 2 then
dispstr 'two'
elseif a = 2 then
dispstr 'again'
else
dispstr 'other'
endif";
        assert_eq!(out(src), "two");
    }

    #[test]
    fn a_nested_if_inside_a_skipped_branch_keeps_its_own_else() {
        let src = "\
if 0 then
  if 1 then
    dispstr 'inner-then'
  else
    dispstr 'inner-else'
  endif
else
  dispstr 'outer-else'
endif";
        assert_eq!(out(src), "outer-else");
    }

    #[test]
    fn a_single_line_if_runs_the_rest_of_the_line() {
        assert_eq!(out("if 1 dispstr 'y'"), "y");
        assert_eq!(out("if 0 dispstr 'y'"), "");
        // ...and needs something to run.
        assert_eq!(err("if 1"), TtlError::Syntax);
    }

    #[test]
    fn while_loops_and_endwhile_seeks_back() {
        assert_eq!(
            out("i = 0\nwhile i < 3\ndispstr i\ni = i + 1\nendwhile"),
            "012"
        );
        // A false condition skips the body without executing any of it.
        assert_eq!(out("while 0\ndispstr 'no'\nendwhile\ndispstr '.'"), ".");
    }

    #[test]
    fn until_is_while_with_the_test_flipped() {
        assert_eq!(
            out("i = 0\nuntil i >= 3\ndispstr i\ni = i + 1\nenduntil"),
            "012"
        );
    }

    #[test]
    fn do_loop_runs_its_body_before_testing() {
        assert_eq!(out("i = 9\ndo while i < 3\ndispstr i\ni = i + 1\nloop"), "");
        assert_eq!(
            out("i = 0\ndo\ndispstr i\ni = i + 1\nloop while i < 3"),
            "012"
        );
        assert_eq!(
            out("i = 0\ndo\ndispstr i\ni = i + 1\nloop until i >= 3"),
            "012"
        );
    }

    #[test]
    fn for_counts_towards_its_end_in_either_direction() {
        assert_eq!(out("for i 1 4\ndispstr i\nnext"), "1234");
        assert_eq!(out("for i 4 1\ndispstr i\nnext"), "4321");
        assert_eq!(out("for i 3 3\ndispstr i\nnext"), "3");
    }

    #[test]
    fn break_leaves_the_loop_and_continue_restarts_it() {
        assert_eq!(
            out("i = 0\nwhile i < 5\ni = i + 1\nif i = 3 then\nbreak\nendif\ndispstr i\nendwhile"),
            "12"
        );
        assert_eq!(
            out(
                "i = 0\nwhile i < 5\ni = i + 1\nif i = 3 then\ncontinue\nendif\ndispstr i\nendwhile"
            ),
            "1245"
        );
    }

    #[test]
    fn break_inside_a_for_stops_it() {
        assert_eq!(
            out("for i 1 9\nif i > 3 then\nbreak\nendif\ndispstr i\nnext"),
            "123"
        );
    }

    #[test]
    fn goto_and_labels() {
        assert_eq!(out("goto skip\ndispstr 'no'\n:skip\ndispstr 'yes'"), "yes");
        assert_eq!(err("goto nowhere"), TtlError::LabelReq);
        assert_eq!(err(":dup\n:dup"), TtlError::LabelAlreadyDef);
    }

    #[test]
    fn call_and_return() {
        assert_eq!(
            out("call sub\ndispstr 'b'\nend\n:sub\ndispstr 'a'\nreturn"),
            "ab"
        );
        assert_eq!(err("return"), TtlError::InvalidCtl);
    }

    #[test]
    fn end_stops_the_macro_where_it_stands() {
        assert_eq!(out("dispstr 'a'\nend\ndispstr 'b'"), "a");
    }

    #[test]
    fn arrays_are_dimensioned_before_use() {
        assert_eq!(out("intdim a 3\na[1] = 7\ndispstr a[1]"), "7");
        assert_eq!(out("strdim s 2\ns[0] = 'hi'\ndispstr s[0]"), "hi");
        assert_eq!(err("intdim a 3\na[3] = 1"), TtlError::OutOfRange);
        // No size to invent, so an index cannot create the variable.
        assert_eq!(err("b[0] = 1"), TtlError::Syntax);
        assert_eq!(err("intdim a 3\nintdim a 3"), TtlError::Syntax);
        assert_eq!(err("intdim sendln 3"), TtlError::Syntax);
        assert_eq!(err("intdim a 0-1"), TtlError::FewMemory);
    }

    #[test]
    fn ifdefined_reports_the_type_number() {
        assert_eq!(out("ifdefined nope\ndispstr result"), "0");
        assert_eq!(out("a = 1\nifdefined a\ndispstr result"), "1");
        assert_eq!(out("a = 'x'\nifdefined a\ndispstr result"), "3");
        assert_eq!(out("intdim a 1\nifdefined a\ndispstr result"), "5");
        assert_eq!(out("strdim a 1\nifdefined a\ndispstr result"), "6");
    }

    #[test]
    fn execcmnd_builds_a_statement_out_of_a_string() {
        assert_eq!(out("s = 'dispstr '#39'hi'#39\nexeccmnd s"), "hi");
        // A comment or a label is not run, and is not an error either.
        assert_eq!(out("s = '; nothing'\nexeccmnd s\ndispstr '.'"), ".");
    }

    #[test]
    fn include_brings_in_labels_and_gives_them_back() {
        let mut host = RecordingHost::new();
        host.files
            .insert(b"inc.ttl".to_vec(), b"dispstr 'in'\n".to_vec());
        let src = b"dispstr 'a'\ninclude 'inc.ttl'\ndispstr 'b'\n".to_vec();
        let mut it = Interp::new("t.ttl", src, &mut host);
        it.run(&mut host);
        assert!(host.errors.is_empty(), "{:?}", host.errors);
        assert_eq!(String::from_utf8_lossy(&host.output), "ainb");
    }

    #[test]
    fn a_missing_include_is_reported_not_ignored() {
        assert_eq!(err("include 'nope.ttl'"), TtlError::CantOpen);
    }

    #[test]
    fn a_c_comment_can_span_lines_and_hide_a_command() {
        assert_eq!(out("dispstr 'a' /* one\ntwo */ \ndispstr 'b'"), "ab");
        assert_eq!(err("dispstr 'a' /* never closed"), TtlError::CloseComment);
    }

    #[test]
    fn a_quoted_comment_marker_is_not_a_comment() {
        assert_eq!(out("dispstr '/*'\ndispstr 'b'"), "/*b");
    }

    #[test]
    fn the_system_variables_are_there_before_the_first_line() {
        assert_eq!(out("dispstr result timeout mtimeout"), "000");
        assert_eq!(out("dispstr param1"), "t.ttl");
        assert_eq!(out("dispstr paramcnt"), "1");
        assert_eq!(out("dispstr params[1]"), "t.ttl");
    }

    #[test]
    fn arguments_land_in_both_the_old_and_the_new_form() {
        let mut host = RecordingHost::new();
        let mut it = Interp::new(
            "t.ttl",
            b"dispstr param1 param2 params[3] paramcnt".to_vec(),
            &mut host,
        );
        it.vars = Vars::new();
        it.define_system_variables(&[b"one".to_vec(), b"two".to_vec()]);
        it.run(&mut host);
        assert!(host.errors.is_empty(), "{:?}", host.errors);
        // param1 is the macro, param2 the first argument, and `params` is the
        // same list one index further on — `params[1]` being the macro too.
        assert_eq!(String::from_utf8_lossy(&host.output), "t.ttlonetwo3");
    }

    #[test]
    fn setexitcode_reaches_the_host() {
        let h = run("setexitcode 3");
        assert_eq!(h.exit_code, 3);
    }

    #[test]
    fn an_unimplemented_command_is_an_unknown_one_for_now() {
        // A real reserved word with no arm in the dispatch. It has to be
        // replaced each time the port catches up with whichever one is named
        // here — `regexoption` is furthest out, since the regex family is a
        // dialect decision rather than a port.
        assert_eq!(err("regexoption 1"), TtlError::NotSupported);
    }
}

//! The macro source, the include stack and the control stack — `ttmbuff.c`.
//!
//! There is no compilation step. A macro is a byte buffer and a cursor into it,
//! and every control structure is a saved cursor: `while` remembers where its
//! own line started so `endwhile` can seek back to it, and `for` does the same.
//! Skipping a block forward is not a seek at all — the interpreter keeps
//! reading and executing lines with a counter that suppresses everything until
//! the matching `endif` goes past. That is why `if` and `while` bodies cost
//! parsing time even when they do not run, and why a syntax error inside a
//! branch that is never taken is still an error.

use crate::error::{TtlError, TtlResult};
use crate::lexer::Lexer;

/// `ttmbuff.c:52` — `include` nests ten deep, the outermost file included.
pub const MAX_NEST_LEVEL: usize = 10;
/// `ttmbuff.c:67` — and ten frames of `call`, `for` and `while` between them.
pub const MAX_SP: usize = 10;

/// What a control-stack frame is holding a position for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ctl {
    Call,
    For,
    While,
}

#[derive(Debug, Clone, Copy)]
struct Frame {
    /// Where to resume. `None` is upstream's `INVALIDPTR`, which `for` uses to
    /// mark the last iteration so that `next` falls out instead of jumping.
    ptr: Option<usize>,
    level: usize,
    kind: Ctl,
}

#[derive(Debug, Clone)]
struct Source {
    /// The file's own name, for the error report. Upstream keeps the basename.
    name: String,
    body: Vec<u8>,
    ptr: usize,
    /// Byte offset of the start of each line, so a position maps to a number.
    line_starts: Vec<usize>,
}

impl Source {
    fn new(name: String, body: Vec<u8>) -> Self {
        let mut line_starts = vec![0usize];
        for (i, b) in body.iter().enumerate() {
            if *b == b'\n' && i != body.len() - 1 {
                line_starts.push(i + 1);
            }
        }
        Self {
            name,
            body,
            ptr: 0,
            line_starts,
        }
    }

    /// `getCurrentLineNumber` — the 1-based line a byte offset falls in.
    fn line_no(&self, pos: usize) -> usize {
        match self.line_starts.iter().position(|&s| pos < s) {
            Some(i) => i,
            None => self.line_starts.len(),
        }
    }
}

/// The macro being run: its files, its cursor and its control stack.
#[derive(Debug, Default, Clone)]
pub struct Buffers {
    files: Vec<Source>,
    stack: Vec<Frame>,
    /// Where the line most recently read began — what `while` and `for` save.
    line_start: usize,
    line_no: usize,
    /// Set by `next` when the loop is to run again, read by `for`.
    next_flag: bool,
    /// How many `endwhile`s to swallow before running anything again.
    pub end_while_flag: u32,
    /// The same for `break` and `continue`, which also count `endif`s.
    pub break_flag: u32,
    pub continue_flag: bool,
}

impl Buffers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Which include level is executing. 0 is the file the user named.
    pub fn nest(&self) -> usize {
        self.files.len().saturating_sub(1)
    }

    pub fn line_no(&self) -> usize {
        self.line_no
    }

    /// `GetMacroFileName` — the file the current line came from.
    pub fn file_name(&self) -> &str {
        self.files.last().map(|f| f.name.as_str()).unwrap_or("")
    }

    /// `LoadMacroFile` at level 0 — start a run.
    pub fn open(&mut self, name: String, body: Vec<u8>) {
        self.files.clear();
        self.stack.clear();
        self.line_start = 0;
        self.line_no = 1;
        self.next_flag = false;
        self.end_while_flag = 0;
        self.break_flag = 0;
        self.continue_flag = false;
        self.files.push(Source::new(name, body));
    }

    /// `BuffInclude` — push a file. Fails at the nesting limit.
    pub fn include(&mut self, name: String, body: Vec<u8>) -> bool {
        if self.files.len() >= MAX_NEST_LEVEL {
            return false;
        }
        self.files.push(Source::new(name, body));
        true
    }

    /// Where the cursor is in the current file — a label's target.
    pub fn pos(&self) -> usize {
        self.files.last().map(|f| f.ptr).unwrap_or(0)
    }

    /// Rewind the current file, for the label-registration pass.
    pub fn rewind(&mut self) {
        if let Some(f) = self.files.last_mut() {
            f.ptr = 0;
        }
        self.line_no = 1;
    }

    /// `CloseBuff` — pop the top file and every control frame that lived in it.
    ///
    /// The caller has to drop that level's labels too; they are in the variable
    /// table, which this type does not own.
    pub fn close_top(&mut self) {
        let level = self.nest();
        self.files.pop();
        self.stack.retain(|f| f.level < level);
    }

    /// `ExitBuffer` — leave an included file early. False at the outermost.
    pub fn exit_buffer(&mut self) -> bool {
        if self.files.len() < 2 {
            return false;
        }
        self.close_top();
        true
    }

    /// `GetRawLine` — the next physical line into the lexer.
    ///
    /// A line ends at any byte below 0x20 other than tab, so a stray NUL or
    /// form feed splits one. Returns false only at the very end of the file.
    pub fn raw_line(&mut self, lx: &mut Lexer) -> bool {
        let Some(f) = self.files.last_mut() else {
            return false;
        };
        self.line_start = f.ptr;

        let start = f.ptr;
        while f.ptr < f.body.len() {
            let b = f.body[f.ptr];
            if b < 0x20 && b != 0x09 {
                break;
            }
            f.ptr += 1;
        }
        lx.set_line(&f.body[start..f.ptr]);

        let pos = f.ptr;
        // Then eat the line terminator, however many bytes of it there are.
        while f.ptr < f.body.len() {
            let b = f.body[f.ptr];
            if b >= 0x20 || b == 0x09 {
                break;
            }
            f.ptr += 1;
        }

        self.line_no = f.line_no(pos);
        !lx.is_empty() || f.ptr < f.body.len()
    }

    /// `GetNewLine` — the next line with anything on it, popping finished
    /// include levels on the way. Blank lines, comments and label definitions
    /// are all skipped here rather than by the interpreter.
    ///
    /// Returns the lowest include level that was closed, so the caller can
    /// drop the labels defined at that level and above.
    pub fn new_line(&mut self, lx: &mut Lexer) -> (bool, Option<usize>) {
        let mut closed = None;
        loop {
            let mut ok = self.raw_line(lx);
            while !ok && self.files.len() > 1 {
                closed = Some(self.nest());
                self.close_top();
                ok = self.raw_line(lx);
            }
            if !ok {
                return (false, closed);
            }
            let b = lx.first_char();
            lx.back();
            if b != 0 && b != b':' {
                return (true, closed);
            }
        }
    }

    /// Where the line that is executing began — what a loop frame saves.
    pub fn line_start(&self) -> usize {
        self.line_start
    }

    fn seek(&mut self, level: usize, ptr: usize) -> Option<usize> {
        let closed = self.unwind_to(level);
        if let Some(f) = self.files.last_mut() {
            f.ptr = ptr;
        }
        closed
    }

    /// Pop include levels until `level` is on top, reporting the lowest popped.
    fn unwind_to(&mut self, level: usize) -> Option<usize> {
        let mut closed = None;
        while self.nest() > level {
            closed = Some(self.nest());
            self.close_top();
        }
        closed
    }

    /// `JumpToLabel`. A label carries the level it was defined at, so jumping
    /// out of an included file into its includer unwinds on the way.
    pub fn jump_to(&mut self, pos: usize, level: usize) -> Option<usize> {
        if level < self.nest() {
            self.seek(level, pos)
        } else {
            if let Some(f) = self.files.last_mut() {
                f.ptr = pos;
            }
            None
        }
    }

    /// `CallToLabel` — and a `call` may not cross an include boundary in
    /// either direction, which is what `ErrCantCall` is for.
    pub fn call_to(&mut self, pos: usize, level: usize) -> TtlResult<()> {
        if level != self.nest() {
            return Err(TtlError::CantCall);
        }
        if self.stack.len() >= MAX_SP {
            return Err(TtlError::StackOver);
        }
        self.stack.push(Frame {
            ptr: Some(self.files.last().map(|f| f.ptr).unwrap_or(0)),
            level: self.nest(),
            kind: Ctl::Call,
        });
        if let Some(f) = self.files.last_mut() {
            f.ptr = pos;
        }
        Ok(())
    }

    /// `ReturnFromSub`. A `return` with a `for` frame on top is `ErrInvalidCtl`,
    /// not a search down the stack.
    pub fn return_from_sub(&mut self) -> TtlResult<Option<usize>> {
        match self.stack.last() {
            Some(f) if f.kind == Ctl::Call => {}
            _ => return Err(TtlError::InvalidCtl),
        }
        let f = self.stack.pop().unwrap();
        Ok(self.seek(f.level, f.ptr.unwrap_or(0)))
    }

    fn push_loop(&mut self, kind: Ctl) -> TtlResult<()> {
        if self.stack.len() >= MAX_SP {
            return Err(TtlError::StackOver);
        }
        self.stack.push(Frame {
            ptr: Some(self.line_start),
            level: self.nest(),
            kind,
        });
        Ok(())
    }

    /// `SetForLoop`.
    pub fn set_for_loop(&mut self) -> TtlResult<()> {
        self.push_loop(Ctl::For)
    }

    /// `LastForLoop` — mark the frame so the next `next` falls out of the loop.
    pub fn last_for_loop(&mut self) {
        if let Some(f) = self.stack.last_mut() {
            if f.kind == Ctl::For {
                f.ptr = None;
            }
        }
    }

    /// `CheckNext` — did the previous `next` send us back here? Consumes it.
    pub fn check_next(&mut self) -> bool {
        std::mem::take(&mut self.next_flag)
    }

    /// `NextLoop`.
    pub fn next_loop(&mut self) -> TtlResult<Option<usize>> {
        match self.stack.last() {
            Some(f) if f.kind == Ctl::For => {}
            _ => return Err(TtlError::InvalidCtl),
        }
        let top = *self.stack.last().unwrap();
        self.next_flag = top.ptr.is_some();
        match top.ptr {
            None => {
                self.stack.pop();
                Ok(None)
            }
            Some(ptr) => Ok(self.seek(top.level, ptr)),
        }
    }

    /// `SetWhileLoop`.
    pub fn set_while_loop(&mut self) -> TtlResult<()> {
        self.push_loop(Ctl::While)
    }

    /// `EndWhileLoop` — the condition was false, so skip to the matching end.
    pub fn end_while_loop(&mut self) {
        self.end_while_flag = 1;
    }

    /// `BackToWhile` — pop the frame, and jump back to it if asked.
    pub fn back_to_while(&mut self, again: bool) -> TtlResult<Option<usize>> {
        match self.stack.last() {
            Some(f) if f.kind == Ctl::While => {}
            _ => return Err(TtlError::InvalidCtl),
        }
        let f = self.stack.pop().unwrap();
        if again {
            Ok(self.seek(f.level, f.ptr.unwrap_or(0)))
        } else {
            // Unwind the include levels even when not jumping — upstream does,
            // and a `while` whose body included a file has to leave it.
            Ok(self.unwind_to(f.level))
        }
    }

    /// `BreakLoop` — leave the innermost loop, or restart it for `continue`.
    ///
    /// Neither one seeks. `break_flag` counts nested loop openings until the
    /// matching close goes past, so the rest of the body is read and discarded.
    pub fn break_loop(&mut self, is_continue: bool) -> TtlResult<Option<usize>> {
        match self.stack.last() {
            Some(f) if matches!(f.kind, Ctl::For | Ctl::While) => {}
            _ => return Err(TtlError::InvalidCtl),
        }
        let mut closed = None;
        if is_continue {
            self.continue_flag = true;
        } else {
            let f = self.stack.pop().unwrap();
            closed = self.unwind_to(f.level);
        }
        self.break_flag = 1;
        Ok(closed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_all(src: &str) -> Vec<String> {
        let mut b = Buffers::new();
        let mut lx = Lexer::new();
        b.open("t.ttl".into(), src.as_bytes().to_vec());
        let mut out = Vec::new();
        while b.new_line(&mut lx).0 {
            out.push(String::from_utf8_lossy(lx.line()).into_owned());
        }
        out
    }

    #[test]
    fn blank_comment_and_label_lines_never_reach_the_interpreter() {
        let out = read_all("a\n\n; comment\n:label\n   \nb\n");
        assert_eq!(out, vec!["a", "b"]);
    }

    #[test]
    fn crlf_and_lf_read_the_same() {
        assert_eq!(read_all("a\r\nb\r\n"), read_all("a\nb\n"));
    }

    #[test]
    fn a_line_number_is_where_the_line_is_not_where_the_cursor_is() {
        let mut b = Buffers::new();
        let mut lx = Lexer::new();
        b.open("t.ttl".into(), b"one\n\nthree\nfour\n".to_vec());
        assert!(b.new_line(&mut lx).0);
        assert_eq!(b.line_no(), 1);
        assert!(b.new_line(&mut lx).0);
        assert_eq!(b.line_no(), 3);
        assert!(b.new_line(&mut lx).0);
        assert_eq!(b.line_no(), 4);
        assert!(!b.new_line(&mut lx).0);
    }

    #[test]
    fn a_call_may_not_cross_an_include_boundary() {
        let mut b = Buffers::new();
        b.open("t.ttl".into(), b"x\n".to_vec());
        assert!(b.include("i.ttl".into(), b"y\n".to_vec()));
        assert_eq!(b.call_to(0, 0), Err(TtlError::CantCall));
        assert_eq!(b.call_to(0, 1), Ok(()));
    }

    #[test]
    fn the_control_stack_is_ten_deep_and_says_so() {
        let mut b = Buffers::new();
        b.open("t.ttl".into(), b"x\n".to_vec());
        for _ in 0..MAX_SP {
            assert_eq!(b.set_while_loop(), Ok(()));
        }
        assert_eq!(b.set_while_loop(), Err(TtlError::StackOver));
    }

    #[test]
    fn a_return_with_a_loop_frame_on_top_is_refused() {
        let mut b = Buffers::new();
        b.open("t.ttl".into(), b"x\n".to_vec());
        b.set_while_loop().unwrap();
        assert_eq!(b.return_from_sub(), Err(TtlError::InvalidCtl));
    }

    #[test]
    fn include_nests_ten_deep() {
        let mut b = Buffers::new();
        b.open("t.ttl".into(), b"x\n".to_vec());
        for _ in 1..MAX_NEST_LEVEL {
            assert!(b.include("i.ttl".into(), b"y\n".to_vec()));
        }
        assert!(!b.include("i.ttl".into(), b"y\n".to_vec()));
    }

    #[test]
    fn closing_a_level_reports_it_so_its_labels_can_go() {
        let mut b = Buffers::new();
        let mut lx = Lexer::new();
        b.open("t.ttl".into(), b"a\n".to_vec());
        b.include("i.ttl".into(), b"b\n".to_vec());
        // `b` is the last line of the include, so the next read pops it and
        // carries on with the includer's own first line.
        assert!(b.new_line(&mut lx).0);
        assert_eq!(lx.line(), b"b");
        let (ok, closed) = b.new_line(&mut lx);
        assert!(ok);
        assert_eq!(lx.line(), b"a");
        assert_eq!(closed, Some(1));
    }
}

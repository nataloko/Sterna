//! The printer — media copy, auto print, and the controller mode that takes the
//! stream away from the screen.
//!
//! A VT engine has no printer any more than it has a window, so this splits the
//! way [`crate::window`] does: the engine decides *what* to print and puts it on
//! a queue ([`crate::Vt::take_printer_events`]) the frontend drains. Everything
//! below the queue — upstream's spool file, its `PassThruDelay` timer, the
//! conversion back to the printer's code page, the abort dialog and the GDI
//! rendering in `teraprn.cpp` — belongs to whoever owns the printer.
//!
//! Upstream's spool holds **code points**, not bytes: `WriteToPrnFile` takes a
//! `BYTE` and stores it into a `char32_t` array (`teraprn.cpp:527`), and
//! `PrnFileDirectProc` converts each one back with `UTF32ToMBCP(u32, CP_ACP)`
//! on the way out. So a raw control byte reaches the printer as the code point
//! of the same value, which is why [`PrinterEvent::Write`] carries a `String`
//! and an escape in it is U+001B rather than a byte.
//!
//! Four gates decide whether any of this happens, and they are not one gate.
//! `ts.TermFlag & TF_PRINTERCTRL` (`PrinterCtrlSequence`, **off** as Tera Term
//! ships) gates `CSI 0 i`, `CSI 5 i`, `CSI ? 1 i` and `CSI ? 5 i` — but *not*
//! `CSI ? 4 i`, so a host can always turn auto print off again. `ts.PrnDev`
//! decides [`Controller`]'s `direct`. DECPEX decides which rectangle `CSI 0 i`
//! prints. And the controller's own mode decides whether the stream reaches the
//! screen at all.

use tt_charset::{Shift, ShiftFlags};

/// One thing the terminal asks of a printer, in the order it asked for it.
///
/// Order is the whole point of a queue here rather than a snapshot: a chunk can
/// open a job, fill it and close it, and a frontend that only saw the last state
/// would print nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrinterEvent {
    /// `OpenPrnFile` — a job begins. Nothing is printed until [`Self::Close`].
    Open,
    /// `WriteToPrnFile`/`WriteToPrnFileUTF32` — code points for the open job.
    Write(String),
    /// `ClosePrnFile` — the job is complete and may be sent to the printer.
    /// Upstream waits `ts.PassThruDelay` seconds first; that timer is the
    /// frontend's, because the engine has no clock.
    Close,
    /// `BuffPrint` — `CSI 0 i`, which is not a byte stream at all. Upstream
    /// raises the print dialog and renders the grid graphically through
    /// `VTPrintInit`, so this is a request rather than a job.
    ///
    /// `scroll_region` is `!PrintEX`: DECPEX set — the default — prints the
    /// whole screen, and DECPEX reset prints the scroll region alone.
    Screen { scroll_region: bool },
}

/// Where the controller-mode parser is. Upstream reuses `ParseMode` for this,
/// which is why `PrnParseControl`, `PrnParseEscape` and `PrnParseCS` are checked
/// for at the *top* of the three ordinary handlers rather than being a parser of
/// their own (`vtterm.c:1048`, `:1474`, `:4053`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Parse {
    #[default]
    First,
    Esc,
    Csi,
}

/// What one byte of the stream turns into while the controller has it.
#[derive(Debug)]
pub(crate) enum Step {
    /// To the printer, as this code point.
    Printer(u32),
    /// Back to the terminal — printable text, which controller mode still
    /// displays.
    Terminal,
    /// Back to the terminal as a whole sequence: the ISO-2022 designations,
    /// which controller mode keeps interpreting and which were held back byte
    /// by byte while they were being read.
    Replay(Vec<u8>),
    /// Consumed and gone: NUL, DEL, DC1, DC3, and every byte of a sequence that
    /// is still being accumulated.
    Drop,
    /// `CSI 4 i`. The controller is off from the next byte on.
    Exit,
}

/// Printer controller mode — `CSI 5 i` until `CSI 4 i`.
///
/// **It is not a diverter, which is the thing to know about it.** Printable
/// characters go on reaching the screen: they are copied to the printer through
/// `OutputLogUTF32` (`vtterm.c:487`), the same tap the session log and the macro
/// language read, so the copy rides [`crate::Vt::set_macro_tap_enabled`]'s
/// machinery rather than this one. What this machine takes away is the
/// *controls* — they are sent to the printer uninterpreted, so a line feed does
/// not feed a line and an `ESC [ 2 J` does not clear anything.
///
/// The exceptions are all in `PrnParseControl` and `PrnParseEscape`, and they
/// all point the same way: while the printer is a Windows print job rather than
/// a device on a port, the terminal still needs to know which character set it
/// is decoding, so the locking shifts and the ISO-2022 designations keep being
/// interpreted. With a device named in `ts.PrnDev` they are not, because those
/// bytes are the *printer's* to interpret. That is `direct`.
#[derive(Clone, Debug, Default)]
pub(crate) struct Controller {
    /// `PrinterMode`.
    on: bool,
    /// `DirectPrn` — `ts.PrnDev[0] != 0`, sampled when the controller turned on
    /// (`vtterm.c:2095`) rather than read live, so changing the setting during a
    /// job does not change how the job is parsed.
    direct: bool,
    state: Parse,
    /// `PrnBuff` holding a sequence written with `Write=FALSE`. It is committed
    /// when the sequence ends and **discarded** when it turns out to be
    /// `CSI 4 i` — upstream spells that `WriteToPrnFile(PrintFile_, 0, FALSE)`,
    /// the clear-buffer form of a function whose four meanings are set out at
    /// `teraprn.cpp:504`.
    pending: Vec<u8>,
    /// `IntChar`, and `ICount` is its length.
    intermediates: Vec<u8>,
    /// `Prv`, set from the first parameter byte only.
    private: bool,
    /// `Param[1]`. The only parameter the controller reads, because `CSI 4 i` is
    /// the only sequence it acts on.
    param: u32,
    /// `FirstPrm` — whether a private marker would still be taken.
    first_param: bool,
    /// Whether the digits being accumulated are still the first parameter's.
    in_first_param: bool,
}

/// `IntCharMax` (`vtterm.c:71`).
const INT_CHAR_MAX: usize = 16;

impl Controller {
    pub(crate) fn is_on(&self) -> bool {
        self.on
    }

    /// `vtterm.c:2091` — the `CSI 5 i` arm, which samples `DirectPrn` here.
    pub(crate) fn start(&mut self, direct: bool) {
        self.on = true;
        self.direct = direct;
        self.state = Parse::First;
        self.pending.clear();
        self.reset_sequence();
    }

    /// `ResetTerminal` and `ChangeEmulation` put `PrinterMode` back
    /// (`vtterm.c:327`) without closing the job; closing it is the caller's.
    pub(crate) fn stop(&mut self) {
        self.on = false;
        self.state = Parse::First;
        self.pending.clear();
    }

    fn reset_sequence(&mut self) {
        self.intermediates.clear();
        self.private = false;
        self.param = 0;
        self.first_param = true;
        self.in_first_param = true;
    }

    /// Commit the sequence accumulated in `pending` to the printer. This is the
    /// `Write=TRUE` half of `WriteToPrnFile`, which flushes what was buffered
    /// *before* it appends its own byte — so a control arriving inside a
    /// half-read sequence pushes that sequence out ahead of itself.
    fn flush_pending(&mut self, out: &mut String) {
        for b in self.pending.drain(..) {
            push_cp(out, u32::from(b));
        }
    }

    /// One byte of an already-C1-rewritten stream.
    ///
    /// `iso` is `ts.ISO2022Flag`, because `PrnParseControl` tests it before
    /// letting SO and SI through — with the flag off the byte is printer data
    /// rather than a shift, which is the opposite of what happens outside
    /// controller mode, where the same byte is simply ignored.
    pub(crate) fn step(&mut self, b: u8, iso: ShiftFlags, out: &mut String) -> Step {
        match self.state {
            Parse::First => self.step_first(b, iso, out),
            Parse::Esc => self.step_esc(b, iso, out),
            Parse::Csi => self.step_csi(b, iso, out),
        }
    }

    /// `PrnParseControl` (`vtterm.c:960`), plus the classification in
    /// `ParseFirstUTF8` (`charset.cpp:488`) that decides what reaches it.
    ///
    /// Called from the two sequence states as well, for the same reason
    /// upstream routes a control inside a sequence back through `ParseControl`:
    /// a control does not belong to the sequence and does not end it.
    fn step_first(&mut self, b: u8, iso: ShiftFlags, out: &mut String) -> Step {
        // Not a control at all — text, which controller mode still displays and
        // which reaches the printer through the tap rather than through here. A
        // UTF-8 lead or continuation byte lands in this arm too, which is why
        // this machine never has to decode one.
        if b >= 0x20 && b != 0x7f && !(0x80..=0x9f).contains(&b) {
            return Step::Terminal;
        }
        match b {
            // `charset.cpp:500` returns on DEL before `ParseControl` can see it,
            // so it is neither displayed nor printed.
            0x00 | 0x7f => Step::Drop,
            // SO/SI keep their meaning while the printer is a print job. The
            // byte is *not* also sent — upstream returns from inside the arm.
            0x0e if iso.allows(Shift::Ls1) && !self.direct => Step::Terminal,
            0x0f if iso.allows(Shift::Ls0) && !self.direct => Step::Terminal,
            // DC1/DC3. Swallowed whole: flow control is not the printer's.
            0x11 | 0x13 => Step::Drop,
            0x1b => {
                self.flush_pending(out);
                self.state = Parse::Esc;
                self.reset_sequence();
                self.pending.push(0x1b);
                Step::Drop
            }
            // `0x9b` is upstream's CSI arm. It cannot arrive here: `rewrite_c1`
            // has already folded an accepted C1 into its `ESC Fe` form and
            // turned a bare one into U+FFFD, so the introducer reaching this
            // machine is always `ESC [`. The one byte in that range that
            // survives the fold is HTS' `0x88`, which upstream sends to the
            // printer from the arm below — and so does this.
            _ => {
                self.flush_pending(out);
                Step::Printer(u32::from(b))
            }
        }
    }

    /// `EscapeSequence` (`vtterm.c:4232`) classifying the byte, and
    /// `PrnParseEscape` (`:1415`) deciding the sequence.
    fn step_esc(&mut self, b: u8, iso: ShiftFlags, out: &mut String) -> Step {
        match b {
            0x00..=0x1f | 0x80..=0x9f => self.step_first(b, iso, out),
            0x20..=0x2f => {
                if self.intermediates.len() < INT_CHAR_MAX {
                    self.intermediates.push(b);
                }
                Step::Drop
            }
            0x30..=0x7e => self.esc_final(b, out),
            // `EscapeSequence` has no arm for DEL at all, so the escape stays
            // open and the byte is gone.
            0x7f => Step::Drop,
            // Its last arm: the escape is abandoned and the byte is text. A
            // UTF-8 lead reaches this.
            _ => {
                self.state = Parse::First;
                self.pending.clear();
                Step::Terminal
            }
        }
    }

    /// `PrnParseEscape`. Two sequences are the terminal's and everything else is
    /// the printer's, including the `ESC` and the intermediates held back while
    /// it was being read.
    fn esc_final(&mut self, b: u8, out: &mut String) -> Step {
        self.state = Parse::First;

        // `ESC [` — the printer sees the introducer and the parameters as they
        // arrive, which is what makes discarding them on `CSI 4 i` possible.
        if self.intermediates.is_empty() && b == b'[' {
            self.state = Parse::Csi;
            self.pending.push(b'[');
            self.reset_sequence();
            return Step::Drop;
        }

        // The ISO-2022 designations, which the terminal keeps interpreting
        // unless the printer is a device: `ESC $ F`, `ESC ( F` and its three
        // siblings, and the three-byte `ESC $ ( F`.
        let designation = matches!(
            self.intermediates.as_slice(),
            [b'$'] | [b'(' | b')' | b'*' | b'+'] | [b'$', b'(' | b')' | b'*' | b'+']
        );
        if designation && !self.direct {
            // Replayed to the terminal rather than performed here: the charset
            // dispatch belongs to `esc_dispatch` and there must not be a second
            // copy of it.
            let mut seq = std::mem::take(&mut self.pending);
            seq.extend_from_slice(&self.intermediates);
            seq.push(b);
            return Step::Replay(seq);
        }

        self.flush_pending(out);
        for &i in &self.intermediates {
            push_cp(out, u32::from(i));
        }
        Step::Printer(u32::from(b))
    }

    /// `ControlSequence` (`vtterm.c:4155`) and `PrnParseCS` (`:4029`). Every
    /// byte that is neither a control nor the final one is sent to the printer
    /// as it arrives — `vtterm.c:4161` — so by the time the final byte is
    /// classified the whole sequence is already buffered.
    fn step_csi(&mut self, b: u8, iso: ShiftFlags, out: &mut String) -> Step {
        match b {
            0x00..=0x1f | 0x80..=0x9f => self.step_first(b, iso, out),
            0x40..=0x7e => self.csi_final(b, out),
            // `b > 0xA0` abandons the sequence and is text; `0xA0` exactly falls
            // off the end of upstream's `if` chain and is simply lost. Kept
            // rather than tidied — by this point it is one byte of a UTF-8
            // sequence that has already been abandoned, so the tidy version
            // prints a replacement character upstream does not.
            0xa1..=0xff => {
                self.state = Parse::First;
                self.pending.clear();
                Step::Terminal
            }
            0xa0 => Step::Drop,
            _ => {
                self.pending.push(b);
                match b {
                    0x20..=0x2f => {
                        if self.intermediates.len() < INT_CHAR_MAX {
                            self.intermediates.push(b);
                        }
                    }
                    0x30..=0x39 if self.in_first_param => {
                        self.param = self
                            .param
                            .saturating_mul(10)
                            .saturating_add(u32::from(b - 0x30));
                    }
                    0x3b => self.in_first_param = false,
                    0x3c..=0x3f if self.first_param => self.private = true,
                    _ => {}
                }
                self.first_param = false;
                Step::Drop
            }
        }
    }

    fn csi_final(&mut self, b: u8, out: &mut String) -> Step {
        self.state = Parse::First;
        // The one sequence controller mode acts on. Everything about it has to
        // match — no intermediate, no private marker, first parameter 4 — or it
        // is printer data like any other sequence.
        if b == b'i' && self.intermediates.is_empty() && !self.private && self.param == 4 {
            self.on = false;
            self.pending.clear();
            return Step::Exit;
        }
        self.flush_pending(out);
        Step::Printer(u32::from(b))
    }
}

/// A code point into the printer stream. `WriteToPrnFileUTF32` refuses a zero
/// (`teraprn.cpp:513` stores only `u32 > 0`) and a surrogate cannot be encoded,
/// so both are dropped here rather than becoming a replacement character.
pub(crate) fn push_cp(out: &mut String, cp: u32) {
    if cp == 0 {
        return;
    }
    if let Some(c) = char::from_u32(cp) {
        out.push(c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(c: &mut Controller, bytes: &[u8]) -> (String, Vec<u8>, bool) {
        let mut printer = String::new();
        let mut terminal = Vec::new();
        let mut exited = false;
        for &b in bytes {
            match c.step(b, ShiftFlags::ALL, &mut printer) {
                Step::Printer(cp) => push_cp(&mut printer, cp),
                Step::Terminal => terminal.push(b),
                Step::Replay(seq) => terminal.extend_from_slice(&seq),
                Step::Drop => {}
                Step::Exit => {
                    exited = true;
                    break;
                }
            }
        }
        (printer, terminal, exited)
    }

    #[test]
    fn text_stays_on_the_screen_and_controls_go_to_the_printer() {
        let mut c = Controller::default();
        c.start(false);
        let (printer, terminal, exited) = run(&mut c, b"ab\r\ncd");
        // The text is the terminal's; the copy to the printer is the tap's job
        // and does not come through here.
        assert_eq!(terminal, b"abcd");
        assert_eq!(printer, "\r\n");
        assert!(!exited);
    }

    #[test]
    fn an_unknown_sequence_reaches_the_printer_whole() {
        let mut c = Controller::default();
        c.start(false);
        let (printer, terminal, _) = run(&mut c, b"\x1b[2J\x1b#8");
        assert!(terminal.is_empty());
        assert_eq!(printer, "\u{1b}[2J\u{1b}#8");
    }

    #[test]
    fn a_control_inside_a_sequence_pushes_it_out_ahead_of_itself() {
        let mut c = Controller::default();
        c.start(false);
        let (printer, _, _) = run(&mut c, b"\x1b[12\x07m");
        // `WriteToPrnFile(b, TRUE)` flushes the buffer and then appends, so the
        // bell lands between the parameters and the final byte.
        assert_eq!(printer, "\u{1b}[12\u{7}m");
    }

    #[test]
    fn the_exit_sequence_is_not_printed() {
        let mut c = Controller::default();
        c.start(false);
        let (printer, _, exited) = run(&mut c, b"x\x1b[4i");
        assert!(exited);
        assert!(!c.is_on());
        // `ESC [ 4` was buffered and then discarded, and `x` was the terminal's.
        assert_eq!(printer, "");
    }

    #[test]
    fn only_the_bare_four_leaves_controller_mode() {
        for seq in [&b"\x1b[?4i"[..], b"\x1b[14i", b"\x1b[4 i", b"\x1b[i"] {
            let mut c = Controller::default();
            c.start(false);
            let (_, _, exited) = run(&mut c, seq);
            assert!(!exited, "{seq:?} should not have left controller mode");
            assert!(c.is_on());
        }
        // A second parameter does not disqualify it: `PrnParseCS` reads
        // `Param[1]` and nothing else.
        let mut c = Controller::default();
        c.start(false);
        assert!(run(&mut c, b"\x1b[4;1i").2);
    }

    #[test]
    fn a_designation_is_the_terminals_until_a_device_is_named() {
        let mut c = Controller::default();
        c.start(false);
        let (printer, terminal, _) = run(&mut c, b"\x1b(B");
        assert_eq!(terminal, b"\x1b(B");
        assert_eq!(printer, "");

        let mut direct = Controller::default();
        direct.start(true);
        let (printer, terminal, _) = run(&mut direct, b"\x1b(B");
        assert!(terminal.is_empty());
        assert_eq!(printer, "\u{1b}(B");
    }

    #[test]
    fn the_shifts_follow_the_same_rule_and_the_flag_word_as_well() {
        let mut c = Controller::default();
        c.start(false);
        let mut printer = String::new();
        assert!(matches!(
            c.step(0x0e, ShiftFlags::ALL, &mut printer),
            Step::Terminal
        ));
        // With the shift disabled the byte is printer data rather than being
        // ignored, which is the opposite of what it is outside this mode.
        assert!(matches!(
            c.step(0x0e, ShiftFlags::NONE, &mut printer),
            Step::Printer(0x0e)
        ));
    }

    #[test]
    fn flow_control_and_nul_and_del_never_reach_the_printer() {
        let mut c = Controller::default();
        c.start(false);
        let (printer, terminal, _) = run(&mut c, &[0x00, 0x11, 0x13, 0x7f, 0x07]);
        assert!(terminal.is_empty());
        assert_eq!(printer, "\u{7}");
    }
}

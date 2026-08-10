//! The control lines, which only a serial connection has.
//!
//! Five commands — `setdtr`, `setrts`, `setbaud`/`setspeed`, `setflowctrl`
//! and `getmodemstatus` — plus the reset they share. Upstream keeps them in
//! `ttdde.c` next to the macro plumbing, but nothing about them is a macro's:
//! they are the serial port's, and the File menu's speed submenu reaches the
//! same code. So they live on the session, and `tt-macro` is one caller.
//!
//! **Every one of them is guarded the same way** — `!cv.Open || cv.PortType
//! != IdSerial` — and the guard here is [`Transport::as_serial`] answering
//! `None`, which covers both halves at once. What the guard rejects is not an
//! error: upstream answers `DDE_FNOTPROCESSED`, the macro reads that as
//! success, and a script that runs `setdtr` over SSH carries on. The `bool`
//! each of these returns is for a caller that wants to know anyway, and for
//! the tests.

use tt_config::SerialFlow;
use tt_conn::serial::{FlowControl, ModemLines, SerialParams};

use crate::{Event, Session};

impl Session {
    /// `setdtr` — raise or lower DTR by hand.
    ///
    /// Refused unless the flow control is "none", which is upstream's second
    /// guard (`ttdde.c:1036`) and the documentation's: with DSR/DTR handshaking
    /// on, the line belongs to the driver, and a terminal that let a script
    /// fight it would produce a port that stalls for reasons nothing can see.
    /// Note it is the **setting** that is tested, not the port's live state —
    /// so a `setflowctrl` earlier in the same script is what opens the door.
    pub fn set_dtr(&mut self, on: bool) -> bool {
        let Some(port) = self.hand_driven_port() else {
            return false;
        };
        // The error is dropped on purpose: `EscapeCommFunction`'s return is
        // assigned to a local upstream and never read, so whether the pin
        // moved is not something a macro can find out about.
        let _ = port.set_dtr(on);
        true
    }

    /// `setrts` — the same, for RTS.
    pub fn set_rts(&mut self, on: bool) -> bool {
        let Some(port) = self.hand_driven_port() else {
            return false;
        };
        let _ = port.set_rts(on);
        true
    }

    /// `setbaud` / `setspeed` — the line's speed, in bits per second.
    ///
    /// Upstream writes `ts.Baud` and calls `CommResetSerial`, which re-applies
    /// the *whole* `DCB`; this changes the speed and re-applies the whole
    /// termios, which is the same shape for a different reason — see
    /// [`reset_serial`](Session::reset_serial). One visible consequence
    /// survives either way: DTR and RTS are re-asserted on the way past, so a
    /// `setdtr 0` before a `setbaud` does not outlive it.
    ///
    /// **The value is a speed and not an index.** Upstream's was an index
    /// until 4.66 — `setbaud 12` meant 115200 — and the arm is a plain `atoi`
    /// now, so an old script silently asks a modern Tera Term for 12 baud.
    /// Reproduced: guessing which one was meant would break the scripts
    /// written since.
    ///
    pub fn set_baud(&mut self, baud: u32) -> bool {
        if self.serial().is_none() {
            return false;
        }
        // Written down as well as applied, because upstream assigns `ts.Baud`
        // and the settings dialog reads it back — a speed changed by a script
        // must be the speed the dialog shows.
        self.settings.serial_baud = baud.min(i32::MAX as u32) as i32;
        let applied = self.reset_serial(|p| p.baud = baud);
        if applied {
            // `CmdSetBaud` posts `WM_USER_CHANGETITLE` immediately after the
            // reset (`ttdde.c:988`). The payload is still the terminal title;
            // the event is the edge which tells a frontend to ask for the new
            // transport speed while composing `TitleFormat`.
            self.events.push(Event::Title(self.vt.window_title()));
        }
        applied
    }

    /// `setflowctrl`.
    ///
    /// **Applied to the port, which upstream does not do** — `CmdSetFlowCtrl`
    /// assigns `ts.Flow` and stops there (`ttdde.c:1002`), with no
    /// `CommResetSerial` under it and no other path that would apply one. So
    /// upstream's `setflowctrl 2` leaves the port running with whatever flow
    /// control it was opened with until something else resets it — a
    /// `setbaud`, or the serial dialog's OK — while `setflowctrl.html` says
    /// flatly that the command "change[s] flow control".
    ///
    /// This is the third place the port follows the manual instead of the
    /// code, after `logwrite` while paused and `/NOLOG`, and the reason is
    /// that the gap loses data: a script that turns hardware handshaking on
    /// before sending a large paste, and does not get it, drops bytes on a
    /// real cable. The documented idiom for the neighbouring commands —
    /// `setflowctrl 3` so that `setdtr` passes its "flow control is none"
    /// guard — is the other half: under upstream's version the guard opens
    /// while the driver still has `CRTSCTS`, and the pin is then being driven
    /// by two things at once. (Measured on the FTDI rig: the by-hand clear
    /// does move RTS, so this is a fight rather than a silent failure — the
    /// driver raises it again when it wants to.) Twenty-eighth on the list of
    /// upstream defects; `PLAN.md` has it.
    pub fn set_flow_control(&mut self, flow: FlowControl) -> bool {
        if self.serial().is_none() {
            return false;
        }
        self.settings.serial_flow = match flow {
            FlowControl::None => SerialFlow::None,
            FlowControl::XonXoff => SerialFlow::XonXoff,
            FlowControl::RtsCts => SerialFlow::Hardware,
            FlowControl::DsrDtr => SerialFlow::DsrDtr,
        };
        self.reset_serial(|p| p.flow = flow)
    }

    /// `getmodemstatus` — CTS, DSR, RI and CD as one snapshot.
    ///
    /// `None` is "could not ask", which is a connection that is not serial and
    /// a `TIOCMGET` that failed — upstream's two ways of reaching
    /// `DDE_FNOTPROCESSED` here, and the one place in this file where the
    /// ioctl's own failure is passed on rather than dropped.
    pub fn modem_lines(&mut self) -> Option<ModemLines> {
        self.serial().and_then(|p| p.modem_lines().ok())
    }

    /// The serial port under the connection, if that is what it is.
    fn serial(&mut self) -> Option<&mut tt_conn::serial::SerialConn> {
        self.conn.as_mut().and_then(|c| c.as_serial())
    }

    /// ...and only while the control lines are ours to drive, which is
    /// `setdtr`'s and `setrts`'s extra guard.
    fn hand_driven_port(&mut self) -> Option<&mut tt_conn::serial::SerialConn> {
        match self.settings.serial_flow {
            SerialFlow::None => self.serial(),
            _ => None,
        }
    }

    /// `CommResetSerial` — change one thing and put the whole set back.
    ///
    /// Upstream builds the `DCB` out of `ts` and nothing else, because there
    /// every field in it *is* a setting and the port was opened from the same
    /// struct. Here the two can disagree: [`Session::connect`] takes any
    /// transport, and the shell's `--port`/`--baud` line opens one from
    /// [`SerialParams`] the settings never saw. Re-applying the settings would
    /// then change four things the command never mentioned — a `setbaud` that
    /// silently moves a 115200 port to the 9600 in the file, and takes the
    /// parity with it.
    ///
    /// So the **port** is the truth about itself and the caller edits one
    /// field of it. What is kept from upstream is the shape: it is still a
    /// whole `tcsetattr` rather than a poke at one flag, so DTR and RTS are
    /// re-asserted from [`SerialParams::dtr`] and [`SerialParams::rts`] on the
    /// way past — which is `dcb.fDtrControl` doing the same thing, and is why
    /// a `setdtr 0` before a `setbaud` does not survive it.
    fn reset_serial(&mut self, change: impl FnOnce(&mut SerialParams)) -> bool {
        let Some(port) = self.serial() else {
            return false;
        };
        let mut params = *port.params();
        change(&mut params);
        port.apply(&params).is_ok()
    }
}

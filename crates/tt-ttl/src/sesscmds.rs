//! The session — the link, the connection, the control lines and the
//! transfers.
//!
//! Upstream these are the *thin* commands: almost every one is two lines of
//! `ttl.cpp` handing a one-byte opcode to `SendCmnd`, and all the work is on
//! the other side of the DDE conversation in `teraterm/ttdde.c`. The thinness
//! is the trap. `SendCmnd` is where the link check lives, so a command that
//! never mentions `Linked` still fails with `ErrLinkFirst`; `IdTTLWaitCmndEnd`
//! and `IdTTLWaitCmndResult` are the difference between a command that reports
//! and one that does not; and `DDE_FNOTPROCESSED` — which is what the terminal
//! answers when the port is not serial — reads to the macro as success.
//!
//! All three are reproduced. What is *not* reproduced is the process
//! boundary: `connect` cannot spawn a second terminal here, because the
//! terminal is the caller.

use std::time::Duration;

use crate::error::{TtlError, TtlResult};
use crate::expr;
use crate::host::{FlowControl, ScriptHost, Xfer, XmodemOpt};
use crate::interp::Interp;
use crate::rsv::Rsv;

impl Interp {
    /// Dispatch for the commands in this file. `None` means "not one of mine".
    pub(crate) fn session_command(
        &mut self,
        host: &mut dyn ScriptHost,
        w: Rsv,
    ) -> Option<TtlResult<()>> {
        Some(match w {
            // --- the link ---
            Rsv::Connect => self.cmd_connect(host, false),
            Rsv::CygConnect => self.cmd_connect(host, true),
            Rsv::Disconnect => self.cmd_disconnect(host),
            Rsv::TestLink => self.cmd_test_link(host),
            Rsv::Unlink => self.cmd_unlink(host),
            Rsv::CloseTT => self.cmd_close_tt(host),
            Rsv::SetSync => self.cmd_set_sync(host),

            // --- the control lines ---
            Rsv::SetDtr => self.cmd_set_line(host, true),
            Rsv::SetRts => self.cmd_set_line(host, false),
            Rsv::SetBaud => self.cmd_set_baud(host),
            Rsv::SetFlowCtrl => self.cmd_set_flow_ctrl(host),
            Rsv::GetModemStatus => self.cmd_get_modem_status(host),
            Rsv::SendBreak => self.cmd_send_break(host),

            // --- the transfers ---
            Rsv::XmodemSend => self.cmd_xmodem_send(host),
            Rsv::XmodemRecv => self.cmd_xmodem_recv(host),
            Rsv::YmodemSend => self.xfer_file(host, |p| Xfer::YmodemSend { path: p }),
            Rsv::YmodemRecv => self.xfer_bare(host, Xfer::YmodemRecv),
            Rsv::ZmodemSend => self.cmd_zmodem_send(host),
            Rsv::ZmodemRecv => self.xfer_bare(host, Xfer::ZmodemRecv),
            Rsv::KmtSend => self.xfer_file(host, |p| Xfer::KmtSend { path: p }),
            Rsv::KmtRecv => self.xfer_bare(host, Xfer::KmtRecv),
            Rsv::KmtGet => self.xfer_file(host, |p| Xfer::KmtGet { path: p }),
            Rsv::KmtFinish => self.xfer_bare(host, Xfer::KmtFinish),
            Rsv::BPlusSend => self.xfer_file(host, |p| Xfer::BPlusSend { path: p }),
            Rsv::BPlusRecv => self.xfer_bare(host, Xfer::BPlusRecv),
            Rsv::QuickVANSend => self.xfer_file(host, |p| Xfer::QuickVanSend { path: p }),
            Rsv::QuickVANRecv => self.xfer_bare(host, Xfer::QuickVanRecv),
            Rsv::SendFile => self.cmd_send_file(host),
            Rsv::RecvFile => self.cmd_recv_file(host),

            _ => return None,
        })
    }

    // ---- the three argument shapes upstream shares ----

    /// `TTLCommCmd` — no arguments.
    ///
    /// The order of the two checks is upstream's and is visible: a trailing
    /// token gives `ErrSyntax` even with no terminal attached, where `send`
    /// would have said `ErrLinkFirst` first.
    pub(crate) fn comm_cmd(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        self.end_of_line()?;
        if host.linked() {
            Ok(())
        } else {
            Err(TtlError::LinkFirst)
        }
    }

    /// `TTLCommCmdInt` — one integer, which upstream renders to decimal and
    /// the terminal reads back with `atoi`.
    pub(crate) fn comm_cmd_int(&mut self, host: &mut dyn ScriptHost) -> TtlResult<i32> {
        let v = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        self.comm_cmd(host)?;
        Ok(v)
    }

    /// `TTLCommCmdFile` — one string, which must not be empty.
    pub(crate) fn comm_cmd_file(&mut self, host: &mut dyn ScriptHost) -> TtlResult<Vec<u8>> {
        let s = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        if s.is_empty() {
            return Err(TtlError::Syntax);
        }
        self.comm_cmd(host)?;
        Ok(s)
    }

    // ---- the link ----

    /// `connect` / `cygconnect`.
    ///
    /// `result` is the same three-value answer `testlink` gives, read back
    /// afterwards: 0 no terminal, 1 a terminal with no connection, 2 both. An
    /// already-connected terminal is left alone — upstream returns before it
    /// sends anything, and the documentation says the command "is ignored".
    ///
    /// `connect` requires its argument and `cygconnect`'s is optional, which
    /// is the only difference between them upstream keeps in this function;
    /// the rest is which executable it would have launched, and there is no
    /// launching here.
    fn cmd_connect(&mut self, host: &mut dyn ScriptHost, cygwin: bool) -> TtlResult<()> {
        let cmdline = if !cygwin || self.lx.parameter_given() {
            expr::get_str_val(&mut self.lx, &mut self.vars)?
        } else {
            Vec::new()
        };
        self.end_of_line()?;

        if host.linked() && host.com_ready() {
            self.set_result(2);
            return Ok(());
        }
        host.connect(&cmdline, cygwin)?;
        let code = self.link_state(host);
        self.set_result(code);
        Ok(())
    }

    /// `disconnect [<confirm>]`. The argument defaults to **1**, so a bare
    /// `disconnect` is the one that puts the dialog up.
    fn cmd_disconnect(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let confirm = if self.lx.parameter_given() {
            expr::get_int_val(&mut self.lx, &mut self.vars)?
        } else {
            1
        };
        self.comm_cmd(host)?;
        host.disconnect(confirm != 0)
    }

    fn cmd_test_link(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        self.end_of_line()?;
        let code = self.link_state(host);
        self.set_result(code);
        Ok(())
    }

    /// The number `testlink` and `connect` both report.
    fn link_state(&mut self, host: &mut dyn ScriptHost) -> i32 {
        if !host.linked() {
            0
        } else if host.com_ready() {
            2
        } else {
            1
        }
    }

    /// `unlink`. Not an error without a link — upstream's `if (Linked)` simply
    /// falls through, so a macro may unlink twice.
    fn cmd_unlink(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        self.end_of_line()?;
        if host.linked() {
            host.unlink();
        }
        Ok(())
    }

    /// `closett` — close the terminal, then let go of it.
    fn cmd_close_tt(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        self.comm_cmd(host)?;
        host.close_terminal()?;
        host.unlink();
        Ok(())
    }

    fn cmd_set_sync(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let v = self.comm_cmd_int(host)?;
        host.set_sync(v != 0);
        Ok(())
    }

    // ---- the control lines ----

    /// `setdtr` / `setrts`. The terminal tests the integer against zero, so
    /// any non-zero value raises the line.
    fn cmd_set_line(&mut self, host: &mut dyn ScriptHost, dtr: bool) -> TtlResult<()> {
        let v = self.comm_cmd_int(host)?;
        if dtr {
            host.set_dtr(v != 0);
        } else {
            host.set_rts(v != 0);
        }
        Ok(())
    }

    fn cmd_send_break(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        self.comm_cmd(host)?;
        host.send_break()
    }

    /// `setbaud` / `setspeed`. A value that is not positive is dropped without
    /// complaint: `ttdde.c:983` guards the assignment with `val > 0` and the
    /// terminal answers as though it had worked.
    fn cmd_set_baud(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let v = self.comm_cmd_int(host)?;
        if v > 0 {
            host.set_baud(v as u32);
        }
        Ok(())
    }

    /// `setflowctrl`. A value outside 1..=4 does nothing at all — the
    /// terminal's `switch` has no arm for it (`ttdde.c:1002`) — and is not an
    /// error.
    fn cmd_set_flow_ctrl(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let v = self.comm_cmd_int(host)?;
        if let Some(flow) = FlowControl::from_code(v) {
            host.set_flow_control(flow);
        }
        Ok(())
    }

    /// `getmodemstatus <intvar>` — CTS, DSR, RI and RLSD as bits 1, 2, 4, 8.
    ///
    /// **`result` is always 0, including when it failed**, and that is
    /// upstream's. The documentation promises 1 on failure and `TTLGetModemStatus`
    /// has the arm that would set it, but the arm is unreachable: it fires on
    /// a non-zero return from `GetTTParam`, which returns `ErrLinkFirst` or
    /// nothing else — and the `ErrLinkFirst` case has already been taken three
    /// lines earlier. When the terminal declines, because the port is not
    /// serial or `GetCommModemStatus` failed, `GetTTParam` leaves the buffer
    /// alone and returns 0; the buffer was `memset` to zero, `atoi("")` is 0,
    /// and the macro is told all four lines are low. `GetTTParam`'s own
    /// comment at `ttmdde.c:1067` says the transaction failing *should* be an
    /// error, and the `return 0` under it says it is not.
    ///
    /// Reproduced rather than fixed: a script that reads `result` and believes
    /// it has been doing so for a decade.
    fn cmd_get_modem_status(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let var = expr::get_int_var(&mut self.lx, &mut self.vars)?;
        self.comm_cmd(host)?;
        let n = match host.modem_lines() {
            Some(m) => {
                i32::from(m.cts)
                    | i32::from(m.dsr) << 1
                    | i32::from(m.ring) << 2
                    | i32::from(m.carrier) << 3
            }
            None => 0,
        };
        self.vars.set_int(var, n);
        self.set_result(0);
        Ok(())
    }

    // ---- the transfers ----

    /// Run a transfer and report it. Every protocol command sets `result` to 1
    /// on success and 0 otherwise (`filesys_proto.cpp:442`).
    fn run_xfer(&mut self, host: &mut dyn ScriptHost, req: &Xfer<'_>) -> TtlResult<()> {
        let ok = host.transfer(req)?;
        self.set_result(i32::from(ok));
        Ok(())
    }

    /// A transfer command with no arguments: the receiving half of everything
    /// except XMODEM, plus `kmtfinish`.
    fn xfer_bare(&mut self, host: &mut dyn ScriptHost, req: Xfer<'static>) -> TtlResult<()> {
        self.comm_cmd(host)?;
        self.run_xfer(host, &req)
    }

    /// A transfer command whose only argument is a filename.
    fn xfer_file(
        &mut self,
        host: &mut dyn ScriptHost,
        make: fn(&[u8]) -> Xfer<'_>,
    ) -> TtlResult<()> {
        let path = self.comm_cmd_file(host)?;
        self.run_xfer(host, &make(&path))
    }

    /// `xmodemsend <file> <option>`.
    ///
    /// The option is folded to 1K-or-not, because that is all the sender
    /// decides: `Xopt1kCRC` survives and **everything else becomes `XoptCRC`**,
    /// including a literal 1 for checksum. Whether the blocks carry a checksum
    /// or a CRC is the receiver's choice, announced in the character it opens
    /// with.
    fn cmd_xmodem_send(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let path = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let opt = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        if path.is_empty() {
            return Err(TtlError::Syntax);
        }
        self.comm_cmd(host)?;
        let opt = match opt {
            3 => XmodemOpt::Crc1K,
            _ => XmodemOpt::Crc,
        };
        self.run_xfer(host, &Xfer::XmodemSend { path: &path, opt })
    }

    /// `xmodemrecv <file> <binary> <option>`.
    ///
    /// The mirror of the fold in `xmodemsend`, and the other way round: the
    /// receiver picks checksum or CRC and cannot pick the block size, so a 3
    /// means CRC — upstream comments the arm "for compatibility" — and
    /// anything unrecognised falls back to `XoptCheck` rather than to CRC.
    fn cmd_xmodem_recv(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let path = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let binary = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        let opt = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        if path.is_empty() {
            return Err(TtlError::Syntax);
        }
        self.comm_cmd(host)?;
        let opt = match opt {
            2 | 3 => XmodemOpt::Crc,
            _ => XmodemOpt::Checksum,
        };
        self.run_xfer(
            host,
            &Xfer::XmodemRecv {
                path: &path,
                binary: binary != 0,
                opt,
            },
        )
    }

    fn cmd_zmodem_send(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let path = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let binary = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        if path.is_empty() {
            return Err(TtlError::Syntax);
        }
        self.comm_cmd(host)?;
        self.run_xfer(
            host,
            &Xfer::ZmodemSend {
                path: &path,
                binary: binary != 0,
            },
        )
    }

    /// `sendfile <file> <binary>` — the File menu's "Send file", not a
    /// protocol.
    ///
    /// **It sets no `result`.** Upstream waits on `IdTTLWaitCmndEnd` rather
    /// than `IdTTLWaitCmndResult`, and `FileSendEnd` reports 0 to a macro that
    /// is not listening for one — so a macro that checks `result` after a
    /// `sendfile` is reading whatever the command before it left there.
    fn cmd_send_file(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let path = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let binary = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        if path.is_empty() {
            return Err(TtlError::Syntax);
        }
        self.comm_cmd(host)?;
        host.transfer(&Xfer::SendFile {
            path: &path,
            binary: binary != 0,
        })?;
        Ok(())
    }

    /// `recvfile <file> <binary> <auto-stop wait time>`.
    ///
    /// The binary flag is parsed and thrown away — `TTLRecvFile` overwrites it
    /// with 1 on the next line and the documentation says so: the data is
    /// written unchanged whatever the argument. A negative wait time is
    /// floored at zero, and zero means wait for ever.
    fn cmd_recv_file(&mut self, host: &mut dyn ScriptHost) -> TtlResult<()> {
        let path = expr::get_str_val(&mut self.lx, &mut self.vars)?;
        let _binary = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        let autostop = expr::get_int_val(&mut self.lx, &mut self.vars)?;
        if path.is_empty() {
            return Err(TtlError::Syntax);
        }
        self.comm_cmd(host)?;
        self.run_xfer(
            host,
            &Xfer::RecvFile {
                path: &path,
                autostop: Duration::from_secs(autostop.max(0) as u64),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::host::{ModemLines, RecordingHost};
    use crate::interp::Interp;
    use crate::TtlError;

    fn run(src: &str) -> RecordingHost {
        let mut host = RecordingHost::new();
        host.linked = true;
        let mut it = Interp::new("t.ttl", src.as_bytes().to_vec(), &mut host);
        it.run(&mut host);
        host
    }

    fn out(src: &str) -> String {
        let h = run(src);
        assert!(h.errors.is_empty(), "unexpected errors: {:?}", h.errors);
        String::from_utf8_lossy(&h.output).into_owned()
    }

    #[test]
    fn testlink_reports_the_link_and_the_connection_as_one_number() {
        let mut host = RecordingHost::new();
        let mut it = Interp::new("t.ttl", b"testlink\ndispstr result".to_vec(), &mut host);
        it.run(&mut host);
        assert_eq!(host.output, b"0", "no terminal at all");

        assert_eq!(out("testlink\ndispstr result"), "1", "a terminal, no line");
        assert_eq!(out("connect 'host'\ntestlink\ndispstr result"), "2", "both");
    }

    #[test]
    fn connect_leaves_an_open_connection_alone() {
        let h = run("connect 'first'\nconnect 'second'\ndispstr result");
        assert_eq!(h.output, b"2");
        assert_eq!(
            h.connects.len(),
            1,
            "the second connect is ignored, not queued"
        );
        assert_eq!(h.connects[0], (b"first".to_vec(), false));
    }

    #[test]
    fn a_connect_that_did_not_come_up_reports_one() {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.connect_fails = true;
        let mut it = Interp::new(
            "t.ttl",
            b"connect 'host'\ndispstr result".to_vec(),
            &mut host,
        );
        it.run(&mut host);
        assert_eq!(host.output, b"1");
    }

    #[test]
    fn cygconnect_may_be_given_nothing_and_connect_may_not() {
        let h = run("cygconnect");
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(h.connects[0], (Vec::new(), true));

        let h = run("connect");
        assert_eq!(h.errors[0].0, TtlError::Syntax);
    }

    #[test]
    fn disconnect_defaults_to_asking_first() {
        let h = run("connect 'x'\ndisconnect");
        assert_eq!(h.disconnects, [true]);
        // `disconnect` writes no `result` of its own — upstream waits on
        // nothing — so what it did has to be asked for afterwards.
        let h = run("connect 'x'\ndisconnect 0\ntestlink\ndispstr result");
        assert_eq!(h.disconnects, [false]);
        assert_eq!(h.output, b"1", "the terminal stays, the line goes");
    }

    #[test]
    fn unlink_is_idempotent_and_closett_is_not_available_after_it() {
        let h = run("unlink\nunlink");
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(h.unlinks, 1, "the second sees no link and does nothing");

        let h = run("unlink\nclosett");
        assert_eq!(h.errors[0].0, TtlError::LinkFirst);

        let h = run("closett");
        assert_eq!(h.closes, 1);
        assert_eq!(h.unlinks, 1, "closing the terminal lets go of it too");
    }

    #[test]
    fn nothing_in_the_family_works_without_a_link() {
        for src in [
            "sendbreak",
            "setdtr 1",
            "setbaud 9600",
            "setsync 1",
            "disconnect",
            "zmodemrecv",
            "zmodemsend 'f' 1",
            "getmodemstatus n",
        ] {
            let mut host = RecordingHost::new();
            let mut it = Interp::new("t.ttl", src.as_bytes().to_vec(), &mut host);
            it.run(&mut host);
            assert_eq!(
                host.errors.first().map(|e| e.0),
                Some(TtlError::LinkFirst),
                "{src}"
            );
        }
    }

    #[test]
    fn a_trailing_token_is_a_syntax_error_before_it_is_a_link_error() {
        // `TTLCommCmd` checks the line before it checks the link, where `send`
        // checks the link first. Both orders are upstream's.
        let mut host = RecordingHost::new();
        let mut it = Interp::new("t.ttl", b"sendbreak junk".to_vec(), &mut host);
        it.run(&mut host);
        assert_eq!(host.errors[0].0, TtlError::Syntax);
    }

    #[test]
    fn the_control_lines_reach_the_host_in_order() {
        let h = run("setdtr 1\nsetrts 0\nsetbaud 115200\nsetspeed 9600\nsendbreak");
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(
            h.lines,
            ["dtr=1", "rts=0", "baud=115200", "baud=9600", "break"]
        );
    }

    #[test]
    fn a_baud_that_is_not_positive_is_dropped_without_complaint() {
        let h = run("setbaud 0\nsetbaud -1\nsetbaud 300");
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(h.lines, ["baud=300"]);
    }

    #[test]
    fn setflowctrl_takes_four_values_and_ignores_the_rest() {
        let h = run("setflowctrl 1\nsetflowctrl 2\nsetflowctrl 3\nsetflowctrl 4");
        assert_eq!(
            h.lines,
            ["flow=XonXoff", "flow=RtsCts", "flow=None", "flow=DsrDtr"]
        );
        let h = run("setflowctrl 0\nsetflowctrl 5");
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert!(h.lines.is_empty(), "out of range is a no-op, not an error");
    }

    #[test]
    fn getmodemstatus_packs_the_four_lines_into_bits() {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.modem = Some(ModemLines {
            cts: true,
            dsr: false,
            ring: false,
            carrier: true,
        });
        let src = b"n = 0\ngetmodemstatus n\ndispstr n".to_vec();
        let mut it = Interp::new("t.ttl", src, &mut host);
        it.run(&mut host);
        assert!(host.errors.is_empty(), "{:?}", host.errors);
        assert_eq!(host.output, b"9", "CTS is 1 and RLSD is 8");
    }

    #[test]
    fn a_port_that_cannot_answer_looks_exactly_like_one_with_every_line_low() {
        // Upstream's dead `SetResult(1)` arm, reproduced. See the command.
        let h = run("n = 7\ngetmodemstatus n\ndispstr result");
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(h.output, b"0", "and it overwrote n with 0 on the way");
    }

    #[test]
    fn every_protocol_reaches_the_host_with_its_own_arguments() {
        let h = run(concat!(
            "xmodemsend 'a' 3\n",
            "xmodemrecv 'b' 1 2\n",
            "ymodemsend 'c'\n",
            "ymodemrecv\n",
            "zmodemsend 'd' 1\n",
            "zmodemrecv\n",
            "kmtsend 'e'\nkmtrecv\nkmtget 'f'\nkmtfinish\n",
            "bplussend 'g'\nbplusrecv\n",
            "quickvansend 'h'\nquickvanrecv\n",
        ));
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(h.transfers.len(), 14);
        assert!(h.transfers[0].contains("XmodemSend"), "{}", h.transfers[0]);
        assert!(h.transfers[0].contains("Crc1K"), "{}", h.transfers[0]);
        assert!(
            h.transfers[1].contains("binary: true"),
            "{}",
            h.transfers[1]
        );
        assert!(h.transfers[3].contains("YmodemRecv"), "{}", h.transfers[3]);
        assert!(h.transfers[9].contains("KmtFinish"), "{}", h.transfers[9]);
    }

    #[test]
    fn the_xmodem_option_folds_differently_on_each_side() {
        // Send: only 1K survives, and a literal "checksum" becomes CRC.
        let h = run("xmodemsend 'f' 1\nxmodemsend 'f' 2\nxmodemsend 'f' 3\nxmodemsend 'f' 9");
        let opts: Vec<&str> = h
            .transfers
            .iter()
            .map(|t| if t.contains("Crc1K") { "1k" } else { "crc" })
            .collect();
        assert_eq!(opts, ["crc", "crc", "1k", "crc"]);

        // Receive: 1K means CRC, and anything unrecognised means checksum.
        let h =
            run("xmodemrecv 'f' 1 1\nxmodemrecv 'f' 1 2\nxmodemrecv 'f' 1 3\nxmodemrecv 'f' 1 9");
        let opts: Vec<&str> = h
            .transfers
            .iter()
            .map(|t| if t.contains("Crc") { "crc" } else { "check" })
            .collect();
        assert_eq!(opts, ["check", "crc", "crc", "check"]);
    }

    #[test]
    fn a_transfer_reports_one_for_success_and_zero_for_failure() {
        assert_eq!(out("zmodemsend 'f' 1\ndispstr result"), "1");

        let mut host = RecordingHost::new();
        host.linked = true;
        host.transfer_fails = true;
        let src = b"zmodemsend 'f' 1\ndispstr result".to_vec();
        let mut it = Interp::new("t.ttl", src, &mut host);
        it.run(&mut host);
        assert_eq!(host.output, b"0");
    }

    #[test]
    fn sendfile_reports_nothing_at_all() {
        // `result` still holds what `testlink` put there: upstream waits on
        // the command *ending* rather than on a result, so nothing writes one.
        let h = run("testlink\nsendfile 'f' 1\ndispstr result");
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(h.output, b"1");
        assert!(h.transfers[0].contains("SendFile"), "{}", h.transfers[0]);
    }

    #[test]
    fn recvfile_floors_its_wait_at_zero_and_ignores_its_binary_flag() {
        // The brackets are not decoration: each argument is a whole
        // expression, so a bare `0 -5` is one argument worth minus five and
        // the command then runs out of parameters. Upstream parses the same
        // way, and every multi-integer command has the same edge.
        let h = run("recvfile 'f' 0 (-5)\nrecvfile 'g' 0 3\ndispstr result");
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert!(
            h.transfers[0].contains("autostop: 0ns"),
            "{}",
            h.transfers[0]
        );
        assert!(
            h.transfers[1].contains("autostop: 3s"),
            "{}",
            h.transfers[1]
        );
        assert_eq!(h.output, b"1");
    }

    #[test]
    fn a_transfer_command_needs_a_filename_that_is_not_empty() {
        for src in ["zmodemsend '' 1", "kmtsend ''", "xmodemsend '' 3"] {
            let h = run(src);
            assert_eq!(
                h.errors.first().map(|e| e.0),
                Some(TtlError::Syntax),
                "{src}"
            );
            assert!(h.transfers.is_empty(), "{src}");
        }
    }

    #[test]
    fn setsync_is_a_flag_and_nothing_more() {
        let h = run("setsync 1\nsetsync 0\nsetsync 42");
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(h.syncs, [true, false, true]);
    }
}

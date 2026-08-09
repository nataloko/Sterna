//! The serial control lines, and the two commands that pace what `send`
//! writes.
//!
//! Every one of these is a no-op unless the connection is serial, and
//! `setdtr`/`setrts` need the flow control to be "none" as well. None of that
//! is visible from here: the terminal answers `DDE_FNOTPROCESSED`, the macro
//! carries on, and a host declining quietly is the faithful shape — see
//! [`ScriptHost`](tt_ttl::ScriptHost)'s note on the family.
//!
//! **Two of them are stricter than TTL, in the same direction.** `setbaud 0`
//! and `setflowctrl 7` are both silently dropped upstream, because the
//! terminal's `switch` has no arm for them and a DDE command that matches
//! nothing is not an error. A script that says either meant something, and
//! saying nothing is the worst of the three possible answers.

use mlua::{Scope, Table};
use tt_ttl::FlowControl;

use crate::{choice, lua_err, Host};

pub(crate) fn install<'s, 'e>(
    scope: &'s Scope<'s, 'e>,
    tt: &Table,
    host: &'e Host<'e>,
) -> mlua::Result<()> {
    tt.set(
        "setdtr",
        scope.create_function(move |_, on: bool| {
            crate::conn::link(host)?;
            host.borrow_mut().set_dtr(on);
            Ok(())
        })?,
    )?;
    tt.set(
        "setrts",
        scope.create_function(move |_, on: bool| {
            crate::conn::link(host)?;
            host.borrow_mut().set_rts(on);
            Ok(())
        })?,
    )?;
    tt.set(
        "setbaud",
        scope.create_function(move |_, baud: i64| {
            crate::conn::link(host)?;
            if baud <= 0 {
                return Err(mlua::Error::runtime(format!("baud {baud} is not a speed")));
            }
            host.borrow_mut().set_baud(baud as u32);
            Ok(())
        })?,
    )?;
    tt.set(
        "setflowctrl",
        scope.create_function(move |_, name: String| {
            let flow = choice(
                &name,
                "flow control",
                &[
                    ("xon", FlowControl::XonXoff),
                    ("rts", FlowControl::RtsCts),
                    ("none", FlowControl::None),
                    ("dsr", FlowControl::DsrDtr),
                ],
            )?;
            crate::conn::link(host)?;
            host.borrow_mut().set_flow_control(flow);
            Ok(())
        })?,
    )?;

    // A table rather than TTL's bit mask, and `nil` rather than four zeroes
    // when the port cannot answer. Upstream cannot tell those apart —
    // `getmodemstatus` reports 0 for "not a serial port" and 0 for "all four
    // lines low", and `result` is 0 either way because the arm that would say
    // otherwise is unreachable. A modem script that tests carrier deserves to
    // know which it got.
    tt.set(
        "getmodemstatus",
        scope.create_function(move |lua, ()| {
            crate::conn::link(host)?;
            let lines = host.borrow_mut().modem_lines();
            match lines {
                None => Ok(None),
                Some(m) => {
                    let t = lua.create_table()?;
                    t.set("cts", m.cts)?;
                    t.set("dsr", m.dsr)?;
                    t.set("ring", m.ring)?;
                    t.set("carrier", m.carrier)?;
                    Ok(Some(t))
                }
            }
        })?,
    )?;

    // `true` if the connection took it, which is `false` for anything that is
    // not serial rather than a refusal — the same shape as the control lines.
    tt.set(
        "setserialdelaychar",
        scope.create_function(move |_, ms: i32| {
            crate::conn::link(host)?;
            host.borrow_mut()
                .set_serial_delay(false, ms)
                .map_err(lua_err)
        })?,
    )?;
    tt.set(
        "setserialdelayline",
        scope.create_function(move |_, ms: i32| {
            crate::conn::link(host)?;
            host.borrow_mut()
                .set_serial_delay(true, ms)
                .map_err(lua_err)
        })?,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::tests::run;
    use tt_ttl::ModemLines;

    #[test]
    fn the_control_lines_reach_the_host_in_order() {
        let (host, r) = run("tt.setdtr(true); tt.setrts(false); tt.setbaud(115200)");
        r.unwrap();
        assert_eq!(host.lines, ["dtr=1", "rts=0", "baud=115200"]);
    }

    #[test]
    fn flow_control_is_named_rather_than_numbered() {
        let (host, r) = run("tt.setflowctrl('rts')");
        r.unwrap();
        assert_eq!(host.lines, ["flow=RtsCts"]);
    }

    /// The divergence: upstream drops both of these without a word.
    #[test]
    fn a_meaningless_speed_or_flow_control_is_refused() {
        let (host, r) = run("tt.setbaud(0)");
        assert!(r.unwrap_err().to_string().contains("not a speed"));
        assert!(host.lines.is_empty());

        let (host, r) = run("tt.setflowctrl('rtscts')");
        let msg = r.unwrap_err().to_string();
        assert!(
            msg.contains("flow control") && msg.contains("rtscts"),
            "{msg}"
        );
        assert!(host.lines.is_empty());
    }

    #[test]
    fn modem_status_is_four_names_not_a_bit_mask() {
        let mut host = tt_ttl::RecordingHost::new();
        host.linked = true;
        host.modem = Some(ModemLines {
            cts: true,
            dsr: false,
            ring: false,
            carrier: true,
        });
        let src = "local m = tt.getmodemstatus(); tt.dispstr(tostring(m.cts), tostring(m.carrier), tostring(m.dsr))";
        crate::Script::new("t.lua", src.as_bytes().to_vec())
            .run(&mut host)
            .unwrap();
        assert_eq!(host.output, b"truetruefalse");
    }

    /// The half upstream cannot express: a port that could not be asked.
    #[test]
    fn a_port_that_cannot_answer_says_nil_rather_than_all_low() {
        let (host, r) = run("tt.dispstr(tostring(tt.getmodemstatus()))");
        r.unwrap();
        assert_eq!(host.output, b"nil");
    }
}

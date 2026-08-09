//! The sixteen transfer commands.
//!
//! One Lua function per command rather than a protocol plus a direction,
//! because the commands are not symmetrical and [`Xfer`] says so: `xmodemsend`
//! has no binary flag, the receiving halves of ZMODEM, YMODEM, Kermit, B-Plus
//! and Quick-VAN name no file at all, `kmtget` names the file on the *far*
//! machine, and `sendfile` is not a protocol. A single `tt.transfer{...}`
//! would need a table whose fields are wrong for two thirds of the callers.
//!
//! Each returns `true` when the file arrived. Each blocks — that is what the
//! macro thread is for.

use mlua::{BString, Scope, Table};
use tt_ttl::{Xfer, XmodemOpt};

use crate::conn::link;
use crate::{choice, lua_err, Host};

/// Run one, having checked there is something to run it over.
///
/// The link check is the language's, as it is for `send`: upstream's
/// `TTLCommCmd` makes it before dispatch, so `zmodemsend` with no terminal is
/// `ErrLinkFirst` rather than a protocol talking to nothing.
fn run(host: &Host<'_>, req: &Xfer<'_>) -> mlua::Result<bool> {
    link(host)?;
    host.borrow_mut().transfer(req).map_err(lua_err)
}

/// `xmodemsend`'s option, which is only ever "1K blocks or not".
///
/// The sender does not choose between checksum and CRC — the receiver does,
/// in the character it opens with — so upstream folds everything that is not
/// a literal 3 to CRC. Naming the two arms is the same fold with the dead
/// third value gone.
fn send_opt(name: Option<String>) -> mlua::Result<XmodemOpt> {
    match name {
        None => Ok(XmodemOpt::Crc),
        Some(n) => choice(
            &n,
            "xmodem option",
            &[("crc", XmodemOpt::Crc), ("1k", XmodemOpt::Crc1K)],
        ),
    }
}

/// And the receiver's, which is the mirror: it picks checksum or CRC and
/// cannot pick the block size, so there is no `1k` here.
fn recv_opt(name: Option<String>) -> mlua::Result<XmodemOpt> {
    match name {
        None => Ok(XmodemOpt::Checksum),
        Some(n) => choice(
            &n,
            "xmodem option",
            &[("checksum", XmodemOpt::Checksum), ("crc", XmodemOpt::Crc)],
        ),
    }
}

pub(crate) fn install<'s, 'e>(
    scope: &'s Scope<'s, 'e>,
    tt: &Table,
    host: &'e Host<'e>,
) -> mlua::Result<()> {
    tt.set(
        "xmodemsend",
        scope.create_function(move |_, (path, opt): (BString, Option<String>)| {
            run(
                host,
                &Xfer::XmodemSend {
                    path: &path,
                    opt: send_opt(opt)?,
                },
            )
        })?,
    )?;
    tt.set(
        "xmodemrecv",
        scope.create_function(
            move |_, (path, binary, opt): (BString, Option<bool>, Option<String>)| {
                run(
                    host,
                    &Xfer::XmodemRecv {
                        path: &path,
                        binary: binary.unwrap_or(false),
                        opt: recv_opt(opt)?,
                    },
                )
            },
        )?,
    )?;

    tt.set(
        "ymodemsend",
        scope.create_function(move |_, path: BString| {
            run(host, &Xfer::YmodemSend { path: &path })
        })?,
    )?;
    tt.set(
        "ymodemrecv",
        scope.create_function(move |_, ()| run(host, &Xfer::YmodemRecv))?,
    )?;

    tt.set(
        "zmodemsend",
        scope.create_function(move |_, (path, binary): (BString, Option<bool>)| {
            run(
                host,
                &Xfer::ZmodemSend {
                    path: &path,
                    binary: binary.unwrap_or(false),
                },
            )
        })?,
    )?;
    tt.set(
        "zmodemrecv",
        scope.create_function(move |_, ()| run(host, &Xfer::ZmodemRecv))?,
    )?;

    tt.set(
        "kmtsend",
        scope.create_function(move |_, path: BString| run(host, &Xfer::KmtSend { path: &path }))?,
    )?;
    tt.set(
        "kmtrecv",
        scope.create_function(move |_, ()| run(host, &Xfer::KmtRecv))?,
    )?;
    // The name is the **remote** one: `kermit.c:1160` takes its basename
    // before it goes in the `R` packet, so `tt.kmtget('sub/x')` asks the peer
    // for `x`.
    tt.set(
        "kmtget",
        scope.create_function(move |_, path: BString| run(host, &Xfer::KmtGet { path: &path }))?,
    )?;
    tt.set(
        "kmtfinish",
        scope.create_function(move |_, ()| run(host, &Xfer::KmtFinish))?,
    )?;

    tt.set(
        "bplussend",
        scope
            .create_function(move |_, path: BString| run(host, &Xfer::BPlusSend { path: &path }))?,
    )?;
    tt.set(
        "bplusrecv",
        scope.create_function(move |_, ()| run(host, &Xfer::BPlusRecv))?,
    )?;

    tt.set(
        "quickvansend",
        scope.create_function(move |_, path: BString| {
            run(host, &Xfer::QuickVanSend { path: &path })
        })?,
    )?;
    tt.set(
        "quickvanrecv",
        scope.create_function(move |_, ()| run(host, &Xfer::QuickVanRecv))?,
    )?;

    // Not a protocol: the file's bytes down the line, with CR/LF translation
    // and control-character stripping unless `binary`.
    tt.set(
        "sendfile",
        scope.create_function(move |_, (path, binary): (BString, Option<bool>)| {
            run(
                host,
                &Xfer::SendFile {
                    path: &path,
                    binary: binary.unwrap_or(false),
                },
            )
        })?,
    )?;
    // `autostop` is seconds of **quiet after something arrived**, not a
    // deadline: `raw.c:168` arms the timer inside the packet reader, so the
    // first byte starts the clock and a capture the host never answers waits
    // for ever. Nothing here can fix that — it is the vendored C — but a
    // script that knows it can set `tt.timeout` around the call.
    tt.set(
        "recvfile",
        scope.create_function(move |_, (path, autostop): (BString, Option<f64>)| {
            let secs = autostop.unwrap_or(0.0).max(0.0);
            run(
                host,
                &Xfer::RecvFile {
                    path: &path,
                    autostop: std::time::Duration::from_secs_f64(secs),
                },
            )
        })?,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::tests::run;

    #[test]
    fn a_send_carries_its_own_arguments() {
        let (host, r) = run("tt.zmodemsend('/tmp/f', true)");
        r.unwrap();
        assert_eq!(
            host.transfers,
            [r#"ZmodemSend { path: [47, 116, 109, 112, 47, 102], binary: true }"#]
        );
    }

    #[test]
    fn a_receive_that_names_nothing_takes_nothing() {
        let (host, r) = run("tt.zmodemrecv()");
        r.unwrap();
        assert_eq!(host.transfers, ["ZmodemRecv"]);
    }

    #[test]
    fn the_xmodem_options_are_named_and_default_the_way_upstream_folds_them() {
        let (host, r) = run("tt.xmodemsend('f'); tt.xmodemsend('f', '1k'); tt.xmodemrecv('f')");
        r.unwrap();
        assert!(
            host.transfers[0].contains("opt: Crc"),
            "{:?}",
            host.transfers
        );
        assert!(host.transfers[1].contains("opt: Crc1K"));
        assert!(host.transfers[2].contains("opt: Checksum"));
    }

    /// The receiver cannot choose the block size, so `1k` is not one of its
    /// options and saying so beats folding it to something else.
    #[test]
    fn a_receiver_asking_for_1k_is_told_rather_than_folded() {
        let (_, r) = run("tt.xmodemrecv('f', false, '1k')");
        assert!(r.unwrap_err().to_string().contains("xmodem option"));
    }

    #[test]
    fn a_transfer_reports_whether_the_file_arrived() {
        let (host, r) = run("tt.dispstr(tostring(tt.kmtrecv()))");
        r.unwrap();
        assert_eq!(host.output, b"true");

        let mut host = tt_ttl::RecordingHost::new();
        host.linked = true;
        host.transfer_fails = true;
        crate::Script::new("t.lua", b"tt.dispstr(tostring(tt.kmtrecv()))".to_vec())
            .run(&mut host)
            .unwrap();
        assert_eq!(host.output, b"false");
    }

    #[test]
    fn a_transfer_with_no_terminal_is_a_link_error() {
        let mut host = tt_ttl::RecordingHost::new();
        let r = crate::Script::new("t.lua", b"tt.zmodemrecv()".to_vec()).run(&mut host);
        assert!(r.unwrap_err().to_string().contains("Link macro first"));
        assert!(host.transfers.is_empty());
    }
}

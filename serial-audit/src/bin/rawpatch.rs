//! Spike 4, part 2 — can the gaps found in the audit be patched through
//! `serialport-rs`'s raw fd, or do they need a serial layer of our own?
//!
//! Tests each gap against real hardware and, critically, whether the patch
//! *survives* subsequent serialport-rs API calls.

use serialport::{SerialPort, TTYPort};
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::time::Duration;

const A: &str = "/dev/ttyUSB0";
const B: &str = "/dev/ttyUSB1";

// Linux-only, absent from the libc crate's default exports on some targets.
const CMSPAR: libc::tcflag_t = 0o10000000000;

// The raw-fd escape hatch is only available on the concrete platform type:
// `TTYPort` implements AsRawFd but `Box<dyn SerialPort>` does not. Any code
// reaching for it is therefore already platform-specific.
fn open(path: &str, baud: u32) -> serialport::Result<TTYPort> {
    TTYPort::open(&serialport::new(path, baud).timeout(Duration::from_millis(400)))
}

fn get_termios(fd: i32) -> libc::termios {
    unsafe {
        let mut t: libc::termios = std::mem::zeroed();
        libc::tcgetattr(fd, &mut t);
        t
    }
}

fn set_termios(fd: i32, t: &libc::termios) -> bool {
    unsafe { libc::tcsetattr(fd, libc::TCSANOW, t) == 0 }
}

fn res(pass: bool, what: &str, detail: &str) {
    println!(
        "  [{}] {:<40} {}",
        if pass { "OK  " } else { "NO  " },
        what,
        detail
    );
}

fn main() {
    println!("=== gap 1: MARK/SPACE parity via CMSPAR on the raw fd ===");
    match open(A, 9600) {
        Ok(p) => {
            let fd = p.as_raw_fd();
            let mut t = get_termios(fd);
            t.c_cflag |= libc::PARENB | CMSPAR;
            t.c_cflag &= !libc::PARODD; // CMSPAR + !PARODD = SPACE parity
            let applied = set_termios(fd, &t);
            let back = get_termios(fd);
            let stuck = back.c_cflag & CMSPAR != 0;
            res(applied && stuck, "set SPACE parity (CMSPAR, !PARODD)", "");

            t.c_cflag |= libc::PARODD; // CMSPAR + PARODD = MARK parity
            set_termios(fd, &t);
            let back = get_termios(fd);
            res(
                back.c_cflag & CMSPAR != 0 && back.c_cflag & libc::PARODD != 0,
                "set MARK parity (CMSPAR + PARODD)",
                "",
            );

            // Does the patch survive a serialport-rs call that rewrites termios?
            let mut p = p;
            p.set_baud_rate(19200).ok();
            let after = get_termios(fd);
            res(
                after.c_cflag & CMSPAR != 0,
                "CMSPAR survives set_baud_rate()",
                if after.c_cflag & CMSPAR != 0 {
                    "patch is stable"
                } else {
                    "CLOBBERED - serialport rewrites cflag wholesale"
                },
            );
        }
        Err(e) => println!("  open failed: {e}"),
    }

    println!("\n=== gap 2: DSR/DTR flow control (commlib.c fOutxDsrFlow) ===");
    println!("  Linux termios has CRTSCTS and IXON/IXOFF. There is no DSR flow-control bit");
    println!("  at all — this is a kernel limitation, not a serialport-rs omission.");
    res(
        false,
        "kernel-level DSR/DTR flow control",
        "must be emulated in userspace: poll DSR, gate writes",
    );

    println!("\n=== gap 3: XON/XOFF characters and thresholds ===");
    match open(A, 9600) {
        Ok(p) => {
            let fd = p.as_raw_fd();
            let mut t = get_termios(fd);
            t.c_cc[libc::VSTART] = 0x11;
            t.c_cc[libc::VSTOP] = 0x13;
            let applied = set_termios(fd, &t);
            let back = get_termios(fd);
            res(
                applied && back.c_cc[libc::VSTART] == 0x11 && back.c_cc[libc::VSTOP] == 0x13,
                "XON/XOFF characters (VSTART/VSTOP)",
                "settable via raw termios",
            );
            res(
                false,
                "XON/XOFF thresholds (XonLim/XoffLim)",
                "kernel owns its buffer watermarks; not settable on Linux",
            );
        }
        Err(e) => println!("  open failed: {e}"),
    }

    println!("\n=== gap 4: distinguishing an incoming BREAK from a real NUL ===");
    match (open(A, 9600), open(B, 9600)) {
        (Ok(mut a), Ok(mut b)) => {
            // PARMRK, with IGNBRK/BRKINT off, reports a break as FF 00 00
            // and doubles a legitimate FF byte.
            let fd = b.as_raw_fd();
            let mut t = get_termios(fd);
            t.c_iflag |= libc::PARMRK | libc::INPCK;
            t.c_iflag &= !(libc::IGNBRK | libc::BRKINT | libc::IGNPAR);
            let applied = set_termios(fd, &t);
            res(applied, "enable PARMRK on the receiving port", "");

            b.clear(serialport::ClearBuffer::Input).ok();
            a.write_all(b"X").ok();
            a.flush().ok();
            std::thread::sleep(Duration::from_millis(120));
            a.set_break().ok();
            std::thread::sleep(Duration::from_millis(250));
            a.clear_break().ok();
            std::thread::sleep(Duration::from_millis(120));
            // now a genuine NUL byte in the data stream
            a.write_all(&[0x00]).ok();
            a.write_all(b"Y").ok();
            a.flush().ok();
            std::thread::sleep(Duration::from_millis(300));

            let mut buf = [0u8; 64];
            let n = b.read(&mut buf).unwrap_or(0);
            let got = &buf[..n];
            println!("  raw bytes received: {:02x?}", got);
            let has_marker = got.windows(3).any(|w| w == [0xff, 0x00, 0x00]);
            res(
                has_marker,
                "break arrives as the FF 00 00 marker",
                "distinguishable from a plain 0x00 in the stream",
            );

            // Does PARMRK survive serialport-rs touching the port?
            b.set_baud_rate(19200).ok();
            let after = get_termios(fd);
            res(
                after.c_iflag & libc::PARMRK != 0,
                "PARMRK survives set_baud_rate()",
                if after.c_iflag & libc::PARMRK != 0 {
                    "patch is stable"
                } else {
                    "CLOBBERED - serialport rewrites iflag wholesale"
                },
            );
        }
        _ => println!("  could not open the pair"),
    }

    println!("\n=== does serialport-rs clobber foreign termios changes generally? ===");
    if let Ok(mut p) = open(A, 9600) {
        let fd = p.as_raw_fd();
        let mut t = get_termios(fd);
        t.c_iflag |= libc::PARMRK;
        set_termios(fd, &t);
        // Each entry pokes the port through a different serialport-rs call, to
        // see whether that call rewrites termios and undoes our PARMRK.
        type Poke = Box<dyn Fn(&mut TTYPort)>;
        let checks: [(&str, Poke); 4] = [
            ("write_request_to_send", Box::new(|p: &mut TTYPort| { p.write_request_to_send(true).ok(); })),
            ("set_flow_control", Box::new(|p: &mut TTYPort| { p.set_flow_control(serialport::FlowControl::Hardware).ok(); })),
            ("set_parity", Box::new(|p: &mut TTYPort| { p.set_parity(serialport::Parity::Even).ok(); })),
            ("set_timeout", Box::new(|p: &mut TTYPort| { p.set_timeout(Duration::from_millis(200)).ok(); })),
        ];
        for (name, f) in checks {
            let mut t = get_termios(fd);
            t.c_iflag |= libc::PARMRK;
            set_termios(fd, &t);
            f(&mut p);
            let after = get_termios(fd);
            res(
                after.c_iflag & libc::PARMRK != 0,
                &format!("PARMRK survives {name}()"),
                "",
            );
        }
    }

    println!("\ndone.");
}

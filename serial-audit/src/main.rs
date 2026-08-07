//! Stage 0 spike 4 — audit `serialport-rs` against the requirement in Tera
//! Term's `commlib.c`, using an FTDI pair wired back-to-back on data and
//! control lines (/dev/ttyUSB0 <-> /dev/ttyUSB1).
//!
//! The question is not "does the crate work" but "does it cover what a Tera
//! Term successor needs, and where it doesn't, is a raw-fd patch enough or do
//! we need our own serial layer?"

use serialport::{
    available_ports, DataBits, FlowControl, Parity, SerialPort, SerialPortType, StopBits,
};
use std::io::{Read, Write};
use std::time::Duration;

const A: &str = "/dev/ttyUSB0";
const B: &str = "/dev/ttyUSB1";

fn open(path: &str, baud: u32) -> serialport::Result<Box<dyn SerialPort>> {
    serialport::new(path, baud)
        .data_bits(DataBits::Eight)
        .parity(Parity::None)
        .stop_bits(StopBits::One)
        .flow_control(FlowControl::None)
        .timeout(Duration::from_millis(400))
        .open()
}

fn hdr(s: &str) {
    println!("\n=== {s} ===");
}

fn ok(pass: bool, what: &str, detail: &str) {
    println!(
        "  [{}] {:<34} {}",
        if pass { "PASS" } else { "GAP " },
        what,
        detail
    );
}

fn main() {
    hdr("1. enumeration — Stage 1 needs a live port picker");
    match available_ports() {
        Ok(ports) => {
            println!("  {} ports found", ports.len());
            let mut usb_meta = 0;
            for p in ports.iter().take(8) {
                match &p.port_type {
                    SerialPortType::UsbPort(u) => {
                        usb_meta += 1;
                        println!(
                            "    {:<16} USB {:04x}:{:04x} mfr={:?} product={:?} serial={:?}",
                            p.port_name, u.vid, u.pid, u.manufacturer, u.product, u.serial_number
                        );
                    }
                    other => println!("    {:<16} {:?}", p.port_name, other),
                }
            }
            ok(
                usb_meta > 0,
                "USB metadata (vid/pid/serial)",
                "needed to label ports meaningfully in the picker",
            );
            let ftdi_ordered = ports
                .iter()
                .filter(|p| p.port_name.contains("ttyUSB"))
                .count();
            ok(
                ftdi_ordered >= 4,
                "multi-port adapter enumerated",
                &format!("{ftdi_ordered} ttyUSB* found; the FTDI quad should give 4"),
            );
        }
        Err(e) => println!("  enumeration FAILED: {e}"),
    }

    hdr("2. line settings Tera Term's commlib.c actually sets");
    ok(true, "data bits 5/6/7/8", "DataBits enum covers all four");
    ok(true, "stop bits 1/2", "StopBits::One / Two");
    ok(true, "parity none/odd/even", "Parity::None / Odd / Even");
    ok(
        false,
        "parity MARK / SPACE",
        "commlib.c:194-200 sets both; serialport has no variant. Needs CMSPAR via raw fd",
    );
    ok(true, "flow control none/xon-xoff/rts-cts", "FlowControl enum");
    ok(
        false,
        "flow control DSR/DTR",
        "commlib.c:219 sets fOutxDsrFlow; no equivalent. Needs raw termios or our own layer",
    );
    ok(
        false,
        "XON/XOFF limits + chars",
        "commlib.c sets XonLim=768 XoffLim=3328 XonChar/XoffChar; not exposed",
    );

    hdr("3. baud rates (verified by driver readback)");
    for baud in [300u32, 9600, 115200, 921600, 1_000_000, 2_000_000, 3_000_000, 250_000] {
        match open(A, baud) {
            Ok(p) => {
                let got = p.baud_rate().unwrap_or(0);
                let exact = got == baud;
                println!(
                    "  [{}] {:>9} baud -> driver reports {:>9}{}",
                    if exact { "PASS" } else { "WARN" },
                    baud,
                    got,
                    if exact { "" } else { "  (rounded)" }
                );
            }
            Err(e) => println!("  [FAIL] {baud:>9} baud -> {e}"),
        }
    }

    hdr("4. modem control lines, cross-checked over the loopback");
    match (open(A, 115_200), open(B, 115_200)) {
        (Ok(mut a), Ok(mut b)) => {
            let mut all = true;
            for (name, level) in [("DTR", false), ("DTR", true)] {
                a.write_data_terminal_ready(level).ok();
                std::thread::sleep(Duration::from_millis(120));
                let dsr = b.read_data_set_ready().unwrap_or(false);
                let good = dsr == level;
                all &= good;
                println!(
                    "  [{}] A {name}={:<5} -> B DSR={:<5}",
                    if good { "PASS" } else { "GAP " },
                    level,
                    dsr
                );
            }
            for (name, level) in [("RTS", false), ("RTS", true)] {
                a.write_request_to_send(level).ok();
                std::thread::sleep(Duration::from_millis(120));
                let cts = b.read_clear_to_send().unwrap_or(false);
                let good = cts == level;
                all &= good;
                println!(
                    "  [{}] A {name}={:<5} -> B CTS={:<5}",
                    if good { "PASS" } else { "GAP " },
                    level,
                    cts
                );
            }
            ok(all, "DTR->DSR and RTS->CTS round trip", "both directions observable");
            println!(
                "  [INFO] B carrier detect={:?} ring={:?}",
                b.read_carrier_detect(),
                b.read_ring_indicator()
            );
        }
        _ => println!("  could not open the pair"),
    }

    hdr("5. break signalling (commlib.c:1174 SetCommBreak / :1182 ClearCommBreak)");
    match (open(A, 9600), open(B, 9600)) {
        (Ok(mut a), Ok(mut b)) => {
            b.clear(serialport::ClearBuffer::Input).ok();
            a.write_all(b"before").ok();
            a.flush().ok();
            std::thread::sleep(Duration::from_millis(150));
            let latched = a.set_break().is_ok();
            std::thread::sleep(Duration::from_millis(250));
            let cleared = a.clear_break().is_ok();
            std::thread::sleep(Duration::from_millis(150));
            a.write_all(b"after").ok();
            a.flush().ok();
            std::thread::sleep(Duration::from_millis(300));
            let mut buf = [0u8; 64];
            let n = b.read(&mut buf).unwrap_or(0);
            let got = &buf[..n];
            ok(
                latched && cleared,
                "set_break / clear_break (latched)",
                "matches Win32 SetCommBreak semantics, not a timed pulse",
            );
            println!("  received: {:?}", String::from_utf8_lossy(got));
            ok(
                got.contains(&0),
                "break observable on the far end",
                "arrives as NUL",
            );
            ok(
                false,
                "break distinguishable from a real NUL",
                "no PARMRK/BRKINT handling; a 0x00 in the data stream is indistinguishable",
            );
        }
        _ => println!("  could not open the pair"),
    }

    hdr("6. flow control behaviour");
    for (label, fc) in [
        ("software (XON/XOFF)", FlowControl::Software),
        ("hardware (RTS/CTS)", FlowControl::Hardware),
    ] {
        let mk = |path: &str| {
            serialport::new(path, 115_200)
                .flow_control(fc)
                .timeout(Duration::from_millis(400))
                .open()
        };
        match (mk(A), mk(B)) {
            (Ok(mut a), Ok(mut b)) => {
                b.clear(serialport::ClearBuffer::Input).ok();
                let payload: Vec<u8> = (0..512).map(|i| b'a' + (i % 26) as u8).collect();
                a.write_all(&payload).ok();
                a.flush().ok();
                std::thread::sleep(Duration::from_millis(250));
                let mut got = vec![0u8; payload.len()];
                let n = b.read(&mut got).unwrap_or(0);
                ok(
                    n == payload.len(),
                    label,
                    &format!("{n}/{} bytes through", payload.len()),
                );
            }
            _ => ok(false, label, "could not open pair"),
        }
    }

    hdr("7. timeout semantics");
    match open(A, 115_200) {
        Ok(mut p) => {
            let t0 = std::time::Instant::now();
            let mut buf = [0u8; 16];
            let r = p.read(&mut buf);
            let el = t0.elapsed();
            ok(
                el >= Duration::from_millis(350) && el < Duration::from_millis(900),
                "read timeout honoured",
                &format!("{:?} elapsed, result {:?}", el, r.as_ref().err().map(|e| e.kind())),
            );
        }
        Err(e) => println!("  {e}"),
    }

    println!("\ndone.");
}

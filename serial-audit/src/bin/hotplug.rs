//! Spike 4, part 3 — hotplug. The one piece that needs a human to pull a cable.
//!
//! Answers two things Stage 1's port picker and connection UI depend on:
//!
//!   1. does re-enumeration notice a device arriving/leaving?
//!   2. what does an already-open port do when the device is yanked?
//!
//! Run it, then unplug the FTDI adapter, wait, and plug it back in.

use serialport::{available_ports, TTYPort};
use std::collections::BTreeSet;
use std::io::Read;
use std::time::{Duration, Instant};

fn ports() -> BTreeSet<String> {
    available_ports()
        .map(|v| {
            v.into_iter()
                .filter(|p| p.port_name.contains("ttyUSB"))
                .map(|p| p.port_name)
                .collect()
        })
        .unwrap_or_default()
}

fn main() {
    let secs: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(90);

    let mut open_port: Option<TTYPort> = TTYPort::open(
        &serialport::new("/dev/ttyUSB0", 115_200).timeout(Duration::from_millis(300)),
    )
    .ok();
    println!(
        "holding /dev/ttyUSB0 open: {}",
        if open_port.is_some() { "yes" } else { "FAILED" }
    );

    let mut seen = ports();
    println!("baseline ttyUSB*: {:?}", seen);
    println!("\n>>> unplug the FTDI adapter now, wait ~5s, then plug it back in <<<\n");

    let start = Instant::now();
    let mut removal_seen = false;
    let mut readd_seen = false;
    let mut open_port_error_reported = false;

    while start.elapsed() < Duration::from_secs(secs) {
        std::thread::sleep(Duration::from_millis(400));

        let now = ports();
        if now != seen {
            let gone: Vec<_> = seen.difference(&now).cloned().collect();
            let added: Vec<_> = now.difference(&seen).cloned().collect();
            if !gone.is_empty() {
                println!("[{:>5.1}s] REMOVED: {:?}", start.elapsed().as_secs_f32(), gone);
                removal_seen = true;
            }
            if !added.is_empty() {
                println!("[{:>5.1}s] ADDED:   {:?}", start.elapsed().as_secs_f32(), added);
                if removal_seen {
                    readd_seen = true;
                }
            }
            seen = now;
        }

        // what happens to the fd we are holding?
        if let Some(p) = open_port.as_mut() {
            let mut buf = [0u8; 8];
            match p.read(&mut buf) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => {
                    if !open_port_error_reported {
                        println!(
                            "[{:>5.1}s] open port errored: kind={:?} raw_os={:?} — {}",
                            start.elapsed().as_secs_f32(),
                            e.kind(),
                            e.raw_os_error(),
                            e
                        );
                        println!("           (this is the signal the UI must turn into 'device disconnected')");
                        open_port_error_reported = true;
                        open_port = None;
                    }
                }
            }
        }

        if removal_seen && readd_seen && open_port_error_reported {
            println!("\nall three observed, stopping early.");
            break;
        }
    }

    println!("\n=== result ===");
    println!(
        "  [{}] removal detected by re-enumeration",
        if removal_seen { "OK  " } else { "MISS" }
    );
    println!(
        "  [{}] re-attach detected by re-enumeration",
        if readd_seen { "OK  " } else { "MISS" }
    );
    println!(
        "  [{}] open port surfaced a distinguishable error",
        if open_port_error_reported { "OK  " } else { "MISS" }
    );
    println!("  final ttyUSB*: {:?}", ports());
}

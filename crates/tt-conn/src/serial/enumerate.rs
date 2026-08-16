//! Finding serial ports, and giving each one an identity that survives a
//! reconnect.
//!
//! **The identity is the point.** `/dev/ttyUSB<n>` is assigned in attach
//! order, so unplugging two adapters and plugging them back in the other order
//! swaps their names — reconnect by that name and you are talking to the wrong
//! device. The USB serial number would be the obvious fix and is not one: the
//! FTDI Quad RS232-HS this was developed against reports `serial = None` for
//! every port, and even when a serial number exists it names the *adapter*,
//! not which of its four ports you meant.
//!
//! What does hold still is where the device is plugged in.
//! `/dev/serial/by-path/` encodes the USB topology plus the interface number,
//! so a given socket on a given hub keeps its name across replug — and across
//! swapping the adapter for an identical one, which is what a person
//! debugging a rack actually wants.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsbInfo {
    pub vid: u16,
    pub pid: u16,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    /// Often `None` on multi-port adapters, which is why it is not the
    /// identity. See the module docs.
    pub serial: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortInfo {
    /// The kernel device node, e.g. `/dev/ttyUSB0`. Fine to show; wrong to
    /// store.
    pub device: String,
    /// A `/dev/serial/by-path/…` path, when the port is on a bus that has
    /// one. **This is what to save in a session profile.**
    pub stable_id: Option<String>,
    pub usb: Option<UsbInfo>,
}

impl PortInfo {
    /// What to pass to [`super::SerialConn::open`] — the stable path if there
    /// is one, the device node otherwise.
    pub fn open_path(&self) -> &str {
        self.stable_id.as_deref().unwrap_or(&self.device)
    }

    /// A one-line description for a port picker.
    pub fn label(&self) -> String {
        match &self.usb {
            Some(u) => {
                let name = u
                    .product
                    .clone()
                    .or_else(|| u.manufacturer.clone())
                    .unwrap_or_else(|| format!("{:04x}:{:04x}", u.vid, u.pid));
                format!("{} — {}", self.device, name)
            }
            None => self.device.clone(),
        }
    }
}

/// Every serial port the system can see, sorted by device node so the picker
/// does not reshuffle between refreshes.
pub fn enumerate() -> Result<Vec<PortInfo>> {
    let by_path = by_path_map();
    let mut out: Vec<PortInfo> = serialport::available_ports()?
        .into_iter()
        .map(|p| {
            let usb = match p.port_type {
                serialport::SerialPortType::UsbPort(u) => Some(UsbInfo {
                    vid: u.vid,
                    pid: u.pid,
                    manufacturer: u.manufacturer,
                    product: u.product,
                    serial: u.serial_number,
                }),
                _ => None,
            };
            let stable_id = canonical(&p.port_name)
                .and_then(|c| by_path.get(&c).cloned())
                .map(|p| p.to_string_lossy().into_owned());
            PortInfo {
                device: p.port_name,
                stable_id,
                usb,
            }
        })
        .collect();
    out.sort_by(|a, b| a.device.cmp(&b.device));
    Ok(out)
}

/// `ts.ComPort` — a Tera Term command line's `/C=<n>`, resolved to a port.
///
/// **A decision, not a translation** (2026-08-09, recorded in `docs/history.md`): the
/// number is a 1-based index into [`enumerate`], so `/C=1` is the first entry
/// the port picker shows. The alternative was a literal `COM<n>` →
/// `/dev/ttyS<n-1>` map, which is stable and useless on a machine whose only
/// ports are USB adapters — this one owns four of them and no `ttyS0` worth
/// opening.
///
/// What that inherits is the instability already documented above: enumeration
/// is sorted by device node and `ttyUSB<n>` is assigned in attach order, so
/// replugging two adapters can swap which one is `/C=1`. Anything that wants to
/// *remember* a port must store [`PortInfo::stable_id`]; a number on a command
/// line is a choice made afresh each time, which is what it is upstream too.
///
/// `None` when there are fewer ports than that, which is upstream's own answer
/// to a `/C=` it cannot honour — the port is dropped and the New Connection
/// dialog opens.
pub fn port_by_number(n: u16) -> Result<Option<PortInfo>> {
    if n == 0 {
        return Ok(None);
    }
    Ok(enumerate()?.into_iter().nth(usize::from(n) - 1))
}

/// The other direction, for writing a command line or a settings file back:
/// which `/C=` would name this device, if any.
///
/// Matches on the device node *or* the stable path, because either may be what
/// the caller has in hand — a session opened from a profile knows only the
/// `by-path` name.
pub fn number_of_port(path: &str) -> Result<Option<u16>> {
    Ok(enumerate()?
        .iter()
        .position(|p| p.device == path || p.stable_id.as_deref() == Some(path))
        .map(|i| (i + 1) as u16))
}

/// Is there a serial device at this path — asked **without opening it**.
///
/// The constraint is `inuse`'s: opening a port raises DTR for as long as the
/// descriptor lives, so anything that asks this on a timer would reset an
/// Arduino-style board once per tick. Upstream asks the same question the same
/// way — `CheckComPort` (`ttpcmn/ttcmn_cominfo.c:89`) enumerates and compares,
/// it does not probe.
///
/// On Unix this follows a `/dev/serial/by-path/…` symlink to the node it names
/// and insists the answer is a character device, so a remembered path that has
/// become an ordinary file reads as absent rather than as a port. On Windows
/// there is no node to stat — `Path::exists("COM3")` is false for a port that
/// is plugged in and working — so it goes through [`enumerate`], which is what
/// makes this the expensive arm of the two.
pub fn present(path: &str) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        std::fs::metadata(path).is_ok_and(|md| md.file_type().is_char_device())
    }
    #[cfg(not(unix))]
    {
        // Case-insensitively, because `com3` names the same port as `COM3` and
        // a settings file may hold either.
        enumerate().is_ok_and(|ports| {
            ports.iter().any(|p| {
                p.device.eq_ignore_ascii_case(path)
                    || p.stable_id
                        .as_deref()
                        .is_some_and(|id| id.eq_ignore_ascii_case(path))
            })
        })
    }
}

/// Does this path name a physical socket rather than a kernel device node?
///
/// The distinction is the one this module opens with: `/dev/ttyUSB<n>` is
/// assigned in attach order, so it can name a different adapter after a replug,
/// while a `/dev/serial/…` name is the USB topology and holds still. It is
/// therefore the difference between reopening *this* port and reopening
/// whatever has taken its number — see [`PortInfo::stable_id`].
///
/// Always false on Windows: a `COM<n>` is a kernel name and there is no
/// topology spelling of it.
pub fn is_stable_path(path: &str) -> bool {
    cfg!(unix) && path.starts_with("/dev/serial/")
}

fn canonical(path: &str) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

/// Map each real device node to one `by-path` name.
///
/// A single port usually has **two** entries — `…-usb-…` and `…-usbv2-…`, the
/// second being udev's newer topology scheme — and they are equally valid.
/// Picking the alphabetically first makes the choice deterministic, so a
/// stored profile keeps working rather than depending on readdir order.
fn by_path_map() -> HashMap<PathBuf, PathBuf> {
    let dir = Path::new("/dev/serial/by-path");
    let Ok(entries) = std::fs::read_dir(dir) else {
        // Not an error: the directory only exists when udev is running and
        // something is plugged in.
        return HashMap::new();
    };
    let mut links: Vec<(PathBuf, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let link = e.path();
            let target = std::fs::canonicalize(&link).ok()?;
            Some((target, link))
        })
        .collect();
    links.sort();

    let mut map = HashMap::new();
    for (target, link) in links {
        map.entry(target).or_insert(link);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumeration_does_not_fail_without_hardware() {
        // The by-path directory is absent on a machine with nothing plugged
        // in, and that must read as "no stable id", not as an error.
        let ports = enumerate().expect("enumeration should not fail");
        for p in &ports {
            assert!(!p.device.is_empty());
            assert!(p.open_path() == p.device || p.open_path().contains("by-path"));
        }
    }

    /// `/C=<n>` and back again, on whatever this machine has — including a
    /// machine with nothing, where every answer is `None` rather than an error.
    #[test]
    fn a_com_number_is_an_index_into_the_picker() {
        let ports = enumerate().expect("enumeration");
        // Zero is not a port: upstream's own bound is `1 <= n <= MaxComPort`.
        assert_eq!(port_by_number(0).expect("zero").map(|p| p.device), None);
        for (i, p) in ports.iter().enumerate() {
            let n = (i + 1) as u16;
            assert_eq!(
                port_by_number(n).expect("by number").map(|p| p.device),
                Some(p.device.clone())
            );
            // The round trip, from either name the caller might hold.
            assert_eq!(number_of_port(&p.device).expect("by device"), Some(n));
            if let Some(id) = &p.stable_id {
                assert_eq!(number_of_port(id).expect("by stable id"), Some(n));
            }
        }
        // One past the end is "no such port", which is what makes a `/C=` that
        // cannot be honoured open the dialog instead of failing.
        let past = (ports.len() + 1) as u16;
        assert!(port_by_number(past).expect("past the end").is_none());
        assert!(number_of_port("/dev/nonexistent")
            .expect("unknown")
            .is_none());
    }

    /// The auto-reopen loop asks this once a tick while it waits, so the two
    /// answers it must never get wrong are "a path that is not there" and "a
    /// path that is there and is not a port".
    #[test]
    fn a_missing_node_is_absent_and_an_ordinary_file_is_not_a_port() {
        assert!(!present("/dev/sterna-no-such-port"));
        assert!(!present(""));
        // Every enumerated port is present by definition, whichever of its two
        // names the caller holds.
        for p in enumerate().expect("enumeration") {
            assert!(present(&p.device), "{} enumerated but absent", p.device);
            if let Some(id) = &p.stable_id {
                assert!(present(id), "{id} enumerated but absent");
            }
        }
        #[cfg(unix)]
        {
            // A directory exists and is not a serial port. `Path::exists`
            // alone would call this one present.
            assert!(!present("/tmp"));
            // ...and so does a character device that is not an adapter, which
            // is the point: this answers "is there a device", not "is it the
            // right one". Reopening is by the path that was opened.
            assert!(present("/dev/null"));
        }
    }

    #[test]
    fn only_a_topology_name_survives_a_replug() {
        assert_eq!(
            is_stable_path("/dev/serial/by-path/pci-0000:00:14.0-usb-0:2:1.0-port0"),
            cfg!(unix)
        );
        assert_eq!(
            is_stable_path("/dev/serial/by-id/usb-FTDI_x-if00-port0"),
            cfg!(unix)
        );
        assert!(!is_stable_path("/dev/ttyUSB0"));
        assert!(!is_stable_path("COM3"));
    }
}

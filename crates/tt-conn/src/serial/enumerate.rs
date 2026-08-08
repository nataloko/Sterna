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
}

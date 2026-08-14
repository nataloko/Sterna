//! What each window has open, published where the others can read it.
//!
//! A picker greys out a port something else holds. On Linux the kernel answers
//! that question for every program on the machine (`tt_conn::serial::holders`);
//! Windows has no non-destructive question to ask at all, so a window that
//! wants its ports respected has to say what it took. Tera Term does the same
//! thing with a bitmap of COM numbers in the shared memory its instances map —
//! `SetCOMFlag` when a port opens (`commlib.c:484`), `ClearCOMFlag` when it
//! closes (`:851`), and the New Connection dialog hides what is flagged
//! (`hostdlg.c:180`). The reach is the same here: this sees Sterna's windows
//! and nothing else.
//!
//! **A claim is only as true as the window that made it**, so it is read
//! through the endpoint list rather than on its own: a claim whose window is
//! no longer listening is ignored and unlinked. That makes the crash case
//! self-healing at the moment it stops mattering — a dead window holds no
//! port. The alternative, asking each window over the socket, would be exact
//! and can block: `Client::call` sets no timeout and a window sitting in a
//! modal dialog holds its connection open, so a dropdown would hang on the way
//! open.

use std::io;
use std::path::{Path, PathBuf};

use crate::addr;

/// One window's open device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Claim {
    /// The window's endpoint name — its `/D=` topic, which is its process id
    /// unless it was given one.
    pub name: String,
    /// What it passed to `SerialConn::open`: a device node or a stable
    /// `by-path` name. Compared by the reader, which knows how to resolve
    /// both.
    pub device: String,
}

impl Claim {
    /// The process id, when the name is the default one. `None` for a window
    /// launched with a `/D=` topic of its own, which is not a lie about the
    /// pid but the absence of one.
    pub fn pid(&self) -> Option<u32> {
        self.name.parse().ok()
    }
}

/// Where the claims live.
///
/// Unix keeps them beside the sockets, in the same `0700` runtime directory
/// that is already part of the access control. Windows cannot: [`addr::dir`]
/// there is `\\.\pipe`, a namespace with no files in it, so the claims go
/// under `%LOCALAPPDATA%`, which is per-user and survives no longer than it
/// should because every read prunes.
pub fn dir() -> io::Result<PathBuf> {
    #[cfg(unix)]
    {
        addr::dir()
    }
    #[cfg(windows)]
    {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA is not set"))?;
        let dir = base.join("Sterna").join("claims");
        if !dir.is_dir() {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(dir)
    }
}

/// Publish what this window has open. An empty list withdraws the claim.
///
/// A list because a window is not a session: tabs and tiles each have their
/// own connection, and a window with two serial consoles open holds both.
/// Idempotent on purpose — the frontend publishes the whole set on every
/// connection change rather than working out which changes matter.
pub fn claim(name: &str, devices: &[&str]) -> io::Result<()> {
    claim_in(&dir()?, name, devices)
}

/// [`claim`] into a given directory — the seam the tests use.
pub fn claim_in(dir: &Path, name: &str, devices: &[&str]) -> io::Result<()> {
    if !addr::valid_name(name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("bad claim name {name:?}"),
        ));
    }
    let path = dir.join(format!("{name}.port"));
    if devices.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        };
    }
    // One device per line, and a device with a newline in its name is not a
    // device: `serialport-rs` opens a path, and `/dev` has no such entry.
    let body: String = devices
        .iter()
        .filter(|d| !d.is_empty() && !d.contains('\n'))
        .map(|d| format!("{d}\n"))
        .collect();
    std::fs::write(&path, body.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // The device somebody is talking to is not the world's business, and
        // the Unix directory is already 0700.
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Every claim whose window is still listening.
///
/// The pruning is the same shape as [`addr::live`]'s, and for the same reason:
/// this is the only code that learns a claim is stale, so it is the only code
/// that can tidy up after it.
pub fn claims() -> io::Result<Vec<Claim>> {
    let live: Vec<String> = addr::live()?
        .iter()
        .filter_map(|p| addr::name_of(p))
        .collect();
    claims_in(&dir()?, &live)
}

/// [`claims`] against a given directory and a given set of live windows.
pub fn claims_in(dir: &Path, live: &[String]) -> io::Result<Vec<Claim>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        // No directory is no claims, not a failure: nothing has connected yet.
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("port") {
            continue;
        }
        let Some(name) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
            continue;
        };
        if !live.contains(&name) {
            // The window that wrote this is gone, so its port is free.
            let _ = std::fs::remove_file(&path);
            continue;
        }
        let Ok(body) = std::fs::read_to_string(&path) else {
            continue;
        };
        // A half-written file is not a claim. The next write replaces it.
        for device in body.lines().map(str::trim).filter(|d| !d.is_empty()) {
            out.push(Claim {
                name: name.clone(),
                device: device.to_string(),
            });
        }
    }
    out.sort_by(|a, b| (&a.name, &a.device).cmp(&(&b.name, &b.device)));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!("tt-claim-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_claim_round_trips_and_is_withdrawn() {
        let scratch = Scratch::new("round");
        let live = vec!["1234".to_string()];

        assert_eq!(claims_in(&scratch.0, &live).unwrap(), Vec::new());

        claim_in(&scratch.0, "1234", &["/dev/ttyUSB0"]).unwrap();
        let held = claims_in(&scratch.0, &live).unwrap();
        assert_eq!(
            held,
            vec![Claim {
                name: "1234".into(),
                device: "/dev/ttyUSB0".into()
            }]
        );
        assert_eq!(held[0].pid(), Some(1234));

        // The whole set is published each time, so closing one tab and
        // opening another replaces rather than accumulates — and a window with
        // two consoles open holds both.
        claim_in(&scratch.0, "1234", &["/dev/ttyUSB1", "/dev/ttyUSB2"]).unwrap();
        let held = claims_in(&scratch.0, &live).unwrap();
        assert_eq!(held.len(), 2, "{held:?}");
        assert_eq!(held[0].device, "/dev/ttyUSB1");
        assert_eq!(held[1].device, "/dev/ttyUSB2");

        claim_in(&scratch.0, "1234", &[]).unwrap();
        assert_eq!(claims_in(&scratch.0, &live).unwrap(), Vec::new());
        // Withdrawing twice is not an error — the frontend does not track
        // which changes matter.
        claim_in(&scratch.0, "1234", &[]).unwrap();
    }

    /// The whole reason a claim is read through the endpoint list: a window
    /// that died holding a port must not keep it greyed out for ever.
    #[test]
    fn a_claim_whose_window_is_gone_is_ignored_and_removed() {
        let scratch = Scratch::new("stale");
        claim_in(&scratch.0, "999", &["/dev/ttyUSB0"]).unwrap();
        claim_in(&scratch.0, "1000", &["/dev/ttyUSB1"]).unwrap();

        let held = claims_in(&scratch.0, &["1000".to_string()]).unwrap();
        assert_eq!(held.len(), 1, "only the live window's claim: {held:?}");
        assert_eq!(held[0].name, "1000");
        assert!(
            !scratch.0.join("999.port").exists(),
            "the dead window's claim is tidied away"
        );
    }

    #[test]
    fn a_name_that_could_escape_the_directory_is_refused() {
        let scratch = Scratch::new("name");
        // The same rule as the socket's, and for the same reason: a `/D=`
        // topic comes off a command line.
        assert!(claim_in(&scratch.0, "../../escape", &["/dev/null"]).is_err());
        assert!(claim_in(&scratch.0, "", &["/dev/null"]).is_err());
    }

    #[test]
    fn an_empty_file_is_not_a_claim() {
        let scratch = Scratch::new("empty");
        std::fs::write(scratch.0.join("1234.port"), "").unwrap();
        std::fs::write(scratch.0.join("notes.txt"), "/dev/ttyUSB0").unwrap();
        assert_eq!(
            claims_in(&scratch.0, &["1234".to_string()]).unwrap(),
            Vec::new()
        );
    }
}

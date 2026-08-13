//! Settings: the INI file Tera Term already has, and the schema over it.
//!
//! Two halves. [`ini`] is `GetPrivateProfile*` reproduced — see
//! `ini-audit/README.md` for what "bug-compatible" turned out to mean, measured
//! rather than assumed. [`schema`] and the generated [`Settings`] are the list
//! of what lives in that file, which exists **once**: `PLAN.md` puts ~13.8k
//! lines of dialog code over a 909-line settings struct, and hand-porting that
//! is where this project would stop.
//!
//! [`cmdline`] is the third thing that reads settings from outside the process,
//! and it is here rather than in the frontend because a command line's first
//! job is to say which INI file to read — upstream puts `_ParseParam` in the
//! same DLL as `_ReadIniFile` for exactly that reason.
//!
//! ```sh
//! cargo run -p tt-config --bin gen-settings   # after editing the schema
//! ```

pub mod buttons;
pub mod cmdline;
pub mod esc;
pub mod gen;
pub mod hex;
pub mod ini;
pub mod keyboard;
pub mod schema;
pub mod services;

mod generated;

pub use buttons::Button;
pub use generated::*;
pub use hex::{hex_decode, hex_decode_str, hex_escape_str};
pub use ini::{Encoding, Ini};
pub use keyboard::{KeyboardAction, KeyboardMap, Shortcut, UserKey, UserKeyType};
pub use schema::{DebugModes, Field, Kind};

impl Settings {
    /// Cross-field rules the generated rows cannot express on their own.
    ///
    /// `ttset.c:1798` turns `Debug` back off when `DebugModes` permits no
    /// display mode. Doing it here means a loaded file and `setsetting` agree,
    /// and a subsequent save writes the effective `Debug=off` upstream does.
    ///
    /// `ttset.c:1223` is the second, and it is a rule about *two* keys in a
    /// particular order: `ComPort` is read at `:916` and `MaxComPort` at
    /// `:1218`, and only then is the port tested against it. A row can say
    /// `int(1..256)` and cannot say "bounded by whatever that other setting
    /// loaded", which is why this was an open item rather than a bound. It is
    /// also **a reset and not a clamp** — an out-of-range port becomes 1, not
    /// the nearest end — so a file naming COM300 on a machine whose
    /// `MaxComPort` is the default 256 opens the *first* port, and a
    /// `MaxComPort=4` opens the first port for anything above COM4.
    fn normalize(&mut self) {
        if DebugModes::parse_ini(&self.debug_modes).is_empty() {
            self.debug_enabled = false;
        }
        if self.serial_com_port < 1 || self.serial_com_port > self.serial_max_com_port {
            self.serial_com_port = 1;
        }
    }
}

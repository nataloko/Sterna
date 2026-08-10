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

pub mod cmdline;
pub mod gen;
pub mod hex;
pub mod ini;
pub mod schema;
pub mod services;

mod generated;

pub use generated::*;
pub use hex::{hex_decode, hex_decode_str};
pub use ini::{Encoding, Ini};
pub use schema::{DebugModes, Field, Kind};

impl Settings {
    /// Cross-field rules the generated rows cannot express on their own.
    ///
    /// `ttset.c:1798` turns `Debug` back off when `DebugModes` permits no
    /// display mode. Doing it here means a loaded file and `setsetting` agree,
    /// and a subsequent save writes the effective `Debug=off` upstream does.
    fn normalize(&mut self) {
        if DebugModes::parse_ini(&self.debug_modes).is_empty() {
            self.debug_enabled = false;
        }
    }
}

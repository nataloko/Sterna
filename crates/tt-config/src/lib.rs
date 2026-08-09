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
pub mod ini;
pub mod schema;
pub mod services;

mod generated;

pub use generated::*;
pub use ini::{Encoding, Ini};
pub use schema::{Field, Kind};

//! Settings: the INI file Tera Term already has, and the schema over it.
//!
//! Two halves. [`ini`] is `GetPrivateProfile*` reproduced — see
//! `ini-audit/README.md` for what "bug-compatible" turned out to mean, measured
//! rather than assumed. [`schema`] and the generated [`Settings`] are the list
//! of what lives in that file, which exists **once**: `PLAN.md` puts ~13.8k
//! lines of dialog code over a 909-line settings struct, and hand-porting that
//! is where this project would stop.
//!
//! ```sh
//! cargo run -p tt-config --bin gen-settings   # after editing the schema
//! ```

pub mod ini;
pub mod schema;

mod generated;

pub use generated::*;
pub use ini::{Encoding, Ini};
pub use schema::{Field, Kind};

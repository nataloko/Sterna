//! Settings: the INI file Tera Term already has, and the schema over it.
//!
//! See `ini-audit/README.md` for what "bug-compatible with `GetPrivateProfile*`"
//! turned out to mean, measured rather than assumed.

pub mod ini;

pub use ini::{Encoding, Ini};

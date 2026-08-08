//! Generate `include/termitta.h` from the source, and keep the committed copy
//! honest.
//!
//! The header is committed rather than generated into `OUT_DIR` because the Qt
//! shell's CMake has to find it without asking Cargo where it hid, and because
//! a header diff in review is the only place a renumbered enum shows up — the
//! C ABI takes `Key`, `Parity` and friends straight from the core crates, so
//! reordering one of those enums is an ABI break that nothing else would flag.
//! CI runs `git diff --exit-code` over it.
//!
//! Deliberately **not** `parse_deps`: that makes cbindgen shell out to `cargo
//! metadata` from inside a build script, which can block on the package cache
//! lock. Each dependency source file that contributes a type is listed instead,
//! so the parse is hermetic and the list of what crosses the ABI is explicit.

use std::path::{Path, PathBuf};

/// Files outside this crate that define types the ABI hands out.
const DEP_SOURCES: &[&str] = &[
    "../tt-grid/src/lib.rs",        // Cell
    "../tt-vt/src/keys.rs",         // Key
    "../tt-vt/src/mouse.rs",        // MouseEvent, Modifiers, Tracking
    "../tt-vt/src/term_id.rs",      // TermId
    "../tt-conn/src/serial/mod.rs", // Parity, FlowControl, PinControl
];

fn main() {
    let crate_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo"));

    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=cbindgen.toml");
    for src in DEP_SOURCES {
        println!("cargo:rerun-if-changed={src}");
    }

    let config = cbindgen::Config::from_file(crate_dir.join("cbindgen.toml"))
        .expect("cbindgen.toml is not readable");

    // `with_src` for every file, never `with_crate`: the latter shells out to
    // `cargo metadata` from inside a build script, and passing both parses
    // lib.rs twice, which emits every declaration twice.
    let mut builder = cbindgen::Builder::new()
        .with_config(config)
        .with_src(crate_dir.join("src/lib.rs"));
    for src in DEP_SOURCES {
        builder = builder.with_src(crate_dir.join(src));
    }

    let bindings = match builder.generate() {
        Ok(b) => b,
        // A syntax error in a source file is the compiler's to report, with a
        // line number; failing the build here would bury it.
        Err(e) => {
            println!("cargo:warning=cbindgen: {e}");
            return;
        }
    };

    let header = crate_dir.join("include/termitta.h");
    std::fs::create_dir_all(header.parent().expect("has a parent"))
        .expect("cannot create include/");
    // `write_to_file` already skips an identical write, which keeps the source
    // tree's mtimes still and stops a rebuild loop.
    bindings.write_to_file(&header);
    announce(&header);
}

/// Tell dependents where the header is, so a `cc`- or CMake-driven consumer
/// does not hardcode the path.
fn announce(header: &Path) {
    let dir = header.parent().expect("has a parent");
    println!("cargo:include={}", dir.display());
    println!("cargo:header={}", header.display());
}

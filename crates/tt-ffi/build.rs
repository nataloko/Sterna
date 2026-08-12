//! Generate `include/sterna.h` from the source, and keep the committed copy
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
    "../tt-session/src/log.rs",     // Timestamp
];

fn main() {
    let crate_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo"));

    soname();
    update_key(&crate_dir);

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

    let header = crate_dir.join("include/sterna.h");
    std::fs::create_dir_all(header.parent().expect("has a parent"))
        .expect("cannot create include/");
    // `write_to_file` already skips an identical write, which keeps the source
    // tree's mtimes still and stops a rebuild loop.
    bindings.write_to_file(&header);
    announce(&header);
}

/// Compile the release-signing public key into the shared library.
///
/// The text file is intentionally the canonical copy: a reviewer can compare
/// its fingerprint with the offline backup without reading a generated Rust
/// array. Decoding in the build script keeps a base64 parser out of the
/// shipped library and makes a malformed or missing key a build failure.
fn update_key(crate_dir: &Path) {
    let path = crate_dir.join("../../packaging/update/public-key.txt");
    println!("cargo:rerun-if-changed={}", path.display());
    let encoded = std::fs::read_to_string(&path).expect("updater public key is not readable");
    let bytes = decode_base64(encoded.trim()).expect("updater public key is not base64");
    let key: [u8; 32] = bytes
        .try_into()
        .expect("updater public key is not 32-byte Ed25519");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set"));
    std::fs::write(
        out.join("update_key.rs"),
        format!("const UPDATE_PUBLIC_KEY: [u8; 32] = {key:?};\n"),
    )
    .expect("cannot write updater public key");
}

fn decode_base64(text: &str) -> Option<Vec<u8>> {
    fn value(byte: u8) -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    if text.is_empty() || !text.len().is_multiple_of(4) {
        return None;
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    for (index, chunk) in bytes.chunks_exact(4).enumerate() {
        let last = index + 1 == bytes.len() / 4;
        let a = value(chunk[0])?;
        let b = value(chunk[1])?;
        let c = if chunk[2] == b'=' {
            if !last || chunk[3] != b'=' {
                return None;
            }
            0
        } else {
            value(chunk[2])?
        };
        let d = if chunk[3] == b'=' {
            if !last {
                return None;
            }
            0
        } else {
            value(chunk[3])?
        };
        out.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            out.push((b << 4) | (c >> 2));
        }
        if chunk[3] != b'=' {
            out.push((c << 6) | d);
        }
    }
    Some(out)
}

/// Give the shared library a `DT_SONAME`, which Cargo does not.
///
/// Without one, whatever links against it records the path it was *handed* at
/// link time. Build the shell out of tree and the executable ends up with a
/// `DT_NEEDED` of `cargo/debug/libsterna.so`, relative — so it runs from the
/// build directory and nowhere else, and the failure is a loader message about
/// a missing file that plainly exists.
///
/// `rustc-cdylib-link-arg` rather than `RUSTFLAGS` because it applies to the
/// cdylib alone; the same flag through the environment would attach a soname to
/// every test binary in the workspace as well.
fn soname() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-soname,libsterna.so");
    }
}

/// Tell dependents where the header is, so a `cc`- or CMake-driven consumer
/// does not hardcode the path.
fn announce(header: &Path) {
    let dir = header.parent().expect("has a parent");
    println!("cargo:include={}", dir.display());
    println!("cargo:header={}", header.display());
}

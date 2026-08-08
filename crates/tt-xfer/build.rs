//! Compile the vendored protocols and the host layer that attaches them.
//!
//! Two compilers, two warning policies: Tera Term's sources are someone else's
//! code written for MSVC and are built with warnings off, ours are built with
//! them on. That split is the same one `oracle/Makefile` and `xfer/Makefile`
//! make, and for the same reason — a warning we cannot act on is a warning
//! that trains us to ignore the ones we can.

use std::path::{Path, PathBuf};

/// Upstream's, compiled unmodified. `raw.c` is here even though it is barely a
/// protocol: it reads `cv->InBuff` directly, so it is the one file that proves
/// the comm layer is TComVar-shaped rather than merely vtable-shaped.
const VENDOR_C: &[&str] = &[
    "ttpfile/xmodem.c",
    "ttpfile/ymodem.c",
    "ttpfile/zmodem.c",
    "ttpfile/kermit.c",
    "ttpfile/bplus.c",
    "ttpfile/quickvan.c",
    "ttpfile/ftlib.c",
    "ttpfile/raw.c",
];

const VENDOR_CXX: &[&str] = &["ttpfile/protolog.cpp", "common/asprintf.cpp"];

/// The portability layer. Not vendored — ours, shared with `oracle/` and
/// `xfer/`, which is why it is not under either of them.
const SHIM_C: &[&str] = &["winshim.c", "msvc_crt.c", "swscanf_s.c", "codeconv_min.c"];

const OURS_C: &[&str] = &["tt_xfer.c", "fileio_posix.c"];

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.join("../..").canonicalize().unwrap();
    let vendor = root.join("vendor/ttpfile");
    let shim = root.join("winshim");
    let csrc = manifest.join("csrc");

    for dir in [&vendor, &shim] {
        assert!(dir.is_dir(), "missing {}", dir.display());
    }

    let includes = [
        shim.clone(),
        csrc.clone(),
        vendor.join("common"),
        vendor.join("teraterm"),
        vendor.join("ttpfile"),
    ];
    // msvc_compat.h is force-included rather than added to each source,
    // because the sources it adapts are the ones we must not edit.
    let force_include = shim.join("msvc_compat.h");

    let base = |build: &mut cc::Build| {
        for inc in &includes {
            build.include(inc);
        }
        build.flag("-include").flag(force_include.to_str().unwrap());
    };

    let mut theirs = cc::Build::new();
    base(&mut theirs);
    theirs.warnings(false).extra_warnings(false).flag("-w");
    for f in VENDOR_C {
        theirs.file(vendor.join(f));
    }
    for f in SHIM_C {
        theirs.file(shim.join(f));
    }
    theirs.compile("ttpfile_c");

    let mut theirs_cxx = cc::Build::new();
    base(&mut theirs_cxx);
    theirs_cxx
        .cpp(true)
        // Pinned, not inherited. `common/ttcstd.h:45` typedefs `char8_t` under
        // `__cplusplus >= 202002L` — the guard is inverted, since that is
        // exactly when the language already has it — so the file compiles at
        // C++17 and not at C++20. GCC 13 defaults to gnu++17 and GCC 16
        // defaults to C++20, which is why this built in one container and not
        // the other.
        .std("gnu++17")
        .warnings(false)
        .extra_warnings(false)
        .flag("-w");
    for f in VENDOR_CXX {
        theirs_cxx.file(vendor.join(f));
    }
    theirs_cxx.compile("ttpfile_cxx");

    let mut ours = cc::Build::new();
    base(&mut ours);
    ours.warnings(true)
        .flag("-Wextra")
        .flag("-Wno-unused-parameter");
    for f in OURS_C {
        ours.file(csrc.join(f));
    }
    ours.compile("tt_xfer_host");

    // protolog.cpp and asprintf.cpp are C++; the C objects reference them.
    println!("cargo:rustc-link-lib=dylib=stdc++");

    for dir in [&vendor, &shim, &csrc] {
        rerun_if_changed_recursive(dir);
    }
}

fn rerun_if_changed_recursive(dir: &Path) {
    println!("cargo:rerun-if-changed={}", dir.display());
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rerun_if_changed_recursive(&path);
        } else {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}

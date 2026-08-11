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
const POSIX_SHIM_C: &[&str] = &["winshim.c", "msvc_crt.c", "swscanf_s.c"];

const OURS_C: &[&str] = &["tt_xfer.c"];

fn main() {
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = plain(manifest.join("../..").canonicalize().unwrap());
    let vendor = root.join("vendor/ttpfile");
    let shim = root.join("winshim");
    let csrc = manifest.join("csrc");

    for dir in [&vendor, &shim] {
        assert!(dir.is_dir(), "missing {}", dir.display());
    }

    let mut includes = vec![
        csrc.clone(),
        vendor.join("common"),
        vendor.join("teraterm"),
        vendor.join("ttpfile"),
    ];
    // Only POSIX wants our windows.h ahead of the system headers. On Windows
    // that would shadow the real SDK with the oracle's deliberately tiny
    // stand-in, which was the original reason this crate did not build there.
    if !windows {
        includes.insert(0, shim.clone());
    }
    // The protocols are vendored and remain unmodified. This header selects
    // the real or shim windows.h, makes their implicit CRT includes explicit,
    // and redirects the three HWND-shaped callbacks into this library.
    let force_include = csrc.join("platform.h");

    let base = |build: &mut cc::Build| {
        for inc in &includes {
            build.include(inc);
        }
        if msvc {
            build.flag(format!("/FI{}", force_include.display()));
        } else {
            build.flag("-include").flag(force_include.to_str().unwrap());
        }
    };

    let mut theirs = cc::Build::new();
    base(&mut theirs);
    theirs
        .warnings(false)
        .extra_warnings(false)
        // Every constructor is reached through `tt_xfer_create`, which lives
        // in the host archive emitted later. MinGW scans static archives once
        // from left to right, so without this it sees no reason to extract a
        // protocol and then reports XCreate/YCreate/... as unresolved when it
        // reaches the host. They are all runtime-selectable and all belong in
        // the library anyway.
        .link_lib_modifier("+whole-archive");
    if !msvc {
        theirs.flag("-w");
    }
    for f in VENDOR_C {
        theirs.file(vendor.join(f));
    }
    if windows {
        // protolog.cpp is the only protocol source that reaches codeconv, and
        // only for its two path conversions. Use the real Windows code pages;
        // codeconv_min is the POSIX oracle's deliberately limited substitute.
        theirs.file(csrc.join("codeconv_windows.c"));
    } else {
        theirs.file(shim.join("codeconv_min.c"));
        for f in POSIX_SHIM_C {
            theirs.file(shim.join(f));
        }
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
        .std(if msvc { "c++17" } else { "gnu++17" })
        .warnings(false)
        .extra_warnings(false)
        // ProtoLogCreate is first referenced by the C archive and its path
        // conversions live back in that archive. Loading both vendored
        // archives whole also closes that static-link cycle.
        .link_lib_modifier("+whole-archive");
    if !msvc {
        theirs_cxx.flag("-w");
    }
    for f in VENDOR_CXX {
        theirs_cxx.file(vendor.join(f));
    }
    theirs_cxx.compile("ttpfile_cxx");

    let mut ours = cc::Build::new();
    base(&mut ours);
    ours.warnings(true)
        .flag_if_supported("-Wextra")
        .flag_if_supported("-Wno-unused-parameter")
        // A downstream that depends on tt-session but never starts a transfer
        // still receives the whole vendored archives above. Keep their host
        // callbacks beside them; otherwise MinGW quite correctly omits this
        // unreferenced archive and the forced protocol objects have nowhere
        // to resolve ProtoEnd and the redirected window calls.
        .link_lib_modifier("+whole-archive");
    for f in OURS_C {
        ours.file(csrc.join(f));
    }
    ours.file(csrc.join(if windows {
        "fileio_windows.c"
    } else {
        "fileio_posix.c"
    }));
    ours.compile("tt_xfer_host");

    // protolog.cpp and asprintf.cpp are C++; the C objects reference them.
    // MSVC's C++ runtime is selected by the linker flags cc emits; there is
    // no library named stdc++ in that toolchain.
    if !msvc {
        println!("cargo:rustc-link-lib=dylib=stdc++");
    }

    for dir in [&vendor, &shim, &csrc] {
        rerun_if_changed_recursive(dir);
    }
}

/// Drop the `\\?\` that `canonicalize` puts on a Windows path.
///
/// `cl.exe` cannot open a source file spelt that way, and it does not say so:
/// it reports `C1083: Cannot open source file: '\\raw.c'` — a name that is not
/// the one it was handed and that exists nowhere, so the obvious reading is
/// that the vendored tree is missing rather than that the prefix is. MinGW
/// takes it, which is why the cross build was green while the native one was
/// not. The prefix's purpose is to escape `MAX_PATH`; every path here is a
/// checkout of this repository and nowhere near it.
///
/// Only the drive spelling is shortened. `\\?\UNC\server\share` loses its host
/// if the prefix goes, which would turn a working build on a network share
/// into a mysterious one.
fn plain(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        let b = rest.as_bytes();
        if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\' {
            return PathBuf::from(rest);
        }
    }
    path
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

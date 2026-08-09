//! `cargo run -p tt-config --bin gen-settings [-- --check]`.
//!
//! The work is in `tt_config::gen`; this is the part that touches the disk.
//!
//! Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

use tt_config::gen;

fn main() {
    let schema =
        std::fs::read_to_string(gen::schema_path()).expect("schema/settings.txt is readable");
    let generated = gen::generate(&schema);
    let target = gen::generated_path();

    if std::env::args().any(|a| a == "--check") {
        let existing = std::fs::read_to_string(&target).unwrap_or_default();
        if existing != generated {
            eprintln!(
                "src/generated.rs is stale — run `cargo run -p tt-config --bin gen-settings`"
            );
            std::process::exit(1);
        }
        println!("generated file is current");
        return;
    }

    // Leave the mtime alone when nothing changed, so re-running does not
    // force a rebuild of everything downstream.
    if std::fs::read_to_string(&target).is_ok_and(|existing| existing == generated) {
        println!("unchanged: {}", target.display());
        return;
    }
    std::fs::write(&target, &generated).expect("cannot write the generated file");
    println!("wrote {}", target.display());
}

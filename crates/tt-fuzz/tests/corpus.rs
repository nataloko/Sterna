//! Replay every byte stream in the repository through the properties.
//!
//! This is the half of the fuzzing story that runs on stable and in CI. The
//! fuzzer in `crates/fuzz/` needs nightly and a machine willing to spend
//! minutes on it; this needs neither, and it is what stops a bug the fuzzer
//! already found from coming back.
//!
//! Two sources, deliberately:
//!
//! - **`oracle/cases/*/input`** — the differential corpus. Those streams were
//!   written to exercise the engine's corners, so they are the best seed
//!   material in the repository and free to reuse. The differential suite asks
//!   whether they produce Tera Term's answer; this asks whether they leave the
//!   grid self-consistent under every chunking, which is a question that suite
//!   never puts.
//! - **`crates/fuzz/artifacts/`** — whatever libFuzzer found and somebody
//!   fixed. A crash file committed there becomes a permanent regression case
//!   without anyone writing a test for it.

use std::fs;
use std::path::{Path, PathBuf};

use tt_fuzz::{chunk, config, vt_chunking, vt_stream};
use tt_vt::Config;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/tt-fuzz is two levels down")
        .to_path_buf()
}

/// Every committed byte stream, as (name, bytes).
fn corpus() -> Vec<(String, Vec<u8>)> {
    let root = repo_root();
    let mut out = Vec::new();

    let cases = root.join("oracle/cases");
    let mut dirs: Vec<PathBuf> = fs::read_dir(&cases)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", cases.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    dirs.sort();
    for dir in dirs {
        let input = dir.join("input");
        if let Ok(bytes) = fs::read(&input) {
            let name = dir.file_name().unwrap().to_string_lossy().into_owned();
            out.push((name, bytes));
        }
    }

    // Absent until the fuzzer finds something, which is the normal state.
    collect(&root.join("crates/fuzz/artifacts"), &mut out);
    out
}

fn collect(dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect(&path, out);
        } else if let Ok(bytes) = fs::read(&path) {
            out.push((format!("artifact {}", path.display()), bytes));
        }
    }
}

/// The chunkings each stream is replayed under.
///
/// One byte at a time is the one that matters — it puts a boundary between
/// every pair of bytes at once, so a single pass over a corpus file tests every
/// split point in it. The others are there because a *uniform* chunking can
/// hide a bug that only shows when the boundaries move relative to the
/// sequences: 3 and 5 cycle out of phase with almost everything.
const CHUNKINGS: &[&[usize]] = &[&[1], &[2], &[3, 5], &[7, 1, 13], &[usize::MAX]];

/// The two sizes. 20x6 keeps wrapping, scrolling and eviction a few bytes
/// away; 80x24 is what a real terminal is, and it is the one where a corpus
/// case written for 80 columns actually lands where its author meant it to.
fn configs() -> Vec<Config> {
    vec![config(), Config::default()]
}

#[test]
fn the_corpus_leaves_the_grid_consistent() {
    let corpus = corpus();
    assert!(corpus.len() > 50, "corpus is {} streams", corpus.len());

    for (name, bytes) in &corpus {
        for cfg in configs() {
            for sizes in CHUNKINGS {
                let chunks = chunk(bytes, sizes);
                if let Err(e) = vt_stream(cfg.clone(), &chunks) {
                    panic!("{name} at {}x{} chunked {sizes:?}: {e}", cfg.cols, cfg.rows);
                }
            }
        }
    }
}

#[test]
fn the_corpus_does_not_care_where_the_chunks_fall() {
    for (name, bytes) in &corpus() {
        for cfg in configs() {
            for sizes in CHUNKINGS {
                let chunks = chunk(bytes, sizes);
                if let Err(e) = vt_chunking(cfg.clone(), &chunks) {
                    panic!("{name} at {}x{}: {e}", cfg.cols, cfg.rows);
                }
            }
        }
    }
}

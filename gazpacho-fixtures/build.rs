//! Hashes the generation code so the on-disk fixture cache invalidates
//! automatically when it changes: the registry keys its directory on
//! `GAZPACHO_FIXTURES_HASH` (see `registry::fixtures_dir`).

use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=build.rs");

    // `src/` is flat, so no recursion needed. `main.rs` is excluded: the CLI
    // can't change what fixture bytes get generated.
    let mut files: Vec<PathBuf> = fs::read_dir("src")
        .expect("reading src/")
        .map(|entry| entry.expect("reading src/ entry").path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "rs")
                && path.file_name().is_some_and(|name| name != "main.rs")
        })
        .collect();
    files.sort();

    // FNV-1a: deterministic across platforms and toolchains, no dependencies.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for &byte in bytes {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    for path in &files {
        eat(path.to_string_lossy().as_bytes());
        eat(&fs::read(path).expect("reading source file"));
    }

    println!("cargo:rustc-env=GAZPACHO_FIXTURES_HASH={hash:016x}");
}

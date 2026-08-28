//! Source-shape tests for the isolated cargo-fuzz crate (m7fs.1).
//!
//! These do not run libFuzzer. They pin the policy that `fuzz/` stays out of
//! the engine workspace and that the three named targets + seed corpora exist.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;

fn log_check(id: &str, subject: &str, ok: bool) {
    eprintln!(
        "check id={id} subject={subject} outcome={}",
        if ok { "PASS" } else { "FAIL" }
    );
    assert!(ok, "{id}: {subject}");
}

#[test]
fn fuzz_crate_is_isolated_and_has_three_targets() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    log_check(
        "m7fs.1.not-member",
        "fuzz/ is not a workspace member",
        !workspace.contains("\"fuzz\"") && !workspace.contains("/fuzz"),
    );

    let toml = fs::read_to_string(root.join("fuzz/Cargo.toml")).unwrap();
    log_check(
        "m7fs.1.isolated",
        "empty [workspace] table",
        toml.contains("[workspace]"),
    );
    log_check(
        "m7fs.1.libfuzzer",
        "libfuzzer-sys is a fuzz-crate-only dep",
        toml.contains("libfuzzer-sys") && !workspace.contains("libfuzzer-sys"),
    );

    for bin in ["fuzz_markdown_render", "fuzz_font_subset", "fuzz_zlib"] {
        log_check(
            "m7fs.1.bin",
            bin,
            toml.contains(&format!("name = \"{bin}\""))
                && root
                    .join("fuzz/fuzz_targets")
                    .join(format!("{bin}.rs"))
                    .is_file(),
        );
        let corpus = root.join("fuzz/corpus").join(bin);
        let seeds = fs::read_dir(&corpus)
            .unwrap_or_else(|e| panic!("corpus {}: {e}", corpus.display()))
            .count();
        log_check("m7fs.1.corpus", &format!("{bin} seeds={seeds}"), seeds >= 1);
    }

    let compress = fs::read_to_string(root.join("src/compress.rs")).unwrap();
    log_check(
        "m7fs.1.zlib-pub",
        "zlib_decompress is pub",
        compress.contains("pub fn zlib_decompress"),
    );
}

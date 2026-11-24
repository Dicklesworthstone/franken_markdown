//! Companion interactive HTML and native artifacts publish as one file set.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "# Publication\n\n{{#include parts/body.md}}\n";
const BODY: &str = "*Cross-line\n: emphasis*\n\nIncluded [reference][destination].\n\n[destination]: https://example.invalid/reference\n\n![Local chart](chart.svg)\n";
const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="red"/></svg>"#;
const SIMPLE: &str = "# Publication\n\nA small document with **emphasis**.\n";

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fmd-interactive-outputs-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn read(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.0.join(name)).unwrap()
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_fmd"))
            .current_dir(&self.0)
            .env("SOURCE_DATE_EPOCH", "1700000000")
            .arg("--no-config")
            .args(args)
            .output()
            .unwrap()
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn succeeded(output: &Output) {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_format(format: &str, bytes: &[u8]) {
    match format {
        "html" => {
            assert!(String::from_utf8_lossy(bytes).contains("id=\"fmd-raw-source\""));
        }
        "pdf" => assert!(bytes.starts_with(b"%PDF-")),
        "epub" => assert!(bytes.starts_with(b"PK\x03\x04")),
        "svg" => assert!(String::from_utf8_lossy(bytes).contains("<svg")),
        _ => panic!("unexpected format"),
    }
}

#[test]
fn explicit_companions_match_standalone_exports_and_report_actual_paths() {
    let dir = Workspace::new();
    dir.write("notes.md", SOURCE);
    dir.write("parts/body.md", BODY);
    dir.write("parts/chart.svg", SVG);
    let mut expected = BTreeMap::new();
    for (format, target) in [
        ("html", "interactive-html"),
        ("pdf", "pdf"),
        ("epub", "epub"),
        ("svg", "svg"),
    ] {
        let path = format!("standalone-{format}.out");
        succeeded(&dir.run(&[
            "notes.md",
            "--profile",
            "commonmark-gfm",
            "--to",
            target,
            "--out",
            &path,
        ]));
        let bytes = dir.read(&path);
        assert_format(format, &bytes);
        expected.insert(format, bytes);
    }
    let html = String::from_utf8_lossy(&expected["html"]);
    assert!(html.contains("<em>Cross-line\n: emphasis</em>"));
    assert!(html.contains("data:image/svg+xml;base64,"));

    for target in ["pdf", "epub", "svg", "both"] {
        let format = if target == "both" { "pdf" } else { target };
        let prefix = format!("edition-{target}");
        let requested = format!("{prefix}.requested");
        dir.write(&requested, b"unrelated existing file");
        let output = dir.run(&[
            "notes.md",
            "--profile",
            "commonmark-gfm",
            "--to",
            target,
            "--interactive-html",
            "--out",
            &requested,
            "--json",
        ]);
        succeeded(&output);
        assert!(output.stdout.is_empty(), "{target}");
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        for emitted in ["html", format] {
            let path = format!("{prefix}.{emitted}");
            assert_eq!(dir.read(&path), expected[emitted], "{target}: {emitted}");
            assert!(
                diagnostics.lines().any(|line| {
                    line.contains(r#""event":"wrote""#)
                        && line.contains(&format!(r#""format":"{emitted}""#))
                        && line.contains(&format!(r#""path":"{path}""#))
                }),
                "{target}: {diagnostics}"
            );
        }
        assert_eq!(dir.read(&requested), b"unrelated existing file");
    }
    assert_eq!(dir.read("notes.md"), SOURCE.as_bytes());
    assert_eq!(dir.read("parts/body.md"), BODY.as_bytes());
    assert_eq!(dir.read("parts/chart.svg"), SVG.as_bytes());
}

#[test]
fn omitted_output_uses_sibling_files_in_the_input_directory() {
    for target in ["pdf", "epub", "svg"] {
        let dir = Workspace::new();
        dir.write("chapters/notes.md", SIMPLE);
        let output = dir.run(&[
            "chapters/notes.md",
            "--to",
            target,
            "--interactive-html",
            "--json",
        ]);
        succeeded(&output);
        assert!(
            output.stdout.is_empty(),
            "{target} must not mix document streams"
        );
        assert_format("html", &dir.read("chapters/notes.html"));
        assert_format(target, &dir.read(&format!("chapters/notes.{target}")));
        assert!(!dir.0.join("notes.html").exists());
        assert!(!dir.0.join("document.html").exists());
        assert_eq!(dir.read("chapters/notes.md"), SIMPLE.as_bytes());
    }
}

#[test]
fn raw_text_companions_derive_document_files_without_stdout_data() {
    let dir = Workspace::new();
    let output = dir.run(&["--text", SIMPLE, "--to", "svg", "--self-hosting"]);
    succeeded(&output);
    assert!(output.stdout.is_empty());
    assert_format("html", &dir.read("document.html"));
    assert_format("svg", &dir.read("document.svg"));
}

#[test]
fn derived_companions_cannot_replace_primary_or_included_sources() {
    let dir = Workspace::new();
    dir.write("edition.html", SIMPLE);
    dir.write("edition.svg", b"previous SVG");
    let output = dir.run(&[
        "edition.html",
        "--to",
        "svg",
        "--interactive-html",
        "--out",
        "edition.svg",
    ]);
    assert_eq!(output.status.code(), Some(64));
    assert!(output.stdout.is_empty());
    assert_eq!(dir.read("edition.html"), SIMPLE.as_bytes());
    assert_eq!(dir.read("edition.svg"), b"previous SVG");

    dir.write("main.md", "# Root\n\n{{#include parts/chapter.html}}\n");
    dir.write(
        "parts/chapter.html",
        "Included [reference](https://example.invalid/).\n",
    );
    dir.write("parts/chapter.epub", b"previous EPUB");
    let output = dir.run(&[
        "main.md",
        "--to",
        "epub",
        "--interactive-html",
        "--out",
        "parts/chapter.epub",
    ]);
    assert_eq!(output.status.code(), Some(64));
    assert!(output.stdout.is_empty());
    assert_eq!(
        dir.read("parts/chapter.html"),
        b"Included [reference](https://example.invalid/).\n"
    );
    assert_eq!(dir.read("parts/chapter.epub"), b"previous EPUB");
}

#[cfg(unix)]
#[test]
fn derived_companion_aliases_preserve_the_original_source() {
    use std::os::unix::fs::symlink;

    for extension in ["html", "svg"] {
        let dir = Workspace::new();
        dir.write("source.md", SIMPLE);
        let alias = dir.0.join(format!("edition.{extension}"));
        symlink("source.md", &alias).unwrap();
        let output = dir.run(&[
            "source.md",
            "--to",
            "svg",
            "--interactive-html",
            "--out",
            "edition.requested",
        ]);
        assert_eq!(output.status.code(), Some(64), "{extension}");
        assert!(output.stdout.is_empty());
        assert_eq!(dir.read("source.md"), SIMPLE.as_bytes());
        assert!(std::fs::symlink_metadata(&alias).unwrap().is_symlink());
        assert!(!dir.0.join("edition.requested").exists());
    }
}

#[test]
fn render_and_staging_failures_preserve_previous_companions() {
    let dir = Workspace::new();
    dir.write("source.md", SIMPLE);
    dir.write("edition.html", b"previous HTML");
    dir.write("edition.pdf", b"previous PDF");
    let output = dir.run(&[
        "source.md",
        "--to",
        "pdf",
        "--interactive-html",
        "--out",
        "edition.pdf",
        "--pdf-split-footnotes",
        "--pdf-optimal-pagination",
    ]);
    assert_eq!(output.status.code(), Some(70));
    assert!(output.stdout.is_empty());
    assert_eq!(dir.read("edition.html"), b"previous HTML");
    assert_eq!(dir.read("edition.pdf"), b"previous PDF");

    dir.write("edition.svg/keep.txt", b"existing directory");
    let output = dir.run(&[
        "source.md",
        "--to",
        "svg",
        "--interactive-html",
        "--out",
        "edition.svg",
    ]);
    assert_eq!(output.status.code(), Some(73));
    assert!(output.stdout.is_empty());
    assert_eq!(dir.read("edition.html"), b"previous HTML");
    assert_eq!(dir.read("edition.svg/keep.txt"), b"existing directory");
    assert_eq!(dir.read("source.md"), SIMPLE.as_bytes());
}

#[test]
fn a_search_sidecar_cannot_alias_either_derived_companion() {
    for extension in ["html", "svg"] {
        let dir = Workspace::new();
        dir.write("source.md", SIMPLE);
        dir.write("edition.html", b"previous HTML");
        dir.write("edition.svg", b"previous SVG");
        let sidecar = format!("edition.{extension}");
        let output = dir.run(&[
            "source.md",
            "--to",
            "svg",
            "--interactive-html",
            "--out",
            "edition.requested",
            "--search-index",
            &sidecar,
        ]);
        assert_eq!(output.status.code(), Some(73), "{extension}");
        assert!(output.stdout.is_empty());
        assert_eq!(dir.read("edition.html"), b"previous HTML");
        assert_eq!(dir.read("edition.svg"), b"previous SVG");
        assert!(!dir.0.join("edition.requested").exists());
    }
}

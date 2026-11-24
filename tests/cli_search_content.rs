//! Full-content native sidecars share the document grammar and publication transaction.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fmd-search-content-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, text: &str) {
        std::fs::write(self.0.join(name), text).unwrap();
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.0.join(name)).unwrap()
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_fmd"))
            .current_dir(&self.0)
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

#[test]
fn native_sidecar_includes_all_supported_content_and_real_note_targets() {
    let dir = Workspace::new();
    let source = "# fn-n\n\nProse needle.[^n]\n\n~~~rust\nlaunch_unique();\n~~~\n\n| Key | Value |\n| --- | --- |\n| timeout_unique | 30 |\n\nCache_unique\n: Stores reusable values\n\n$$\nx_unique^2 + y^2\n$$\n\n[^n]: Note_unique body.\n\n[^unused]: Invisible_unique note.\n";
    dir.write("doc.md", source);
    let args = [
        "doc.md",
        "--profile",
        "gfm-plus",
        "--out",
        "doc.html",
        "--search-index",
        "search.json",
    ];
    succeeded(&dir.run(&args));
    let index = dir.read("search.json");
    assert!(index.starts_with("{\"schema\":\"fmd-search-index-v1\",\"entries\":["));
    for text in [
        "launch_unique();",
        "timeout_unique 30",
        "Cache_unique Stores reusable values",
        "x_unique^2 + y^2",
        "Note_unique body.",
    ] {
        assert!(index.contains(text), "missing {text:?}: {index}");
    }
    assert!(!index.contains("Invisible_unique"));
    assert!(index.contains("\"anchor\":\"fn-n-2\",\"text\":\"Note_unique body.\""));
    assert!(dir.read("doc.html").contains("<li id=\"fn-n-2\">"));
    assert_eq!(dir.read("doc.md"), source);

    succeeded(&dir.run(&args));
    assert_eq!(dir.read("search.json"), index);
}

#[test]
fn native_sidecar_uses_the_selected_authoring_grammar() {
    let dir = Workspace::new();
    dir.write("doc.md", "# Guide\n\nCache\n: Stores reusable values\n");
    for (profile, text) in [
        ("commonmark-gfm", "Cache : Stores reusable values"),
        ("gfm-plus", "Cache Stores reusable values"),
    ] {
        succeeded(&dir.run(&[
            "doc.md",
            "--profile",
            profile,
            "--out",
            "doc.html",
            "--search-index",
            "search.json",
        ]));
        let index = dir.read("search.json");
        assert!(
            index.contains(&format!("\"anchor\":\"guide\",\"text\":\"{text}\"")),
            "{profile}: {index}"
        );
    }
}

#[test]
fn a_failed_sidecar_write_preserves_existing_publication_and_source() {
    let dir = Workspace::new();
    let source = "# Guide\n\n~~~rust\nlaunch_unique();\n~~~\n";
    dir.write("doc.md", source);
    dir.write("doc.html", "previous publication");
    std::fs::create_dir(dir.0.join("search.json")).unwrap();
    let output = dir.run(&[
        "doc.md",
        "--out",
        "doc.html",
        "--search-index",
        "search.json",
    ]);
    assert!(!output.status.success());
    assert_eq!(dir.read("doc.html"), "previous publication");
    assert_eq!(dir.read("doc.md"), source);
    assert!(dir.0.join("search.json").is_dir());
}

//! Executable regressions for chapter/include-only roles in native publishing.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use franken_markdown::{BookInput, HtmlOptions, build_book};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn workspace() -> PathBuf {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!(
        "fmd-book-sources-{}-{stamp}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed),
    ));
    fs::create_dir_all(root.join("book/parts")).unwrap();
    root
}

fn invoke(binary: &str, root: &Path, args: &[&str]) -> Output {
    Command::new(binary).arg("--no-config").arg("book").arg("book")
        .args(args).arg("--json").env_remove("SOURCE_DATE_EPOCH")
        .current_dir(root).output().unwrap()
}

fn fmd(root: &Path, args: &[&str]) -> Output {
    invoke(env!("CARGO_BIN_EXE_fmd"), root, args)
}

fn succeeded(output: &Output) -> String {
    assert!(output.status.success(), "status {:?}: stderr={} stdout={}",
        output.status.code(), String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout));
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn failed(output: &Output, code: i32) -> String {
    assert_eq!(output.status.code(), Some(code), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(output.stdout.is_empty(), "errors must not contaminate data stdout");
    let message = String::from_utf8(output.stderr.clone()).unwrap();
    assert!(message.contains("\"ok\":false"), "{message}");
    message
}

#[test]
fn shared_markdown_expands_but_does_not_become_a_page_or_search_chapter() {
    let root = workspace();
    fs::write(root.join("book/start.md"), "# Start\n\n{{#include parts/shared.md}}\n").unwrap();
    fs::write(root.join("book/end.md"), "# End\n").unwrap();
    fs::write(root.join("book/parts/shared.md"), "Shared prose from the resource.\n").unwrap();
    fs::write(root.join("book/book.toml"), "order=['start.md']\ninclude_only=['parts/shared.md']\n").unwrap();
    let receipt = succeeded(&fmd(&root, &["--to", "html", "--out-dir", "site"]));
    assert!(receipt.contains("\"chapters\":2"), "{receipt}");
    assert!(!receipt.contains("\"path\":\"parts/shared.md\""), "{receipt}");
    let page = fs::read_to_string(root.join("site/start.html")).unwrap();
    assert!(page.contains("Shared prose from the resource."));
    assert!(!page.contains("{{#include"));
    assert!(!root.join("site/parts__shared.html").exists());
    let search = fs::read_to_string(root.join("site/search-index.json")).unwrap();
    assert!(!search.contains("parts__shared.html"));
    assert!(fs::read_to_string(root.join("site/index.html")).unwrap().contains("url=start.html"));
}

#[test]
fn absent_include_only_preserves_explicit_order_and_lexical_remainder() {
    let root = workspace();
    fs::write(root.join("book/a.md"), "# A\n").unwrap();
    fs::write(root.join("book/z.md"), "# Z\n").unwrap();
    fs::write(root.join("book/parts/shared.md"), "# Shared\n").unwrap();
    fs::write(root.join("book/book.toml"), "chapters=['z.md']\n").unwrap();
    let receipt = succeeded(&fmd(&root, &["--to", "html", "--out-dir", "site"]));
    assert!(receipt.contains("\"chapters\":3"));
    assert!(root.join("site/parts__shared.html").exists());
    assert!(receipt.find("\"path\":\"z.md\"").unwrap() < receipt.find("\"path\":\"a.md\"").unwrap());
}

#[test]
fn unused_resources_are_not_parsed_and_epub_matches_the_selected_core_book() {
    let root = workspace();
    let source = "# Start\n\nOnly the publishing chapter.\n";
    fs::write(root.join("book/start.md"), source).unwrap();
    fs::write(root.join("book/parts/unused.md"), [0xff, 0xfe, 0x00]).unwrap();
    fs::write(root.join("book/book.toml"), "include_only=['parts/unused.md']\n").unwrap();
    let book = build_book(&[BookInput { path: "start.md".into(), source: source.into() }]).unwrap();
    let expected = franken_markdown::epub::render_book_epub(&book, &HtmlOptions {
        title: Some("Start".into()), ..HtmlOptions::default()
    }).unwrap();
    for (binary, destination) in [(env!("CARGO_BIN_EXE_fmd"), "short.epub"),
        (env!("CARGO_BIN_EXE_franken_markdown"), "long.epub")]
    {
        let receipt = succeeded(&invoke(binary, &root, &["--to", "epub", "--out", destination]));
        assert!(receipt.contains("\"chapters\":1"));
        assert_eq!(fs::read(root.join(destination)).unwrap(), expected);
    }
}

#[test]
fn invalid_role_manifests_preserve_existing_outputs() {
    let root = workspace();
    fs::write(root.join("book/a.md"), "# A\n").unwrap();
    fs::write(root.join("book/parts/shared.md"), "Shared.\n").unwrap();
    fs::write(root.join("previous.epub"), "previous publication").unwrap();
    for manifest in [
        "include_only=['missing.md']", "include_only=['../outside.md']",
        "include_only=['parts/shared.md','./parts/shared.md']",
        "order=['parts/shared.md']\ninclude_only=['parts/shared.md']",
        "include_only=['a.md','parts/shared.md']", "include_only='parts/shared.md'",
    ] {
        fs::write(root.join("book/book.toml"), manifest).unwrap();
        failed(&fmd(&root, &["--to", "epub", "--out", "previous.epub"]), 66);
        assert_eq!(fs::read(root.join("previous.epub")).unwrap(), b"previous publication");
    }
}

#[test]
fn unused_declared_resources_remain_protected_from_output_replacement() {
    let root = workspace();
    fs::write(root.join("book/a.md"), "# A\n").unwrap();
    fs::write(root.join("book/parts/unused.md"), "KEEP UNUSED SOURCE").unwrap();
    fs::write(root.join("book/book.toml"), "include_only=['parts/unused.md']\n").unwrap();
    let error = failed(&fmd(&root, &["--to", "epub", "--out", "book/parts/unused.md"]), 73);
    assert!(error.contains("overwrite a book input"));
    assert_eq!(fs::read(root.join("book/parts/unused.md")).unwrap(), b"KEEP UNUSED SOURCE");
}

#[test]
fn referenced_include_only_sources_still_enforce_utf8_and_cycles() {
    let root = workspace();
    fs::write(root.join("book/start.md"), "# Start\n\n{{#include parts/shared.md}}\n").unwrap();
    fs::write(root.join("book/book.toml"), "include_only=['parts/shared.md']\n").unwrap();
    for bytes in [vec![0xff], b"{{#include shared.md}}\n".to_vec()] {
        fs::write(root.join("book/parts/shared.md"), bytes).unwrap();
        failed(&fmd(&root, &["--to", "html", "--out-dir", "never-created"]), 66);
        assert!(!root.join("never-created").exists());
    }
}

#[test]
fn output_name_collisions_are_checked_only_for_published_chapters() {
    let root = workspace();
    fs::create_dir(root.join("book/a")).unwrap();
    fs::write(root.join("book/a/b.md"), "# Chapter\n").unwrap();
    fs::write(root.join("book/a__b.md"), "Shared resource.\n").unwrap();
    fs::write(root.join("book/book.toml"), "include_only=['a__b.md']\n").unwrap();
    let receipt = succeeded(&fmd(&root, &["--to", "html", "--out-dir", "site"]));
    assert!(receipt.contains("\"chapters\":1"));
    assert!(fs::read_to_string(root.join("site/a__b.html")).unwrap().contains("Chapter"));
}

#[cfg(unix)]
#[test]
fn include_only_cannot_turn_a_symlink_into_an_authorized_source() {
    let root = workspace();
    fs::write(root.join("book/a.md"), "# A\n").unwrap();
    fs::write(root.join("outside.md"), "OUTSIDE SOURCE").unwrap();
    std::os::unix::fs::symlink(root.join("outside.md"), root.join("book/parts/shared.md")).unwrap();
    fs::write(root.join("book/book.toml"), "include_only=['parts/shared.md']\n").unwrap();
    failed(&fmd(&root, &["--to", "html", "--out-dir", "never-created"]), 66);
    assert!(!root.join("never-created").exists());
    assert_eq!(fs::read(root.join("outside.md")).unwrap(), b"OUTSIDE SOURCE");
}

//! Real executable coverage for native EPUB and transactional book publication.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use franken_markdown::{BookInput, HtmlOptions, PdfImageAsset, build_book};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn workspace() -> PathBuf {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!(
        "fmd-native-book-{}-{stamp}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    root
}

fn invoke(binary: &str, args: &[&str], cwd: &Path) -> Output {
    Command::new(binary).arg("--no-config").args(args)
        .env_remove("SOURCE_DATE_EPOCH").current_dir(cwd).output().unwrap()
}

fn fmd(args: &[&str], cwd: &Path) -> Output {
    invoke(env!("CARGO_BIN_EXE_fmd"), args, cwd)
}

fn succeeded(output: &Output) {
    assert!(output.status.success(), "status {:?}: stderr={} stdout={}",
        output.status.code(), String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout));
}

fn failed(output: &Output, code: i32, message: &str) {
    assert_eq!(output.status.code(), Some(code), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(output.stdout.is_empty(), "errors must not contaminate data stdout");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("\"ok\":false"), "{stderr}");
    assert!(stderr.contains(message), "expected {message:?} in {stderr}");
}

fn svg(width: u32) -> Vec<u8> {
    format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"10\"><rect width=\"{width}\" height=\"10\"/></svg>").into_bytes()
}

#[test]
fn epub_command_matches_core_bytes_and_honors_manifest_metadata_and_order() {
    let root = workspace();
    fs::create_dir_all(root.join("book/guide")).unwrap();
    let start = "# Start\n\n[Next](../index.md#home)\n";
    let home = "# Home\n\n[Back](guide/start.md#start)\n";
    fs::write(root.join("book/guide/start.md"), start).unwrap();
    fs::write(root.join("book/index.md"), home).unwrap();
    fs::write(root.join("book/book.toml"), "title='Manual'\nlang='fr'\norder=['index.md', 'guide/start.md']\n").unwrap();
    let output = fmd(&["book", "book", "--to", "epub", "--out", "dist/manual.epub", "--json"], &root);
    succeeded(&output);
    let receipt = String::from_utf8_lossy(&output.stdout);
    assert!(receipt.contains("\"target\":\"epub\""));
    assert!(receipt.contains("\"chapters\":2"));
    assert!(receipt.contains("\"unresolved_links\":0"));
    assert!(receipt.contains("\"out_name\":\"chapter-1.xhtml\""));
    let book = build_book(&[
        BookInput { path: "index.md".into(), source: home.into() },
        BookInput { path: "guide/start.md".into(), source: start.into() },
    ]).unwrap();
    let expected = franken_markdown::epub::render_book_epub(&book, &HtmlOptions {
        title: Some("Manual".into()), lang: Some("fr".into()), ..HtmlOptions::default()
    }).unwrap();
    assert_eq!(fs::read(root.join("dist/manual.epub")).unwrap(), expected);
    let repeat = fmd(&["book", "book", "--to", "epub", "--out", "dist/again.epub", "--json"], &root);
    succeeded(&repeat);
    assert_eq!(fs::read(root.join("dist/again.epub")).unwrap(), expected);
}

#[test]
fn both_executable_names_accept_epub_and_share_identical_results() {
    let root = workspace();
    fs::create_dir(root.join("book")).unwrap();
    fs::write(root.join("book/a.md"), "# A\n").unwrap();
    for (binary, output) in [(env!("CARGO_BIN_EXE_fmd"), "short.epub"),
        (env!("CARGO_BIN_EXE_franken_markdown"), "long.epub")]
    {
        succeeded(&invoke(binary, &["book", "book", "--to", "epub", "--out-dir", output, "--json"], &root));
    }
    assert_eq!(fs::read(root.join("short.epub")).unwrap(), fs::read(root.join("long.epub")).unwrap());
}

#[test]
fn chapter_local_images_with_identical_basenames_are_packaged_not_lost() {
    let root = workspace();
    fs::create_dir_all(root.join("book/a")).unwrap();
    fs::create_dir_all(root.join("book/b")).unwrap();
    fs::write(root.join("book/a/start.md"), "# A\n\n![First](figure.svg)\n").unwrap();
    fs::write(root.join("book/b/end.md"), "# B\n\n![Second](figure.svg)\n").unwrap();
    fs::write(root.join("book/a/figure.svg"), svg(10)).unwrap();
    fs::write(root.join("book/b/figure.svg"), svg(20)).unwrap();
    let output = fmd(&["book", "book", "--to", "epub", "--out", "images.epub", "--json"], &root);
    succeeded(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("\"images\":2"));
    let book = build_book(&[
        BookInput { path: "a/start.md".into(), source: "# A\n\n![First](/a/figure.svg)\n".into() },
        BookInput { path: "b/end.md".into(), source: "# B\n\n![Second](/b/figure.svg)\n".into() },
    ]).unwrap();
    let expected = franken_markdown::epub::render_book_epub(&book, &HtmlOptions {
        title: Some("A".into()),
        image_assets: vec![
            PdfImageAsset { destination: "/a/figure.svg".into(), bytes: svg(10) },
            PdfImageAsset { destination: "/b/figure.svg".into(), bytes: svg(20) },
        ],
        ..HtmlOptions::default()
    }).unwrap();
    assert_eq!(fs::read(root.join("images.epub")).unwrap(), expected);
}

#[test]
fn missing_images_are_reported_without_fetching_remote_destinations() {
    let root = workspace();
    fs::create_dir(root.join("book")).unwrap();
    fs::write(root.join("book/a.md"), "# A\n\n![Missing](absent.png) ![Remote](https://invalid.example/image.png)\n").unwrap();
    let result = fmd(&["book", "book", "--to", "epub", "--out", "book.epub", "--json"], &root);
    succeeded(&result);
    let receipt = String::from_utf8_lossy(&result.stdout);
    assert!(receipt.contains("image_unavailable"));
    assert!(receipt.contains("image_not_loaded"));
    assert!(receipt.contains("\"images\":0"));
}

#[test]
fn nested_includes_expand_inside_root_and_respect_expanded_size_cap() {
    let root = workspace();
    fs::create_dir_all(root.join("book/.parts")).unwrap();
    fs::write(root.join("book/start.md"), "# Start\n\n{{#include .parts/one.md}}\n").unwrap();
    fs::write(root.join("book/.parts/one.md"), "Included first.\n{{#include two.md}}\n").unwrap();
    fs::write(root.join("book/.parts/two.md"), "Included second.\n").unwrap();
    succeeded(&fmd(&["book", "book", "--to", "html", "--out-dir", "site", "--json"], &root));
    let html = fs::read_to_string(root.join("site/start.html")).unwrap();
    assert!(html.contains("Included first."));
    assert!(html.contains("Included second."));
    fs::write(root.join("book/start.md"), "{{#include .parts/two.md}}\n{{#include .parts/two.md}}\n").unwrap();
    fs::write(root.join("book/.parts/two.md"), "x".repeat(60)).unwrap();
    let result = fmd(&["book", "book", "--to", "html", "--out-dir", "site", "--max-input-bytes", "80", "--json"], &root);
    failed(&result, 66, "include_size");
    assert_eq!(fs::read_to_string(root.join("site/start.html")).unwrap(), html);
}

#[test]
fn invalid_manifest_preserves_previous_publication_without_partial_outputs() {
    let root = workspace();
    fs::create_dir(root.join("book")).unwrap();
    fs::write(root.join("book/a.md"), "# A\n").unwrap();
    fs::write(root.join("previous.epub"), b"old publication").unwrap();
    for manifest in [
        "order=['missing.md']", "order=['a.md','./a.md']", "order=['../outside.md']",
        "order=['a.md'", "unknown='ignored-before'",
    ] {
        fs::write(root.join("book/book.toml"), manifest).unwrap();
        let result = fmd(&["book", "book", "--to", "epub", "--out", "previous.epub", "--json"], &root);
        failed(&result, 66, "book_error");
        assert_eq!(fs::read(root.join("previous.epub")).unwrap(), b"old publication");
    }
}

#[test]
fn index_chapter_survives_combined_export_and_local_images_reach_html() {
    let root = workspace();
    fs::create_dir(root.join("book")).unwrap();
    fs::write(root.join("book/index.md"), "# Home\n\nUNIQUE_HOME_TEXT\n\n![Figure](figure.svg)\n").unwrap();
    fs::write(root.join("book/figure.svg"), svg(10)).unwrap();
    let result = fmd(&["book", "book", "--to", "both", "--out-dir", "site", "--json"], &root);
    succeeded(&result);
    let chapter = fs::read_to_string(root.join("site/~chapter-index.html")).unwrap();
    assert!(chapter.contains("UNIQUE_HOME_TEXT"));
    assert!(chapter.contains("data:image/svg+xml;base64,"));
    let landing = fs::read_to_string(root.join("site/index.html")).unwrap();
    assert!(landing.contains("url=~chapter-index.html"));
    assert!(fs::read(root.join("site/book.pdf")).unwrap().starts_with(b"%PDF-"));
    assert!(!String::from_utf8_lossy(&result.stdout).contains("\"pages\":0"));
}

#[test]
fn output_cannot_replace_an_input_or_destroy_old_files_on_failure() {
    let root = workspace();
    fs::create_dir(root.join("book")).unwrap();
    let source = "# Do not overwrite\n";
    fs::write(root.join("book/a.md"), source).unwrap();
    let result = fmd(&["book", "book", "--to", "epub", "--out", "book/a.md", "--json"], &root);
    failed(&result, 73, "overwrite a book input");
    assert_eq!(fs::read_to_string(root.join("book/a.md")).unwrap(), source);
    fs::create_dir_all(root.join("site/book.pdf")).unwrap();
    fs::write(root.join("site/a.html"), "old HTML").unwrap();
    let result = fmd(&["book", "book", "--to", "both", "--out-dir", "site", "--json"], &root);
    failed(&result, 73, "output is a directory");
    assert_eq!(fs::read_to_string(root.join("site/a.html")).unwrap(), "old HTML");
    assert!(!root.join("site/index.html").exists());
}

#[test]
fn binary_stdout_and_incompatible_output_flags_are_usage_errors() {
    let root = workspace();
    for args in [
        vec!["book", "missing", "--to", "epub", "--out", "-", "--json"],
        vec!["book", "missing", "--to", "html", "--out", "site", "--json"],
        vec!["book", "missing", "--to", "epub", "--out", "x", "--out-dir", "y", "--json"],
    ] {
        failed(&fmd(&args, &root), 64, "usage_error");
    }
}

#[cfg(unix)]
#[test]
fn directory_symlink_cycles_are_skipped_and_explicit_symlink_includes_refused() {
    use std::os::unix::fs::symlink;
    let root = workspace();
    fs::create_dir(root.join("book")).unwrap();
    fs::write(root.join("book/a.md"), "# A\n").unwrap();
    symlink(".", root.join("book/loop")).unwrap();
    let output = fmd(&["book", "book", "--to", "epub", "--out", "book.epub", "--json"], &root);
    succeeded(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("symlink_skipped"));
    fs::write(root.join("secret.md"), "PRIVATE_TEXT").unwrap();
    symlink("../secret.md", root.join("book/linked.md")).unwrap();
    fs::write(root.join("book/a.md"), "{{#include linked.md}}\n").unwrap();
    let output = fmd(&["book", "book", "--to", "epub", "--out", "denied.epub", "--json"], &root);
    failed(&output, 66, "symlink component");
    assert!(!root.join("denied.epub").exists());
}

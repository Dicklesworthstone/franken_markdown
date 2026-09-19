//! Real executable regressions for read-only and fail-before-write book checks.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use franken_markdown::book::{BookInput, build_book};
use franken_markdown::book::validation::check_book_links;

static NEXT: AtomicU64 = AtomicU64::new(0);

fn workspace() -> PathBuf {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!(
        "fmd-book-check-{}-{stamp}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed),
    ));
    fs::create_dir_all(root.join("book/parts")).unwrap();
    root
}

fn command(binary: &str, root: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(binary);
    cmd.arg("--no-config").arg("book").arg("book").args(args)
        .env_remove("SOURCE_DATE_EPOCH").current_dir(root);
    cmd
}

fn fmd(root: &Path, args: &[&str]) -> Output {
    command(env!("CARGO_BIN_EXE_fmd"), root, args).output().unwrap()
}

fn status(output: &Output, expected: i32) {
    assert_eq!(output.status.code(), Some(expected), "stderr={} stdout={}",
        String::from_utf8_lossy(&output.stderr), String::from_utf8_lossy(&output.stdout));
}

fn failure(output: &Output, code: i32, tag: &str) {
    status(output, code);
    assert!(output.stdout.is_empty(), "failed checks must not emit partial data");
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(message.contains("\"ok\":false"), "{message}");
    assert!(message.contains(tag), "{message}");
}

fn core_report(files: &[(&str, &str)]) -> String {
    let inputs = files.iter().map(|(path, source)| BookInput {
        path: (*path).into(), source: (*source).into(),
    }).collect::<Vec<_>>();
    check_book_links(&build_book(&inputs).unwrap()).unwrap().to_json().unwrap()
}

#[test]
fn clean_read_only_json_is_byte_identical_to_the_core_report_in_both_binaries() {
    let root = workspace();
    let first = "# Start\n\n[Next](end.md#end) [Remote](https://example.invalid/) [Download](manual.zip)\n";
    let last = "# End\n\n[Back](start.md#start)\n";
    fs::write(root.join("book/start.md"), first).unwrap();
    fs::write(root.join("book/end.md"), last).unwrap();
    fs::write(root.join("book/book.toml"), "order=['start.md','end.md']\n").unwrap();
    let expected = core_report(&[("start.md", first), ("end.md", last)]) + "\n";
    for binary in [env!("CARGO_BIN_EXE_fmd"), env!("CARGO_BIN_EXE_franken_markdown")] {
        let output = command(binary, &root, &["--check-links", "--json"]).output().unwrap();
        status(&output, 0);
        assert!(output.stderr.is_empty());
        assert_eq!(output.stdout, expected.as_bytes());
    }
    for path in ["book-site", "book.pdf", "book.epub"] { assert!(!root.join(path).exists()); }
}

#[test]
fn expanded_resource_findings_belong_to_the_publishing_chapter() {
    let root = workspace();
    fs::write(root.join("book/start.md"), "# Start\n\n{{#include parts/shared.md}}\n").unwrap();
    fs::write(root.join("book/parts/shared.md"), "[Bad](#absent) [Not a chapter](parts/shared.md)\n").unwrap();
    fs::write(root.join("book/book.toml"), "include_only=['parts/shared.md']\n").unwrap();
    let output = fmd(&root, &["--check-links", "--json"]);
    status(&output, 65);
    let json = String::from_utf8(output.stdout).unwrap();
    assert!(json.contains("\"schema\":\"fmd-book-link-report-v1\""));
    assert!(json.contains("\"scope\":\"expanded-html-navigation\""));
    assert!(json.contains("\"path\":\"start.md\""));
    assert!(!json.contains("\"path\":\"parts/shared.md\""));
    assert!(json.contains("\"code\":\"missing_anchor\""));
    assert!(json.contains("\"code\":\"missing_chapter\""));
    assert!(json.contains("\"chapters\":1,\"checked\":2,\"external\":0,\"unchecked\":0,\"findings\":2"));
    assert!(output.stderr.is_empty(), "completed findings belong in the report");
    assert!(!root.join("book-site").exists());
}

#[test]
fn included_headings_are_available_to_forward_references() {
    let root = workspace();
    fs::write(root.join("book/start.md"), "# Start\n\n[Forward](end.md#shared-target)\n").unwrap();
    fs::write(root.join("book/end.md"), "# End\n\n{{#include parts/shared.md}}\n").unwrap();
    fs::write(root.join("book/parts/shared.md"), "## Shared target\n").unwrap();
    fs::write(root.join("book/book.toml"), "order=['start.md','end.md']\ninclude_only=['parts/shared.md']\n").unwrap();
    let output = fmd(&root, &["--check-links", "--json"]);
    status(&output, 0);
    assert!(String::from_utf8_lossy(&output.stdout).contains("\"findings\":0"));
}

#[test]
fn check_only_never_enters_the_image_loader_or_pdf_metadata_path() {
    let root = workspace();
    let mut source = String::from("# Start\n\n[Here](#start)\n\n");
    // Normal image discovery rejects over 4096 image diagnostics. A source
    // navigation check must not even enter that phase, irrespective of URLs.
    for index in 0..4097 {
        source.push_str(&format!("![Unverified](https://example.invalid/{index}.svg)\n\n"));
    }
    fs::write(root.join("book/start.md"), &source).unwrap();
    let output = command(env!("CARGO_BIN_EXE_fmd"), &root, &["--check-links", "--json"])
        .env("SOURCE_DATE_EPOCH", "deliberately-invalid").output().unwrap();
    status(&output, 0);
    assert!(output.stderr.is_empty(), "image warnings must not leak into source checking");
    assert_eq!(output.stdout, (core_report(&[("start.md", &source)]) + "\n").as_bytes());
    assert!(!root.join("book.pdf").exists());
}

#[test]
fn read_only_check_preserves_existing_publications_and_original_sources() {
    let root = workspace();
    let source = "# Start\n\n[Bad](#absent)\n";
    fs::write(root.join("book/start.md"), source).unwrap();
    fs::create_dir(root.join("book-site")).unwrap();
    for (path, bytes) in [("book.pdf", "OLD PDF"), ("book.epub", "OLD EPUB"),
        ("book-site/start.html", "OLD CHAPTER"), ("book-site/index.html", "OLD INDEX")]
    {
        fs::write(root.join(path), bytes).unwrap();
    }
    status(&fmd(&root, &["--check-links", "--json"]), 65);
    assert_eq!(fs::read_to_string(root.join("book/start.md")).unwrap(), source);
    for (path, bytes) in [("book.pdf", "OLD PDF"), ("book.epub", "OLD EPUB"),
        ("book-site/start.html", "OLD CHAPTER"), ("book-site/index.html", "OLD INDEX")]
    {
        assert_eq!(fs::read_to_string(root.join(path)).unwrap(), bytes);
    }
    assert_eq!(fs::read_dir(root.join("book-site")).unwrap().count(), 2);
}

#[test]
fn check_mode_rejects_publication_options_before_loading_input() {
    let root = workspace();
    for conflicting in [
        vec!["--out", "never.pdf"], vec!["--out-dir", "never"], vec!["--to", "html"],
        vec!["--css", "absent.css"], vec!["--title", "Title"], vec!["--font", "serif"],
        vec!["--deny-broken-links"], vec!["--robot-triage"],
    ] {
        let mut args = vec!["--check-links", "--json"];
        args.extend(conflicting);
        failure(&fmd(&root, &args), 64, "usage_error");
    }
    assert!(!root.join("never").exists());
    assert!(!root.join("never.pdf").exists());
}

#[test]
fn missing_includes_cycles_and_input_limits_fail_without_partial_reports() {
    let root = workspace();
    fs::write(root.join("book/start.md"), "# Start\n\n{{#include parts/missing.md}}\n").unwrap();
    failure(&fmd(&root, &["--check-links", "--json"]), 66, "book_error");
    fs::write(root.join("book/parts/missing.md"), "{{#include missing.md}}\n").unwrap();
    fs::write(root.join("book/book.toml"), "include_only=['parts/missing.md']\n").unwrap();
    failure(&fmd(&root, &["--check-links", "--json"]), 66, "book_error");
    fs::write(root.join("book/parts/missing.md"), "Now resolved.\n").unwrap();
    failure(&fmd(&root, &["--check-links", "--json", "--max-input-bytes", "8"]), 66, "book_error");
    for path in ["book-site", "book.pdf", "book.epub"] { assert!(!root.join(path).exists()); }
}

#[test]
fn finding_budget_is_an_incomplete_check_error_not_a_truncated_verdict() {
    let root = workspace();
    fs::write(root.join("book/start.md"), format!("# Start\n\n{}", "[Broken](#absent)\n\n".repeat(4097))).unwrap();
    failure(&fmd(&root, &["--check-links", "--json"]), 70, "book_check_error");
    assert!(!root.join("book-site").exists());
}

#[test]
fn human_check_keeps_stdout_empty_and_explains_scope_and_findings() {
    let root = workspace();
    fs::write(root.join("book/start.md"), "# Start\n\n[Bad](#missing) [Remote](https://example.invalid/) [Download](data.zip)\n").unwrap();
    let output = fmd(&root, &["--check-links"]);
    status(&output, 65);
    assert!(output.stdout.is_empty());
    let text = String::from_utf8(output.stderr).unwrap();
    assert!(text.contains("expanded HTML navigation"));
    assert!(text.contains("missing_anchor"));
    assert!(text.contains("1 external URL(s), 1 other local resource(s)"));
    assert!(text.contains("No publication outputs were written"));
}

#[test]
fn strict_publication_refuses_findings_before_asset_loading_stylesheets_or_writes() {
    let root = workspace();
    let mut source = String::from("# Start\n\n[Broken](#missing)\n\n");
    for index in 0..4097 { source.push_str(&format!("![Unverified](https://example.invalid/{index}.svg)\n\n")); }
    fs::write(root.join("book/start.md"), source).unwrap();
    fs::write(root.join("previous.epub"), "PREVIOUS PUBLICATION").unwrap();
    let output = fmd(&root, &["--to", "epub", "--out", "previous.epub", "--css", "absent.css", "--deny-broken-links", "--json"]);
    failure(&output, 65, "book_link_findings");
    assert!(String::from_utf8_lossy(&output.stderr).contains("--check-links --json"));
    assert_eq!(fs::read(root.join("previous.epub")).unwrap(), b"PREVIOUS PUBLICATION");
    let output = fmd(&root, &["--to", "html", "--out-dir", "never-created", "--deny-broken-links", "--json"]);
    failure(&output, 65, "book_link_findings");
    assert!(!root.join("never-created").exists());
}

#[test]
fn successful_strict_epub_matches_default_bytes_and_attaches_the_core_report() {
    let root = workspace();
    let source = "# Start\n\n[Here](#start) [Remote](https://example.invalid/) [Download](manual.zip)\n";
    fs::write(root.join("book/start.md"), source).unwrap();
    let normal = fmd(&root, &["--to", "epub", "--out", "normal.epub", "--json"]);
    status(&normal, 0);
    assert!(!String::from_utf8_lossy(&normal.stdout).contains("\"link_check\""));
    let strict = fmd(&root, &["--to", "epub", "--out", "strict.epub", "--deny-broken-links", "--json"]);
    status(&strict, 0);
    assert_eq!(fs::read(root.join("normal.epub")).unwrap(), fs::read(root.join("strict.epub")).unwrap());
    let report = core_report(&[("start.md", source)]);
    assert!(String::from_utf8_lossy(&strict.stdout).contains(&format!("\"link_check\":{report}")));
}

#[test]
fn strict_guard_is_opt_in_and_default_rendering_still_allows_broken_links() {
    let root = workspace();
    fs::write(root.join("book/start.md"), "# Start\n\n[Broken](#missing)\n").unwrap();
    let output = fmd(&root, &["--to", "html", "--out-dir", "normal", "--json"]);
    status(&output, 0);
    assert!(fs::read_to_string(root.join("normal/start.html")).unwrap().contains("#missing"));
    failure(&fmd(&root, &["--to", "html", "--out-dir", "strict", "--deny-broken-links", "--json"]), 65, "book_link_findings");
    assert!(!root.join("strict").exists());
}

#[cfg(unix)]
#[test]
fn discovery_warnings_are_separate_json_diagnostics_not_report_mutations() {
    let root = workspace();
    let source = "# Start\n";
    fs::write(root.join("book/start.md"), source).unwrap();
    fs::write(root.join("outside.md"), "OUTSIDE").unwrap();
    std::os::unix::fs::symlink(root.join("outside.md"), root.join("book/link.md")).unwrap();
    let output = fmd(&root, &["--check-links", "--json"]);
    status(&output, 0);
    assert_eq!(output.stdout, (core_report(&[("start.md", source)]) + "\n").as_bytes());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(stderr.lines().count(), 1);
    assert!(stderr.contains("\"code\":\"book_input_warning\""));
    assert!(stderr.contains("symlink_skipped: link.md"));
}

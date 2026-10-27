//! Native sites must resolve chapter source paths, never output-name lookalikes.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn workspace() -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "fmd-book-navigation-{}-{stamp}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(root.join("book/a")).unwrap();
    fs::create_dir_all(root.join("book/guide")).unwrap();
    fs::write(root.join("empty.css"), "").unwrap();
    fs::write(root.join("book/a/b.md"), "# Real\n").unwrap();
    fs::write(root.join("book/target.md"), "# Target\n").unwrap();
    root
}

fn publish(root: &Path, target: &str) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args([
            "--no-config",
            "book",
            "book",
            "--to",
            target,
            "--out-dir",
            "site",
            "--css",
            "empty.css",
            "--json",
        ])
        .env_remove("SOURCE_DATE_EPOCH")
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn nonexistent_flattened_and_extension_aliases_do_not_capture_other_chapters() {
    let root = workspace();
    fs::write(
        root.join("book/start.md"),
        "# Start\n\n[Unknown flat](/a__b.md#real)\n\n[Unknown extension](/target.markdown?print#target)\n\n[Actual](/a/b.md?print#real)\n",
    )
    .unwrap();
    let receipt = publish(&root, "html");
    assert!(receipt.contains("\"unresolved_links\":2"), "{receipt}");
    let html = fs::read_to_string(root.join("site/start.html")).unwrap();
    for expected in [
        "href=\"/a__b.md#real\">Unknown flat</a>",
        "href=\"/target.markdown?print#target\">Unknown extension</a>",
        "href=\"a__b.html?print#real\">Actual</a>",
    ] {
        assert!(html.contains(expected), "missing {expected}");
    }
}

#[test]
fn nested_missing_siblings_remain_unknown_in_tables_quotes_and_notes() {
    let root = workspace();
    fs::write(
        root.join("book/guide/start.md"),
        "# Guide\n\n> [Missing sibling](target.md#target)\n\n| Reference |\n| --- |\n| [Actual parent](../target.md?print#target) |\n| [Missing flat](/a__b.md#real) |\n\nNote[^n].\n\n[^n]: [Missing note sibling](target.md) and [Real note link](../a/b.md#real).\n",
    )
    .unwrap();
    let receipt = publish(&root, "html");
    assert!(receipt.contains("\"unresolved_links\":3"), "{receipt}");
    let html = fs::read_to_string(root.join("site/guide__start.html")).unwrap();
    for expected in [
        "href=\"target.md#target\">Missing sibling</a>",
        "href=\"target.html?print#target\">Actual parent</a>",
        "href=\"/a__b.md#real\">Missing flat</a>",
        "href=\"target.md\">Missing note sibling</a>",
        "href=\"a__b.html#real\">Real note link</a>",
    ] {
        assert!(html.contains(expected), "missing {expected}");
    }
}

#[test]
fn combined_publication_keeps_valid_aliases_and_unresolved_links_distinct() {
    let root = workspace();
    fs::create_dir_all(root.join("book/reference")).unwrap();
    fs::write(root.join("book/reference/index.md"), "# Reference\n").unwrap();
    fs::write(
        root.join("book/guide/start.md"),
        "# Guide\n\n[Extensionless](../target?print#target) [Directory](../reference/#reference) [Unknown](target.md).\n",
    )
    .unwrap();
    let receipt = publish(&root, "both");
    assert!(receipt.contains("\"unresolved_links\":1"), "{receipt}");
    let html = fs::read_to_string(root.join("site/guide__start.html")).unwrap();
    for expected in [
        "href=\"target.html?print#target\">Extensionless</a>",
        "href=\"reference__index.html#reference\">Directory</a>",
        "href=\"target.md\">Unknown</a>",
    ] {
        assert!(html.contains(expected), "missing {expected}");
    }
    assert!(
        fs::read(root.join("site/book.pdf"))
            .unwrap()
            .starts_with(b"%PDF-")
    );
}

#[test]
fn raw_html_chapter_links_participate_in_receipts_and_fail_before_publication() {
    let root = workspace();
    fs::write(root.join("book/guide/start.md"), "# Guide\n\nInline <a href='../target.md?x=1&amp;y=2#target'>Actual</a>.\n\n<div><a href='missing.md'>Missing chapter</a><a href='../target.md#absent'>Missing heading</a><!-- <a href='comment.md'> --><img src='image.md'></div>\n").unwrap();
    let receipt = publish(&root, "html");
    assert!(receipt.contains("\"unresolved_links\":1"), "{receipt}");
    let html = fs::read_to_string(root.join("site/guide__start.html")).unwrap();
    assert!(html.contains("href=\"target.html?x=1&amp;y=2#target\""));
    for args in [
        vec!["--check-links", "--json"],
        vec![
            "--deny-broken-links",
            "--to",
            "html",
            "--out-dir",
            "guarded",
            "--json",
        ],
    ] {
        let check_only = args[0] == "--check-links";
        let output = Command::new(env!("CARGO_BIN_EXE_fmd"))
            .args(["--no-config", "book", "book"])
            .args(args)
            .env_remove("SOURCE_DATE_EPOCH")
            .current_dir(&root)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(65),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if check_only {
            assert!(report.contains("missing_chapter"), "{report}");
            assert!(report.contains("missing_anchor"), "{report}");
        } else {
            assert!(report.contains("book_link_findings"), "{report}");
            assert!(report.contains("2 broken expanded-book"), "{report}");
        }
    }
    assert!(!root.join("guarded").exists());
}

//! Authoring profiles across real native book exports and source-only checks.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use franken_markdown::book::render_book_pdf;
use franken_markdown::{BookInput, HtmlOptions, PdfOptions, Profile, build_book};

const SOURCE: &str = "# Start\n\nTerm\n: Definition with **emphasis** and [detail](#after).\n\n<!-- pagebreak -->\n\n## After\n\nAfter the boundary.\n";

fn workspace() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "fmd-book-profile-{}-{stamp}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
    ));
    fs::create_dir_all(root.join("book/parts")).unwrap();
    fs::write(root.join("book/start.md"), SOURCE).unwrap();
    root
}

fn invoke(binary: &str, root: &Path, args: &[&str]) -> Output {
    Command::new(binary)
        .args(["--no-config", "book", "book", "--json"])
        .args(args)
        .env_remove("SOURCE_DATE_EPOCH")
        .current_dir(root)
        .output()
        .unwrap()
}

fn fmd(root: &Path, args: &[&str]) -> Output {
    invoke(env!("CARGO_BIN_EXE_fmd"), root, args)
}

fn status(output: &Output, expected: i32) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn every_publication_target_honors_profiles_and_preserves_default_bytes() {
    let root = workspace();
    let book = build_book(&[BookInput {
        path: "start.md".into(),
        source: SOURCE.into(),
    }])
    .unwrap();
    let mut outputs = Vec::new();
    for (name, profile) in [
        ("ordinary", None),
        ("plus", Some(Profile::GfmPlus)),
        ("commonmark", Some(Profile::CommonMarkGfm)),
    ] {
        let mut args = vec![
            "--to",
            "both",
            "--out-dir",
            name,
            "--toc=false",
            "--pdf-page-numbers=false",
            "--deny-broken-links",
        ];
        if let Some(profile) = profile {
            args.extend(["--profile", profile.as_str()]);
        }
        let output = fmd(&root, &args);
        status(&output, 0);
        let html = fs::read_to_string(root.join(name).join("start.html")).unwrap();
        let extended = profile != Some(Profile::CommonMarkGfm);
        assert_eq!(html.contains("<dl>"), extended);
        assert_eq!(html.contains("class=\"fmd-pagebreak\""), extended);
        assert!(html.contains("href=\"#after\""));
        assert!(html.contains("id=\"after\""));
        assert!(html.contains("<strong>emphasis</strong>"));
        let receipt = String::from_utf8(output.stdout).unwrap();
        assert!(receipt.contains("\"link_check\":"));
        assert!(receipt.contains(if extended {
            "\"pages\":2,"
        } else {
            "\"pages\":1,"
        }));
        let pdf = fs::read(root.join(name).join("book.pdf")).unwrap();
        assert_eq!(
            pdf,
            render_book_pdf(
                &book,
                &PdfOptions {
                    title: Some("Start".into()),
                    profile,
                    ..PdfOptions::default()
                },
            )
            .unwrap()
        );

        let path = format!("{name}.epub");
        let mut args = vec!["--to", "epub", "--out", &path];
        if let Some(profile) = profile {
            args.extend(["--profile", profile.as_str()]);
        }
        status(&fmd(&root, &args), 0);
        let epub = fs::read(root.join(path)).unwrap();
        assert_eq!(
            epub,
            franken_markdown::epub::render_book_epub(
                &book,
                &HtmlOptions {
                    title: Some("Start".into()),
                    profile,
                    ..HtmlOptions::default()
                },
            )
            .unwrap()
        );
        outputs.push((html, pdf, epub));
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_ne!(outputs[0].0, outputs[2].0);
    assert_ne!(outputs[0].1, outputs[2].1);
    assert_ne!(outputs[0].2, outputs[2].2);
}

#[test]
fn profiles_keep_include_navigation_and_source_diagnostics_in_both_binaries() {
    let root = workspace();
    fs::write(
        root.join("book/start.md"),
        "# Start\n\n{{#include parts/shared.md}}\n",
    )
    .unwrap();
    fs::write(root.join("book/end.md"), "# End\n").unwrap();
    fs::write(
        root.join("book/parts/shared.md"),
        "Term\n: Definition links to [End](../end.md#end).\n\n<!-- pagebreak -->\n\n```text\nunclosed source fence\n",
    )
    .unwrap();
    fs::write(
        root.join("book/book.toml"),
        "order=['start.md','end.md']\ninclude_only=['parts/shared.md']\n",
    )
    .unwrap();
    let baseline = fmd(&root, &["--check-links"]);
    status(&baseline, 0);
    assert!(String::from_utf8_lossy(&baseline.stderr).contains("unclosed_fence"));
    for binary in [
        env!("CARGO_BIN_EXE_fmd"),
        env!("CARGO_BIN_EXE_franken_markdown"),
    ] {
        for profile in ["commonmark-gfm", "gfm-plus"] {
            let checked = invoke(binary, &root, &["--check-links", "--profile", profile]);
            status(&checked, 0);
            assert_eq!(checked.stdout, baseline.stdout);
            assert_eq!(checked.stderr, baseline.stderr);
            let published = invoke(
                binary,
                &root,
                &[
                    "--to",
                    "html",
                    "--out-dir",
                    "site",
                    "--profile",
                    profile,
                    "--deny-broken-links",
                ],
            );
            status(&published, 0);
            let receipt = String::from_utf8(published.stdout).unwrap();
            assert!(receipt.contains("\"source_diagnostics\":"));
            assert!(receipt.contains("unclosed_fence"));
            let html = fs::read_to_string(root.join("site/start.html")).unwrap();
            assert_eq!(html.contains("<dl>"), profile == "gfm-plus");
            assert!(html.contains("href=\"end.html#end\""));
            assert!(!html.contains("{{#include"));
            assert!(!root.join("site/parts__shared.html").exists());
        }
    }
}

#[test]
fn shared_profile_aliases_are_honored() {
    let root = workspace();
    for profile in ["gfm", " COMMONMARK-GFM ", "gfm_plus", "PLUS"] {
        let output = fmd(
            &root,
            &["--to", "html", "--out-dir", "site", "--profile", profile],
        );
        status(&output, 0);
        let html = fs::read_to_string(root.join("site/start.html")).unwrap();
        assert_eq!(
            html.contains("<dl>"),
            Profile::parse(profile) == Some(Profile::GfmPlus)
        );
    }
}

#[test]
fn invalid_profiles_fail_before_configuration_sources_or_assets_are_opened() {
    let root = workspace();
    fs::write(root.join("config"), "invalid config text").unwrap();
    fs::write(root.join("keep.epub"), b"prior publication").unwrap();
    for profile in ["", "unknown", "gfm-plus/other"] {
        for target in ["html", "pdf", "both", "epub"] {
            let output = Command::new(env!("CARGO_BIN_EXE_fmd"))
                .args([
                    "book",
                    "missing-book",
                    "--json",
                    "--to",
                    target,
                    "--out-dir",
                    "keep.epub",
                    "--profile",
                    profile,
                    "--css",
                    "missing.css",
                    "--pdf-font",
                    "body-regular=missing.ttf",
                ])
                .env("FMD_CONFIG", root.join("config"))
                .current_dir(&root)
                .output()
                .unwrap();
            status(&output, 64);
            assert!(output.stdout.is_empty());
            let error = String::from_utf8(output.stderr).unwrap();
            assert!(error.contains("usage_error"), "{error}");
            assert!(
                error.contains("unknown markdown authoring profile"),
                "{error}"
            );
            assert!(!error.contains("config_error"), "{error}");
            assert_eq!(
                fs::read(root.join("keep.epub")).unwrap(),
                b"prior publication"
            );
        }
        let output = fmd(&root, &["--check-links", "--profile", profile]);
        status(&output, 64);
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn commonmark_source_parsing_controls_included_links_and_literal_include_examples() {
    let root = workspace();
    fs::write(
        root.join("book/start.md"),
        "# Start\n\n{{#include parts/shared.md}}\n",
    )
    .unwrap();
    fs::write(root.join("book/end.md"), "# End\n").unwrap();
    fs::write(
        root.join("book/book.toml"),
        "include_only=['parts/shared.md']\n",
    )
    .unwrap();
    let shared = "*Term\n: definition*\n\n[Term\n: definition](../end.md#end)\n\n`Term\n: definition\n{{#include missing.md}}\ntail`\n";
    fs::write(root.join("book/parts/shared.md"), shared).unwrap();
    let args = [
        "--to",
        "html",
        "--out-dir",
        "site",
        "--profile",
        "commonmark-gfm",
        "--deny-broken-links",
    ];
    let output = fmd(&root, &args);
    status(&output, 0);
    let page = fs::read_to_string(root.join("site/start.html")).unwrap();
    assert!(page.contains("<em>Term\n: definition</em>"));
    assert!(page.contains("href=\"end.html#end\""));
    assert!(page.contains("<code>Term : definition {{#include missing.md}} tail</code>"));
    fs::write(
        root.join("book/parts/shared.md"),
        shared.replace("#end)", "#missing)"),
    )
    .unwrap();
    let checked = fmd(&root, &["--check-links", "--profile", "commonmark-gfm"]);
    status(&checked, 65);
    assert!(String::from_utf8_lossy(&checked.stdout).contains("missing_anchor"));
    let denied = fmd(&root, &args);
    status(&denied, 65);
    assert!(
        String::from_utf8_lossy(&denied.stderr)
            .contains("--check-links --json --profile commonmark-gfm")
    );
    assert_eq!(
        fs::read_to_string(root.join("site/start.html")).unwrap(),
        page
    );
}

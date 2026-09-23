//! Native book options exercise real font loading, publication and diagnostics.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use franken_markdown::book::render_book_pdf;
use franken_markdown::{
    BookInput, FontAssetSlot, FontAssets, FontScale, HtmlOptions, PageMargins, PageSize,
    PdfOptions, Theme, build_book,
};

const STATIC_FONT: &[u8] = include_bytes!("../fmd-font/fonts/computer-modern/cmunrm.ttf");

fn workspace(source: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "fmd-book-options-{}-{stamp}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(root.join("book")).unwrap();
    fs::write(root.join("book/start.md"), source).unwrap();
    root
}

fn fmd(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fmd"))
        .arg("--no-config")
        .args(args)
        .env_remove("SOURCE_DATE_EPOCH")
        .current_dir(root)
        .output()
        .unwrap()
}

fn succeeded(output: &Output) {
    assert!(
        output.status.success(),
        "status {:?}: stderr={} stdout={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
}

fn failed(output: &Output, code: i32, detail: &str) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("\"ok\":false") && stderr.contains(detail),
        "{stderr}"
    );
}

#[test]
fn configured_book_pdf_matches_core_and_emits_requested_geometry() {
    let source = "# Manual\n\nBody text.\n\n## Detail\n\n### Deep detail\n\n```rust\nlet answer = 42;\nprintln!(\"{}\", answer);\n```\n";
    let root = workspace(source);
    fs::write(root.join("body.ttf"), STATIC_FONT).unwrap();
    let output = fmd(
        &root,
        &[
            "book",
            "book",
            "--to",
            "pdf",
            "--out",
            "manual.pdf",
            "--json",
            "--pdf-font",
            "body-regular=body.ttf",
            "--pdf-font-weight",
            "650",
            "--font-scale",
            "125%",
            "--page-size",
            "360x504",
            "--margin-top-pt",
            "24",
            "--margin-right-pt",
            "30",
            "--margin-bottom-pt",
            "36",
            "--margin-left-pt",
            "42",
            "--pdf-line-numbers",
            "--toc-depth",
            "2",
        ],
    );
    succeeded(&output);
    let bytes = fs::read(root.join("manual.pdf")).unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("/MediaBox [0 0 360 504]"));
    let book = build_book(&[BookInput {
        path: "start.md".into(),
        source: source.into(),
    }])
    .unwrap();
    let mut theme = Theme::default().with_font_scale(FontScale::parse("125%").unwrap());
    theme.page.size = PageSize {
        name: "custom",
        width_pt: 360.0,
        height_pt: 504.0,
    };
    theme.page.margins = PageMargins {
        top_pt: 24.0,
        right_pt: 30.0,
        bottom_pt: 36.0,
        left_pt: 42.0,
    };
    let assets = FontAssets::default()
        .with_slot(FontAssetSlot::BodyRegular, STATIC_FONT.to_vec())
        .unwrap()
        .with_slot_weight(FontAssetSlot::BodyRegular, 650)
        .unwrap();
    let options = PdfOptions {
        theme,
        title: Some("Manual".into()),
        font_assets: assets,
        base_font_size: Some(13.75),
        toc: true,
        toc_depth: Some(2),
        page_numbers: true,
        code_line_numbers: true,
        ..PdfOptions::default()
    };
    assert_eq!(bytes, render_book_pdf(&book, &options).unwrap());
    let receipt = String::from_utf8_lossy(&output.stdout);
    assert!(receipt.contains("font_weight_ignored_static"));
    // The PDF continues to carry all source headings even when only the first
    // two levels appear in its generated contents.
    assert!(String::from_utf8_lossy(&bytes).contains("/Title (Deep detail)"));
}

#[test]
fn host_variable_fonts_and_scale_reach_html_and_epub() {
    let source = "# Hello\n\nHello **variable**.\n";
    let root = workspace(source);
    let variable = franken_markdown::text::variable_triangle_fixture();
    fs::write(root.join("variable.ttf"), &variable).unwrap();
    let common = [
        "--pdf-font",
        "body-regular=variable.ttf",
        "--pdf-font-weight",
        "650",
        "--font-scale",
        "125%",
    ];
    let mut html_args = vec![
        "book",
        "book",
        "--to",
        "html",
        "--out-dir",
        "site",
        "--json",
    ];
    html_args.extend(common);
    let html_run = fmd(&root, &html_args);
    succeeded(&html_run);
    let html = fs::read_to_string(root.join("site/start.html")).unwrap();
    assert!(html.contains("font-weight: 650;"));
    assert!(html.contains("--fmd-base: 20px;"));
    assert!(!String::from_utf8_lossy(&html_run.stdout).contains("font_weight_ignored_static"));
    let mut epub_args = vec![
        "book",
        "book",
        "--to",
        "epub",
        "--out",
        "manual.epub",
        "--json",
    ];
    epub_args.extend(common);
    succeeded(&fmd(&root, &epub_args));
    let book = build_book(&[BookInput {
        path: "start.md".into(),
        source: source.into(),
    }])
    .unwrap();
    let assets = FontAssets::default()
        .with_slot(FontAssetSlot::BodyRegular, variable)
        .unwrap()
        .with_slot_weight(FontAssetSlot::BodyRegular, 650)
        .unwrap();
    let expected = franken_markdown::epub::render_book_epub(
        &book,
        &HtmlOptions {
            theme: Theme::default().with_font_scale(FontScale::parse("125%").unwrap()),
            title: Some("Hello".into()),
            font_assets: assets,
            ..HtmlOptions::default()
        },
    )
    .unwrap();
    assert_eq!(fs::read(root.join("manual.epub")).unwrap(), expected);
}

#[test]
fn changing_variable_weight_changes_the_embedded_pdf_font() {
    let root = workspace("# Hello\n\nHello variable.\n");
    fs::write(
        root.join("variable.ttf"),
        franken_markdown::text::variable_triangle_fixture(),
    )
    .unwrap();
    for (weight, destination) in [("400", "regular.pdf"), ("650", "semibold.pdf")] {
        let output = fmd(
            &root,
            &[
                "book",
                "book",
                "--to",
                "pdf",
                "--out",
                destination,
                "--json",
                "--pdf-font",
                "body-regular=variable.ttf",
                "--pdf-font-weight",
                weight,
            ],
        );
        succeeded(&output);
        assert!(!String::from_utf8_lossy(&output.stdout).contains("font_weight_ignored_static"));
    }
    assert_ne!(
        fs::read(root.join("regular.pdf")).unwrap(),
        fs::read(root.join("semibold.pdf")).unwrap()
    );
}

#[test]
fn invalid_options_are_rejected_before_missing_sources_are_read() {
    let root = workspace("# Valid\n");
    for (options, tag) in [
        (vec!["--pdf-font", "missing-delimiter"], "usage_error"),
        (vec!["--pdf-font", "unknown=font.ttf"], "usage_error"),
        (vec!["--pdf-font-weight", "1001"], "usage_error"),
        (vec!["--pdf-font-weight", "mono=NaN"], "usage_error"),
        (vec!["--font-scale", "NaN"], "usage_error"),
        (vec!["--toc-depth", "7"], "usage_error"),
        (vec!["--page-size", "0x600"], "invalid_page"),
        (vec!["--page-size", "NaNx600"], "invalid_page"),
        (vec!["--page-size", "14401x600"], "invalid_page"),
        (vec!["--margin-top-pt", "NaN"], "invalid_page"),
        (
            vec!["--margin-left-pt", "540.0000001", "--margin-right-pt", "0"],
            "invalid_page",
        ),
        (
            vec![
                "--page-size",
                "144.00731624x300",
                "--margin-left-pt",
                "36.00576428",
                "--margin-right-pt",
                "36.00155196",
            ],
            "invalid_page",
        ),
    ] {
        let mut args = vec![
            "book",
            "absent",
            "--to",
            "pdf",
            "--out",
            "untouched.pdf",
            "--json",
        ];
        args.extend(options);
        failed(&fmd(&root, &args), 64, tag);
        assert!(!root.join("untouched.pdf").exists());
    }
    for target in ["html", "epub"] {
        for options in [
            vec!["--page-size", "a4"],
            vec!["--margin-top-pt", "36"],
            vec!["--pdf-line-numbers"],
            vec!["--toc-depth", "2"],
        ] {
            let mut args = vec!["book", "absent", "--to", target, "--json"];
            args.extend(options);
            failed(&fmd(&root, &args), 64, "unsupported_target_option");
        }
    }
}

#[test]
fn font_files_are_bounded_validated_and_protected_from_publication_overwrite() {
    let root = workspace("# Fonts\n\nBody text.\n");
    fs::write(root.join("body.ttf"), STATIC_FONT).unwrap();
    fs::write(root.join("other.ttf"), STATIC_FONT).unwrap();
    let result = fmd(
        &root,
        &[
            "book",
            "book",
            "--to",
            "pdf",
            "--out",
            "body.ttf",
            "--json",
            "--pdf-font",
            "body-regular=body.ttf",
            "--pdf-font",
            "body-regular=other.ttf",
        ],
    );
    failed(&result, 73, "overwrite a book input");
    assert_eq!(fs::read(root.join("body.ttf")).unwrap(), STATIC_FONT);
    fs::write(root.join("bad.ttf"), b"not a font").unwrap();
    fs::File::create(root.join("huge.ttf"))
        .unwrap()
        .set_len(32 * 1024 * 1024 + 1)
        .unwrap();
    fs::create_dir(root.join("directory.ttf")).unwrap();
    for mapping in [
        "body-regular=bad.ttf",
        "body-regular=huge.ttf",
        "body-regular=directory.ttf",
    ] {
        let result = fmd(
            &root,
            &[
                "book",
                "book",
                "--to",
                "both",
                "--out-dir",
                "unwritten",
                "--json",
                "--pdf-font",
                mapping,
            ],
        );
        failed(&result, 66, "font_error");
        assert!(!root.join("unwritten").exists());
    }
}

#[test]
fn native_pdf_reports_actual_image_and_glyph_fallbacks_in_json_and_text() {
    let root = workspace("# Start\n\n[Next](nested/next.md)\n");
    fs::create_dir(root.join("book/nested")).unwrap();
    fs::write(
        root.join("book/nested/next.md"),
        "# Next\n\n\u{10ffff}\n\n![Good](good.svg)\n\n![Bad](bad.png)\n\n![Missing](absent.png)\n",
    )
    .unwrap();
    fs::write(
        root.join("book/nested/good.svg"),
        b"<svg width=\"10\" height=\"10\"><rect width=\"10\" height=\"10\"/></svg>",
    )
    .unwrap();
    fs::write(
        root.join("book/nested/bad.png"),
        b"\x89PNG\r\n\x1a\ntruncated",
    )
    .unwrap();
    let common = ["book", "book", "--to", "pdf", "--out", "manual.pdf"];
    let mut args = common.to_vec();
    args.push("--json");
    let output = fmd(&root, &args);
    succeeded(&output);
    assert!(
        output.stderr.is_empty(),
        "JSON receipt owns warnings, stderr stays empty on success"
    );
    let receipt = String::from_utf8_lossy(&output.stdout);
    for code in ["missing_glyphs:", "unsupported_image:", "unresolved_image:"] {
        assert!(receipt.contains(code), "missing {code}: {receipt}");
    }
    assert!(receipt.contains("nested/bad.png"));
    assert!(
        !receipt.contains("good.svg"),
        "chapter-relative asset resolved successfully: {receipt}"
    );
    let first = fs::read(root.join("manual.pdf")).unwrap();
    let output = fmd(&root, &common);
    succeeded(&output);
    assert!(output.stdout.is_empty());
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    assert!(diagnostics.contains("fmd: warning: missing_glyphs:"));
    assert!(diagnostics.contains("fmd: warning: unsupported_image:"));
    assert_eq!(fs::read(root.join("manual.pdf")).unwrap(), first);
}

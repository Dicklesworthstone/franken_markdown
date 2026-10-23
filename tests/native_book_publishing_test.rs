//! Real native book exports must retain the shared renderer's publishing controls.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use franken_markdown::book::{BookInput, BookRenderer};
use franken_markdown::layout::MicrotypeOptions;
use franken_markdown::{
    FontScale, PdfASettings, PdfImageAsset, PdfOptions, PdfRunningBand, PdfRunningContent,
    PdfRunningImage, PdfRunningImagePosition, Theme,
};

const LOGO: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="20"><rect width="80" height="20" fill="#1460a0"/></svg>"##;
const PARAGRAPH: &str = "Good book typography should survive publication through every supported interface. Measured words, sensible paragraph spacing and stable chapter destinations belong to the complete document, including tables, citations and the generated contents.";

fn source(path: &str, text: &str) -> BookInput {
    BookInput {
        path: path.into(),
        source: text.into(),
    }
}

fn workspace(inputs: &[BookInput]) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "fmd-book-publishing-{}-{stamp}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(root.join("book")).unwrap();
    for input in inputs {
        let path = root.join("book").join(&input.path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, &input.source).unwrap();
    }
    root
}

fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_fmd"));
    command
        .current_dir(root)
        .env("FMD_CONFIG", root.join("config"))
        .env_remove("SOURCE_DATE_EPOCH");
    command
}

fn publish(root: &Path, target: &str, extra: &[&str]) -> Output {
    command(root)
        .args([
            "book",
            "book",
            "--to",
            target,
            "--out-dir",
            "dist",
            "--json",
        ])
        .args(extra)
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
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(detail),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn options() -> PdfOptions {
    PdfOptions {
        title: Some("Manual".into()),
        toc: true,
        page_numbers: true,
        ..PdfOptions::default()
    }
}

fn fixture() -> Vec<BookInput> {
    vec![
        source(
            "one.md",
            &format!(
                "# Manual\n\n[Next](two.md#manual) and note[^n].\n\n{PARAGRAPH}\n\n[^n]: First chapter note.\n"
            ),
        ),
        source(
            "two.md",
            &format!(
                "# Manual\n\n[Back](one.md#manual).\n\n{PARAGRAPH}\n\n| Value | Description |\n| ---: | --- |\n| 42 | Useful data |\n"
            ),
        ),
    ]
}

fn expected(inputs: &[BookInput], options: &PdfOptions, archival: PdfASettings) -> Vec<u8> {
    BookRenderer::new(inputs)
        .unwrap()
        .render_pdf_with_options(options, archival)
        .unwrap()
}

fn contains(bytes: &[u8], text: &str) -> bool {
    bytes
        .windows(text.len())
        .any(|window| window == text.as_bytes())
}

// Inspect object dictionaries through the writer's xref, not guessed delimiters
// that might also occur inside compressed font, image or page streams.
fn dictionaries(bytes: &[u8]) -> Vec<&str> {
    let start = bytes
        .windows(9)
        .rposition(|part| part == b"startxref")
        .unwrap();
    let offset: usize = std::str::from_utf8(&bytes[start + 9..])
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let mut lines = std::str::from_utf8(&bytes[offset..]).unwrap().lines();
    assert_eq!(lines.next(), Some("xref"));
    let count: usize = lines
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    lines.next().unwrap();
    let mut positions = Vec::new();
    for id in 1..count {
        let row = lines.next().unwrap();
        let position: usize = row.split_whitespace().next().unwrap().parse().unwrap();
        assert!(bytes[position..].starts_with(format!("{id} 0 obj").as_bytes()));
        positions.push(position);
    }
    positions.sort_unstable();
    let mut result = Vec::new();
    for (index, &position) in positions.iter().enumerate() {
        let end = positions.get(index + 1).copied().unwrap_or(offset);
        let object = &bytes[position..end];
        let end = object
            .windows(2)
            .position(|part| part == b">>")
            .map(|end| end + 2);
        if let Some(end) = end
            && let Ok(dictionary) = std::str::from_utf8(&object[..end])
        {
            result.push(dictionary);
        }
    }
    result
}

fn pages(bytes: &[u8]) -> usize {
    dictionaries(bytes)
        .into_iter()
        .find(|dict| dict.contains("/Type /Pages "))
        .unwrap()
        .split("/Count ")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn absent_and_explicit_off_settings_preserve_default_book_bytes() {
    let inputs = fixture();
    let root = workspace(&inputs);
    let baseline = expected(&inputs, &options(), PdfASettings::OFF);
    for flags in [
        vec![],
        vec![
            "--pdf-a",
            "off",
            "--microtype",
            "off",
            "--pdf-optimal-pagination=false",
            "--typography-homogeneous=false",
            "--typography-antiriver=false",
            "--typography-pareto=false",
        ],
    ] {
        succeeded(&publish(&root, "pdf", &flags));
        assert_eq!(fs::read(root.join("dist/book.pdf")).unwrap(), baseline);
    }
}

#[test]
fn advanced_typography_flags_match_shared_book_rendering_and_keep_navigation() {
    let inputs = fixture();
    let root = workspace(&inputs);
    for (mode, microtype) in [
        ("protrusion", MicrotypeOptions::CONSERVATIVE),
        (
            "expansion",
            MicrotypeOptions {
                protrusion: false,
                max_expansion_per_mille: 15,
            },
        ),
    ] {
        let output = publish(
            &root,
            "pdf",
            &[
                "--microtype",
                mode,
                "--typography-homogeneous",
                "--typography-antiriver",
                "--typography-pareto",
                "--pdf-optimal-pagination",
                "--pdf-base-font-size",
                "13",
                "--pdf-heading-scale",
                "1.2",
                "--pdf-table-font-size",
                "10",
                "--toc=false",
                "--pdf-page-numbers=false",
            ],
        );
        succeeded(&output);
        let mut chosen = options();
        chosen.microtype = microtype;
        chosen.gradual_demerits = true;
        chosen.river_penalty = true;
        chosen.pareto_line_breaking = true;
        chosen.optimal_pagination = true;
        chosen.base_font_size = Some(13.0);
        chosen.heading_scale = Some(1.2);
        chosen.table_font_size = Some(10.0);
        chosen.toc = false;
        chosen.page_numbers = false;
        let bytes = fs::read(root.join("dist/book.pdf")).unwrap();
        assert_eq!(bytes, expected(&inputs, &chosen, PdfASettings::OFF));
        assert!(!contains(&bytes, "fmd-book-pdf-nav/"));
        assert!(
            dictionaries(&bytes)
                .iter()
                .filter(|dict| dict.contains("/Subtype /Link") && dict.contains("/Dest ["))
                .count()
                >= 2
        );
    }
}

#[test]
fn persistent_render_defaults_apply_and_explicit_flags_can_disable_them() {
    let inputs = fixture();
    let root = workspace(&inputs);
    fs::write(root.join("config"), "lang=de\nfont_scale=125%\ntoc=false\ntoc_depth=2\npdf_page_numbers=false\nmicrotype=protrusion\npdf_optimal_pagination=true\ntypography_homogeneous=true\ntypography_antiriver=true\ntypography_pareto=true\n").unwrap();
    let mut chosen = options();
    chosen.lang = Some("de".into());
    chosen.theme = Theme::default().with_font_scale(FontScale::parse("125%").unwrap());
    chosen.base_font_size = Some(13.75);
    chosen.toc = false;
    chosen.toc_depth = Some(2);
    chosen.page_numbers = false;
    chosen.microtype = MicrotypeOptions::CONSERVATIVE;
    chosen.optimal_pagination = true;
    chosen.gradual_demerits = true;
    chosen.river_penalty = true;
    chosen.pareto_line_breaking = true;
    succeeded(&publish(&root, "pdf", &[]));
    assert_eq!(
        fs::read(root.join("dist/book.pdf")).unwrap(),
        expected(&inputs, &chosen, PdfASettings::OFF)
    );
    succeeded(&publish(
        &root,
        "pdf",
        &[
            "--lang",
            "fr",
            "--font-scale",
            "100%",
            "--toc",
            "--toc-depth",
            "1",
            "--pdf-page-numbers",
            "--microtype",
            "off",
            "--pdf-optimal-pagination=false",
            "--typography-homogeneous=false",
            "--typography-antiriver=false",
            "--typography-pareto=false",
        ],
    ));
    let explicit = PdfOptions {
        lang: Some("fr".into()),
        base_font_size: Some(11.0),
        toc_depth: Some(1),
        ..options()
    };
    assert_eq!(
        fs::read(root.join("dist/book.pdf")).unwrap(),
        expected(&inputs, &explicit, PdfASettings::OFF)
    );
}

#[test]
fn html_configuration_and_chapter_frontmatter_retain_their_precedence() {
    let inputs = vec![
        source(
            "one.md",
            "---\nlang=fr\ntoc=false\n---\n# One\n\n## Detail\n",
        ),
        source("two.md", "# Two\n\n## Detail\n"),
    ];
    let root = workspace(&inputs);
    fs::write(root.join("config"), "lang=de\nfont_scale=125%\nhtml_font_format=ttf\ntoc=true\ntoc_depth=1\npdf_optimal_pagination=true\n").unwrap();
    succeeded(&publish(&root, "html", &[]));
    let first = fs::read_to_string(root.join("dist/one.html")).unwrap();
    let second = fs::read_to_string(root.join("dist/two.html")).unwrap();
    assert!(first.contains("<html lang=\"fr\""));
    // Existing book semantics use the first chapter's language as book default.
    assert!(second.contains("<html lang=\"fr\""));
    for html in [&first, &second] {
        assert!(html.contains("--fmd-base: 20px"));
        assert!(html.contains("data:font/ttf;base64,"));
    }
    assert!(!first.contains("class=\"toc\""));
    assert!(second.contains("class=\"toc\""));
    succeeded(&publish(
        &root,
        "html",
        &[
            "--lang",
            "es",
            "--font-scale",
            "100%",
            "--html-font-format",
            "woff1",
            "--toc=false",
        ],
    ));
    let first = fs::read_to_string(root.join("dist/one.html")).unwrap();
    let second = fs::read_to_string(root.join("dist/two.html")).unwrap();
    assert!(first.contains("<html lang=\"fr\""));
    assert!(second.contains("<html lang=\"es\""));
    assert!(second.contains("--fmd-base: 16px"));
    assert!(second.contains("data:font/woff;base64,"));
    assert!(!second.contains("class=\"toc\""));
}

#[test]
fn contents_depth_works_for_html_and_epub_without_removing_source_headings() {
    let inputs = vec![source("one.md", "# Manual\n\n## Detail\n\n### Appendix\n")];
    let root = workspace(&inputs);
    // The explicit depth must override a different persistent default.
    fs::write(root.join("config"), "toc_depth=3\n").unwrap();
    let mut previous_epub = None;
    for (depth, flag) in [(1, "1"), (2, "2")] {
        succeeded(&publish(&root, "html", &["--toc", "--toc-depth", flag]));
        let html = fs::read_to_string(root.join("dist/one.html")).unwrap();
        let contents = html
            .split_once("class=\"toc\"")
            .unwrap()
            .1
            .split_once("</nav>")
            .unwrap()
            .0;
        assert!(contents.contains("href=\"#manual\""));
        assert_eq!(contents.contains("href=\"#detail\""), depth >= 2);
        assert!(!contents.contains("href=\"#appendix\""));
        assert!(html.contains("id=\"detail\""));
        assert!(html.contains("id=\"appendix\""));

        succeeded(&publish(&root, "epub", &["--toc", "--toc-depth", flag]));
        let epub = fs::read(root.join("dist/book.epub")).unwrap();
        let book = franken_markdown::build_book(&inputs).unwrap();
        let chosen = franken_markdown::HtmlOptions {
            title: Some("Manual".into()),
            toc: true,
            toc_depth: Some(depth),
            ..franken_markdown::HtmlOptions::default()
        };
        assert_eq!(
            epub,
            franken_markdown::epub::render_book_epub(&book, &chosen).unwrap()
        );
        if let Some(previous) = previous_epub {
            assert_ne!(epub, previous, "chapter contents must reflect the depth");
        }
        previous_epub = Some(epub);
    }
}

#[test]
fn running_text_images_rules_and_skip_first_match_core_and_repeat_deterministically() {
    let inputs = fixture();
    let root = workspace(&inputs);
    fs::write(root.join("logo.svg"), LOGO).unwrap();
    let flags = [
        "--author",
        "Writer",
        "--pdf-image",
        "brand=logo.svg",
        "--pdf-header-left",
        "{title}",
        "--pdf-header-right",
        "{author}",
        "--pdf-footer-center",
        "{page} / {pages}",
        "--pdf-footer-right",
        "{date}",
        "--pdf-header-image",
        "brand",
        "--pdf-header-image-position",
        "right",
        "--pdf-header-image-height-pt",
        "18",
        "--pdf-footer-image",
        "brand",
        "--pdf-footer-image-height-pt",
        "12",
        "--pdf-header-rule",
        "--pdf-footer-rule",
        "--pdf-running-skip-first",
    ];
    let mut chosen = options();
    chosen.author = Some("Writer".into());
    chosen.image_assets = vec![PdfImageAsset::new("brand", LOGO.to_vec())];
    chosen.running = PdfRunningContent {
        header: PdfRunningBand {
            left: Some("{title}".into()),
            right: Some("{author}".into()),
            rule: true,
            image: Some(PdfRunningImage {
                dest: "brand".into(),
                position: PdfRunningImagePosition::Right,
                height_pt: Some(18),
            }),
            ..PdfRunningBand::default()
        },
        footer: PdfRunningBand {
            center: Some("{page} / {pages}".into()),
            right: Some("{date}".into()),
            rule: true,
            image: Some(PdfRunningImage {
                dest: "brand".into(),
                position: PdfRunningImagePosition::Left,
                height_pt: Some(12),
            }),
            ..PdfRunningBand::default()
        },
        skip_first_page: true,
    };
    let expected = expected(&inputs, &chosen, PdfASettings::OFF);
    for _ in 0..2 {
        let output = publish(&root, "both", &flags);
        succeeded(&output);
        assert!(String::from_utf8_lossy(&output.stdout).contains("\"images\":1"));
        assert_eq!(fs::read(root.join("dist/book.pdf")).unwrap(), expected);
        assert!(root.join("dist/one.html").is_file());
    }
}

#[test]
fn pdfa_publication_uses_actual_metadata_page_count_and_chapter_destinations() {
    let inputs = fixture();
    let root = workspace(&inputs);
    for strict in [false, true] {
        let mut cmd = command(&root);
        cmd.env("SOURCE_DATE_EPOCH", "1700000000").args([
            "book",
            "book",
            "--to",
            "pdf",
            "--out",
            "archive.pdf",
            "--json",
            "--author",
            "Writer",
            "--pdf-a",
            "2b",
            "--pdf-optimal-pagination",
        ]);
        if strict {
            cmd.arg("--pdf-a-strict");
        }
        let output = cmd.output().unwrap();
        succeeded(&output);
        let bytes = fs::read(root.join("archive.pdf")).unwrap();
        let chosen = PdfOptions {
            author: Some("Writer".into()),
            metadata_epoch_seconds: Some(1_700_000_000),
            optimal_pagination: true,
            ..options()
        };
        let settings = if strict {
            PdfASettings::a2b_strict()
        } else {
            PdfASettings::a2b()
        };
        assert_eq!(bytes, expected(&inputs, &chosen, settings));
        assert!(contains(&bytes, "/OutputIntents ["));
        assert!(contains(&bytes, "/Type /Metadata"));
        assert!(!contains(&bytes, "fmd-book-pdf-nav/"));
        let receipt = String::from_utf8(output.stdout).unwrap();
        assert!(receipt.contains(&format!(
            "\"pdf_a\":{{\"profile\":\"2b\",\"strict\":{strict}}}"
        )));
        assert!(receipt.contains(&format!("\"pages\":{},", pages(&bytes))));
    }
}

#[test]
fn strict_archival_rejection_preserves_every_existing_combined_output() {
    let inputs = vec![
        source(
            "one.md",
            "# One\n\n[Next](two.md#two).\n\n[^unused]: [Forbidden](file:///private/document).\n",
        ),
        source("two.md", "# Two\n"),
    ];
    let root = workspace(&inputs);
    fs::create_dir(root.join("dist")).unwrap();
    for path in ["one.html", "two.html", "index.html", "book.pdf"] {
        fs::write(root.join("dist").join(path), format!("keep {path}")).unwrap();
    }
    let output = publish(&root, "both", &["--pdf-a", "2b", "--pdf-a-strict"]);
    failed(&output, 70, "pdf_a_");
    for path in ["one.html", "two.html", "index.html", "book.pdf"] {
        assert_eq!(
            fs::read_to_string(root.join("dist").join(path)).unwrap(),
            format!("keep {path}")
        );
    }
    assert!(!root.join("dist/search-index.json").exists());
}

#[test]
fn invalid_publishing_values_fail_before_source_or_resource_io() {
    let root = workspace(&[]);
    for flags in [
        vec!["--fit-to-pages", "0"],
        vec!["--pdf-base-font-size", "NaN"],
        vec!["--pdf-base-font-size", "25"],
        vec!["--pdf-heading-scale", "1"],
        vec!["--pdf-table-font-size", "4"],
        vec!["--pdf-a", "wrong"],
        vec!["--pdf-a", "off", "--pdf-a-strict"],
        vec!["--pdf-a-strict"],
        vec!["--pdf-header-image", " "],
        vec![
            "--pdf-header-image",
            "brand",
            "--pdf-header-image-height-pt",
            "0",
        ],
        vec!["--pdf-header-image-position", "right"],
        vec!["--pdf-image", "broken"],
        vec!["--pdf-image", "=missing.svg"],
        vec!["--pdf-image", "brand="],
    ] {
        let output = command(&root)
            .args(["book", "missing", "--to", "pdf", "--json"])
            .args(flags)
            .output()
            .unwrap();
        failed(&output, 64, "usage_error");
    }
}

#[test]
fn explicit_pdf_flags_are_rejected_for_other_targets_and_source_only_checks() {
    let root = workspace(&[]);
    for flags in [
        vec!["--microtype", "off"],
        vec!["--pdf-optimal-pagination=false"],
        vec!["--typography-homogeneous"],
        vec!["--typography-antiriver=false"],
        vec!["--typography-pareto"],
        vec!["--fit-to-pages", "2"],
        vec!["--pdf-base-font-size", "12"],
        vec!["--pdf-heading-scale", "1.2"],
        vec!["--pdf-table-font-size", "9"],
        vec!["--pdf-page-numbers=false"],
        vec!["--pdf-header-left", "Title"],
        vec!["--pdf-footer-right", "{page}"],
        vec!["--pdf-header-rule"],
        vec!["--pdf-footer-rule"],
        vec!["--pdf-running-skip-first"],
        vec!["--pdf-header-image", "brand"],
        vec!["--pdf-footer-image", "brand"],
        vec!["--pdf-a", "off"],
    ] {
        for target in ["html", "epub"] {
            let output = command(&root)
                .args(["book", "missing", "--to", target, "--json"])
                .args(&flags)
                .output()
                .unwrap();
            failed(&output, 64, "unsupported_target_option");
        }
        let output = command(&root)
            .args(["book", "missing", "--check-links", "--json"])
            .args(&flags)
            .output()
            .unwrap();
        failed(&output, 64, "usage_error");
    }
    for flags in [
        vec!["--toc"],
        vec!["--html-font-format", "woff2"],
        vec!["--pdf-image", "brand=missing.svg"],
    ] {
        let output = command(&root)
            .args(["book", "missing", "--check-links", "--json"])
            .args(flags)
            .output()
            .unwrap();
        failed(&output, 64, "usage_error");
    }
}

#[test]
fn explicit_images_are_bounded_and_failed_logos_never_publish_partial_output() {
    let inputs = fixture();
    let root = workspace(&inputs);
    fs::write(root.join("logo.svg"), LOGO).unwrap();
    fs::write(root.join("bad.svg"), b"not an image").unwrap();
    for (flags, code, detail) in [
        (vec!["--pdf-image", "brand=missing.svg"], 66, "image_error"),
        (
            vec![
                "--pdf-image",
                "brand=logo.svg",
                "--max-pdf-image-bytes",
                "1",
            ],
            66,
            "image_error",
        ),
        (
            vec!["--pdf-header-image", "unmapped"],
            70,
            "pdf_render_error",
        ),
        (
            vec![
                "--pdf-image",
                "brand=bad.svg",
                "--pdf-header-image",
                "brand",
            ],
            70,
            "pdf_render_error",
        ),
    ] {
        let output = publish(&root, "both", &flags);
        failed(&output, code, detail);
        assert!(!root.join("dist").exists());
    }
}

#[test]
fn every_explicit_image_and_loaded_config_remain_protected_from_output_overwrite() {
    let inputs = fixture();
    let root = workspace(&inputs);
    fs::write(root.join("old.svg"), LOGO).unwrap();
    fs::write(root.join("new.svg"), LOGO).unwrap();
    fs::write(root.join("config"), "pdf_optimal_pagination=true\n").unwrap();
    for destination in ["old.svg", "new.svg", "config"] {
        let output = command(&root)
            .args([
                "book",
                "book",
                "--to",
                "pdf",
                "--out",
                destination,
                "--json",
                "--pdf-image",
                "brand=old.svg",
                "--pdf-image",
                "brand=new.svg",
                "--pdf-header-image",
                "brand",
            ])
            .output()
            .unwrap();
        failed(&output, 73, "output would overwrite a book input");
        assert_eq!(fs::read(root.join("old.svg")).unwrap(), LOGO);
        assert_eq!(fs::read(root.join("new.svg")).unwrap(), LOGO);
        assert_eq!(
            fs::read_to_string(root.join("config")).unwrap(),
            "pdf_optimal_pagination=true\n"
        );
    }
}

#[test]
fn page_budget_applies_to_the_assembled_book_and_reports_real_page_count() {
    let mut text = String::new();
    // Ten sections are two ordinary pages and fit one at the solver's
    // readable size/margin limits; fifteen still need two at that floor.
    for index in 1..=10 {
        text.push_str(&format!("## Section {index}\n\n{PARAGRAPH}\n\n"));
    }
    let inputs = vec![source("one.md", &text)];
    let root = workspace(&inputs);
    succeeded(&publish(&root, "pdf", &["--toc=false"]));
    let original = fs::read(root.join("dist/book.pdf")).unwrap();
    assert_eq!(pages(&original), 2);
    let output = publish(&root, "pdf", &["--toc=false", "--target-pages", "1"]);
    succeeded(&output);
    let fitted = fs::read(root.join("dist/book.pdf")).unwrap();
    assert_eq!(pages(&fitted), 1);
    assert!(String::from_utf8_lossy(&output.stdout).contains("\"pages\":1,"));
    let chosen = PdfOptions {
        title: Some("Section 1".into()),
        toc: false,
        fit_to_pages: Some(1),
        ..options()
    };
    assert_eq!(fitted, expected(&inputs, &chosen, PdfASettings::OFF));
}

#[test]
fn impossible_page_budget_preserves_existing_combined_publications() {
    // A chapter boundary is a real page break. Two chapters cannot fit one page.
    let inputs = vec![source("one.md", "# One\n"), source("two.md", "# Two\n")];
    let root = workspace(&inputs);
    fs::create_dir(root.join("dist")).unwrap();
    fs::write(root.join("dist/one.html"), "old HTML").unwrap();
    fs::write(root.join("dist/book.pdf"), "old PDF").unwrap();
    let output = publish(&root, "both", &["--toc=false", "--fit-to-pages", "1"]);
    failed(&output, 70, "page_budget_unmet");
    assert_eq!(
        fs::read_to_string(root.join("dist/one.html")).unwrap(),
        "old HTML"
    );
    assert_eq!(
        fs::read_to_string(root.join("dist/book.pdf")).unwrap(),
        "old PDF"
    );
    assert!(!root.join("dist/two.html").exists());
    assert!(!root.join("dist/index.html").exists());
}

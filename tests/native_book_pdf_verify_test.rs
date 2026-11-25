//! Native assembled-PDF checks share publication admission without writing files.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use franken_markdown::book::{BookInput, BookRenderer};
use franken_markdown::{
    BaselineGrid, FontAssetSlot, PageMargins, PageSize, PdfASettings, PdfImageAsset, PdfOptions,
    PdfRunningImage, PdfRunningImagePosition, Theme,
};

const SVG: &[u8] =
    br#"<svg width="20" height="10"><rect width="20" height="10" fill="blue"/></svg>"#;
const FIRST: &str = "# Start\n\n[Next chapter](next.md#next). Note[^n].\n\n![Chart](chart.svg)\n\n| Item | Value |\n|---|---|\n| First | Measured content |\n\n```rust\nlet measured = 1;\n```\n\n[^n]: First chapter note.\n";
const NEXT: &str = "---\nlang: de\n---\n# Next\n\n[Previous chapter](start.md#start). Another note[^n].\n\n[^n]: Second chapter note.\n";

struct Workspace(PathBuf);

impl Workspace {
    fn new(markdown: &str) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "fmd-book-pdf-verify-{}-{stamp}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir_all(path.join("book")).unwrap();
        fs::write(path.join("book/start.md"), markdown).unwrap();
        Self(path)
    }

    fn write(&self, path: &str, bytes: impl AsRef<[u8]>) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn command(&self, binary: &str) -> Command {
        let mut cmd = Command::new(binary);
        cmd.args(["--no-config", "book", "book"])
            .env("SOURCE_DATE_EPOCH", "0")
            .current_dir(&self.0);
        cmd
    }

    fn check(&self, flags: &[&str]) -> Output {
        self.command(env!("CARGO_BIN_EXE_fmd"))
            .args(["--check-pdf", "--json"])
            .args(flags)
            .output()
            .unwrap()
    }

    fn snapshot(&self) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
        fn walk(root: &Path, directory: &Path, entries: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
            for entry in fs::read_dir(directory).unwrap() {
                let path = entry.unwrap().path();
                let directory = path.is_dir();
                entries.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    (!directory).then(|| fs::read(&path).unwrap()),
                );
                if directory {
                    walk(root, &path, entries);
                }
            }
        }
        let mut entries = BTreeMap::new();
        walk(&self.0, &self.0, &mut entries);
        entries
    }
}

fn input(path: &str, source: &str) -> BookInput {
    BookInput {
        path: path.into(),
        source: source.into(),
    }
}

fn status(output: &Output, expected: i32) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn failure(output: &Output, code: i32, tag: &str) {
    status(output, code);
    assert!(output.stdout.is_empty(), "no partial report on failure");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("\"ok\":false"), "{stderr}");
    assert!(stderr.contains(tag), "{stderr}");
}

fn book_options(title: &str) -> PdfOptions {
    PdfOptions {
        title: Some(title.into()),
        toc: true,
        page_numbers: true,
        metadata_epoch_seconds: Some(0),
        ..PdfOptions::default()
    }
}

fn configured_options() -> PdfOptions {
    let mut theme = Theme::serif();
    theme.page.size = PageSize {
        name: "custom",
        width_pt: 360.0,
        height_pt: 504.0,
    };
    theme.page.margins = PageMargins {
        top_pt: 36.0,
        right_pt: 30.0,
        bottom_pt: 36.0,
        left_pt: 24.0,
    };
    theme.page.baseline_grid = Some(BaselineGrid::from_points(13.1254).unwrap());
    theme.spacing.line_height = 1.5;
    theme.spacing.table_cell_padding_x_em = 0.6;
    theme.spacing.table_cell_padding_y_em = 0.4;
    let mut options = PdfOptions {
        theme,
        author: Some("Ada".into()),
        lang: Some("en".into()),
        base_font_size: Some(12.0),
        heading_scale: Some(1.3),
        table_font_size: Some(9.0),
        use_theme_spacing: true,
        footnote_backlinks: true,
        split_footnotes: true,
        code_line_numbers: true,
        toc_depth: Some(2),
        image_assets: ["chart.svg", "logo"]
            .map(|destination| PdfImageAsset {
                destination: destination.into(),
                bytes: SVG.to_vec(),
            })
            .to_vec(),
        ..book_options("A measured book")
    };
    options
        .font_assets
        .set_slot(
            FontAssetSlot::BodyRegular,
            fmd_font::bundled::PLEX_REGULAR.to_vec(),
        )
        .unwrap();
    options
        .font_assets
        .set_slot_weight(FontAssetSlot::BodyRegular, 650)
        .unwrap();
    options.running.header.left = Some("{chapter}".into());
    options.running.header.rule = true;
    options.running.header.image = Some(PdfRunningImage {
        dest: "logo".into(),
        position: PdfRunningImagePosition::Right,
        height_pt: Some(12),
    });
    options.running.footer.center = Some("{page} / {pages}".into());
    options
}

const CONFIGURED_FLAGS: &[&str] = &[
    "--font",
    "serif",
    "--pdf-font",
    "body-regular=body.ttf",
    "--pdf-font-weight",
    "650",
    "--page-size",
    "360x504",
    "--margin-top-pt",
    "36",
    "--margin-right-pt",
    "30",
    "--margin-bottom-pt",
    "36",
    "--margin-left-pt",
    "24",
    "--pdf-baseline-grid-pt",
    "13.1254",
    "--pdf-base-font-size",
    "12",
    "--pdf-heading-scale",
    "1.3",
    "--pdf-table-font-size",
    "9",
    "--pdf-line-height",
    "1.5",
    "--pdf-table-cell-padding-x-em",
    "0.6",
    "--pdf-table-cell-padding-y-em",
    "0.4",
    "--pdf-footnote-backlinks",
    "--pdf-split-footnotes",
    "--pdf-line-numbers",
    "--toc-depth",
    "2",
    "--pdf-header-left",
    "{chapter}",
    "--pdf-header-rule",
    "--pdf-footer-center",
    "{page} / {pages}",
    "--pdf-header-image",
    "logo",
    "--pdf-header-image-position",
    "right",
    "--pdf-header-image-height-pt",
    "12",
    "--pdf-image",
    "logo=logo.svg",
];

#[test]
fn both_binaries_verify_exact_publication_options_without_touching_the_edition() {
    let root = Workspace::new(FIRST);
    root.write("book/next.md", NEXT);
    root.write(
        "book/book.toml",
        "title='A measured book'\nauthor='Ada'\nlang='en'\norder=['start.md','next.md']\n",
    );
    root.write("book/chart.svg", SVG);
    root.write("logo.svg", SVG);
    root.write("body.ttf", fmd_font::bundled::PLEX_REGULAR);
    root.write("book-site/index.html", b"previous site");
    root.write("book.pdf", b"previous PDF");
    root.write("book.epub", b"previous EPUB");
    let snapshot = root.snapshot();
    let renderer = BookRenderer::new(&[input("start.md", FIRST), input("next.md", NEXT)]).unwrap();
    let options = configured_options();
    let expected = renderer
        .verify_pdf_with_options(&options, PdfASettings::OFF)
        .unwrap();
    assert!(
        expected
            .layout
            .findings
            .iter()
            .any(|finding| finding.code == "font_weight_ignored_static")
    );
    assert!(expected.layout.page_count >= 2);
    for binary in [
        env!("CARGO_BIN_EXE_fmd"),
        env!("CARGO_BIN_EXE_franken_markdown"),
    ] {
        let output = root
            .command(binary)
            .args(["--check-pdf", "--json"])
            .args(CONFIGURED_FLAGS)
            .output()
            .unwrap();
        status(&output, 65);
        assert_eq!(
            output.stdout,
            (expected.to_json().unwrap() + "\n").as_bytes()
        );
        assert_eq!(
            root.snapshot(),
            snapshot,
            "checks never create or replace any files or directories"
        );
    }
    let published = root
        .command(env!("CARGO_BIN_EXE_fmd"))
        .args(["--to", "pdf", "--out", "actual.pdf", "--json"])
        .args(CONFIGURED_FLAGS)
        .output()
        .unwrap();
    status(&published, 0);
    assert_eq!(
        fs::read(root.0.join("actual.pdf")).unwrap(),
        renderer
            .render_pdf_with_options(&options, PdfASettings::OFF)
            .unwrap()
    );
}

#[test]
fn included_findings_retain_the_original_file_and_exact_url_span() {
    let markdown = "# Start\n\n{{#include parts/body.md}}\n";
    let included = "A café. [Broken](#absent)\n";
    let root = Workspace::new(markdown);
    root.write("book/parts/body.md", included);
    root.write("book/book.toml", "include_only=['parts/body.md']\n");
    let snapshot = root.snapshot();
    let output = root.check(&[]);
    status(&output, 65);
    let renderer = BookRenderer::from_sources(
        &[input("start.md", markdown)],
        &[input("parts/body.md", included)],
    )
    .unwrap();
    let expected = renderer
        .verify_pdf_with_options(&book_options("Start"), PdfASettings::OFF)
        .unwrap();
    assert_eq!(
        output.stdout,
        (expected.to_json().unwrap() + "\n").as_bytes()
    );
    let start = included.find("#absent").unwrap();
    let location = format!(
        "\"path\":\"parts/body.md\",\"start\":{start},\"end\":{},\"kind\":\"exact\"",
        start + "#absent".len()
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(&location));
    assert_eq!(root.snapshot(), snapshot);
    let human = root
        .command(env!("CARGO_BIN_EXE_fmd"))
        .arg("--check-pdf")
        .output()
        .unwrap();
    status(&human, 65);
    assert!(human.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&human.stderr);
    assert!(
        stderr.contains("assembled PDF verification")
            && stderr.contains("missing_anchor")
            && stderr.contains("start.md"),
        "{stderr}"
    );
}

#[test]
fn clean_check_reports_success_and_keeps_source_only_navigation_unchanged() {
    let source = "# Start\n\n[Heading](#start)\n";
    let root = Workspace::new(source);
    let snapshot = root.snapshot();
    let pdf = root.check(&[]);
    status(&pdf, 0);
    assert!(pdf.stderr.is_empty());
    let renderer = BookRenderer::new(&[input("start.md", source)]).unwrap();
    assert_eq!(
        pdf.stdout,
        (renderer
            .verify_pdf_with_options(&book_options("Start"), PdfASettings::OFF)
            .unwrap()
            .to_json()
            .unwrap()
            + "\n")
            .as_bytes()
    );
    let source_check = root
        .command(env!("CARGO_BIN_EXE_fmd"))
        .args(["--check-links", "--json"])
        .output()
        .unwrap();
    status(&source_check, 0);
    assert_eq!(
        source_check.stdout,
        (renderer.validate_links_with_sources().unwrap().to_json() + "\n").as_bytes()
    );
    assert_eq!(root.snapshot(), snapshot);
}

#[test]
fn native_report_budget_is_exact_utf8_and_refuses_before_any_stdout_data() {
    let root = Workspace::new("# Café 中文\n\n[Heading](#caf%C3%A9-%E4%B8%AD%E6%96%87)\n");
    let snapshot = root.snapshot();
    let result = root.check(&[]);
    assert!(matches!(result.status.code(), Some(0 | 65)));
    let bytes = result.stdout.len() - 1;
    assert!(bytes > String::from_utf8_lossy(&result.stdout).chars().count() - 1);
    for (limit, pass) in [(1, false), (bytes - 1, false), (bytes, true)] {
        let limit = limit.to_string();
        let bounded = root.check(&["--max-report-bytes", &limit]);
        if pass {
            assert_eq!(bounded.status.code(), result.status.code());
            assert_eq!(bounded.stdout, result.stdout);
        } else {
            failure(&bounded, 70, "report_too_large");
        }
    }
    assert_eq!(root.snapshot(), snapshot);
}

#[test]
fn conflicting_output_and_non_pdf_flags_fail_before_source_access() {
    let root = Workspace::new("# Preserved");
    let snapshot = root.snapshot();
    for flags in [
        vec!["--out", "lost.pdf"],
        vec!["--out-dir", "lost"],
        vec!["--to", "pdf"],
        vec!["--check-links"],
        vec!["--deny-broken-links"],
        vec!["--css", "absent.css"],
        vec!["--html-font-format", "woff2"],
        vec!["--font-embedding", "full"],
        vec!["--max-report-bytes", "0"],
        vec!["--max-report-bytes", "8388609"],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_fmd"))
            .args(["--no-config", "book", "absent", "--check-pdf", "--json"])
            .args(flags)
            .current_dir(&root.0)
            .output()
            .unwrap();
        failure(&result, 64, "usage_error");
    }
    let unused = root
        .command(env!("CARGO_BIN_EXE_fmd"))
        .args(["--max-report-bytes", "10", "--json"])
        .output()
        .unwrap();
    failure(&unused, 64, "usage_error");
    assert_eq!(root.snapshot(), snapshot);
}

#[test]
fn archival_settings_use_the_selected_writer_and_propagate_strict_rejection() {
    let source = "# Start\n\nBody[^n].\n\n[^n]: Complete note.\n";
    let root = Workspace::new(source);
    let snapshot = root.snapshot();
    let renderer = BookRenderer::new(&[input("start.md", source)]).unwrap();
    for (flags, settings) in [
        (vec!["--pdf-a", "off"], PdfASettings::OFF),
        (vec!["--pdf-a", "2b"], PdfASettings::a2b()),
        (
            vec!["--pdf-a", "2b", "--pdf-a-strict"],
            PdfASettings::a2b_strict(),
        ),
    ] {
        let output = root.check(&flags);
        status(&output, 0);
        let expected = renderer
            .verify_pdf_with_options(&book_options("Start"), settings)
            .unwrap();
        assert_eq!(
            output.stdout,
            (expected.to_json().unwrap() + "\n").as_bytes()
        );
        assert_eq!(root.snapshot(), snapshot);
    }
    failure(&root.check(&["--pdf-a-strict"]), 64, "usage_error");
    root.write(
        "book/start.md",
        "# Start\n\n[^unused]: [Forbidden](file:///private/document).\n",
    );
    let snapshot = root.snapshot();
    let refused = root.check(&["--pdf-a", "2b", "--pdf-a-strict"]);
    failure(&refused, 70, "pdf_a_");
    assert_eq!(root.snapshot(), snapshot);
}

#[test]
fn strict_pagination_and_unmet_page_budgets_never_become_successful_checks() {
    let root = Workspace::new("A  \nB  \nC");
    let snapshot = root.snapshot();
    let result = root.check(&[
        "--toc=false",
        "--pdf-page-numbers=false",
        "--pdf-base-font-size",
        "24",
        "--margin-top-pt",
        "360",
        "--margin-bottom-pt",
        "360",
        "--pdf-strict-pagination",
    ]);
    failure(&result, 70, "pagination_infeasible");
    let result = root.check(&[
        "--toc=false",
        "--pdf-page-numbers=false",
        "--page-size",
        "612x10200",
        "--margin-top-pt",
        "36",
        "--margin-right-pt",
        "36",
        "--margin-bottom-pt",
        "36",
        "--margin-left-pt",
        "36",
        "--pdf-strict-pagination",
    ]);
    failure(&result, 70, "pagination_budget_exceeded");
    assert_eq!(root.snapshot(), snapshot);
    root.write("book/second.md", "# Second\n\nAnother chapter.");
    let snapshot = root.snapshot();
    let result = root.check(&[
        "--toc=false",
        "--pdf-page-numbers=false",
        "--fit-to-pages",
        "1",
    ]);
    failure(&result, 70, "page_budget_unmet");
    assert_eq!(root.snapshot(), snapshot);
}

#[test]
fn configured_profile_precedes_includes_and_no_config_remains_effective() {
    let source = "# Chapter\n\n`Term\n: definition\n{{#include absent.md}}\ntail`\n";
    let root = Workspace::new(source);
    root.write("config", "profile=commonmark-gfm\n");
    let snapshot = root.snapshot();
    let configured = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args(["book", "book", "--check-pdf", "--json"])
        .env("FMD_CONFIG", root.0.join("config"))
        .env("SOURCE_DATE_EPOCH", "0")
        .current_dir(&root.0)
        .output()
        .unwrap();
    status(&configured, 0);
    let explicit = root.check(&["--profile", "commonmark-gfm"]);
    status(&explicit, 0);
    assert_eq!(configured.stdout, explicit.stdout);
    failure(&root.check(&[]), 66, "book_error");
    let override_profile = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args([
            "book",
            "book",
            "--check-pdf",
            "--json",
            "--profile",
            "gfm-plus",
        ])
        .env("FMD_CONFIG", root.0.join("config"))
        .current_dir(&root.0)
        .output()
        .unwrap();
    failure(&override_profile, 66, "book_error");
    assert_eq!(root.snapshot(), snapshot);
}

#[test]
fn parser_fallbacks_remain_observable_and_make_the_check_nonclean() {
    let source = "# Start\n\n```rust\nlet unclosed = 1;\n";
    let root = Workspace::new(source);
    let snapshot = root.snapshot();
    let output = root.check(&[]);
    status(&output, 65);
    let renderer = BookRenderer::new(&[input("start.md", source)]).unwrap();
    assert!(!renderer.source_diagnostics().is_empty());
    assert_eq!(
        output.stdout,
        (renderer
            .verify_pdf_with_options(&book_options("Start"), PdfASettings::OFF)
            .unwrap()
            .to_json()
            .unwrap()
            + "\n")
            .as_bytes()
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("book_source_diagnostics"), "{stderr}");
    assert!(
        stderr.contains(&renderer.source_diagnostics().to_json()),
        "{stderr}"
    );
    let human = root
        .command(env!("CARGO_BIN_EXE_fmd"))
        .arg("--check-pdf")
        .output()
        .unwrap();
    status(&human, 65);
    assert!(human.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&human.stderr);
    assert!(
        stderr.contains("source warning")
            && stderr.contains("start.md")
            && stderr.contains("expanded chapter bytes"),
        "{stderr}"
    );
    assert_eq!(root.snapshot(), snapshot);
}

//! Native exports and dependency discovery use one selected source grammar.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use franken_markdown::watch::{ManualClock, PollWatcher};
use franken_markdown::{HtmlOptions, PdfOptions, Profile, SvgOptions, parse_markdown_with_profile};

const PROFILE: Option<Profile> = Some(Profile::CommonMarkGfm);
const SOURCE: &str = "# Profiles\n\n*Cross-line\n: emphasis*\n\n[Cross-line\n: link](https://example.com)\n\n`Cross-line\n: code`\n\nFirst\n: value one\nSecond\n: value two\n";
const CODE: &str = "`Example\n: code\n{{#include missing.txt}}\ntail`\n";
const SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"12\" height=\"12\"><rect width=\"12\" height=\"12\" fill=\"red\"/></svg>";

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fmd-native-source-profile-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }

    fn write(&self, name: &str, source: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, source).unwrap();
        path
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

    fn bytes(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.0.join(name)).unwrap()
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

fn html_options() -> HtmlOptions {
    HtmlOptions {
        profile: PROFILE,
        ..HtmlOptions::default()
    }
}

fn pdf_options() -> PdfOptions {
    PdfOptions {
        profile: PROFILE,
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    }
}

#[test]
fn every_native_format_parses_the_requested_dialect_before_rendering() {
    let dir = Workspace::new();
    dir.write("doc.md", SOURCE);
    succeeded(&dir.run(&[
        "doc.md",
        "--profile",
        "commonmark-gfm",
        "--to",
        "both",
        "--out",
        "doc.html",
    ]));
    let html = String::from_utf8(dir.bytes("doc.html")).unwrap();
    assert!(html.contains("<em>Cross-line\n: emphasis</em>"));
    assert!(html.contains("<code>Cross-line : code</code>"));
    assert!(html.contains("<p>First\n: value one\nSecond\n: value two</p>"));
    assert_eq!(
        html,
        franken_markdown::render_html(SOURCE, &html_options()).unwrap()
    );
    assert_eq!(
        dir.bytes("doc.pdf"),
        franken_markdown::render_pdf(SOURCE, &pdf_options()).unwrap()
    );

    let document = parse_markdown_with_profile(SOURCE, PROFILE);
    for (format, expected) in [
        (
            "epub",
            franken_markdown::render_epub(&document, &html_options()).unwrap(),
        ),
        (
            "svg",
            franken_markdown::render_svg(&document, &SvgOptions::default()),
        ),
    ] {
        let output = format!("doc.{format}");
        succeeded(&dir.run(&[
            "doc.md",
            "--profile",
            "commonmark-gfm",
            "--to",
            format,
            "--out",
            &output,
        ]));
        assert_eq!(dir.bytes(&output), expected, "{format}");
    }
    assert_eq!(dir.bytes("doc.md"), SOURCE.as_bytes());
}

#[test]
fn includes_rebase_commonmark_links_and_images_without_expanding_literal_code() {
    let dir = Workspace::new();
    dir.write("main.md", "{{#include parts/body.txt}}\n");
    let body =
        format!("[From\n: include](chapter.md)\n\n![Figure\n: alternative](figure.svg)\n\n{CODE}");
    dir.write("parts/body.txt", &body);
    dir.write("parts/figure.svg", SVG);
    succeeded(&dir.run(&[
        "main.md",
        "--profile",
        "commonmark-gfm",
        "--out",
        "main.html",
    ]));
    let html = String::from_utf8(dir.bytes("main.html")).unwrap();
    assert!(html.contains("href=\"parts/chapter.md\">From\n: include</a>"));
    assert!(html.contains("data:image/svg+xml;base64,"));
    assert!(html.contains("{{#include missing.txt}}"));
    assert_eq!(dir.bytes("parts/body.txt"), body.as_bytes());
}

#[test]
fn text_input_accepts_literal_include_examples_and_rejects_active_directives() {
    let dir = Workspace::new();
    let literal = dir.run(&["--text", CODE, "--profile", "commonmark-gfm", "--out", "-"]);
    succeeded(&literal);
    assert!(
        String::from_utf8(literal.stdout)
            .unwrap()
            .contains("<code>Example : code {{#include missing.txt}} tail</code>")
    );
    let active = dir.run(&[
        "--text",
        "{{#include missing.txt}}\n",
        "--profile",
        "commonmark-gfm",
        "--out",
        "-",
    ]);
    assert_eq!(active.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&active.stderr).contains("needs a file input"));
    assert!(active.stdout.is_empty());
}

#[test]
fn invalid_profiles_are_rejected_before_any_include_is_resolved() {
    let dir = Workspace::new();
    dir.write("main.md", "{{#include missing.txt}}\n");
    let output = dir.run(&["main.md", "--profile", "unknown", "--out", "main.html"]);
    assert_eq!(output.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown markdown authoring profile"));
    assert!(!dir.0.join("main.html").exists());
}

#[test]
fn watch_tracks_profile_specific_image_and_include_dependencies() {
    let dir = Workspace::new();
    let input = dir.write("main.md", "{{#include parts/body.txt}}\n");
    let body = format!("![Figure\n: alternative](figure.svg)\n\n{CODE}");
    let included = dir.write("parts/body.txt", &body);
    let image = dir.write("parts/figure.svg", SVG);
    let clock = ManualClock::new();
    let debounce = Duration::from_millis(10);
    let mut watcher = PollWatcher::new_with_profile(vec![input], debounce, clock.clone(), PROFILE);
    assert!(watcher.dependency_failures().is_empty());
    assert!(watcher.paths().contains(&included));
    assert!(watcher.paths().contains(&image));
    assert!(!watcher.paths().contains(&dir.0.join("parts/missing.txt")));
    assert!(watcher.poll().is_empty());
    dir.write("parts/figure.svg", &SVG.replace("red", "blue"));
    assert!(watcher.poll().is_empty());
    clock.advance(debounce);
    assert!(watcher.poll().iter().any(|event| event.path == image));
}

#[cfg(feature = "batch")]
fn batch_options(
    html: Option<Profile>,
    pdf: Option<Profile>,
) -> franken_markdown::batch::BatchOptions {
    use franken_markdown::batch::{BatchMode, BatchOptions};
    BatchOptions {
        html: HtmlOptions {
            profile: html,
            ..HtmlOptions::default()
        },
        pdf: PdfOptions {
            profile: pdf,
            ..pdf_options()
        },
        mode: BatchMode::Throughput,
        workers: Some(1),
        mem_budget: None,
        continue_on_error: false,
        timeout_secs: None,
        max_input_bytes: 64 * 1024 * 1024,
        max_pdf_image_bytes: 32 * 1024 * 1024,
    }
}

#[cfg(feature = "batch")]
#[test]
fn batch_uses_the_selected_formats_profile_for_its_shared_source_ast() {
    use franken_markdown::batch::{BatchPlan, OutputFormat, run_batch_blocking};
    let dir = Workspace::new();
    let input = dir.write("doc.md", SOURCE);
    let document = parse_markdown_with_profile(SOURCE, PROFILE);
    for (format, extension, html, pdf, expected) in [
        (
            OutputFormat::Html,
            "html",
            PROFILE,
            None,
            franken_markdown::render_html(SOURCE, &html_options())
                .unwrap()
                .into_bytes(),
        ),
        (
            OutputFormat::Pdf,
            "pdf",
            None,
            PROFILE,
            franken_markdown::render_pdf(SOURCE, &pdf_options()).unwrap(),
        ),
        (
            OutputFormat::Epub,
            "epub",
            PROFILE,
            None,
            franken_markdown::render_epub(&document, &html_options()).unwrap(),
        ),
        (
            OutputFormat::Svg,
            "svg",
            None,
            PROFILE,
            franken_markdown::render_svg(&document, &SvgOptions::default()),
        ),
    ] {
        let receipt = run_batch_blocking(
            BatchPlan {
                inputs: vec![input.clone()],
                format,
                out_dir: Some(dir.0.join(extension)),
            },
            &batch_options(html, pdf),
        )
        .unwrap();
        assert_eq!(receipt.failed_count(), 0, "{extension}");
        assert_eq!(
            dir.bytes(&format!("{extension}/doc.{extension}")),
            expected,
            "{extension}"
        );
    }
}

#[cfg(feature = "batch")]
#[test]
fn conflicting_combined_batch_profiles_fail_before_reads_or_publication() {
    use franken_markdown::batch::{BatchPlan, FileErrorKind, OutputFormat, run_batch_blocking};
    let dir = Workspace::new();
    dir.write("out/missing.html", "previous HTML");
    dir.write("out/missing.pdf", "previous PDF");
    let receipt = run_batch_blocking(
        BatchPlan {
            inputs: vec![dir.0.join("missing.md")],
            format: OutputFormat::Both,
            out_dir: Some(dir.0.join("out")),
        },
        &batch_options(PROFILE, None),
    )
    .unwrap();
    assert_eq!(receipt.failed_count(), 1);
    assert_eq!(receipt.first_failure_kind(), Some(FileErrorKind::Render));
    assert!(
        receipt.files[0]
            .error
            .as_deref()
            .unwrap()
            .contains("same dialect")
    );
    assert_eq!(dir.bytes("out/missing.html"), b"previous HTML");
    assert_eq!(dir.bytes("out/missing.pdf"), b"previous PDF");

    // Omitted and explicit GFM-Plus select the same grammar and remain valid.
    let input = dir.write("ordinary.md", "Term\n: Definition.\n");
    let receipt = run_batch_blocking(
        BatchPlan {
            inputs: vec![input],
            format: OutputFormat::Both,
            out_dir: Some(dir.0.join("out")),
        },
        &batch_options(None, Some(Profile::GfmPlus)),
    )
    .unwrap();
    assert_eq!(receipt.failed_count(), 0);
    assert!(
        String::from_utf8(dir.bytes("out/ordinary.html"))
            .unwrap()
            .contains("<dl>")
    );
}

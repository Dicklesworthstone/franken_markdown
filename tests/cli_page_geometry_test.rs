//! Native paper and margin controls exercise the real CLI and rendered PDFs.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use franken_markdown::{PageMargins, PageSize, PdfOptions, Theme, parse_markdown, render_pdf};

const PARAGRAPH: &str = "The selected paper and margins determine real line wrapping and pagination. A report keeps its original text and working [reference](https://example.com/reference), with **strong** and *emphasized* words.\n\n";

fn workspace() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "fmd-page-geometry-{}-{stamp}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_fmd"));
    command
        .current_dir(root)
        .env("FMD_CONFIG", root.join("config"))
        .env_remove("SOURCE_DATE_EPOCH");
    command
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

fn rendered(root: &Path, source: &str, options: &[&str]) -> Vec<u8> {
    let output = command(root)
        .args([
            "--text",
            source,
            "--to",
            "pdf",
            "--out",
            "result.pdf",
            "--json",
        ])
        .args(options)
        .output()
        .unwrap();
    succeeded(&output);
    assert!(output.stdout.is_empty());
    fs::read(root.join("result.pdf")).unwrap()
}

fn assert_media_boxes(bytes: &[u8], width: f64, height: f64) {
    let text = String::from_utf8_lossy(bytes);
    let mut pages = 0;
    for rest in text.split("/MediaBox [").skip(1) {
        let values = rest.split_once(']').unwrap().0;
        let values: Vec<f64> = values
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect();
        assert_eq!(values.len(), 4);
        assert_eq!(&values[..2], &[0.0, 0.0]);
        assert!((values[2] - width).abs() < 0.011, "{values:?}");
        assert!((values[3] - height).abs() < 0.011, "{values:?}");
        pages += 1;
    }
    assert!(pages > 0, "PDF must contain actual page boxes");
}

fn options(width: f32, height: f32, margins: [f32; 4]) -> PdfOptions {
    let [top_pt, right_pt, bottom_pt, left_pt] = margins;
    let mut theme = Theme::default();
    theme.page.size = PageSize {
        name: "custom",
        width_pt: width,
        height_pt: height,
    };
    theme.page.margins = PageMargins {
        top_pt,
        right_pt,
        bottom_pt,
        left_pt,
    };
    PdfOptions {
        theme,
        ..PdfOptions::default()
    }
}

#[test]
fn standard_paper_sizes_reach_every_pdf_page_and_match_native_rendering() {
    let root = workspace();
    let source = format!("# Report\n\n{}", PARAGRAPH.repeat(24));
    for (name, width, height) in [
        ("letter", 612.0, 792.0),
        ("A4", 210.0_f64 * 72.0 / 25.4, 297.0 * 72.0 / 25.4),
        ("a5", 148.0 * 72.0 / 25.4, 210.0 * 72.0 / 25.4),
        ("legal", 612.0, 1008.0),
        ("tabloid", 792.0, 1224.0),
    ] {
        let pdf = rendered(&root, &source, &["--page-size", name]);
        assert_media_boxes(&pdf, width, height);
        let expected =
            render_pdf(&source, &options(width as f32, height as f32, [72.0; 4])).unwrap();
        assert_eq!(pdf, expected, "paper {name}");
    }
}

#[test]
fn explicit_default_geometry_preserves_existing_pdf_bytes() {
    let root = workspace();
    let source = format!("# Default\n\n{}", PARAGRAPH.repeat(5));
    let default = rendered(&root, &source, &[]);
    let explicit = rendered(
        &root,
        &source,
        &[
            "--page-size",
            "letter",
            "--margin-top-pt",
            "72",
            "--margin-right-pt",
            "72",
            "--margin-bottom-pt",
            "72",
            "--margin-left-pt",
            "72",
        ],
    );
    assert_eq!(default, explicit);
    assert_eq!(
        default,
        render_pdf(&source, &PdfOptions::default()).unwrap()
    );
}

#[test]
fn custom_geometry_changes_wrapping_and_preserves_text_links_and_styles() {
    let root = workspace();
    let source = format!("# Custom\n\n{}", PARAGRAPH.repeat(18));
    let flags = [
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
    ];
    let custom = options(360.0, 504.0, [24.0, 30.0, 36.0, 42.0]);
    let pdf = rendered(&root, &source, &flags);
    assert_eq!(pdf, render_pdf(&source, &custom).unwrap());
    assert_eq!(pdf, rendered(&root, &source, &flags));
    assert_media_boxes(&pdf, 360.0, 504.0);
    assert!(String::from_utf8_lossy(&pdf).contains("/URI (https://example.com/reference)"));
    let doc = parse_markdown(&source);
    let layer = franken_markdown::pdf::verification_text_layer(&doc, &custom).unwrap();
    let default =
        franken_markdown::pdf::verification_text_layer(&doc, &PdfOptions::default()).unwrap();
    assert!(
        layer.page_count > default.page_count,
        "the content must actually repaginate"
    );
    for page in &layer.pages {
        for run in page.runs.iter().filter(|run| !run.text.is_empty()) {
            assert!(run.x >= 42.0, "left margin: {run:?}");
            assert!(run.y <= 480.0 && run.y >= 36.0, "vertical margins: {run:?}");
            assert!(run.overshoot.is_none(), "right margin: {run:?}");
        }
    }
    for text in ["selected", "reference", "strong", "emphasized"] {
        assert!(
            layer
                .pages
                .iter()
                .flat_map(|p| &p.runs)
                .any(|r| r.text.contains(text))
        );
    }
    let landscape = rendered(&root, &source, &["--page-size", "792x612"]);
    assert_media_boxes(&landscape, 792.0, 612.0);
    assert_eq!(
        landscape,
        render_pdf(&source, &options(792.0, 612.0, [72.0; 4])).unwrap()
    );
}

#[test]
fn persistent_paper_survives_later_config_edits_and_changes_rendering() {
    let root = workspace();
    for (key, value) in [
        ("page_size", "360.25x504.5"),
        ("margin_left_pt", "42"),
        ("font", "serif"),
    ] {
        succeeded(
            &command(&root)
                .args(["config", "set", key, value, "--json"])
                .output()
                .unwrap(),
        );
    }
    let stored = fs::read_to_string(root.join("config")).unwrap();
    assert!(stored.contains("page_size=360.25x504.5\n"));
    let get = command(&root)
        .args(["config", "get", "page_size", "--json"])
        .output()
        .unwrap();
    succeeded(&get);
    assert!(String::from_utf8_lossy(&get.stdout).contains("\"value\":\"360.25x504.5\""));
    let show = command(&root)
        .args(["config", "show", "--json"])
        .output()
        .unwrap();
    succeeded(&show);
    assert!(String::from_utf8_lossy(&show.stdout).contains("\"page_size\":\"360.25x504.5\""));
    let source = "# Persisted\n\nPaper preferences reach the renderer.\n";
    let mut expected = options(360.25, 504.5, [72.0, 72.0, 72.0, 42.0]);
    expected.theme = expected
        .theme
        .with_font(franken_markdown::FontFamily::Serif);
    assert_eq!(
        rendered(&root, source, &[]),
        render_pdf(source, &expected).unwrap()
    );
}

#[test]
fn flags_override_only_selected_config_values_and_no_config_restores_defaults() {
    let root = workspace();
    fs::write(root.join("config"), "page_size=a5\nmargin_top_pt=24\nmargin_right_pt=30\nmargin_bottom_pt=36\nmargin_left_pt=42\n").unwrap();
    let source = format!("# Precedence\n\n{}", PARAGRAPH.repeat(5));
    let pdf = rendered(
        &root,
        &source,
        &["--page-size", "legal", "--margin-left-pt", "18"],
    );
    assert_eq!(
        pdf,
        render_pdf(&source, &options(612.0, 1008.0, [24.0, 30.0, 36.0, 18.0])).unwrap()
    );
    assert_eq!(
        rendered(&root, &source, &["--no-config"]),
        render_pdf(&source, &PdfOptions::default()).unwrap()
    );
    let configured = rendered(&root, &source, &["--margin-left-pt", "18"]);
    assert_eq!(
        configured,
        render_pdf(
            &source,
            &options(
                (148.0_f64 * 72.0 / 25.4) as f32,
                (210.0_f64 * 72.0 / 25.4) as f32,
                [24.0, 30.0, 36.0, 18.0]
            )
        )
        .unwrap()
    );
}

#[test]
fn invalid_geometry_fails_before_input_reads_or_partial_combined_output() {
    let root = workspace();
    fs::write(root.join("result.html"), b"existing HTML").unwrap();
    fs::write(root.join("result.pdf"), b"existing PDF").unwrap();
    for flags in [
        vec!["--page-size", "a0"],
        vec!["--page-size", "0x600"],
        vec!["--page-size", "NaNx600"],
        vec!["--page-size", "600xinf"],
        vec!["--page-size", "14400.000001x600"],
        vec!["--margin-top-pt", "NaN"],
        vec!["--margin-top-pt=-1"],
        vec!["--margin-top-pt", "14401"],
        vec!["--margin-left-pt", "540.0000001", "--margin-right-pt", "0"],
        vec![
            "--page-size",
            "144.00731624x300",
            "--margin-left-pt",
            "36.00576428",
            "--margin-right-pt",
            "36.00155196",
        ],
        vec!["--page-size", "144x144"],
    ] {
        let output = command(&root)
            .args([
                "absent.md",
                "--to",
                "both",
                "--out",
                "result.html",
                "--json",
            ])
            .args(&flags)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(64),
            "{flags:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("invalid_page"),
            "{flags:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read(root.join("result.html")).unwrap(),
            b"existing HTML"
        );
        assert_eq!(fs::read(root.join("result.pdf")).unwrap(), b"existing PDF");
    }
    // Admission is against the merged rectangle, not the default margins.
    fs::write(
        root.join("config"),
        "margin_top_pt=36\nmargin_right_pt=36\nmargin_bottom_pt=36\nmargin_left_pt=36\n",
    )
    .unwrap();
    assert_media_boxes(
        &rendered(&root, "Tiny.\n", &["--page-size", "144x144"]),
        144.0,
        144.0,
    );
}

#[test]
fn configured_geometry_is_validated_after_explicit_margin_overrides() {
    let root = workspace();
    fs::write(
        root.join("config"),
        "page_size=300x300\nmargin_left_pt=200\nmargin_right_pt=100\n",
    )
    .unwrap();
    let output = command(&root)
        .args(["absent.md", "--to", "pdf", "--out", "unused.pdf", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid_page"));
    assert!(!root.join("unused.pdf").exists());
    let source = "# Corrected\n\nBody.\n";
    assert_eq!(
        rendered(&root, source, &["--margin-left-pt", "36"]),
        render_pdf(source, &options(300.0, 300.0, [72.0, 100.0, 72.0, 36.0])).unwrap()
    );
}

#[test]
fn explicit_geometry_requires_pdf_but_config_remains_usable_for_html() {
    let root = workspace();
    fs::write(root.join("config"), "page_size=a4\n").unwrap();
    for target in ["html", "svg", "epub", "interactive-html"] {
        for flag in [["--page-size", "a4"], ["--margin-top-pt", "36"]] {
            let output = command(&root)
                .args(["absent.md", "--to", target, "--json"])
                .args(flag)
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(64));
            assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported_target_option"));
            assert!(output.stdout.is_empty());
        }
    }
    let html = command(&root)
        .args(["--text", "# HTML still works\n"])
        .output()
        .unwrap();
    succeeded(&html);
    assert!(String::from_utf8_lossy(&html.stdout).contains("HTML still works"));
}

struct WatchChild(Child);

impl Drop for WatchChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn watch_rejects_invalid_geometry_instead_of_polling_after_a_permanent_error() {
    let root = workspace();
    for (target, paper, diagnostic) in [
        ("pdf", "NaNx600", "invalid_page"),
        ("html", "a4", "unsupported_target_option"),
    ] {
        let mut child = WatchChild(
            command(&root)
                .args([
                    "watch",
                    "absent.md",
                    "--to",
                    target,
                    "--out",
                    "untouched.pdf",
                    "--page-size",
                    paper,
                    "--json",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(30);
        let status = loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "invalid geometry started a watch loop"
            );
            std::thread::sleep(Duration::from_millis(25));
        };
        let mut stderr = String::new();
        child
            .0
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut stderr)
            .unwrap();
        assert_eq!(status.code(), Some(64), "{stderr}");
        assert!(stderr.contains(diagnostic), "{stderr}");
        assert!(!stderr.contains("\"event\":\"watching\""));
        assert!(!root.join("untouched.pdf").exists());
    }
}

fn wait_for_pdf(child: &mut WatchChild, path: &Path, expected: &[u8]) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if fs::read(path).is_ok_and(|bytes| bytes == expected) {
            return;
        }
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "watch exited before producing the configured PDF"
        );
        assert!(
            Instant::now() < deadline,
            "watch never produced the expected PDF"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn watch_retains_custom_geometry_on_initial_render_and_rebuild() {
    let root = workspace();
    fs::write(root.join("config"), "page_size=a4\nmargin_top_pt=24\nmargin_right_pt=30\nmargin_bottom_pt=36\nmargin_left_pt=42\n").unwrap();
    let first = "# First version\n\nA watched document.\n";
    fs::write(root.join("document.md"), first).unwrap();
    let expected = options(360.0, 504.0, [24.0, 30.0, 36.0, 18.0]);
    let mut child = WatchChild(
        command(&root)
            .args([
                "watch",
                "document.md",
                "--to",
                "pdf",
                "--out",
                "watched.pdf",
                "--interval",
                "25",
                "--page-size",
                "360x504",
                "--margin-left-pt",
                "18",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    wait_for_pdf(
        &mut child,
        &root.join("watched.pdf"),
        &render_pdf(first, &expected).unwrap(),
    );
    let second = format!("# Updated version\n\n{}", PARAGRAPH.repeat(8));
    fs::write(root.join("document.md"), &second).unwrap();
    wait_for_pdf(
        &mut child,
        &root.join("watched.pdf"),
        &render_pdf(&second, &expected).unwrap(),
    );
}

#[cfg(feature = "batch")]
#[test]
fn batch_uses_the_same_configured_geometry_as_single_document_rendering() {
    let root = workspace();
    fs::write(root.join("config"), "page_size=a4\nmargin_top_pt=24\nmargin_right_pt=30\nmargin_bottom_pt=36\nmargin_left_pt=42\n").unwrap();
    let source = format!("# Batch\n\n{}", PARAGRAPH.repeat(5));
    fs::write(root.join("document.md"), &source).unwrap();
    let output = command(&root)
        .args([
            "batch",
            "document.md",
            "--to",
            "pdf",
            "--out-dir",
            "batch-output",
            "--workers",
            "1",
            "--page-size",
            "360x504",
            "--margin-left-pt",
            "18",
            "--json",
        ])
        .output()
        .unwrap();
    succeeded(&output);
    let expected = render_pdf(&source, &options(360.0, 504.0, [24.0, 30.0, 36.0, 18.0])).unwrap();
    assert_eq!(
        fs::read(root.join("batch-output/document.pdf")).unwrap(),
        expected
    );
}

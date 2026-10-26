//! Opt-in PDF grids reach real native entry points and the browser facade.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use franken_markdown::{BaselineGrid, PdfOptions, render_pdf};

const SOURCE: &str = "# A measured grid\n\nThe body keeps an explicit baseline rhythm across ordinary paragraphs.\n\nA second paragraph follows with **strong text**.\n";

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fmd-baseline-grid-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fmd"));
        command
            .current_dir(&self.0)
            .env("FMD_CONFIG", self.0.join("config"))
            .env_remove("SOURCE_DATE_EPOCH");
        command
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }
    fn render(&self, options: &[&str]) -> Vec<u8> {
        let output = self
            .command()
            .args([
                "--text",
                SOURCE,
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
        fs::read(self.0.join("result.pdf")).unwrap()
    }
}
fn succeeded(output: &Output) {
    assert!(
        output.status.success(),
        "status={:?}; stderr={}; stdout={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
}
fn options(points: f64) -> PdfOptions {
    let mut options = PdfOptions::default();
    options.theme.page.baseline_grid = Some(BaselineGrid::from_points(points).unwrap());
    options
}

#[test]
fn explicit_grid_matches_native_and_browser_facades_without_changing_disabled_bytes() {
    let workspace = Workspace::new();
    let default = workspace.render(&[]);
    assert_eq!(default, workspace.render(&["--pdf-baseline-grid-pt", "0"]));
    let expected = render_pdf(SOURCE, &options(13.1254)).unwrap();
    assert_ne!(expected, default);
    assert_eq!(
        expected,
        workspace.render(&["--pdf-baseline-grid-pt", "13.1254"])
    );
    let browser = franken_markdown::wasm::WasmRenderOptions::default()
        .with_baseline_grid_pt(13.1254)
        .unwrap();
    assert_eq!(
        expected,
        franken_markdown::wasm::render_pdf(SOURCE, &browser)
            .unwrap()
            .bytes
    );
    assert_eq!(
        default,
        franken_markdown::wasm::render_pdf(SOURCE, &browser.with_baseline_grid_pt(0.0).unwrap())
            .unwrap()
            .bytes
    );
}

#[test]
fn grid_config_round_trips_quantization_and_explicit_zero_overrides_it() {
    let workspace = Workspace::new();
    let default = workspace.render(&[]);
    succeeded(&workspace.run(&["config", "set", "pdf_baseline_grid_pt", "13.1254"]));
    let get = workspace.run(&["config", "get", "pdf_baseline_grid_pt"]);
    succeeded(&get);
    assert_eq!(String::from_utf8(get.stdout).unwrap().trim(), "13.125");
    let config = fs::read_to_string(workspace.0.join("config")).unwrap();
    assert!(config.contains("pdf_baseline_grid_pt=13.125"));
    assert_eq!(
        workspace.render(&[]),
        render_pdf(SOURCE, &options(13.125)).unwrap()
    );
    assert_eq!(
        workspace.render(&["--pdf-baseline-grid-pt", "16"]),
        render_pdf(SOURCE, &options(16.0)).unwrap()
    );
    assert_eq!(workspace.render(&["--pdf-baseline-grid-pt", "0"]), default);
    assert_eq!(workspace.render(&["--no-config"]), default);
    // A PDF-specific persistent default must not break HTML publication.
    succeeded(&workspace.run(&["--text", SOURCE, "--to", "html", "--out", "result.html"]));
    let before = fs::read(workspace.0.join("config")).unwrap();
    let invalid = workspace.run(&["config", "set", "pdf_baseline_grid_pt", "NaN", "--json"]);
    assert!(!invalid.status.success());
    assert_eq!(before, fs::read(workspace.0.join("config")).unwrap());
}

#[test]
fn invalid_or_incompatible_grids_fail_before_native_output_writes() {
    let workspace = Workspace::new();
    fs::write(workspace.0.join("result.pdf"), b"existing artifact").unwrap();
    for value in ["NaN", "inf", "0.5", "144.001"] {
        let output = workspace.run(&[
            "--text",
            SOURCE,
            "--to",
            "pdf",
            "--out",
            "result.pdf",
            "--pdf-baseline-grid-pt",
            value,
            "--json",
        ]);
        assert_eq!(output.status.code(), Some(64));
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("invalid_input") && error.contains("pdf_baseline_grid:"),
            "{error}"
        );
        assert_eq!(
            fs::read(workspace.0.join("result.pdf")).unwrap(),
            b"existing artifact"
        );
    }
    let incompatible = workspace.run(&[
        "--text",
        SOURCE,
        "--to",
        "pdf",
        "--out",
        "result.pdf",
        "--pdf-baseline-grid-pt",
        "14",
        "--fit-to-pages",
        "2",
        "--json",
    ]);
    assert_eq!(incompatible.status.code(), Some(64));
    assert!(
        String::from_utf8_lossy(&incompatible.stderr).contains("pdf_baseline_grid_incompatible:")
    );
    let non_pdf = workspace.run(&[
        "--text",
        SOURCE,
        "--to",
        "html",
        "--out",
        "result.html",
        "--pdf-baseline-grid-pt",
        "14",
        "--json",
    ]);
    assert_eq!(non_pdf.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&non_pdf.stderr).contains("unsupported_target_option"));
    assert!(!workspace.0.join("result.html").exists());
}

#[cfg(feature = "batch")]
#[test]
fn batch_uses_configured_grid_and_can_disable_it_per_invocation() {
    let workspace = Workspace::new();
    fs::write(workspace.0.join("document.md"), SOURCE).unwrap();
    fs::write(workspace.0.join("config"), "pdf_baseline_grid_pt=13.125\n").unwrap();
    succeeded(&workspace.run(&[
        "batch",
        "document.md",
        "--to",
        "pdf",
        "--out-dir",
        "batch-output",
        "--workers",
        "1",
    ]));
    assert_eq!(
        fs::read(workspace.0.join("batch-output/document.pdf")).unwrap(),
        render_pdf(SOURCE, &options(13.125)).unwrap()
    );
    succeeded(&workspace.run(&[
        "batch",
        "document.md",
        "--to",
        "pdf",
        "--out-dir",
        "batch-output",
        "--workers",
        "1",
        "--pdf-baseline-grid-pt",
        "0",
    ]));
    assert_eq!(
        fs::read(workspace.0.join("batch-output/document.pdf")).unwrap(),
        render_pdf(SOURCE, &PdfOptions::default()).unwrap()
    );
}

#[test]
fn book_grid_reaches_the_assembled_native_pdf_and_rejects_fitting() {
    let workspace = Workspace::new();
    let book = workspace.0.join("book");
    fs::create_dir_all(&book).unwrap();
    fs::write(book.join("chapter.md"), SOURCE).unwrap();
    fs::write(workspace.0.join("config"), "pdf_baseline_grid_pt=13.125\n").unwrap();
    succeeded(&workspace.run(&[
        "book",
        "book",
        "--to",
        "pdf",
        "--out",
        "book.pdf",
        "--toc=false",
        "--pdf-page-numbers=false",
    ]));
    let actual = fs::read(workspace.0.join("book.pdf")).unwrap();
    let document =
        franken_markdown::book::BookRenderer::new(&[franken_markdown::book::BookInput {
            path: "chapter.md".into(),
            source: SOURCE.into(),
        }])
        .unwrap();
    let mut expected_options = options(13.125);
    expected_options.title = Some("A measured grid".into());
    let expected =
        franken_markdown::book::render_book_pdf(document.book(), &expected_options).unwrap();
    assert_eq!(actual, expected);
    let rejected = workspace.run(&[
        "book",
        "book",
        "--to",
        "pdf",
        "--out",
        "book.pdf",
        "--fit-to-pages",
        "2",
        "--json",
    ]);
    assert_eq!(rejected.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("pdf_baseline_grid_incompatible:"));
    assert_eq!(actual, fs::read(workspace.0.join("book.pdf")).unwrap());
}

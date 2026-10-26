//! Batch publication keeps the same per-document metadata as ordinary rendering.
#![cfg(feature = "batch")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fmd-batch-frontmatter-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) {
        let path = self.path(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fmd"));
        command
            .current_dir(&self.0)
            .env("FMD_CONFIG", self.path("config"))
            .env("SOURCE_DATE_EPOCH", "1700000000");
        command
    }

    fn bytes(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.path(name)).unwrap()
    }

    fn text(&self, name: &str) -> String {
        std::fs::read_to_string(self.path(name)).unwrap()
    }

    fn compare_with_single(
        &self,
        inputs: &[&str],
        targets: &[&str],
        flags: &[&str],
        workers: &str,
    ) {
        for &target in targets {
            let output_dir = format!("batch-{target}-{workers}");
            let run = || {
                self.command()
                    .arg("batch")
                    .args(inputs)
                    .args([
                        "--to",
                        target,
                        "--out-dir",
                        &output_dir,
                        "--workers",
                        workers,
                        "--json",
                    ])
                    .args(flags)
                    .output()
                    .unwrap()
            };
            let first = run();
            succeeded(&first);
            let second = run();
            succeeded(&second);
            assert_eq!(
                first.stdout, second.stdout,
                "deterministic {target} receipt"
            );
            for &input in inputs {
                let stem = std::path::Path::new(input)
                    .file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap();
                let extension = if target == "both" { "html" } else { target };
                let single_path = format!("single-{stem}.{extension}");
                succeeded(
                    &self
                        .command()
                        .args([input, "--to", target, "--out", &single_path])
                        .args(flags)
                        .output()
                        .unwrap(),
                );
                assert_eq!(
                    self.bytes(&format!("{output_dir}/{stem}.{extension}")),
                    self.bytes(&single_path),
                    "{input}, {target}, {workers} workers"
                );
                if target == "both" {
                    assert_eq!(
                        self.bytes(&format!("{output_dir}/{stem}.pdf")),
                        self.bytes(&format!("single-{stem}.pdf")),
                        "combined PDF: {input}, {workers} workers"
                    );
                }
            }
        }
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
        "status {:?}: stdout={} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn each_documents_frontmatter_survives_every_batch_output_format() {
    let dir = Workspace::new();
    dir.write(
        "a.md",
        "---\ntitle: Metadata title A\nauthor: Writer A\nlang: fr\ntoc: true\ntoc_depth: 2\n---\n# Heading A\n\n{{#include parts/shared.txt}}\n",
    );
    dir.write(
        "parts/shared.txt",
        "## Included heading\n\nA paragraph from an included source.\n\n### Deep heading\n",
    );
    dir.write(
        "b.md",
        "\u{feff}---\r\ntitle=Metadata title B\r\nauthor=Writer B\r\nlang=de\r\ntoc=false\r\n---\r\n# Heading B\r\n\r\nAnother document.\r\n",
    );
    for workers in ["1", "2"] {
        dir.compare_with_single(
            &["a.md", "b.md"],
            &["html", "pdf", "both", "epub", "svg"],
            &["--no-config"],
            workers,
        );
    }
    let html_a = dir.text("batch-html-2/a.html");
    let html_b = dir.text("batch-html-2/b.html");
    assert!(html_a.contains("<title>Metadata title A</title>"));
    assert!(html_a.contains("lang=\"fr\""));
    assert!(html_a.contains("<nav class=\"toc\">"));
    assert!(html_a.contains("Included heading"));
    assert!(html_b.contains("<title>Metadata title B</title>"));
    assert!(html_b.contains("lang=\"de\""));
    assert!(!html_b.contains("<nav class=\"toc\">"));
    let pdf = dir.bytes("batch-pdf-2/a.pdf");
    let pdf = String::from_utf8_lossy(&pdf);
    assert!(pdf.contains("/Title (Metadata title A)"));
    assert!(pdf.contains("/Author (Writer A)"));
}

#[test]
fn flags_override_per_file_metadata_which_overrides_native_config() {
    let dir = Workspace::new();
    dir.write("config", "lang=de\ntoc=true\ntoc_depth=1\n");
    dir.write(
        "a.md",
        "---\ntitle: Document title\nauthor: Document author\nlang: fr\ntoc: false\ntoc_depth: 2\n---\n# One\n\n## Two\n\n### Three\n",
    );
    dir.write("b.md", "# Default title\n\n## Default child\n");
    dir.compare_with_single(&["a.md", "b.md"], &["both"], &[], "2");
    let metadata = dir.text("batch-both-2/a.html");
    let defaults = dir.text("batch-both-2/b.html");
    assert!(metadata.contains("lang=\"fr\""));
    assert!(!metadata.contains("<nav class=\"toc\">"));
    assert!(defaults.contains("lang=\"de\""));
    assert!(defaults.contains("<nav class=\"toc\">"));
    assert!(defaults.contains("<title>Default title</title>"));

    dir.compare_with_single(
        &["a.md", "b.md"],
        &["both"],
        &[
            "--title",
            "Explicit title",
            "--author",
            "Explicit author",
            "--lang",
            "es",
            "--toc",
            "--toc-depth",
            "3",
        ],
        "2",
    );
    for name in ["a", "b"] {
        let html = dir.text(&format!("batch-both-2/{name}.html"));
        assert!(html.contains("<title>Explicit title</title>"));
        assert!(html.contains("lang=\"es\""));
        assert!(html.contains("<nav class=\"toc\">"));
    }

    dir.compare_with_single(&["a.md", "b.md"], &["both"], &["--no-config"], "1");
    let defaults = dir.text("batch-both-1/b.html");
    assert!(!defaults.contains("lang=\"de\""));
    assert!(!defaults.contains("<nav class=\"toc\">"));
}

#[test]
fn absent_and_malformed_metadata_keep_ordinary_render_bytes() {
    let dir = Workspace::new();
    dir.write("plain.md", "# Plain document\n\nOriginal body.\n");
    dir.write(
        "malformed.md",
        "---\nThis is body content, not frontmatter.\n---\n\n# Original heading\n",
    );
    dir.compare_with_single(
        &["plain.md", "malformed.md"],
        &["both", "epub"],
        &["--no-config"],
        "1",
    );
    assert!(
        dir.text("batch-both-1/malformed.html")
            .contains("This is body content, not frontmatter.")
    );
}

#[test]
fn public_batch_api_merges_metadata_without_replacing_explicit_options() {
    use franken_markdown::batch::{
        BatchMode, BatchOptions, BatchPlan, OutputFormat, run_batch_blocking,
    };
    use franken_markdown::{
        HtmlOptions, PdfOptions, parse_markdown, render_html_document, render_pdf_document,
    };

    let dir = Workspace::new();
    let source = "---\ntitle: Library title\nauthor: Document author\nlang: fr\ntoc: true\ntoc_depth: 2\n---\n# Heading\n\n## Child heading\n\nA paragraph.\n";
    dir.write("source.md", source);
    let opts = BatchOptions {
        html: HtmlOptions {
            title: Some("Explicit HTML title".into()),
            ..HtmlOptions::default()
        },
        pdf: PdfOptions {
            author: Some("Explicit PDF author".into()),
            lang: Some("en".into()),
            toc_depth: Some(1),
            ..PdfOptions::default()
        },
        mode: BatchMode::Throughput,
        workers: Some(1),
        mem_budget: None,
        continue_on_error: false,
        timeout_secs: None,
        max_input_bytes: 64 * 1024 * 1024,
        max_pdf_image_bytes: 32 * 1024 * 1024,
    };
    let plan = BatchPlan {
        inputs: vec![dir.path("source.md")],
        format: OutputFormat::Both,
        out_dir: Some(dir.path("library")),
    };
    let receipt = run_batch_blocking(plan, &opts).unwrap();
    assert_eq!(receipt.ok_count(), 1, "{}", receipt.to_json());
    let mut expected_html = opts.html.clone();
    expected_html.lang = Some("fr".into());
    expected_html.toc = true;
    expected_html.toc_depth = Some(2);
    let mut expected_pdf = opts.pdf.clone();
    expected_pdf.title = Some("Library title".into());
    expected_pdf.toc = true;
    let doc = parse_markdown(source);
    assert_eq!(
        dir.text("library/source.html"),
        render_html_document(&doc, &expected_html).unwrap()
    );
    assert_eq!(
        dir.bytes("library/source.pdf"),
        render_pdf_document(&doc, &expected_pdf).unwrap()
    );
    assert_eq!(opts.html.lang, None, "shared options remain unchanged");
    assert_eq!(opts.pdf.title, None, "shared options remain unchanged");
}

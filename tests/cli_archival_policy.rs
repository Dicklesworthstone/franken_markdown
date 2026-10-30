//! Archival publication uses one explicit policy across single and batch output.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use franken_markdown::{PdfASettings, PdfOptions, render_pdf_pdfa};

const SOURCE: &str = "# Archive\n\nA [public reference](https://example.com) and ordinary text.\n";

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fmd-archival-policy-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, path: &str, bytes: impl AsRef<[u8]>) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn read(&self, path: &str) -> Vec<u8> {
        std::fs::read(self.0.join(path)).unwrap()
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_fmd"))
            .current_dir(&self.0)
            .env("FMD_CONFIG", self.0.join("absent.config"))
            .env_remove("SOURCE_DATE_EPOCH")
            .args(["--no-config", "--json"])
            .args(args)
            .output()
            .unwrap()
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

fn assert_usage(output: &Output, message: &str) {
    assert_eq!(output.status.code(), Some(64));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("\"code\":\"usage_error\""), "{stderr}");
    assert!(stderr.contains(message), "{stderr}");
    assert!(output.stdout.is_empty());
}

fn assert_archive(bytes: &[u8]) {
    assert!(bytes.starts_with(b"%PDF-1.7"));
    let text = String::from_utf8_lossy(bytes);
    for marker in [
        "/Type /Metadata",
        "/OutputIntents [",
        "<pdfaid:part>2</pdfaid:part>",
        "<pdfaid:conformance>B</pdfaid:conformance>",
        "/FontFile2 ",
        "/ToUnicode ",
    ] {
        assert!(text.contains(marker), "missing {marker}");
    }
}

#[test]
fn strict_archival_mode_requires_an_enabled_profile_and_preserves_prior_outputs() {
    let workspace = Workspace::new();
    workspace.write("document.md", SOURCE);
    workspace.write("archive.html", "previous HTML");
    workspace.write("archive.pdf", "previous PDF");
    for flags in [
        vec!["--pdf-a-strict"],
        vec!["--pdf-a", "off", "--pdf-a-strict"],
        vec!["--pdf-a", "none", "--pdf-a-strict"],
    ] {
        let mut args = vec!["document.md", "--to", "both", "--out", "archive.html"];
        args.extend(flags);
        assert_usage(&workspace.run(&args), "--pdf-a-strict requires --pdf-a 2b");
        assert_eq!(workspace.read("archive.html"), b"previous HTML");
        assert_eq!(workspace.read("archive.pdf"), b"previous PDF");
    }
}

#[test]
fn archival_flags_are_rejected_on_formats_that_cannot_honor_them() {
    let workspace = Workspace::new();
    workspace.write("document.md", SOURCE);
    workspace.write("output", "previous output");
    for target in ["html", "svg", "epub", "interactive-html"] {
        for flags in [vec!["--pdf-a", "2b"], vec!["--pdf-a-strict"]] {
            let mut args = vec!["document.md", "--to", target, "--out", "output"];
            args.extend(flags);
            assert_usage(&workspace.run(&args), "require --to pdf or --to both");
            assert_eq!(workspace.read("output"), b"previous output");
        }
    }
}

#[test]
fn watch_rejects_archival_usage_before_measurement_can_edit_the_source() {
    let workspace = Workspace::new();
    workspace.write("document.md", SOURCE);
    workspace.write("output", "previous output");
    for (target, flags, message) in [
        (
            "html",
            vec!["--pdf-a", "2b"],
            "require --to pdf or --to both",
        ),
        (
            "pdf",
            vec!["--pdf-a", "off", "--pdf-a-strict"],
            "--pdf-a-strict requires --pdf-a 2b",
        ),
        (
            "pdf",
            vec!["--pdf-a-strict"],
            "--pdf-a-strict requires --pdf-a 2b",
        ),
    ] {
        let mut args = vec![
            "watch",
            "document.md",
            "--to",
            target,
            "--out",
            "output",
            "--measure",
            "1",
        ];
        args.extend(flags);
        let output = workspace.run(&args);
        assert_usage(&output, message);
        assert!(!String::from_utf8_lossy(&output.stderr).contains("\"event\":\"watching\""));
        assert_eq!(workspace.read("document.md"), SOURCE.as_bytes());
        assert_eq!(workspace.read("output"), b"previous output");
    }
}

#[test]
fn explicit_profiles_keep_native_render_bytes_equal_to_the_library() {
    let workspace = Workspace::new();
    workspace.write("document.md", SOURCE);
    for (profile, settings) in [("off", PdfASettings::OFF), ("2b", PdfASettings::a2b())] {
        succeeded(&workspace.run(&[
            "document.md",
            "--to",
            "pdf",
            "--out",
            "archive.pdf",
            "--pdf-a",
            profile,
        ]));
        let bytes = workspace.read("archive.pdf");
        assert_eq!(
            bytes,
            render_pdf_pdfa(SOURCE, &PdfOptions::default(), settings).unwrap()
        );
        if settings.mode.is_a2b() {
            assert_archive(&bytes);
        } else {
            assert!(!String::from_utf8_lossy(&bytes).contains("/OutputIntents"));
        }
    }
}

#[cfg(feature = "batch")]
mod batch {
    use super::*;
    use franken_markdown::batch::{
        BatchMode, BatchOptions, BatchPdfAError, BatchPlan, OutputFormat, run_batch_blocking,
        run_batch_blocking_pdfa,
    };
    use franken_markdown::{HtmlOptions, PdfAMode};

    #[test]
    fn batch_archival_outputs_match_single_documents_with_metadata_and_includes() {
        let workspace = Workspace::new();
        workspace.write(
            "one.md",
            "---\ntitle: First archive\nauthor: First writer\nlang: fr\n---\n# One\n\n{{#include part.txt}}\n",
        );
        workspace.write("part.txt", "A shared [reference](https://example.com).\n");
        workspace.write(
            "two.md",
            "---\ntitle: Second archive\nauthor: Second writer\n---\n# Two\n",
        );
        for workers in ["1", "2"] {
            for target in ["pdf", "both"] {
                let args = [
                    "batch",
                    "one.md",
                    "two.md",
                    "--to",
                    target,
                    "--out-dir",
                    "batch",
                    "--workers",
                    workers,
                    "--pdf-a",
                    "2b",
                    "--pdf-a-strict",
                ];
                let first = workspace.run(&args);
                succeeded(&first);
                let second = workspace.run(&args);
                succeeded(&second);
                assert_eq!(first.stdout, second.stdout, "receipt must be deterministic");
                assert!(String::from_utf8_lossy(&first.stderr).contains("\"event\":\"pdf_a\""));
                for name in ["one", "two"] {
                    let source = format!("{name}.md");
                    let out = format!("single-{name}.pdf");
                    succeeded(&workspace.run(&[
                        &source,
                        "--to",
                        target,
                        "--out",
                        &out,
                        "--pdf-a",
                        "2b",
                        "--pdf-a-strict",
                    ]));
                    let bytes = workspace.read(&format!("batch/{name}.pdf"));
                    assert_archive(&bytes);
                    assert_eq!(
                        bytes,
                        workspace.read(&out),
                        "{name}, {target}, workers={workers}"
                    );
                    if target == "both" {
                        assert_eq!(
                            workspace.read(&format!("batch/{name}.html")),
                            workspace.read(&format!("single-{name}.html"))
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn strict_failure_preserves_both_siblings_and_safe_jobs_can_complete() {
        let workspace = Workspace::new();
        workspace.write(
            "bad.md",
            "# Cannot archive\n\n[Local](file:///private/reference.pdf)\n",
        );
        workspace.write("good.md", SOURCE);
        workspace.write("out/bad.html", "previous HTML");
        workspace.write("out/bad.pdf", "previous PDF");
        for workers in ["1", "2"] {
            let args = [
                "batch",
                "bad.md",
                "good.md",
                "--to",
                "both",
                "--out-dir",
                "out",
                "--workers",
                workers,
                "--pdf-a",
                "2b",
                "--pdf-a-strict",
            ];
            let output = workspace.run(&args);
            assert_eq!(output.status.code(), Some(70));
            let receipt = String::from_utf8_lossy(&output.stdout);
            assert!(receipt.contains("pdf_a_file_uri"), "{receipt}");
            assert!(receipt.contains("\"error_kind\":\"render\""), "{receipt}");
            assert_eq!(workspace.read("out/bad.html"), b"previous HTML");
            assert_eq!(workspace.read("out/bad.pdf"), b"previous PDF");
            assert_archive(&workspace.read("out/good.pdf"));
            let mut tolerant = args.to_vec();
            tolerant.push("--continue-on-error");
            let continued = workspace.run(&tolerant);
            succeeded(&continued);
            assert!(String::from_utf8_lossy(&continued.stdout).contains("\"failed\":1"));
            assert_eq!(workspace.read("out/bad.html"), b"previous HTML");
            assert_eq!(workspace.read("out/bad.pdf"), b"previous PDF");
        }
    }

    #[test]
    fn invalid_batch_profiles_fail_before_creating_the_output_directory() {
        let workspace = Workspace::new();
        workspace.write("document.md", SOURCE);
        for (target, flags, message) in [
            ("pdf", vec!["--pdf-a", "1a"], "unknown --pdf-a"),
            (
                "pdf",
                vec!["--pdf-a", "off", "--pdf-a-strict"],
                "requires --pdf-a 2b",
            ),
            (
                "html",
                vec!["--pdf-a", "2b"],
                "require --to pdf or --to both",
            ),
        ] {
            let mut args = vec![
                "batch",
                "document.md",
                "--to",
                target,
                "--out-dir",
                "unused",
            ];
            args.extend(flags);
            assert_usage(&workspace.run(&args), message);
            assert!(!workspace.0.join("unused").exists());
        }
    }

    #[test]
    fn library_batch_off_retains_default_bytes_and_archival_mode_is_explicit() {
        let workspace = Workspace::new();
        workspace.write("document.md", SOURCE);
        let options = BatchOptions {
            html: HtmlOptions::default(),
            pdf: PdfOptions::default(),
            mode: BatchMode::Interactive,
            workers: Some(1),
            mem_budget: None,
            continue_on_error: false,
            timeout_secs: None,
            max_input_bytes: 1024 * 1024,
            max_pdf_image_bytes: 1024 * 1024,
        };
        let plan = |format| BatchPlan {
            inputs: vec![workspace.0.join("document.md")],
            format,
            out_dir: Some(workspace.0.join("out")),
        };
        let plain = run_batch_blocking(plan(OutputFormat::Both), &options).unwrap();
        let expected = workspace.read("out/document.pdf");
        let off =
            run_batch_blocking_pdfa(plan(OutputFormat::Both), &options, PdfASettings::OFF).unwrap();
        assert_eq!(plain.to_json(), off.to_json());
        assert_eq!(expected, workspace.read("out/document.pdf"));
        let archival = run_batch_blocking_pdfa(
            plan(OutputFormat::Both),
            &options,
            PdfASettings::a2b_strict(),
        )
        .unwrap();
        assert_eq!(archival.ok_count(), 1);
        assert_archive(&workspace.read("out/document.pdf"));
        assert!(matches!(
            run_batch_blocking_pdfa(plan(OutputFormat::Html), &options, PdfASettings::a2b()),
            Err(BatchPdfAError::InvalidOptions(_))
        ));
        assert!(matches!(
            run_batch_blocking_pdfa(
                plan(OutputFormat::Pdf),
                &options,
                PdfASettings {
                    mode: PdfAMode::Off,
                    strict: true
                }
            ),
            Err(BatchPdfAError::InvalidOptions(_))
        ));
    }
}
